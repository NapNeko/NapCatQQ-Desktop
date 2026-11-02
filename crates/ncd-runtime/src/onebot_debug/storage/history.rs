//! 历史记录入库、内存预算和查询；不执行 I/O。

use std::collections::VecDeque;
use std::time::{SystemTime, UNIX_EPOCH};

use ncd_domain::bot_config::BackendType;
use ncd_domain::onebot_debug::{
    DebugCallOrigin, DebugCallRequest, DebugCallResult, DebugChannelId, DebugError,
    DebugHistoryEntry, DebugHistoryPage, DebugHistoryQuery, DebugHistorySummary,
};
use uuid::Uuid;

use super::super::params::{cap_record_params, serialized_len};

/// 历史保留的条数，与 JSONL 压缩策略共享。
pub(super) const HISTORY_KEEP: usize = 1000;
/// 单条回包与全部驻留回包的序列化字节上限。
pub(super) const HISTORY_RESPONSE_LIMIT: usize = 256 * 1024;
pub(super) const RESPONSE_BUDGET: usize = 32 * 1024 * 1024;
const HISTORY_PAGE_MAX: u32 = 200;

/// 明文历史不保存登录凭据，包括动作的 `_async` 变体。
const CREDENTIAL_ACTIONS: [&str; 7] = [
    "get_cookies",
    "get_credentials",
    "get_csrf_token",
    "get_clientkey",
    "get_rkey",
    "nc_get_rkey",
    "get_rkey_server",
];

/// 内存里的历史：最新的 1000 条，加上回包全文的预算记账
pub(super) struct HistoryMem {
    /// 最旧的在前，最新的在后
    entries: VecDeque<MemEntry>,
    /// 内存里各条 `response` 序列化后的字节数之和
    response_bytes: usize,
    budget: usize,
    /// 已经检查过的前缀不再可能释放回包；每条记录最多检查一次。
    eviction_cursor: usize,
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

/// 磁盘补读失败时，`NeedsResponse` 携带的摘要仍可供界面展示。
pub(super) enum HistoryLookup {
    Complete(DebugHistoryEntry),
    NeedsResponse(DebugHistoryEntry),
}

impl HistoryMem {
    /// 只暴露分页结果，不让调用方依赖队列顺序和预算记账。
    pub(super) fn page(&self, q: &DebugHistoryQuery) -> DebugHistoryPage {
        let limit = q.limit.clamp(1, HISTORY_PAGE_MAX) as usize;
        let offset = q.offset as usize;
        let needle = q
            .text
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(str::to_lowercase);

        let mut total = 0usize;
        let mut entries = Vec::new();
        for item in self
            .entries
            .iter()
            .rev()
            .filter(|m| matches_query(&m.entry, q, needle.as_deref()))
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

    pub(super) fn lookup(&self, id: &str) -> Option<HistoryLookup> {
        let item = self.entries.iter().rev().find(|item| item.entry.id == id)?;
        let entry = item.entry.clone();
        Some(if item.response_on_disk {
            HistoryLookup::NeedsResponse(entry)
        } else {
            HistoryLookup::Complete(entry)
        })
    }

    pub(super) fn new(budget: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            response_bytes: 0,
            budget,
            eviction_cursor: 0,
        }
    }

    pub(super) fn push(
        &mut self,
        entry: DebugHistoryEntry,
        response_bytes: usize,
        persisted: bool,
    ) {
        self.response_bytes += response_bytes;
        self.entries.push_back(MemEntry {
            entry,
            response_bytes,
            persisted,
            response_on_disk: false,
        });
        if self.entries.len() > HISTORY_KEEP {
            if let Some(old) = self.entries.pop_front() {
                self.response_bytes = self.response_bytes.saturating_sub(old.response_bytes);
                self.eviction_cursor = self.eviction_cursor.saturating_sub(1);
            }
        }
        self.enforce_budget();
    }

    /// 超预算就从最老的开始丢内存里的回包，直到回到预算内
    fn enforce_budget(&mut self) {
        while self.response_bytes > self.budget && self.eviction_cursor < self.entries.len() {
            let item = &mut self.entries[self.eviction_cursor];
            self.eviction_cursor += 1;
            if item.persisted && item.response_bytes > 0 {
                item.entry.response = None;
                item.response_on_disk = true;
                self.response_bytes = self.response_bytes.saturating_sub(item.response_bytes);
                item.response_bytes = 0;
            }
        }
    }

    pub(super) fn clear(&mut self) {
        self.entries.clear();
        self.response_bytes = 0;
        self.eviction_cursor = 0;
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

pub(super) fn is_credential_action(action: &str) -> bool {
    let base = action.strip_suffix("_async").unwrap_or(action);
    CREDENTIAL_ACTIONS.contains(&base)
}

/// 入库前的整理，返回 `response` 序列化后占的字节数（没有回包为 0）：
/// - 参数瘦身（过长的字符串换成占位文字，整体过大的再收一遍，与事件流里的调用记录同一套），
///   动过就在 `params_truncated` 上标出来；
/// - 凭据类动作不留回包；
/// - 回包序列化后超过 256 KiB 就整个不存。调用层已经截过（`response_truncated` 为真）的保持为真
pub(super) fn prepare_entry(entry: &mut DebugHistoryEntry) -> usize {
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

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or_default()
}
