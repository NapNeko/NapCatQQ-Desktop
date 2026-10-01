//! 调试台的三份落盘文件：工作区（`workspace.json`）、收藏夹（`collections.json`）、
//! 调用历史（`history.jsonl`），都放在 `<数据根>/onebot-debug/` 下。
//!
//! 规矩和仓库里其它持久化文件一致：
//! - 一个文件只有一个写入口，就是这里。所有写盘在 `write_gate` 里排队，内存和文件在同一把闸里改；
//! - 整份文件的写入一律「临时文件 + fsync + rename」，崩在半路也不会留下写了一半的正本；
//! - 读不出来的文件先改名成 `<文件名>.broken-<Unix 秒>` 保留原件，再从默认值起步并留一条提示，
//!   不静默重置（重置后下一次保存就把用户的数据盖没了）。
//!
//! 历史是这里最占内存的一份：单条回包最大 256 KiB，1000 条理论上近 256 MB。所以内存里只按
//! [`RESPONSE_BUDGET`] 的预算留回包全文，超出的从最老的条目开始把回包从内存丢掉（文件里还在），
//! 要看全文时 [`DebugStore::history_entry`] 再回文件里读；压缩重写也是按行从文件复制，
//! 不从内存重新序列化，被内存丢掉的回包才不会在压缩时一起丢了。

use std::collections::{HashMap, HashSet, VecDeque};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::{Mutex as StdMutex, PoisonError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ncd_domain::bot_config::BackendType;
use ncd_domain::onebot_debug::{
    DebugCallOrigin, DebugCallRequest, DebugCallResult, DebugChannelId, DebugCollections,
    DebugError, DebugHistoryEntry, DebugHistoryPage, DebugHistoryQuery, DebugHistorySummary,
    DebugStorageNotice, DebugWorkspace,
};
use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt, BufReader, BufWriter};
use tokio::sync::Mutex;
use uuid::Uuid;

use super::params::{cap_record_params, serialized_len};

const DIR_NAME: &str = "onebot-debug";
const WORKSPACE_FILE: &str = "workspace.json";
const COLLECTIONS_FILE: &str = "collections.json";
const HISTORY_FILE: &str = "history.jsonl";
/// 三份文件写临时文件时的命名：`<文件名>.tmp-<uuid>`，加载时据此清理崩溃留下的孤儿
const TMP_MARKER: &str = ".tmp-";

/// 标签页上限。超出时丢最老的（当前激活的除外），免得手改文件或前端 bug 把工作区撑大
const MAX_TABS: usize = 50;
const MAX_CLOSED_TABS: usize = 10;
const MAX_RECENT_ACTIONS: usize = 20;

/// 历史保留的条数
const HISTORY_KEEP: usize = 1000;
/// 文件行数超过这个数才压缩重写：留出 200 条余量，不至于每追加一条就重写整个文件
const HISTORY_COMPACT_AT: usize = 1200;
/// 压缩失败后（比如文件被杀软占着）再多追加这么多行才重试，别每追加一条都白折腾一遍
const COMPACT_BACKOFF: usize = 200;
/// 单条历史里回包序列化后的上限，超了整个回包不存，只留个「被截断」的标记
const HISTORY_RESPONSE_LIMIT: usize = 256 * 1024;
/// 内存里所有历史回包全文加起来的字节预算，超了从最老的条目开始丢内存里的回包
const RESPONSE_BUDGET: usize = 32 * 1024 * 1024;
const HISTORY_PAGE_MAX: u32 = 200;

/// rename 失败后的重试：Windows 上杀软 / 索引服务会短暂占着刚写完的文件
const RENAME_RETRIES: u32 = 3;
const RENAME_RETRY_DELAY: Duration = Duration::from_millis(50);

/// 导入文件的大小上限：收藏夹是纯文本，几 MB 已经是几千条请求，再大多半是选错了文件
const IMPORT_MAX_BYTES: u64 = 16 * 1024 * 1024;

/// 这几个动作的回包里是登录凭据（cookies、csrf token、clientkey、下载图片 / 文件用的 rkey），
/// 历史落在明文文件里，一旦导出 / 截图 / 备份数据目录就泄露账号，所以只记「调过」，不记回包。
/// 异步变体（`_async` 后缀）同理
const CREDENTIAL_ACTIONS: [&str; 7] = [
    "get_cookies",
    "get_credentials",
    "get_csrf_token",
    "get_clientkey",
    "get_rkey",
    "nc_get_rkey",
    "get_rkey_server",
];

pub(crate) struct DebugStore {
    dir: PathBuf,
    workspace: Mutex<DebugWorkspace>,
    collections: Mutex<DebugCollections>,
    history: Mutex<HistoryMem>,
    /// 读盘时产生的「文件坏了、已挪开」提示，界面取走一次就清空
    notices: StdMutex<Vec<DebugStorageNotice>>,
    /// 所有写盘排队的闸。写盘方才该碰的状态放在闸里面，不拿到闸就摸不到
    write_gate: Mutex<WriteState>,
}

/// 只有持有 `write_gate` 才能读写的状态
struct WriteState {
    /// `history.jsonl` 里现有的有效行数（含读不出来的行），
    /// 用来判断什么时候该压缩，不必每次追加都数一遍文件
    history_lines: usize,
    /// 压缩失败后，行数超过这个值才再试
    compact_retry_at: usize,
}

/// 内存里的历史：最新的 1000 条，加上回包全文的预算记账
struct HistoryMem {
    /// 最旧的在前，最新的在后
    entries: VecDeque<MemEntry>,
    /// 内存里各条 `response` 序列化后的字节数之和
    response_bytes: usize,
    budget: usize,
}

struct MemEntry {
    entry: DebugHistoryEntry,
    /// 这条的 `response` 还在内存里时，它序列化后的字节数；被丢掉后为 0
    response_bytes: usize,
    /// 这一行已经写进 `history.jsonl`。没写成的（磁盘出错）只在内存里有，
    /// 丢了回包就再也找不回来，所以不参与预算淘汰
    persisted: bool,
    /// `response` 已经从内存里丢掉，全文在文件里
    response_on_disk: bool,
}

impl HistoryMem {
    fn new(budget: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            response_bytes: 0,
            budget,
        }
    }

    fn push(&mut self, entry: DebugHistoryEntry, response_bytes: usize, persisted: bool) {
        self.response_bytes += response_bytes;
        self.entries.push_back(MemEntry {
            entry,
            response_bytes,
            persisted,
            response_on_disk: false,
        });
        while self.entries.len() > HISTORY_KEEP {
            if let Some(old) = self.entries.pop_front() {
                self.response_bytes = self.response_bytes.saturating_sub(old.response_bytes);
            }
        }
        self.enforce_budget();
    }

    /// 超预算就从最老的开始丢内存里的回包，直到回到预算内
    fn enforce_budget(&mut self) {
        for item in self.entries.iter_mut() {
            if self.response_bytes <= self.budget {
                break;
            }
            if item.persisted && item.response_bytes > 0 {
                item.entry.response = None;
                item.response_on_disk = true;
                self.response_bytes = self.response_bytes.saturating_sub(item.response_bytes);
                item.response_bytes = 0;
            }
        }
    }

    fn clear(&mut self) {
        self.entries.clear();
        self.response_bytes = 0;
    }
}

impl DebugStore {
    /// 读三份文件。缺失就用默认值；读不出来的挪开并留提示。本身不创建目录，第一次写才建
    pub async fn load(data_root: &Path) -> Self {
        Self::load_with_budget(data_root, RESPONSE_BUDGET).await
    }

    /// 和 `load` 一样，只是回包内存预算可调（测试里调小，免得要写几十 MB 才能碰到淘汰）
    async fn load_with_budget(data_root: &Path, budget: usize) -> Self {
        let dir = data_root.join(DIR_NAME);
        let mut notices = Vec::new();

        // 上次崩在「写临时文件」和「rename」之间会留下孤儿，没人会再认领它们
        remove_orphan_tmp_files(&dir).await;

        let workspace =
            load_json_or_default::<DebugWorkspace>(&dir, WORKSPACE_FILE, &mut notices).await;
        let collections =
            load_json_or_default::<DebugCollections>(&dir, COLLECTIONS_FILE, &mut notices).await;
        let (history, history_lines) = load_history(&dir, budget, &mut notices).await;

        Self {
            dir,
            // 文件可能被手改过，读进来也按上限收一遍
            workspace: Mutex::new(normalize_workspace(workspace)),
            collections: Mutex::new(collections),
            history: Mutex::new(history),
            notices: StdMutex::new(notices),
            write_gate: Mutex::new(WriteState {
                history_lines,
                compact_retry_at: 0,
            }),
        }
    }

    /// 取走读盘时积下的提示，取完就空了
    pub fn take_storage_notices(&self) -> Vec<DebugStorageNotice> {
        // 持锁期间只做 mem::take，不会 panic；就算别处 panic 把锁毒了，里面的数据依然可用
        let mut guard = self.notices.lock().unwrap_or_else(PoisonError::into_inner);
        std::mem::take(&mut *guard)
    }

    fn path(&self, file: &str) -> PathBuf {
        self.dir.join(file)
    }

    async fn ensure_dir(&self) -> Result<(), String> {
        tokio::fs::create_dir_all(&self.dir)
            .await
            .map_err(|e| format!("创建调试台数据目录失败 ({}): {e}", self.dir.display()))
    }

    /// 写一份整文件。参数里的 `WriteState` 只是「已经拿到写入闸」的凭证
    async fn write_file(&self, _gate: &WriteState, file: &str, bytes: &[u8]) -> Result<(), String> {
        self.ensure_dir().await?;
        write_atomic(&self.path(file), bytes).await
    }

    // -----------------------------------------------------------------------
    // 工作区
    // -----------------------------------------------------------------------

    pub async fn workspace(&self) -> DebugWorkspace {
        self.workspace.lock().await.clone()
    }

    /// 先落盘再改内存：写失败时内存仍是上一份已落盘的状态，界面重读拿到的和文件一致
    pub async fn save_workspace(&self, ws: DebugWorkspace) -> Result<(), String> {
        let ws = normalize_workspace(ws);
        let bytes =
            serde_json::to_vec_pretty(&ws).map_err(|e| format!("序列化调试台工作区失败: {e}"))?;
        let gate = self.write_gate.lock().await;
        self.write_file(&gate, WORKSPACE_FILE, &bytes).await?;
        *self.workspace.lock().await = ws;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // 收藏夹
    // -----------------------------------------------------------------------

    pub async fn collections(&self) -> DebugCollections {
        self.collections.lock().await.clone()
    }

    pub async fn save_collections(&self, c: DebugCollections) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(&c).map_err(|e| format!("序列化收藏夹失败: {e}"))?;
        let gate = self.write_gate.lock().await;
        self.write_file(&gate, COLLECTIONS_FILE, &bytes).await?;
        *self.collections.lock().await = c;
        Ok(())
    }

    /// 把当前收藏夹导出成 JSON 文件。目标是用户选的路径，同样先写临时文件再改名，
    /// 覆盖已有文件时不会留下半截
    pub async fn export_collections(&self, path: &Path) -> Result<(), String> {
        let snapshot = self.collections().await;
        let bytes =
            serde_json::to_vec_pretty(&snapshot).map_err(|e| format!("序列化收藏夹失败: {e}"))?;
        write_atomic(path, &bytes).await
    }

