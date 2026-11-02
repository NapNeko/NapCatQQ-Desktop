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

mod config;
mod files;
mod history;
mod history_file;

#[cfg(test)]
mod tests;

use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::{Mutex as StdMutex, PoisonError};
use std::time::Duration;

use ncd_domain::onebot_debug::{
    DebugCollections, DebugHistoryEntry, DebugHistoryPage, DebugHistoryQuery, DebugStorageNotice,
    DebugWorkspace,
};
use serde_json::Value;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

use config::{merge_collections, normalize_workspace};
use files::{
    load_json_or_default, read_json_read_only, remove_orphan_tmp_files, strip_bom, write_atomic,
};
use history::{HistoryMem, matches_query, prepare_entry, summarize};
use history_file::{
    count_lines, load_history, read_history_entry, read_history_entry_bounded, rewrite_lines,
    scan_history_bounded,
};

pub(crate) use history::history_entry_from_call;

pub(super) const DIR_NAME: &str = "onebot-debug";
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
const READ_ONLY_RESPONSE_BUDGET: usize = 1024 * 1024;
const READ_ONLY_HISTORY_LINE_LIMIT: u64 = 1024 * 1024;
const READ_ONLY_HISTORY_SCAN_LIMIT: u64 = 32 * 1024 * 1024;
const READ_ONLY_HISTORY_RETAINED_LIMIT: usize = 8 * 1024 * 1024;
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

impl DebugStore {
    /// 读三份文件。缺失就用默认值；读不出来的挪开并留提示。本身不创建目录，第一次写才建
    pub async fn load(data_root: &Path) -> Self {
        Self::load_with_budget(data_root, RESPONSE_BUDGET).await
    }

    /// MCP 冷读取不触发 GUI 的隔离、历史修复和临时文件清理，也不接管其缓存。
    pub(super) async fn load_read_only(data_root: &Path) -> Result<Self, String> {
        let dir = data_root.join(DIR_NAME);
        let workspace = read_json_read_only::<DebugWorkspace>(&dir.join(WORKSPACE_FILE)).await?;
        let collections =
            read_json_read_only::<DebugCollections>(&dir.join(COLLECTIONS_FILE)).await?;
        let (history, history_lines) = match tokio::fs::File::open(dir.join(HISTORY_FILE)).await {
            Ok(file) => {
                let scan = scan_history_bounded(file, READ_ONLY_RESPONSE_BUDGET, true)
                    .await
                    .map_err(|_| "无法只读加载调用历史".to_owned())?;
                if scan.bad > 0 {
                    return Err("调用历史含损坏记录，请在桌面调试台处理".to_owned());
                }
                (scan.mem, scan.lines)
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {
                (HistoryMem::new(READ_ONLY_RESPONSE_BUDGET), 0)
            }
            Err(_) => return Err("无法只读加载调用历史".to_owned()),
        };
        Ok(Self {
            dir,
            workspace: Mutex::new(normalize_workspace(workspace)),
            collections: Mutex::new(collections),
            history: Mutex::new(history),
            notices: StdMutex::new(Vec::new()),
            write_gate: Mutex::new(WriteState {
                history_lines,
                compact_retry_at: 0,
            }),
        })
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

    /// Commit imported configuration under the write gate; history stays in its existing cache.
    pub async fn replace_config_with<R>(
        &self,
        workspace: Option<DebugWorkspace>,
        collections: Option<DebugCollections>,
        write: impl std::future::Future<Output = Result<R, String>>,
    ) -> Result<R, String> {
        if workspace.is_none() && collections.is_none() {
            return write.await;
        }
        let _gate = self.write_gate.lock().await;
        let mut workspace_cache = if workspace.is_some() {
            Some(self.workspace.lock().await)
        } else {
            None
        };
        let mut collections_cache = if collections.is_some() {
            Some(self.collections.lock().await)
        } else {
            None
        };
        let result = write.await?;
        if let (Some(cache), Some(workspace)) = (workspace_cache.as_mut(), workspace) {
            **cache = normalize_workspace(workspace);
        }
        if let (Some(cache), Some(collections)) = (collections_cache.as_mut(), collections) {
            **cache = collections;
        }
        Ok(result)
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

    pub(super) async fn history_entry_read_only(
        &self,
        id: &str,
    ) -> Result<Option<DebugHistoryEntry>, String> {
        let (entry, on_disk) = {
            let history = self.history.lock().await;
            let Some(item) = history.entries.iter().rev().find(|m| m.entry.id == id) else {
                return Ok(None);
            };
            (item.entry.clone(), item.response_on_disk)
        };
        if !on_disk {
            return Ok(Some(entry));
        }
        // GUI 缓存已加载也不能让 MCP 的磁盘补读绕过行与扫描上限。
        let _gate = self.write_gate.lock().await;
        let full = read_history_entry_bounded(&self.path(HISTORY_FILE), id)
            .await
            .map_err(|_| "调用历史全文不可读、损坏或超过只读加载限制".to_owned())?
            .ok_or_else(|| "调用历史全文已不可用，请在桌面调试台处理".to_owned())?;
        Ok(Some(full))
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