    /// 从导出文件合并进当前收藏夹：不覆盖已有内容，id 撞了的换新 id，文件夹名照原样保留
    /// （同名文件夹不合并、不改名）。文件读不出来时只报错，不动当前收藏夹，也不挪用户的文件
    pub async fn import_collections(&self, path: &Path) -> Result<DebugCollections, String> {
        let meta = tokio::fs::metadata(path)
            .await
            .map_err(|e| format!("读取导入文件失败: {e}"))?;
        if meta.len() > IMPORT_MAX_BYTES {
            return Err(format!(
                "导入文件太大（{} MB），收藏夹文件不会超过 16 MB",
                meta.len() / (1024 * 1024)
            ));
        }
        let bytes = tokio::fs::read(path)
            .await
            .map_err(|e| format!("读取导入文件失败: {e}"))?;
        let bytes = strip_bom(&bytes);

        // `DebugCollections` 的字段都有默认值，任何 JSON 对象（比如选成了 workspace.json）
        // 都能解析成一份空收藏夹，导入会「成功」却什么都没导。先确认长得像收藏夹
        let raw: Value =
            serde_json::from_slice(bytes).map_err(|e| format!("导入文件不是有效的 JSON: {e}"))?;
        let looks_like_collections = raw
            .as_object()
            .is_some_and(|o| o.contains_key("folders") || o.contains_key("requests"));
        if !looks_like_collections {
            return Err("导入文件不是收藏夹导出文件（缺少 folders / requests）".to_owned());
        }
        let imported: DebugCollections =
            serde_json::from_value(raw).map_err(|e| format!("导入文件的收藏夹结构不对: {e}"))?;

        // 读当前、合并、落盘要在同一把闸里，否则并发的保存会被这次合并覆盖
        let gate = self.write_gate.lock().await;
        let current = self.collections.lock().await.clone();
        let merged = merge_collections(current, imported);
        let bytes =
            serde_json::to_vec_pretty(&merged).map_err(|e| format!("序列化收藏夹失败: {e}"))?;
        self.write_file(&gate, COLLECTIONS_FILE, &bytes).await?;
        *self.collections.lock().await = merged.clone();
        Ok(merged)
    }

    // -----------------------------------------------------------------------
    // 历史
    // -----------------------------------------------------------------------

    /// 追加一条历史：文件里追加一行，内存里只留最新 1000 条（回包全文另有内存预算），
    /// 文件行数超过 1200 时压缩重写。
    ///
    /// 历史是尽力而为的记录：写盘失败照样进内存（本次运行里还能查到），只是把错误交给调用方记日志。
    /// 追加成功后压缩失败不算追加失败：这条已经记上了，压缩错误只记日志，过一阵再试
    pub async fn append_history(&self, mut entry: DebugHistoryEntry) -> Result<(), String> {
        let response_bytes = prepare_entry(&mut entry);
        let mut line =
            serde_json::to_vec(&entry).map_err(|e| format!("序列化调用历史失败: {e}"))?;
        line.push(b'\n');

        let mut gate = self.write_gate.lock().await;
        let appended = self.append_line(&gate, &line).await;
        self.history
            .lock()
            .await
            .push(entry, response_bytes, appended.is_ok());
        appended?;

        gate.history_lines += 1;
        if gate.history_lines > HISTORY_COMPACT_AT && gate.history_lines > gate.compact_retry_at {
            if let Err(e) = self.compact_history(&mut gate).await {
                tracing::warn!("压缩调试台调用历史失败，稍后重试: {e}");
                gate.compact_retry_at = gate.history_lines + COMPACT_BACKOFF;
            }
        }
        Ok(())
    }

    async fn append_line(&self, _gate: &WriteState, line: &[u8]) -> Result<(), String> {
        self.ensure_dir().await?;
        let path = self.path(HISTORY_FILE);
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .await
            .map_err(|e| format!("打开调用历史文件失败 ({}): {e}", path.display()))?;
        file.write_all(line)
            .await
            .map_err(|e| format!("追加调用历史失败: {e}"))?;
        file.flush()
            .await
            .map_err(|e| format!("追加调用历史失败: {e}"))
    }

    /// 把文件里最新的 1000 行按行复制进新文件再换上。不从内存重新序列化：
    /// 内存里有些条目的回包已经丢了，文件里才是全文
    async fn compact_history(&self, gate: &mut WriteState) -> Result<(), String> {
        let path = self.path(HISTORY_FILE);
        let total = count_lines(&path).await?;
        let skip = total.saturating_sub(HISTORY_KEEP);
        let kept = rewrite_lines(&path, &path, |ordinal| ordinal >= skip).await?;
        gate.history_lines = kept;
        gate.compact_retry_at = 0;
        Ok(())
    }

    /// 按条件筛，最新的在前。`limit` 收在 1..=200，`total` 是筛完的总数（分页用）
    pub async fn history(&self, q: DebugHistoryQuery) -> DebugHistoryPage {
        let limit = q.limit.clamp(1, HISTORY_PAGE_MAX) as usize;
        let offset = q.offset as usize;
        let needle = q
            .text
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(str::to_lowercase);

        let history = self.history.lock().await;
        let mut total = 0usize;
        let mut entries = Vec::new();
        for item in history
            .entries
            .iter()
            .rev()
            .filter(|m| matches_query(&m.entry, &q, needle.as_deref()))
        {
            if total >= offset && entries.len() < limit {
                entries.push(summarize(&item.entry));
            }
            total += 1;
        }
        DebugHistoryPage {
            entries,
            total: u32::try_from(total).unwrap_or(u32::MAX),
        }
    }

    /// 取一条完整记录。回包已被内存预算挤掉的，回文件里读全文
    pub async fn history_entry(&self, id: &str) -> Option<DebugHistoryEntry> {
        let (mut entry, on_disk) = {
            let history = self.history.lock().await;
            let item = history.entries.iter().rev().find(|m| m.entry.id == id)?;
            (item.entry.clone(), item.response_on_disk)
        };
        if !on_disk {
            return Some(entry);
        }

        // 和追加、压缩互斥，免得读到写了一半的行，或者读到一半文件被换掉
        let _gate = self.write_gate.lock().await;
        if let Some(full) = read_history_entry(&self.path(HISTORY_FILE), id).await {
            return Some(full);
        }
        // 文件被删了或这一行读不出来：回包确实拿不到了。标成「被截断」，
        // 界面才不会把 `None` 当成「这次调用没有回包」
        tracing::warn!("历史记录 {id} 的回包全文在 history.jsonl 里读不到");
        entry.response_truncated = true;
        Some(entry)
    }

    /// 清空历史。文件没删掉时不清内存，否则界面上清空了、下次启动又冒出来
    pub async fn clear_history(&self) -> Result<(), String> {
        let mut gate = self.write_gate.lock().await;
        let path = self.path(HISTORY_FILE);
        match tokio::fs::remove_file(&path).await {
            Ok(()) => {}
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            Err(e) => return Err(format!("清空调用历史失败 ({}): {e}", path.display())),
        }
        self.history.lock().await.clear();
        gate.history_lines = 0;
        gate.compact_retry_at = 0;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 一次调用 -> 一条历史
// ---------------------------------------------------------------------------

/// 把一次调用的结果整理成历史记录。只有测试台编辑器和消息输入框发起的调用进历史，
/// 命令面板挑参数之类的（`Picker` / `Other`）返回 `None`。
///
/// 参数里没有令牌（通道令牌走 header / 查询串，不在 params 里），这里也不会去碰通道的令牌。
/// 失败没有单独的耗时，`Timeout` 用它的时限，其它记 0
pub(crate) fn history_entry_from_call(
    bot_id: &str,
    bot_name: &str,
    backend: BackendType,
    req: &DebugCallRequest,
    channel: &DebugChannelId,
    result: &DebugCallResult,
) -> Option<DebugHistoryEntry> {
    if !matches!(
        req.origin,
        DebugCallOrigin::Editor | DebugCallOrigin::Composer | DebugCallOrigin::Mcp
    ) {
        return None;
    }
    let (ok, retcode, error, elapsed_ms, response, response_truncated) = match result {
        DebugCallResult::Ok { outcome } => (
            outcome.ok,
            Some(outcome.retcode),
            None,
            outcome.elapsed_ms,
            Some(outcome.raw.clone()),
            outcome.truncated,
        ),
        DebugCallResult::Err { error } => {
            let elapsed = match error {
                DebugError::Timeout { ms } => *ms,
                _ => 0,
            };
            (false, None, Some(error.clone()), elapsed, None, false)
        }
    };
    Some(DebugHistoryEntry {
        id: Uuid::new_v4().to_string(),
        at_ms: now_ms(),
        bot_id: bot_id.to_owned(),
        bot_name: bot_name.to_owned(),
        backend,
        channel: channel.clone(),
        origin: req.origin.clone(),
        action: req.action.clone(),
        params: req.params.clone(),
        params_truncated: false,
        ok,
        retcode,
        error,
        elapsed_ms,
        response,
        response_truncated,
    })
}

// ---------------------------------------------------------------------------
// 纯逻辑：入库整理、上限、筛选、合并
// ---------------------------------------------------------------------------

fn is_credential_action(action: &str) -> bool {
    let base = action.strip_suffix("_async").unwrap_or(action);
    CREDENTIAL_ACTIONS.contains(&base)
}

/// 入库前的整理，返回 `response` 序列化后占的字节数（没有回包为 0）：
/// - 参数瘦身（过长的字符串换成占位文字，整体过大的再收一遍，与事件流里的调用记录同一套），
///   动过就在 `params_truncated` 上标出来；
/// - 凭据类动作不留回包；
/// - 回包序列化后超过 256 KiB 就整个不存。调用层已经截过（`response_truncated` 为真）的保持为真
fn prepare_entry(entry: &mut DebugHistoryEntry) -> usize {
    entry.params_truncated = cap_record_params(&mut entry.params);

    if is_credential_action(&entry.action) {
        // 不是「回包太大被截断」，所以标记也不置位：这条本来就不该有回包
        entry.response = None;
        entry.response_truncated = false;
        return 0;
    }
    let Some(response) = entry.response.as_ref() else {
        return 0;
    };
    match serialized_len(response) {
        Some(len) if len <= HISTORY_RESPONSE_LIMIT => len,
        _ => {
            entry.response = None;
            entry.response_truncated = true;
            0
        }
    }
}

/// 把工作区收进上限内。约定（与前端 store 一致）：
/// `tabs` 老的在前；`closed_tabs`、`recent_actions` 新的在前，所以后两个截尾、`tabs` 从头丢
fn normalize_workspace(mut ws: DebugWorkspace) -> DebugWorkspace {
    ws.closed_tabs.truncate(MAX_CLOSED_TABS);
    ws.recent_actions.truncate(MAX_RECENT_ACTIONS);

    if ws.tabs.len() > MAX_TABS {
        let mut excess = ws.tabs.len() - MAX_TABS;
        let active = ws.active_tab.clone();
        ws.tabs.retain(|tab| {
            if excess > 0 && active.as_deref() != Some(tab.id.as_str()) {
                excess -= 1;
                false
            } else {
                true
            }
        });
        // 只有多个标签共用了激活 id 这种畸形数据才会还超，硬截掉兜底
        ws.tabs.truncate(MAX_TABS);
    }
    ws
}

fn matches_query(entry: &DebugHistoryEntry, q: &DebugHistoryQuery, needle: Option<&str>) -> bool {
    // 前端清掉筛选框时可能传空串，等同于没有这项条件
    if let Some(action) = q.action.as_deref().filter(|a| !a.is_empty()) {
        if entry.action != action {
            return false;
        }
    }
    if let Some(bot_id) = q.bot_id.as_deref().filter(|b| !b.is_empty()) {
        if entry.bot_id != bot_id {
            return false;
        }
    }
    if q.ok.is_some_and(|ok| ok != entry.ok) {
        return false;
    }
    match needle {
        None => true,
        // needle 已经转成小写；参数按紧凑 JSON 文本搜，键名和值都能命中
        Some(n) => {
            entry.action.to_lowercase().contains(n)
                || entry.bot_name.to_lowercase().contains(n)
                || entry.params.to_string().to_lowercase().contains(n)
        }
    }
}

fn summarize(entry: &DebugHistoryEntry) -> DebugHistorySummary {
    DebugHistorySummary {
        id: entry.id.clone(),
        at_ms: entry.at_ms,
        bot_id: entry.bot_id.clone(),
        bot_name: entry.bot_name.clone(),
        backend: entry.backend,
        channel: entry.channel.clone(),
        origin: entry.origin.clone(),
        action: entry.action.clone(),
        ok: entry.ok,
        retcode: entry.retcode,
        elapsed_ms: entry.elapsed_ms,
        error_kind: entry.error.as_ref().map(|e| e.kind_str().to_owned()),
    }
}

/// 把导入的收藏夹并进现有的：导入的排在现有的后面，id 撞了就换新 id，
/// 请求原来挂的文件夹跟着换后的 id 走；挂在导入文件里根本没有的文件夹上的，落到根
fn merge_collections(mut base: DebugCollections, imported: DebugCollections) -> DebugCollections {
    // 文件夹和请求共用一个集合查重，比只查各自的类别更保守，代价可以忽略
    let mut used: HashSet<String> = base
        .folders
        .iter()
        .map(|f| f.id.clone())
        .chain(base.requests.iter().map(|r| r.id.clone()))
        .collect();
    let mut fresh_id = |old: &str| -> String {
        if used.insert(old.to_owned()) {
            old.to_owned()
        } else {
            loop {
                let id = Uuid::new_v4().to_string();
                if used.insert(id.clone()) {
                    break id;
                }
            }
        }
    };

    // 导入项的 order 整体平移到现有最大值之后，彼此的先后不变
    let folder_base = base
        .folders
        .iter()
        .map(|f| f.order)
        .max()
        .map_or(0, |m| m.saturating_add(1));
    let folder_min = imported.folders.iter().map(|f| f.order).min().unwrap_or(0);
    let request_base = base
        .requests
        .iter()
        .map(|r| r.order)
        .max()
        .map_or(0, |m| m.saturating_add(1));
    let request_min = imported.requests.iter().map(|r| r.order).min().unwrap_or(0);

    let mut folder_ids: HashMap<String, String> = HashMap::new();
    for mut folder in imported.folders {
        let old = folder.id.clone();
        folder.id = fresh_id(&old);
        folder.order = folder_base.saturating_add(folder.order.saturating_sub(folder_min));
        // 导入文件里同一个 id 出现两次时，请求归第一个
        folder_ids.entry(old).or_insert_with(|| folder.id.clone());
        base.folders.push(folder);
    }
    for mut request in imported.requests {
        request.id = fresh_id(&request.id);
        request.order = request_base.saturating_add(request.order.saturating_sub(request_min));
        request.folder_id = request
            .folder_id
            .and_then(|old| folder_ids.get(&old).cloned());
        base.requests.push(request);
    }
    base
}

// ---------------------------------------------------------------------------
// 读盘
// ---------------------------------------------------------------------------

enum Loaded<T> {
    Missing,
    Parsed(T),
    /// 文件在，但读不出来或解析失败；里面是原因
    Broken(String),
}

fn strip_bom(bytes: &[u8]) -> &[u8] {
    // 记事本另存为 UTF-8 会带 BOM，手改过文件的用户不该因此丢数据
    bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes)
}

/// 去掉行尾的 `\n` / `\r\n`（和行首可能有的 BOM）
fn trim_line(buf: &[u8]) -> &[u8] {
    let line = buf.strip_suffix(b"\n").unwrap_or(buf);
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    strip_bom(line)
}

fn is_blank(line: &[u8]) -> bool {
    line.iter().all(u8::is_ascii_whitespace)
}

async fn read_json<T: DeserializeOwned>(path: &Path) -> Loaded<T> {
    let bytes = match tokio::fs::read(path).await {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == ErrorKind::NotFound => return Loaded::Missing,
        Err(e) => return Loaded::Broken(format!("读取失败: {e}")),
    };
    match serde_json::from_slice(strip_bom(&bytes)) {
        Ok(value) => Loaded::Parsed(value),
        Err(e) => Loaded::Broken(e.to_string()),
    }
}

async fn load_json_or_default<T: DeserializeOwned + Default>(
    dir: &Path,
    file: &str,
    notices: &mut Vec<DebugStorageNotice>,
) -> T {
    let path = dir.join(file);
    match read_json(&path).await {
        Loaded::Missing => T::default(),
        Loaded::Parsed(value) => value,
        Loaded::Broken(reason) => {
            let moved = quarantine(&path).await;
            notices.push(notice(file, moved, reason));
            T::default()
        }
    }
}

/// 清掉数据目录里崩溃留下的 `<三份文件之一>.tmp-*`。尽力而为，删不掉的下次再说
async fn remove_orphan_tmp_files(dir: &Path) {
    let Ok(mut entries) = tokio::fs::read_dir(dir).await else {
        return;
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let name = entry.file_name().to_string_lossy().into_owned();
        let is_ours = [WORKSPACE_FILE, COLLECTIONS_FILE, HISTORY_FILE]
            .iter()
            .any(|file| name.starts_with(&format!("{file}{TMP_MARKER}")));
        if !is_ours {
            continue;
        }
        if let Err(e) = tokio::fs::remove_file(entry.path()).await {
            tracing::debug!("清理调试台遗留的临时文件失败 ({name}): {e}");
        }
    }
}

/// 读 `history.jsonl`：一行一条，坏行跳过不连累好行。流式读，内存里只留最新 1000 条，
/// 回包全文按预算留，不会因为文件大就把所有回包都读进来。
/// 返回（内存里的历史，文件现有行数）
async fn load_history(
    dir: &Path,
    budget: usize,
    notices: &mut Vec<DebugStorageNotice>,
) -> (HistoryMem, usize) {
    let path = dir.join(HISTORY_FILE);
    let file = match tokio::fs::File::open(&path).await {
        Ok(file) => file,
        Err(e) if e.kind() == ErrorKind::NotFound => return (HistoryMem::new(budget), 0),
        Err(e) => return unreadable_history(&path, budget, notices, &e).await,
    };
    let scan = match scan_history(file, budget).await {
        Ok(scan) => scan,
        Err(e) => return unreadable_history(&path, budget, notices, &e).await,
    };

    let mut lines = scan.lines;
    if scan.bad > 0 {
        // 原件整份挪开留底，好行（最新 1000 条）按行复制回正本；这样坏行不会每次启动都再报一遍，
        // 磁盘上的行数也和内存对得上
        let reason = format!(
            "{} 行历史记录无法解析，已跳过（第 {} 行：{}），其余 {} 条照常保留",
            scan.bad, scan.first_bad_line, scan.first_error, scan.good_count
        );
        match quarantine(&path).await {
            Ok(moved) => {
                let keep: HashSet<usize> = scan.good_tail.iter().copied().collect();
                lines = match rewrite_lines(&moved, &path, |ordinal| keep.contains(&ordinal)).await
                {
                    Ok(kept) => kept,
                    Err(e) => {
                        // 正本已经挪走了，内存里还有；下次追加会从空文件重新开始
                        tracing::warn!("重写调用历史失败: {e}");
                        0
                    }
                };
                notices.push(notice(HISTORY_FILE, Ok(moved), reason));
            }
            Err(e) => notices.push(notice(HISTORY_FILE, Err(e), reason)),
        }
    } else if scan.lines > HISTORY_COMPACT_AT || !scan.ends_with_newline {
        // 太长要压缩；结尾缺换行（崩在追加中途，但那行恰好写完整了）也要重写一遍，
        // 否则下一条追加会和它粘成一行，两条都读不出来
        let skip = scan.lines.saturating_sub(HISTORY_KEEP);
        match rewrite_lines(&path, &path, |ordinal| ordinal >= skip).await {
            Ok(kept) => lines = kept,
            Err(e) => tracing::warn!("重写调用历史失败: {e}"),
        }
    }
    (scan.mem, lines)
}

async fn unreadable_history(
    path: &Path,
    budget: usize,
    notices: &mut Vec<DebugStorageNotice>,
    error: &std::io::Error,
) -> (HistoryMem, usize) {
    let moved = quarantine(path).await;
    notices.push(notice(HISTORY_FILE, moved, format!("读取失败: {error}")));
    (HistoryMem::new(budget), 0)
}

struct HistoryScan {
    mem: HistoryMem,
    /// 非空行总数（含读不出来的）
    lines: usize,
    bad: usize,
    /// 第一处坏行的物理行号（从 1 数）和原因，给提示用
    first_bad_line: usize,
    first_error: String,
    /// 最新至多 1000 条有效行的「非空行序号」，坏行重写时按它挑行
    good_tail: VecDeque<usize>,
    good_count: usize,
    ends_with_newline: bool,
}

async fn scan_history(file: tokio::fs::File, budget: usize) -> std::io::Result<HistoryScan> {
    let mut reader = BufReader::new(file);
    let mut buf = Vec::new();
    let mut scan = HistoryScan {
        mem: HistoryMem::new(budget),
        lines: 0,
        bad: 0,
        first_bad_line: 0,
        first_error: String::new(),
        good_tail: VecDeque::new(),
        good_count: 0,
        ends_with_newline: true,
    };
    let mut physical = 0usize;
    loop {
        buf.clear();
        if reader.read_until(b'\n', &mut buf).await? == 0 {
            break;
        }
        physical += 1;
        scan.ends_with_newline = buf.ends_with(b"\n");
        let line = trim_line(&buf);
        if is_blank(line) {
            continue;
        }
        let ordinal = scan.lines;
        scan.lines += 1;
        match serde_json::from_slice::<DebugHistoryEntry>(line) {
            Ok(entry) => {
                let bytes = entry
                    .response
                    .as_ref()
                    .and_then(serialized_len)
                    .unwrap_or(0);
                // 每读一条就过一遍预算，峰值内存是预算加一条，而不是整个文件的回包
                scan.mem.push(entry, bytes, true);
                scan.good_count += 1;
                scan.good_tail.push_back(ordinal);
                if scan.good_tail.len() > HISTORY_KEEP {
                    scan.good_tail.pop_front();
                }
            }
            Err(e) => {
                if scan.bad == 0 {
                    scan.first_bad_line = physical;
                    scan.first_error = e.to_string();
                }
                scan.bad += 1;
            }
        }
    }
    Ok(scan)
}

/// 在文件里找 `id` 这一条，读出完整记录。本模块写的每一行都以 `{"id":"<id>",` 开头
/// （`id` 是结构体第一个字段），先比前缀，命中了才解析，不必把每行的回包都解析一遍
async fn read_history_entry(path: &Path, id: &str) -> Option<DebugHistoryEntry> {
    let prefix = format!("{{\"id\":{},", serde_json::to_string(id).ok()?);
    let file = tokio::fs::File::open(path).await.ok()?;
    let mut reader = BufReader::new(file);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        if reader.read_until(b'\n', &mut buf).await.ok()? == 0 {
            return None;
        }
        let line = trim_line(&buf);
        if line.starts_with(prefix.as_bytes()) {
            return serde_json::from_slice(line).ok();
        }
    }
}

// ---------------------------------------------------------------------------
// 按行复制
// ---------------------------------------------------------------------------

async fn count_lines(path: &Path) -> Result<usize, String> {
    let file = tokio::fs::File::open(path)
        .await
        .map_err(|e| format!("打开 {} 失败: {e}", path.display()))?;
    let mut reader = BufReader::new(file);
    let mut buf = Vec::new();
    let mut count = 0usize;
    loop {
        buf.clear();
        let read = reader
            .read_until(b'\n', &mut buf)
            .await
            .map_err(|e| format!("读取 {} 失败: {e}", path.display()))?;
        if read == 0 {
            return Ok(count);
        }
        if !is_blank(trim_line(&buf)) {
            count += 1;
        }
    }
}

/// 把 `src` 里「非空行序号」满足 `keep` 的行按原样复制进临时文件，再原子换成 `dst`
/// （`src` 和 `dst` 可以是同一个文件）。返回复制了几行。
/// 逐行读写，内存里只有一行，被内存预算丢掉的回包不会因此丢失
async fn rewrite_lines(
    src: &Path,
    dst: &Path,
    keep: impl Fn(usize) -> bool,
) -> Result<usize, String> {
    let input = tokio::fs::File::open(src)
        .await
        .map_err(|e| format!("打开 {} 失败: {e}", src.display()))?;
    let (tmp, file) = create_tmp(dst).await?;
    let mut writer = BufWriter::new(file);
    match copy_lines(BufReader::new(input), &mut writer, &keep).await {
        Ok(copied) => {
            commit_tmp(tmp, writer.into_inner(), dst).await?;
            Ok(copied)
        }
        Err(e) => {
            discard_tmp(&tmp, writer.into_inner()).await;
            Err(format!("复制 {} 的内容失败: {e}", src.display()))
        }
    }
}

async fn copy_lines<R, W>(
    mut reader: R,
    writer: &mut W,
    keep: &impl Fn(usize) -> bool,
) -> std::io::Result<usize>
where
    R: AsyncBufRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut buf = Vec::new();
    let mut ordinal = 0usize;
    let mut copied = 0usize;
    loop {
        buf.clear();
        if reader.read_until(b'\n', &mut buf).await? == 0 {
            break;
        }
        let line = trim_line(&buf);
        if is_blank(line) {
            continue;
        }
        if keep(ordinal) {
            writer.write_all(line).await?;
            writer.write_all(b"\n").await?;
            copied += 1;
        }
        ordinal += 1;
    }
    writer.flush().await?;
    Ok(copied)
}

// ---------------------------------------------------------------------------
// 挪开坏文件
// ---------------------------------------------------------------------------

/// `moved` 为 `Err` 时是没挪成的原因
fn notice(file: &str, moved: Result<PathBuf, String>, reason: String) -> DebugStorageNotice {
    match moved {
        Ok(target) => DebugStorageNotice {
            file: file.to_owned(),
            moved_to: target.display().to_string(),
            reason,
        },
        // 没挪成（被别的进程占着之类）：moved_to 留空，原因里说清楚为什么、原件的下场
        Err(why) => DebugStorageNotice {
            file: file.to_owned(),
            moved_to: String::new(),
            reason: format!("{reason}（没能把原文件挪开: {why}；下次保存会覆盖它）"),
        },
    }
}

/// 把读不出来的文件改名成 `<名>.broken-<Unix 秒>` 留底。同一秒内已有同名时追加序号；
/// 挪不动返回原因
async fn quarantine(path: &Path) -> Result<PathBuf, String> {
    let Some(name) = path.file_name().map(|n| n.to_string_lossy().into_owned()) else {
        return Err(format!("不是有效的文件路径: {}", path.display()));
    };
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    for attempt in 0..100u32 {
        let suffix = if attempt == 0 {
            stamp.to_string()
        } else {
            format!("{stamp}-{attempt}")
        };
        let target = path.with_file_name(format!("{name}.broken-{suffix}"));
        // rename 在 Unix 和 Windows 上都会直接覆盖已有的同名文件，而 `.broken-*` 是留给用户的
        // 证据，不能盖掉前一份，所以先避开已经存在的名字
        if tokio::fs::try_exists(&target).await.unwrap_or(true) {
            continue;
        }
        return match rename_with_retry(path, &target).await {
            Ok(()) => Ok(target),
            Err(e) => Err(format!("改名到 {} 失败: {e}", target.display())),
        };
    }
    Err("同一秒内的 .broken 留底重名太多".to_owned())
}

// ---------------------------------------------------------------------------
// 写盘
// ---------------------------------------------------------------------------

/// 临时文件写在目标同目录（同卷才能原子 rename）
async fn create_tmp(target: &Path) -> Result<(PathBuf, tokio::fs::File), String> {
    let Some(name) = target.file_name() else {
        return Err(format!("不是有效的文件路径: {}", target.display()));
    };
    let mut tmp_name = name.to_os_string();
    tmp_name.push(format!("{TMP_MARKER}{}", Uuid::new_v4()));
    let tmp = target.with_file_name(tmp_name);
    let file = tokio::fs::File::create(&tmp)
        .await
        .map_err(|e| format!("创建临时文件失败 ({}): {e}", tmp.display()))?;
    Ok((tmp, file))
}

/// 刷盘并 fsync 临时文件，再改名覆盖目标。任何一步失败都清掉临时文件；
/// fsync 是为了断电后不会出现「改名已生效、内容还没落盘」的空文件
async fn commit_tmp(tmp: PathBuf, mut file: tokio::fs::File, target: &Path) -> Result<(), String> {
    let synced = match file.flush().await {
        Ok(()) => file.sync_all().await,
        Err(e) => Err(e),
    };
    // Windows 上没关句柄就删 / 改名会失败
    drop(file);
    if let Err(e) = synced {
        let _ = tokio::fs::remove_file(&tmp).await;
        return Err(format!("写入临时文件失败 ({}): {e}", tmp.display()));
    }
    if let Err(e) = rename_with_retry(&tmp, target).await {
        let _ = tokio::fs::remove_file(&tmp).await;
        return Err(format!("替换 {} 失败: {e}", target.display()));
    }
    Ok(())
}

async fn discard_tmp(tmp: &Path, file: tokio::fs::File) {
    drop(file);
    let _ = tokio::fs::remove_file(tmp).await;
}

/// rename 失败时短暂重试几次：Windows 上杀软 / 索引服务会在文件刚写完时占着它，
/// 过几十毫秒就放开了。目标不存在这类不会自己好的错误不重试
async fn rename_with_retry(from: &Path, to: &Path) -> std::io::Result<()> {
    let mut retries = 0;
    loop {
        match tokio::fs::rename(from, to).await {
            Ok(()) => return Ok(()),
            Err(e) if retries < RENAME_RETRIES && e.kind() != ErrorKind::NotFound => {
                retries += 1;
                tokio::time::sleep(RENAME_RETRY_DELAY).await;
            }
            Err(e) => return Err(e),
        }
    }
}

async fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let (tmp, mut file) = create_tmp(path).await?;
    if let Err(e) = file.write_all(bytes).await {
        discard_tmp(&tmp, file).await;
        return Err(format!("写入临时文件失败 ({}): {e}", tmp.display()));
    }
    commit_tmp(tmp, file, path).await
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use ncd_domain::onebot_debug::{
        DebugCallOutcome, DebugChannelChoice, DebugRequestDraft, DebugSavedFolder,
        DebugSavedRequest,
    };
    use serde_json::json;

    use super::super::params::PARAM_STRING_LIMIT;
    use super::*;

    fn draft(id: &str) -> DebugRequestDraft {
        DebugRequestDraft {
            id: id.to_owned(),
            action: "send_msg".to_owned(),
            params_text: "{}".to_owned(),
            timeout_ms: None,
            channel: None,
        }
    }

    fn entry(id: &str, action: &str, bot_id: &str, ok: bool) -> DebugHistoryEntry {
        DebugHistoryEntry {
            id: id.to_owned(),
            at_ms: 1_700_000_000_000,
            bot_id: bot_id.to_owned(),
            bot_name: format!("bot-{bot_id}"),
            backend: BackendType::NapCat,
            channel: DebugChannelId::Internal,
            origin: DebugCallOrigin::Editor,
            action: action.to_owned(),
            params: json!({"group_id": 123}),
            params_truncated: false,
            ok,
            retcode: Some(if ok { 0 } else { 1400 }),
            error: None,
            elapsed_ms: 12,
            response: Some(json!({"status": "ok"})),
            response_truncated: false,
        }
    }

    fn query() -> DebugHistoryQuery {
        DebugHistoryQuery {
            action: None,
            bot_id: None,
            ok: None,
            text: None,
            limit: 200,
            offset: 0,
        }
    }

    fn folder(id: &str, name: &str, order: i32) -> DebugSavedFolder {
        DebugSavedFolder {
            id: id.to_owned(),
            name: name.to_owned(),
            order,
        }
    }

    fn saved(id: &str, folder_id: Option<&str>, order: i32) -> DebugSavedRequest {
        DebugSavedRequest {
            id: id.to_owned(),
            name: format!("req-{id}"),
            folder_id: folder_id.map(str::to_owned),
            action: "get_login_info".to_owned(),
            params: json!({}),
            channel: None,
            note: None,
            order,
            created_at_ms: 1,
            updated_at_ms: 2,
        }
    }

    fn file_names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .map(|rd| {
                rd.filter_map(Result::ok)
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }

    fn debug_dir(root: &Path) -> PathBuf {
        root.join(DIR_NAME)
    }

    fn outcome(ok: bool, raw: Value, truncated: bool) -> DebugCallOutcome {
        DebugCallOutcome {
            ok,
            status: if ok { "ok" } else { "failed" }.to_owned(),
            retcode: if ok { 0 } else { 1400 },
            data: Value::Null,
            message: String::new(),
            wording: String::new(),
            raw,
            elapsed_ms: 34,
            channel: DebugChannelId::Internal,
            size_bytes: 10,
            truncated,
        }
    }

    fn request(origin: DebugCallOrigin) -> DebugCallRequest {
        DebugCallRequest {
            request_id: "r1".to_owned(),
            bot_id: "1".to_owned(),
            channel: DebugChannelId::Auto,
            action: "send_msg".to_owned(),
            params: json!({"message": "hi"}),
            timeout_ms: None,
            origin,
        }
    }

    // --- 往返 ---------------------------------------------------------------

    #[tokio::test]
    async fn workspace_round_trips_across_reload() {
        let root = tempfile::tempdir().unwrap();
        let store = DebugStore::load(root.path()).await;
        assert_eq!(store.workspace().await, DebugWorkspace::default());

        let mut ws = DebugWorkspace {
            tabs: vec![draft("a"), draft("b")],
            active_tab: Some("b".to_owned()),
            selected_bot: Some("42".to_owned()),
            recent_actions: vec!["send_msg".to_owned()],
            ..DebugWorkspace::default()
        };
        ws.channel_choice.insert(
            "42".to_owned(),
            DebugChannelChoice {
                call: DebugChannelId::Http { name: "h".into() },
                events: DebugChannelId::Auto,
            },
        );
        ws.layout.left_width = 300;
        store.save_workspace(ws.clone()).await.unwrap();
        assert_eq!(store.workspace().await, ws);

        let reloaded = DebugStore::load(root.path()).await;
        assert_eq!(reloaded.workspace().await, ws);
        assert!(reloaded.take_storage_notices().is_empty());
    }

    #[tokio::test]
    async fn collections_round_trip_across_reload() {
        let root = tempfile::tempdir().unwrap();
        let store = DebugStore::load(root.path()).await;
        let collections = DebugCollections {
            version: 1,
            folders: vec![folder("f1", "常用", 0)],
            requests: vec![saved("r1", Some("f1"), 0)],
        };
        store.save_collections(collections.clone()).await.unwrap();

        let reloaded = DebugStore::load(root.path()).await;
        assert_eq!(reloaded.collections().await, collections);
        assert!(reloaded.take_storage_notices().is_empty());
    }

    #[tokio::test]
    async fn history_round_trips_across_reload() {
        let root = tempfile::tempdir().unwrap();
        let store = DebugStore::load(root.path()).await;
        for i in 0..3 {
            store
                .append_history(entry(&format!("e{i}"), "send_msg", "1", true))
                .await
                .unwrap();
        }
        let reloaded = DebugStore::load(root.path()).await;
        let page = reloaded.history(query()).await;
        let ids: Vec<&str> = page.entries.iter().map(|e| e.id.as_str()).collect();
        // 最新的在前
        assert_eq!(ids, ["e2", "e1", "e0"]);
        assert_eq!(page.total, 3);
        assert_eq!(
            reloaded.history_entry("e1").await,
            Some(entry("e1", "send_msg", "1", true))
        );
        assert!(reloaded.history_entry("nope").await.is_none());
        assert!(reloaded.take_storage_notices().is_empty());
    }

    #[tokio::test]
    async fn saves_leave_no_temp_files() {
        let root = tempfile::tempdir().unwrap();
        let store = DebugStore::load(root.path()).await;
        store
            .save_workspace(DebugWorkspace::default())
            .await
            .unwrap();
        store
            .save_collections(DebugCollections::default())
            .await
            .unwrap();
        store
            .append_history(entry("e", "a", "1", true))
            .await
            .unwrap();
        let names = file_names(&debug_dir(root.path()));
        assert_eq!(
            names,
            ["collections.json", "history.jsonl", "workspace.json"]
        );
    }

    #[tokio::test]
    async fn old_workspace_file_missing_fields_still_loads() {
        // 老版本写出的文件缺新字段：缺的取默认值，不该被当成损坏挪走
        let root = tempfile::tempdir().unwrap();
        let dir = debug_dir(root.path());
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(WORKSPACE_FILE),
            r#"{"tabs":[{"id":"a","action":"x","params_text":"{}","timeout_ms":null,"channel":null}]}"#,
        )
        .unwrap();
        std::fs::write(dir.join(COLLECTIONS_FILE), r#"{"folders":[]}"#).unwrap();

        let store = DebugStore::load(root.path()).await;
        let ws = store.workspace().await;
        assert_eq!(ws.tabs.len(), 1);
        assert_eq!(ws.layout, DebugWorkspace::default().layout);
        assert_eq!(store.collections().await, DebugCollections::default());
        assert!(store.take_storage_notices().is_empty());
        assert_eq!(file_names(&dir), ["collections.json", "workspace.json"]);
    }

    // --- 损坏文件 -------------------------------------------------------------

    #[tokio::test]
    async fn corrupt_workspace_is_moved_aside_with_notice() {
        let root = tempfile::tempdir().unwrap();
        let dir = debug_dir(root.path());
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(WORKSPACE_FILE), "{ not json").unwrap();

        let store = DebugStore::load(root.path()).await;
        assert_eq!(store.workspace().await, DebugWorkspace::default());

        let notices = store.take_storage_notices();
        assert_eq!(notices.len(), 1);
        assert_eq!(notices[0].file, WORKSPACE_FILE);
        assert!(!notices[0].reason.is_empty());
        let moved = PathBuf::from(&notices[0].moved_to);
        assert!(
            moved
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("workspace.json.broken-")
        );
        // 原件完整留在挪走的位置，正本没了（等下次保存重建）
        assert_eq!(std::fs::read_to_string(&moved).unwrap(), "{ not json");
        assert!(!dir.join(WORKSPACE_FILE).exists());
        // 提示只取一次
        assert!(store.take_storage_notices().is_empty());

        // 之后照常保存，不会碰那份留底
        store
            .save_workspace(DebugWorkspace::default())
            .await
            .unwrap();
        assert!(moved.exists());
        assert!(dir.join(WORKSPACE_FILE).exists());
    }

    #[tokio::test]
    async fn corrupt_collections_is_moved_aside_with_notice() {
        let root = tempfile::tempdir().unwrap();
        let dir = debug_dir(root.path());
        std::fs::create_dir_all(&dir).unwrap();
        // 类型不对（folders 应该是数组）也算坏
        std::fs::write(dir.join(COLLECTIONS_FILE), r#"{"folders": 7}"#).unwrap();

        let store = DebugStore::load(root.path()).await;
        assert_eq!(store.collections().await, DebugCollections::default());
        let notices = store.take_storage_notices();
        assert_eq!(notices.len(), 1);
        assert_eq!(notices[0].file, COLLECTIONS_FILE);
        assert!(PathBuf::from(&notices[0].moved_to).exists());
    }

    #[tokio::test]
    async fn bom_prefixed_json_is_accepted() {
        let root = tempfile::tempdir().unwrap();
        let dir = debug_dir(root.path());
        std::fs::create_dir_all(&dir).unwrap();
        let mut bytes = b"\xEF\xBB\xBF".to_vec();
        bytes.extend_from_slice(br#"{"selected_bot":"9"}"#);
        std::fs::write(dir.join(WORKSPACE_FILE), bytes).unwrap();

        let store = DebugStore::load(root.path()).await;
        assert_eq!(store.workspace().await.selected_bot.as_deref(), Some("9"));
        assert!(store.take_storage_notices().is_empty());
    }

    #[tokio::test]
    async fn bad_history_lines_are_skipped_and_original_kept() {
        let root = tempfile::tempdir().unwrap();
        let dir = debug_dir(root.path());
        std::fs::create_dir_all(&dir).unwrap();
        let good1 = serde_json::to_string(&entry("g1", "a", "1", true)).unwrap();
        let good2 = serde_json::to_string(&entry("g2", "b", "1", true)).unwrap();
        // 中间一行坏的，末尾还有一行写到一半的（模拟崩在追加中途）
        let content = format!("{good1}\nthis is not json\n{good2}\n{{\"id\":\"half");
        std::fs::write(dir.join(HISTORY_FILE), &content).unwrap();

        let store = DebugStore::load(root.path()).await;
        let page = store.history(query()).await;
        let ids: Vec<&str> = page.entries.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, ["g2", "g1"]);

        let notices = store.take_storage_notices();
        assert_eq!(notices.len(), 1);
        assert_eq!(notices[0].file, HISTORY_FILE);
        assert!(notices[0].reason.contains("2 行"), "{}", notices[0].reason);
        // 原件（含坏行）整份留底
        assert_eq!(
            std::fs::read_to_string(&notices[0].moved_to).unwrap(),
            content
        );
        // 正本只剩好行，下次启动不再报
        let rewritten = std::fs::read_to_string(dir.join(HISTORY_FILE)).unwrap();
        assert_eq!(rewritten.lines().count(), 2);
        assert!(
            DebugStore::load(root.path())
                .await
                .take_storage_notices()
                .is_empty()
        );

        // 追加接在干净的正本后面
        store
            .append_history(entry("g3", "c", "1", true))
            .await
            .unwrap();
        let reloaded = DebugStore::load(root.path()).await;
        assert_eq!(reloaded.history(query()).await.total, 3);
    }

    #[tokio::test]
    async fn blank_history_lines_are_not_bad() {
        let root = tempfile::tempdir().unwrap();
        let dir = debug_dir(root.path());
        std::fs::create_dir_all(&dir).unwrap();
        let good = serde_json::to_string(&entry("g", "a", "1", true)).unwrap();
        std::fs::write(dir.join(HISTORY_FILE), format!("\n{good}\r\n\n")).unwrap();

        let store = DebugStore::load(root.path()).await;
        assert_eq!(store.history(query()).await.total, 1);
        assert!(store.take_storage_notices().is_empty());
    }

    // --- 历史压缩与截断 -------------------------------------------------------------

    #[tokio::test]
    async fn history_compaction_keeps_newest_thousand() {
        let root = tempfile::tempdir().unwrap();
        let store = DebugStore::load(root.path()).await;
        let path = debug_dir(root.path()).join(HISTORY_FILE);

        for i in 1..=1200 {
            store
                .append_history(entry(&format!("e{i}"), "a", "1", true))
                .await
                .unwrap();
        }
        // 正好 1200 行还没到压缩线
        assert_eq!(
            std::fs::read_to_string(&path).unwrap().lines().count(),
            1200
        );

        store
            .append_history(entry("e1201", "a", "1", true))
            .await
            .unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        let ids: Vec<String> = content
            .lines()
            .map(|l| serde_json::from_str::<DebugHistoryEntry>(l).unwrap().id)
            .collect();
        assert_eq!(ids.len(), 1000);
        assert_eq!(ids.first().map(String::as_str), Some("e202"));
        assert_eq!(ids.last().map(String::as_str), Some("e1201"));

        // 内存同样只留最新 1000
        let page = store.history(query()).await;
        assert_eq!(page.total, 1000);
        assert!(store.history_entry("e201").await.is_none());
        assert!(store.history_entry("e202").await.is_some());

        // 压缩后继续追加，行数从 1000 往上数
        store
            .append_history(entry("e1202", "a", "1", true))
            .await
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap().lines().count(),
            1001
        );
    }

    #[tokio::test]
    async fn oversized_history_file_is_compacted_on_load() {
        let root = tempfile::tempdir().unwrap();
        let dir = debug_dir(root.path());
        std::fs::create_dir_all(&dir).unwrap();
        let mut content = String::new();
        for i in 1..=1300 {
            content.push_str(
                &serde_json::to_string(&entry(&format!("e{i}"), "a", "1", true)).unwrap(),
            );
            content.push('\n');
        }
        std::fs::write(dir.join(HISTORY_FILE), content).unwrap();

        let store = DebugStore::load(root.path()).await;
        assert_eq!(store.history(query()).await.total, 1000);
        assert!(store.history_entry("e301").await.is_some());
        assert!(store.history_entry("e300").await.is_none());
        let lines = std::fs::read_to_string(dir.join(HISTORY_FILE))
            .unwrap()
            .lines()
            .count();
        assert_eq!(lines, 1000);
        assert!(store.take_storage_notices().is_empty());
    }

    #[tokio::test]
    async fn oversized_response_is_dropped_and_flagged() {
        let root = tempfile::tempdir().unwrap();
        let store = DebugStore::load(root.path()).await;

        let mut big = entry("big", "get_group_member_list", "1", true);
        big.response = Some(Value::String("x".repeat(HISTORY_RESPONSE_LIMIT + 1)));
        store.append_history(big).await.unwrap();

        // 刚好在上限内的保留：字符串序列化后多两个引号，所以内容取 limit - 2
        let mut edge = entry("edge", "a", "1", true);
        edge.response = Some(Value::String("x".repeat(HISTORY_RESPONSE_LIMIT - 2)));
        store.append_history(edge).await.unwrap();

        let big = store.history_entry("big").await.unwrap();
        assert_eq!(big.response, None);
        assert!(big.response_truncated);
        let edge = store.history_entry("edge").await.unwrap();
        assert!(edge.response.is_some());
        assert!(!edge.response_truncated);

        // 落盘的也是截掉的版本，重开之后一样
        let reloaded = DebugStore::load(root.path()).await;
        let big = reloaded.history_entry("big").await.unwrap();
        assert_eq!(big.response, None);
        assert!(big.response_truncated);
    }

    #[tokio::test]
    async fn already_truncated_flag_is_preserved() {
        let root = tempfile::tempdir().unwrap();
        let store = DebugStore::load(root.path()).await;
        let mut e = entry("t", "a", "1", true);
        e.response_truncated = true;
        store.append_history(e).await.unwrap();
        let stored = store.history_entry("t").await.unwrap();
        assert!(stored.response_truncated);
        assert!(stored.response.is_some());
    }

    #[tokio::test]
    async fn clear_history_removes_file_and_memory() {
        let root = tempfile::tempdir().unwrap();
        let store = DebugStore::load(root.path()).await;
        // 没有文件时清空也不报错
        store.clear_history().await.unwrap();
        store
            .append_history(entry("e", "a", "1", true))
            .await
            .unwrap();
        store.clear_history().await.unwrap();
        assert_eq!(store.history(query()).await.total, 0);
        assert!(!debug_dir(root.path()).join(HISTORY_FILE).exists());
        // 清空后还能继续记
        store
            .append_history(entry("e2", "a", "1", true))
            .await
            .unwrap();
        assert_eq!(
            DebugStore::load(root.path())
                .await
                .history(query())
                .await
                .total,
            1
        );
    }

    // --- 查询 -------------------------------------------------------------

    async fn seeded_store(root: &Path) -> DebugStore {
        let store = DebugStore::load(root).await;
        let mut e1 = entry("e1", "send_group_msg", "1", true);
        e1.params = json!({"group_id": 111, "message": "Hello World"});
        let mut e2 = entry("e2", "get_login_info", "2", false);
        e2.bot_name = "小号Bot".to_owned();
        e2.params = json!({});
        let mut e3 = entry("e3", "send_group_msg", "2", false);
        e3.params = json!({"group_id": 222});
        e3.error = Some(DebugError::Timeout { ms: 5000 });
        e3.retcode = None;
        for e in [e1, e2, e3] {
            store.append_history(e).await.unwrap();
        }
        store
    }

    fn ids(page: &DebugHistoryPage) -> Vec<&str> {
        page.entries.iter().map(|e| e.id.as_str()).collect()
    }

    #[tokio::test]
    async fn query_filters_by_action_bot_and_ok() {
        let root = tempfile::tempdir().unwrap();
        let store = seeded_store(root.path()).await;

        let by_action = store
            .history(DebugHistoryQuery {
                action: Some("send_group_msg".into()),
                ..query()
            })
            .await;
        assert_eq!(ids(&by_action), ["e3", "e1"]);

        // action 是精确匹配，不是子串
        let partial = store
            .history(DebugHistoryQuery {
                action: Some("send_group".into()),
                ..query()
            })
            .await;
        assert_eq!(partial.total, 0);

        let by_bot = store
            .history(DebugHistoryQuery {
                bot_id: Some("2".into()),
                ..query()
            })
            .await;
        assert_eq!(ids(&by_bot), ["e3", "e2"]);

        let failed = store
            .history(DebugHistoryQuery {
                ok: Some(false),
                ..query()
            })
            .await;
        assert_eq!(ids(&failed), ["e3", "e2"]);

        let combined = store
            .history(DebugHistoryQuery {
                action: Some("send_group_msg".into()),
                bot_id: Some("2".into()),
                ok: Some(false),
                ..query()
            })
            .await;
        assert_eq!(ids(&combined), ["e3"]);
        // 摘要带出错误类型
        assert_eq!(combined.entries[0].error_kind.as_deref(), Some("timeout"));

        // 空串筛选等同没设
        let blank = store
            .history(DebugHistoryQuery {
                action: Some(String::new()),
                bot_id: Some(String::new()),
                text: Some("  ".into()),
                ..query()
            })
            .await;
        assert_eq!(blank.total, 3);
    }

    #[tokio::test]
    async fn query_text_matches_action_params_and_bot_name_case_insensitively() {
        let root = tempfile::tempdir().unwrap();
        let store = seeded_store(root.path()).await;
        let text = |t: &str| DebugHistoryQuery {
            text: Some(t.to_owned()),
            ..query()
        };

        // action
        assert_eq!(ids(&store.history(text("LOGIN")).await), ["e2"]);
        // params 里的值，大小写不敏感
        assert_eq!(ids(&store.history(text("hello world")).await), ["e1"]);
        // params 里的键
        assert_eq!(ids(&store.history(text("group_id")).await), ["e3", "e1"]);
        // params 里的数字
        assert_eq!(ids(&store.history(text("222")).await), ["e3"]);
        // bot 名
        assert_eq!(ids(&store.history(text("小号bot")).await), ["e2"]);
        assert_eq!(store.history(text("no-such-thing")).await.total, 0);
    }

    #[tokio::test]
    async fn query_paginates_and_clamps_limit() {
        let root = tempfile::tempdir().unwrap();
        let store = DebugStore::load(root.path()).await;
        for i in 0..250 {
            store
                .append_history(entry(&format!("e{i}"), "a", "1", true))
                .await
                .unwrap();
        }

        // limit 0 收成 1
        let one = store
            .history(DebugHistoryQuery {
                limit: 0,
                ..query()
            })
            .await;
        assert_eq!(one.entries.len(), 1);
        assert_eq!(one.total, 250);
        assert_eq!(one.entries[0].id, "e249");

        // 过大的 limit 收成 200
        let capped = store
            .history(DebugHistoryQuery {
                limit: 9999,
                ..query()
            })
            .await;
        assert_eq!(capped.entries.len(), 200);
        assert_eq!(capped.total, 250);

        // offset 翻页；最后一页不足 limit
        let tail = store
            .history(DebugHistoryQuery {
                limit: 100,
                offset: 200,
                ..query()
            })
            .await;
        assert_eq!(tail.entries.len(), 50);
        assert_eq!(tail.entries.last().map(|e| e.id.as_str()), Some("e0"));

        let beyond = store
            .history(DebugHistoryQuery {
                offset: 10_000,
                ..query()
            })
            .await;
        assert!(beyond.entries.is_empty());
        assert_eq!(beyond.total, 250);
    }

    // --- 工作区上限 -------------------------------------------------------------

    #[tokio::test]
    async fn workspace_limits_are_enforced_on_save() {
        let root = tempfile::tempdir().unwrap();
        let store = DebugStore::load(root.path()).await;

        let ws = DebugWorkspace {
            closed_tabs: (0..15).map(|i| draft(&format!("c{i}"))).collect(),
            recent_actions: (0..30).map(|i| format!("act{i}")).collect(),
            // 60 个标签，激活的恰好是最老的那个
            tabs: (0..60).map(|i| draft(&format!("t{i}"))).collect(),
            active_tab: Some("t0".to_owned()),
            ..DebugWorkspace::default()
        };
        store.save_workspace(ws).await.unwrap();

        let saved = store.workspace().await;
        assert_eq!(saved.closed_tabs.len(), 10);
        // 新的在前，截尾
        assert_eq!(saved.closed_tabs[0].id, "c0");
        assert_eq!(saved.recent_actions.len(), 20);
        assert_eq!(saved.recent_actions[0], "act0");
        assert_eq!(saved.tabs.len(), 50);
        // 激活的保住，丢的是最老的非激活标签：t1..=t10
        assert_eq!(saved.tabs[0].id, "t0");
        assert_eq!(saved.tabs[1].id, "t11");
        assert_eq!(saved.tabs.last().map(|t| t.id.as_str()), Some("t59"));

        // 落盘的也是收过的
        let reloaded = DebugStore::load(root.path()).await;
        assert_eq!(reloaded.workspace().await, saved);
    }

    #[tokio::test]
    async fn oversized_workspace_file_is_trimmed_on_load() {
        let root = tempfile::tempdir().unwrap();
        let dir = debug_dir(root.path());
        std::fs::create_dir_all(&dir).unwrap();
        let ws = DebugWorkspace {
            recent_actions: (0..40).map(|i| format!("act{i}")).collect(),
            ..DebugWorkspace::default()
        };
        std::fs::write(dir.join(WORKSPACE_FILE), serde_json::to_vec(&ws).unwrap()).unwrap();

        let store = DebugStore::load(root.path()).await;
        assert_eq!(store.workspace().await.recent_actions.len(), 20);
    }

    #[test]
    fn many_tabs_sharing_the_active_id_still_fit() {
        // 畸形数据：全部标签 id 相同且都是激活的，也不能超上限或死循环
        let ws = DebugWorkspace {
            tabs: (0..80).map(|_| draft("same")).collect(),
            active_tab: Some("same".to_owned()),
            ..DebugWorkspace::default()
        };
        assert_eq!(normalize_workspace(ws).tabs.len(), MAX_TABS);
    }

    // --- 导入导出 -------------------------------------------------------------

    #[tokio::test]
    async fn import_merges_and_renames_clashing_ids() {
        let root = tempfile::tempdir().unwrap();
        let store = DebugStore::load(root.path()).await;
        store
            .save_collections(DebugCollections {
                version: 1,
                folders: vec![folder("f1", "常用", 0)],
                requests: vec![saved("r1", Some("f1"), 0), saved("r9", None, 3)],
            })
            .await
            .unwrap();

        // 导入文件里 f1 / r1 都和现有的撞了；r3 挂在导入文件里没有的文件夹上
        let incoming = DebugCollections {
            version: 1,
            folders: vec![folder("f1", "常用", 5), folder("f2", "群管理", 6)],
            requests: vec![
                saved("r1", Some("f1"), 10),
                saved("r2", Some("f2"), 11),
                saved("r3", Some("ghost"), 12),
            ],
        };
        let file = root.path().join("in.json");
        std::fs::write(&file, serde_json::to_vec_pretty(&incoming).unwrap()).unwrap();

        let merged = store.import_collections(&file).await.unwrap();

        // 现有的原样在前
        assert_eq!(merged.folders[0], folder("f1", "常用", 0));
        assert_eq!(merged.requests[0], saved("r1", Some("f1"), 0));
        assert_eq!(merged.folders.len(), 3);
        assert_eq!(merged.requests.len(), 5);

        // 文件夹名照原样保留，同名也不合并、不改名
        let names: Vec<&str> = merged.folders.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["常用", "常用", "群管理"]);

        // 全部 id 唯一
        let mut all: HashSet<&str> = HashSet::new();
        for id in merged
            .folders
            .iter()
            .map(|f| f.id.as_str())
            .chain(merged.requests.iter().map(|r| r.id.as_str()))
        {
            assert!(all.insert(id), "重复的 id: {id}");
        }

        // 撞了的换新 id，没撞的沿用
        let new_f1 = &merged.folders[1];
        assert_ne!(new_f1.id, "f1");
        assert_eq!(merged.folders[2].id, "f2");
        let imported_r1 = &merged.requests[2];
        assert_ne!(imported_r1.id, "r1");
        // 请求跟着换后的文件夹 id 走
        assert_eq!(imported_r1.folder_id.as_deref(), Some(new_f1.id.as_str()));
        assert_eq!(merged.requests[3].id, "r2");
        assert_eq!(merged.requests[3].folder_id.as_deref(), Some("f2"));
        // 挂在不存在的文件夹上的落到根
        assert_eq!(merged.requests[4].folder_id, None);

        // 导入的 order 排在现有的后面，彼此先后不变
        assert!(new_f1.order > 0);
        assert!(merged.folders[2].order > new_f1.order);
        assert!(imported_r1.order > 3);
        assert!(merged.requests[3].order > imported_r1.order);

        // 已落盘、返回值和内存一致
        assert_eq!(store.collections().await, merged);
        assert_eq!(
            DebugStore::load(root.path()).await.collections().await,
            merged
        );
    }

    #[tokio::test]
    async fn import_into_empty_keeps_ids_and_starts_orders_at_zero() {
        let root = tempfile::tempdir().unwrap();
        let store = DebugStore::load(root.path()).await;
        let incoming = DebugCollections {
            version: 1,
            folders: vec![folder("f1", "A", 7)],
            requests: vec![saved("r1", Some("f1"), 4), saved("r2", None, 5)],
        };
        let file = root.path().join("in.json");
        std::fs::write(&file, serde_json::to_vec(&incoming).unwrap()).unwrap();

        let merged = store.import_collections(&file).await.unwrap();
        assert_eq!(merged.folders[0].id, "f1");
        assert_eq!(merged.folders[0].order, 0);
        assert_eq!(merged.requests[0].id, "r1");
        assert_eq!(merged.requests[0].order, 0);
        assert_eq!(merged.requests[1].order, 1);
        assert_eq!(merged.requests[0].folder_id.as_deref(), Some("f1"));
    }

    #[tokio::test]
    async fn import_rejects_bad_files_without_touching_state() {
        let root = tempfile::tempdir().unwrap();
        let store = DebugStore::load(root.path()).await;
        let before = DebugCollections {
            version: 1,
            folders: vec![folder("f1", "常用", 0)],
            requests: Vec::new(),
        };
        store.save_collections(before.clone()).await.unwrap();

        let cases: [(&str, &str); 4] = [
            ("garbage.json", "not json at all"),
            ("array.json", "[1,2,3]"),
            // 合法 JSON 对象但不是收藏夹（比如误选了 workspace.json）
            ("workspace.json", r#"{"tabs":[],"version":1}"#),
            ("wrong-type.json", r#"{"folders": 7}"#),
        ];
        for (name, content) in cases {
            let file = root.path().join(name);
            std::fs::write(&file, content).unwrap();
            let err = store.import_collections(&file).await.unwrap_err();
            assert!(!err.is_empty(), "{name}");
            // 用户的文件不能被挪走
            assert!(file.exists(), "{name}");
        }
        let missing = store
            .import_collections(&root.path().join("nope.json"))
            .await
            .unwrap_err();
        assert!(missing.contains("读取导入文件失败"));

        assert_eq!(store.collections().await, before);
        assert!(store.take_storage_notices().is_empty());
    }

    #[tokio::test]
    async fn export_then_import_round_trips() {
        let root = tempfile::tempdir().unwrap();
        let src = DebugStore::load(&root.path().join("a")).await;
        let collections = DebugCollections {
            version: 1,
            folders: vec![folder("f1", "常用", 0)],
            requests: vec![saved("r1", Some("f1"), 0)],
        };
        src.save_collections(collections.clone()).await.unwrap();

        let out = root.path().join("export.json");
        // 覆盖已有文件也行
        std::fs::write(&out, "old").unwrap();
        src.export_collections(&out).await.unwrap();
        let exported: DebugCollections =
            serde_json::from_slice(&std::fs::read(&out).unwrap()).unwrap();
        assert_eq!(exported, collections);
        // 导出目录里不留临时文件
        assert_eq!(file_names(root.path()), ["a", "export.json"]);

        let dst = DebugStore::load(&root.path().join("b")).await;
        let merged = dst.import_collections(&out).await.unwrap();
        assert_eq!(merged, collections);
    }

    #[tokio::test]
    async fn export_to_missing_directory_fails_cleanly() {
        let root = tempfile::tempdir().unwrap();
        let store = DebugStore::load(root.path()).await;
        let err = store
            .export_collections(&root.path().join("no-dir").join("x.json"))
            .await
            .unwrap_err();
        assert!(err.contains("创建临时文件失败"), "{err}");
    }

    // --- 调用 -> 历史 -------------------------------------------------------------

    #[test]
    fn only_editor_and_composer_calls_are_recorded() {
        let result = DebugCallResult::Ok {
            outcome: outcome(true, json!({"status": "ok"}), false),
        };
        let channel = DebugChannelId::Http { name: "h".into() };
        for origin in [DebugCallOrigin::Editor, DebugCallOrigin::Composer] {
            let e = history_entry_from_call(
                "1",
                "小号",
                BackendType::SnowLuma,
                &request(origin.clone()),
                &channel,
                &result,
            )
            .unwrap();
            assert_eq!(e.origin, origin);
            assert_eq!(e.bot_id, "1");
            assert_eq!(e.bot_name, "小号");
            assert_eq!(e.backend, BackendType::SnowLuma);
            // 记的是实际用到的通道，不是请求里的 auto
            assert_eq!(e.channel, channel);
            assert_eq!(e.action, "send_msg");
            assert_eq!(e.params, json!({"message": "hi"}));
            assert!(e.ok);
            assert_eq!(e.retcode, Some(0));
            assert_eq!(e.error, None);
            assert_eq!(e.elapsed_ms, 34);
            assert_eq!(e.response, Some(json!({"status": "ok"})));
            assert!(!e.response_truncated);
            assert!(!e.id.is_empty());
        }
        for origin in [DebugCallOrigin::Picker, DebugCallOrigin::Other] {
            assert!(
                history_entry_from_call(
                    "1",
                    "x",
                    BackendType::NapCat,
                    &request(origin),
                    &channel,
                    &result
                )
                .is_none()
            );
        }
    }

    #[test]
    fn failed_calls_are_recorded_with_error() {
        let req = request(DebugCallOrigin::Editor);
        let channel = DebugChannelId::Internal;

        let timeout = history_entry_from_call(
            "1",
            "b",
            BackendType::NapCat,
            &req,
            &channel,
            &DebugCallResult::Err {
                error: DebugError::Timeout { ms: 5000 },
            },
        )
        .unwrap();
        assert!(!timeout.ok);
        assert_eq!(timeout.retcode, None);
        assert_eq!(timeout.error, Some(DebugError::Timeout { ms: 5000 }));
        assert_eq!(timeout.elapsed_ms, 5000);
        assert_eq!(timeout.response, None);

        let cancelled = history_entry_from_call(
            "1",
            "b",
            BackendType::NapCat,
            &req,
            &channel,
            &DebugCallResult::Err {
                error: DebugError::Cancelled,
            },
        )
        .unwrap();
        assert_eq!(cancelled.elapsed_ms, 0);

        // OneBot 回包里 retcode 非 0：拿到了回包，ok 为假，没有 error，回包照存
        let failed = history_entry_from_call(
            "1",
            "b",
            BackendType::NapCat,
            &req,
            &channel,
            &DebugCallResult::Ok {
                outcome: outcome(false, json!({"retcode": 1400}), true),
            },
        )
        .unwrap();
        assert!(!failed.ok);
        assert_eq!(failed.retcode, Some(1400));
        assert_eq!(failed.error, None);
        assert!(failed.response_truncated);
    }

    // --- 内存预算 -------------------------------------------------------------

    fn entry_with_response(id: &str, bytes: usize) -> DebugHistoryEntry {
        let mut e = entry(id, "get_group_member_list", "1", true);
        e.response = Some(Value::String("x".repeat(bytes)));
        e
    }

    #[tokio::test]
    async fn response_budget_evicts_oldest_and_history_entry_reads_disk() {
        let root = tempfile::tempdir().unwrap();
        // 每条回包序列化后 20482 字节，预算 100 KiB 只装得下 4 条
        let budget = 100 * 1024;
        let store = DebugStore::load_with_budget(root.path(), budget).await;
        for i in 0..20 {
            store
                .append_history(entry_with_response(&format!("e{i}"), 20 * 1024))
                .await
                .unwrap();
        }
        {
            let mem = store.history.lock().await;
            assert!(mem.response_bytes <= budget);
            let in_memory = mem
                .entries
                .iter()
                .filter(|m| m.entry.response.is_some())
                .count();
            assert_eq!(in_memory, 4);
            // 最老的被丢了并记着「在文件里」，最新的还在
            assert!(mem.entries[0].response_on_disk);
            assert!(mem.entries[0].entry.response.is_none());
            assert!(!mem.entries[19].response_on_disk);
            assert!(mem.entries[19].entry.response.is_some());
        }

        // 列表摘要不受影响；被丢的回包从文件里读回全文
        assert_eq!(store.history(query()).await.total, 20);
        let full = store.history_entry("e0").await.unwrap();
        assert_eq!(full.response, Some(Value::String("x".repeat(20 * 1024))));
        assert!(!full.response_truncated);
        assert_eq!(full.params, json!({"group_id": 123}));
        // 内存里还留着的直接返回
        let newest = store.history_entry("e19").await.unwrap();
        assert!(newest.response.is_some());
    }

    #[tokio::test]
    async fn evicted_response_missing_from_disk_is_flagged_truncated() {
        let root = tempfile::tempdir().unwrap();
        let store = DebugStore::load_with_budget(root.path(), 30 * 1024).await;
        for i in 0..3 {
            store
                .append_history(entry_with_response(&format!("e{i}"), 20 * 1024))
                .await
                .unwrap();
        }
        std::fs::remove_file(debug_dir(root.path()).join(HISTORY_FILE)).unwrap();

        let gone = store.history_entry("e0").await.unwrap();
        assert_eq!(gone.response, None);
        // 不能让界面把「拿不到」当成「这次调用没有回包」
        assert!(gone.response_truncated);
    }

    #[tokio::test]
    async fn load_respects_response_budget() {
        let root = tempfile::tempdir().unwrap();
        let budget = 100 * 1024;
        {
            // 写的时候不设限，文件里每条都有全文
            let store = DebugStore::load(root.path()).await;
            for i in 0..20 {
                store
                    .append_history(entry_with_response(&format!("e{i}"), 20 * 1024))
                    .await
                    .unwrap();
            }
        }
        let reloaded = DebugStore::load_with_budget(root.path(), budget).await;
        {
            let mem = reloaded.history.lock().await;
            assert!(mem.response_bytes <= budget);
            let in_memory = mem
                .entries
                .iter()
                .filter(|m| m.entry.response.is_some())
                .count();
            assert_eq!(in_memory, 4);
            assert_eq!(mem.entries.len(), 20);
        }
        let oldest = reloaded.history_entry("e0").await.unwrap();
        assert_eq!(oldest.response, Some(Value::String("x".repeat(20 * 1024))));
        assert!(reloaded.take_storage_notices().is_empty());
    }

    #[tokio::test]
    async fn streaming_compaction_keeps_responses_evicted_from_memory() {
        let root = tempfile::tempdir().unwrap();
        // 预算只够留十几条回包，其余 1000 多条的全文只在文件里
        let store = DebugStore::load_with_budget(root.path(), 64 * 1024).await;
        for i in 1..=1201 {
            store
                .append_history(entry_with_response(&format!("e{i}"), 2 * 1024))
                .await
                .unwrap();
        }
        // 第 1201 条触发了压缩，只剩最新 1000 行，而且每一行的回包都还在
        let path = debug_dir(root.path()).join(HISTORY_FILE);
        let content = std::fs::read_to_string(&path).unwrap();
        let entries: Vec<DebugHistoryEntry> = content
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(entries.len(), 1000);
        assert_eq!(entries[0].id, "e202");
        assert_eq!(entries[999].id, "e1201");
        assert!(entries.iter().all(|e| e.response.is_some()));

        // 内存里早就丢了回包的条目，压缩后仍能读回全文
        {
            let mem = store.history.lock().await;
            assert!(mem.entries[0].response_on_disk);
        }
        let full = store.history_entry("e202").await.unwrap();
        assert_eq!(full.response, Some(Value::String("x".repeat(2 * 1024))));
        assert!(store.history_entry("e201").await.is_none());
        // 压缩后继续追加
        store
            .append_history(entry_with_response("e1202", 16))
            .await
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap().lines().count(),
            1001
        );
    }

    #[test]
    fn unpersisted_responses_are_never_evicted() {
        // 磁盘写失败的条目只在内存里有回包，丢了就找不回来
        let mut mem = HistoryMem::new(100);
        mem.push(entry_with_response("a", 200), 202, false);
        mem.push(entry_with_response("b", 200), 202, true);
        assert!(mem.entries[0].entry.response.is_some());
        assert!(!mem.entries[0].response_on_disk);
        assert!(mem.entries[1].entry.response.is_none());
        assert!(mem.entries[1].response_on_disk);
        assert_eq!(mem.response_bytes, 202);
    }

    // --- 参数与凭据 -------------------------------------------------------------

    #[tokio::test]
    async fn oversized_param_strings_are_replaced_by_placeholder() {
        let root = tempfile::tempdir().unwrap();
        let store = DebugStore::load(root.path()).await;
        let mut e = entry("p", "send_msg", "1", true);
        e.params = json!({
            "file": "a".repeat(PARAM_STRING_LIMIT + 1),
            "edge": "b".repeat(PARAM_STRING_LIMIT),
            "nested": [{"deep": "c".repeat(100_000)}],
            // 多字节字符按字节数算
            "cjk": "汉".repeat(30_000),
            "n": 1,
            "s": "small",
        });
        store.append_history(e).await.unwrap();

        let stored = store.history_entry("p").await.unwrap();
        assert_eq!(stored.params["file"], "<已省略 65537 字节>");
        assert_eq!(stored.params["nested"][0]["deep"], "<已省略 100000 字节>");
        assert_eq!(stored.params["cjk"], "<已省略 90000 字节>");
        // 被瘦身的要标出来：前端据此拦住「把占位文字当参数重放」
        assert!(stored.params_truncated);
        // 刚好在上限内的不动
        assert_eq!(
            stored.params["edge"].as_str().map(str::len),
            Some(PARAM_STRING_LIMIT)
        );
        assert_eq!(stored.params["n"], 1);
        assert_eq!(stored.params["s"], "small");

        // 落盘的也是换过的版本
        let reloaded = DebugStore::load(root.path()).await;
        let stored = reloaded.history_entry("p").await.unwrap();
        assert_eq!(stored.params["file"], "<已省略 65537 字节>");
        assert!(stored.params_truncated);
    }

    #[tokio::test]
    async fn untrimmed_params_stay_unflagged_and_old_entries_default_false() {
        let root = tempfile::tempdir().unwrap();
        let store = DebugStore::load(root.path()).await;
        store
            .append_history(entry("small", "send_msg", "1", true))
            .await
            .unwrap();
        let stored = store.history_entry("small").await.unwrap();
        assert!(!stored.params_truncated);

        // 老版本写下的历史里没有这个字段：按没瘦过读，不报错
        let path = root.path().join("onebot-debug").join("history.jsonl");
        let raw = std::fs::read_to_string(&path).unwrap();
        let cleaned = raw
            .lines()
            .filter(|l| !l.is_empty())
            .map(|l| {
                let mut v: serde_json::Value = serde_json::from_str(l).unwrap();
                v.as_object_mut().unwrap().remove("params_truncated");
                serde_json::to_string(&v).unwrap()
            })
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(&path, cleaned).unwrap();
        let reloaded = DebugStore::load(root.path()).await;
        let stored = reloaded.history_entry("small").await.unwrap();
        assert!(!stored.params_truncated);
    }

    #[tokio::test]
    async fn credential_action_responses_are_not_stored() {
        let root = tempfile::tempdir().unwrap();
        let store = DebugStore::load(root.path()).await;
        let names = [
            "get_cookies",
            "get_credentials",
            "get_csrf_token",
            "get_clientkey",
            "get_rkey",
            "nc_get_rkey",
            "get_rkey_server",
            "get_cookies_async",
            "get_credentials_async",
            "get_csrf_token_async",
            "get_clientkey_async",
            "nc_get_rkey_async",
        ];
        for name in names {
            let mut e = entry(name, name, "1", true);
            e.response = Some(json!({"data": {"cookies": "uin=o123; skey=SECRET"}}));
            // 就算调用层带了「被截断」的标记，也不置位：这条本来就不该有回包
            e.response_truncated = true;
            store.append_history(e).await.unwrap();
        }
        let mut normal = entry("normal", "get_login_info", "1", true);
        normal.response = Some(json!({"data": {"nickname": "x"}}));
        store.append_history(normal).await.unwrap();

        async fn check(store: &DebugStore, names: &[&str]) {
            for name in names {
                let stored = store.history_entry(name).await.unwrap();
                assert_eq!(stored.response, None, "{name}");
                assert!(!stored.response_truncated, "{name}");
                assert_eq!(stored.action, *name);
            }
            assert!(
                store
                    .history_entry("normal")
                    .await
                    .unwrap()
                    .response
                    .is_some()
            );
        }
        check(&store, &names).await;

        // 文件里也不能有明文
        let content = std::fs::read_to_string(debug_dir(root.path()).join(HISTORY_FILE)).unwrap();
        assert!(!content.contains("SECRET"));
        check(&DebugStore::load(root.path()).await, &names).await;
    }

    #[test]
    fn credential_action_matching_is_exact() {
        assert!(is_credential_action("get_cookies"));
        assert!(is_credential_action("get_clientkey_async"));
        assert!(!is_credential_action("get_cookies_extra"));
        assert!(!is_credential_action("get_login_info"));
        assert!(!is_credential_action("_async"));
    }

    // --- 写盘失败、临时文件 -------------------------------------------------------------

    #[tokio::test]
    async fn failed_writes_leave_memory_unchanged() {
        let root = tempfile::tempdir().unwrap();
        let dir = debug_dir(root.path());
        let store = DebugStore::load(root.path()).await;

        let ws_a = DebugWorkspace {
            selected_bot: Some("a".to_owned()),
            ..DebugWorkspace::default()
        };
        let col_a = DebugCollections {
            version: 1,
            folders: vec![folder("f1", "常用", 0)],
            requests: Vec::new(),
        };
        store.save_workspace(ws_a.clone()).await.unwrap();
        store.save_collections(col_a.clone()).await.unwrap();
        store
            .append_history(entry("h1", "a", "1", true))
            .await
            .unwrap();

        // 把目标文件换成同名目录：临时文件写得成，rename 必然失败
        for file in [WORKSPACE_FILE, COLLECTIONS_FILE, HISTORY_FILE] {
            std::fs::remove_file(dir.join(file)).unwrap();
            std::fs::create_dir(dir.join(file)).unwrap();
        }

        let ws_b = DebugWorkspace {
            selected_bot: Some("b".to_owned()),
            ..DebugWorkspace::default()
        };
        let err = store.save_workspace(ws_b).await.unwrap_err();
        assert!(err.contains("替换"), "{err}");
        assert_eq!(store.workspace().await, ws_a);

        let col_b = DebugCollections {
            version: 1,
            folders: vec![folder("f2", "别的", 0)],
            requests: Vec::new(),
        };
        store.save_collections(col_b).await.unwrap_err();
        assert_eq!(store.collections().await, col_a);

        let incoming = root.path().join("in.json");
        std::fs::write(
            &incoming,
            serde_json::to_vec(&DebugCollections {
                version: 1,
                folders: vec![folder("f9", "导入", 0)],
                requests: Vec::new(),
            })
            .unwrap(),
        )
        .unwrap();
        store.import_collections(&incoming).await.unwrap_err();
        assert_eq!(store.collections().await, col_a);

        // 清空历史：文件删不掉就不清内存
        store.clear_history().await.unwrap_err();
        assert_eq!(store.history(query()).await.total, 1);

        // 失败的写盘不留临时文件（现在目录里只有那三个占位目录）
        assert_eq!(
            file_names(&dir),
            ["collections.json", "history.jsonl", "workspace.json"]
        );
    }

    #[tokio::test]
    async fn failed_append_keeps_entry_in_memory_and_reports_error() {
        let root = tempfile::tempdir().unwrap();
        let dir = debug_dir(root.path());
        let store = DebugStore::load(root.path()).await;
        // 加载之后再让 history.jsonl 变成目录，打开追加必然失败
        // （加载前就是目录的话，会被当成读不出来的文件挪开）
        std::fs::create_dir_all(dir.join(HISTORY_FILE)).unwrap();

        let err = store
            .append_history(entry("h1", "a", "1", true))
            .await
            .unwrap_err();
        assert!(err.contains("打开调用历史文件失败"), "{err}");
        // 本次运行里仍能查到
        assert_eq!(store.history(query()).await.total, 1);
        let mem = store.history.lock().await;
        assert!(!mem.entries[0].persisted);
    }

    #[tokio::test]
    async fn orphan_tmp_files_are_removed_on_load() {
        let root = tempfile::tempdir().unwrap();
        let dir = debug_dir(root.path());
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("workspace.json.tmp-1111"), "x").unwrap();
        std::fs::write(dir.join("collections.json.tmp-2222"), "x").unwrap();
        std::fs::write(dir.join("history.jsonl.tmp-3333"), "x").unwrap();
        // 不是我们的文件，不能碰
        std::fs::write(dir.join("notes.tmp-4444"), "keep").unwrap();
        std::fs::write(dir.join(WORKSPACE_FILE), "{}").unwrap();

        let store = DebugStore::load(root.path()).await;
        assert_eq!(file_names(&dir), ["notes.tmp-4444", "workspace.json"]);
        assert!(store.take_storage_notices().is_empty());
    }

    #[tokio::test]
    async fn valid_last_line_without_newline_is_repaired_on_load() {
        // 崩在追加中途、恰好整行都写进去了只缺换行：读得出来，但不修的话下一条会和它粘成一行
        let root = tempfile::tempdir().unwrap();
        let dir = debug_dir(root.path());
        std::fs::create_dir_all(&dir).unwrap();
        let g1 = serde_json::to_string(&entry("g1", "a", "1", true)).unwrap();
        let g2 = serde_json::to_string(&entry("g2", "a", "1", true)).unwrap();
        std::fs::write(dir.join(HISTORY_FILE), format!("{g1}\n{g2}")).unwrap();

        let store = DebugStore::load(root.path()).await;
        assert_eq!(store.history(query()).await.total, 2);
        assert!(store.take_storage_notices().is_empty());
        store
            .append_history(entry("g3", "a", "1", true))
            .await
            .unwrap();
        let reloaded = DebugStore::load(root.path()).await;
        assert_eq!(reloaded.history(query()).await.total, 3);
        assert!(reloaded.take_storage_notices().is_empty());
        assert_eq!(
            std::fs::read_to_string(dir.join(HISTORY_FILE))
                .unwrap()
                .lines()
                .count(),
            3
        );
    }

    #[test]
    fn notice_explains_why_file_was_not_moved() {
        let moved = notice(
            "workspace.json",
            Err("拒绝访问".to_owned()),
            "EOF".to_owned(),
        );
        assert_eq!(moved.moved_to, "");
        assert!(moved.reason.contains("EOF"));
        assert!(moved.reason.contains("拒绝访问"));

        let ok = notice("workspace.json", Ok(PathBuf::from("a/b")), "EOF".to_owned());
        assert_eq!(ok.moved_to, PathBuf::from("a/b").display().to_string());
        assert_eq!(ok.reason, "EOF");
    }

    // Windows 上用「不允许别人删除 / 替换」的方式打开文件，模拟杀软占着 history.jsonl，
    // 让压缩的 rename 一直失败
    #[cfg(windows)]
    #[tokio::test]
    async fn failed_compaction_does_not_fail_append_and_backs_off() {
        use std::os::windows::fs::OpenOptionsExt;

        let root = tempfile::tempdir().unwrap();
        let store = DebugStore::load(root.path()).await;
        let path = debug_dir(root.path()).join(HISTORY_FILE);
        store
            .append_history(entry("e1", "a", "1", true))
            .await
            .unwrap();

        // FILE_SHARE_READ | FILE_SHARE_WRITE，不含 FILE_SHARE_DELETE
        let blocker = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(3)
            .open(&path)
            .unwrap();
        for i in 2..=1201 {
            // 第 1201 条触发压缩，压缩失败也不能让追加失败
            store
                .append_history(entry(&format!("e{i}"), "a", "1", true))
                .await
                .unwrap();
        }
        assert_eq!(
            std::fs::read_to_string(&path).unwrap().lines().count(),
            1201
        );
        {
            let gate = store.write_gate.lock().await;
            assert_eq!(gate.compact_retry_at, 1201 + COMPACT_BACKOFF);
        }
        // 退避期内不再重试：到 1401 行为止文件只增不减
        for i in 1202..=1401 {
            store
                .append_history(entry(&format!("e{i}"), "a", "1", true))
                .await
                .unwrap();
        }
        assert_eq!(
            std::fs::read_to_string(&path).unwrap().lines().count(),
            1401
        );

        // 占用解除后，退避期一过就压缩成功
        drop(blocker);
        store
            .append_history(entry("e1402", "a", "1", true))
            .await
            .unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content.lines().count(), 1000);
        assert!(content.lines().last().unwrap().contains("\"e1402\""));
        let gate = store.write_gate.lock().await;
        assert_eq!(gate.compact_retry_at, 0);
        assert_eq!(gate.history_lines, 1000);
    }
}
