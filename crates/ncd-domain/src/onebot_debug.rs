//! OneBot 调试台的跨边界数据：可调用的通道、动作目录、调用请求与结果、事件流，
//! 以及工作区、收藏夹、历史记录这几份落盘文件的结构。
//!
//! 线上字段一律 snake_case，带内容的枚举用 `kind`（接收器状态用 `state`）打标签。
//! `serde_json::Value` 没有 TS 实现，字段上用 `#[ts(type = ...)]` 直接指定前端类型；
//! 64 位整数同理写成 `number`，前端拿到的都是 JS 安全整数范围内的值。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::bot_config::BackendType;

/// 调试事件通道的信封版本
pub const DEBUG_EVENT_VERSION: u32 = 1;

// ---------------------------------------------------------------------------
// 目标与通道
// ---------------------------------------------------------------------------

/// Bot 跑在哪
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/debug/")]
pub enum DebugHost {
    Local,
    Remote { server_id: String },
    Docker { server_id: String },
}

/// 调试台左栏里的一个 Bot
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/debug/")]
pub struct DebugTarget {
    pub bot_id: String,
    pub name: String,
    #[ts(type = "number")]
    pub qq_id: u64,
    pub backend: BackendType,
    pub host: DebugHost,
    pub running: bool,
    /// 上游报告的登录状态；探不到时为空
    pub online: Option<bool>,
}

/// 一条调用 / 收事件的通道。会当作 map 键用，所以要 `Eq + Hash`
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/debug/")]
pub enum DebugChannelId {
    /// 由桌面端按可用性自动挑
    Auto,
    /// 上游 WebUI 自带的调试接口
    Internal,
    /// 用户配置的 HTTP 服务端
    Http { name: String },
    /// 用户配置的 WebSocket 服务端
    Ws { name: String },
}

impl DebugChannelId {
    /// 通道的稳定字符串键：`auto` / `internal` / `http:<name>` / `ws:<name>`。
    /// 用作 map 键和隧道键，不给人看
    pub fn label_key(&self) -> String {
        match self {
            Self::Auto => "auto".to_owned(),
            Self::Internal => "internal".to_owned(),
            Self::Http { name } => format!("http:{name}"),
            Self::Ws { name } => format!("ws:{name}"),
        }
    }
}

/// 通道当前能不能用，以及不能用的原因
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/debug/")]
pub enum DebugChannelStatus {
    /// 还没探过
    Unknown,
    Available,
    /// 远端通道已经经 SSH 隧道映射到本机端口
    Tunneled {
        local_port: u16,
    },
    Unreachable {
        reason: String,
    },
    /// 鉴权没过；`status` 是上游回的 HTTP 状态码
    AuthFailed {
        status: u16,
    },
    /// 上游版本太老，没有调试接口
    UpstreamTooOld,
    BotNotRunning,
    NotLoggedIn,
    Unsupported {
        reason: String,
    },
}

/// 通道列表里的一项
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/debug/")]
pub struct DebugChannelInfo {
    pub id: DebugChannelId,
    pub label: String,
    pub can_call: bool,
    pub can_receive: bool,
    pub status: DebugChannelStatus,
    /// 给人看的地址，如 `127.0.0.1:3000/`、`隧道 → 远端 127.0.0.1:3001`
    pub endpoint: Option<String>,
    /// 令牌的打码结果（`mask_token`），永远不带明文
    pub token_hint: Option<String>,
}

/// 一个 Bot 的全部通道，以及「自动」当前会落到哪条
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/debug/")]
pub struct DebugChannels {
    pub bot_id: String,
    pub channels: Vec<DebugChannelInfo>,
    pub auto_call: Option<DebugChannelId>,
    pub auto_events: Option<DebugChannelId>,
}

// ---------------------------------------------------------------------------
// 动作目录
// ---------------------------------------------------------------------------

/// 动作的副作用等级，前端据此决定要不要二次确认。要 `Eq + Hash`（分组计数）
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/debug/")]
pub enum DebugActionSafety {
    ReadOnly,
    SideEffect,
    Dangerous,
}

/// 动作的分类。要 `Eq + Hash`（分组）
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/debug/")]
pub enum DebugActionCategory {
    Message,
    GroupInfo,
    GroupAdmin,
    Friend,
    File,
    Request,
    Account,
    Face,
    Stream,
    Extension,
}

/// 目录来自哪：上游现取的，还是随桌面端打包的快照
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/debug/")]
pub enum DebugCatalogSource {
    Live,
    Snapshot,
}

/// 目录列表里的一行
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/debug/")]
pub struct DebugActionSummary {
    pub name: String,
    pub aliases: Vec<String>,
    pub summary: String,
    pub category: DebugActionCategory,
    pub safety: DebugActionSafety,
    pub stream: bool,
    /// 当前后端是否真的实现了这个动作
    pub supported: bool,
    /// 另一个后端有没有同名动作；没有另一份目录时为空
    pub other_backend_present: Option<bool>,
    /// 两个后端的参数不兼容（即 [`DebugOtherBackend::breaking`]）；只是写法不同不算
    pub param_diff: bool,
}

/// 文档里给的一条报错样例
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/debug/")]
pub struct DebugErrorExample {
    #[ts(type = "number")]
    pub retcode: i64,
    pub message: String,
}

/// 同名动作在两个后端上的参数差异
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/debug/")]
pub enum DebugParamDiffKind {
    OnlyHere,
    OnlyOther,
    TypeDiffers { here: String, other: String },
    RequiredDiffers { here: bool, other: bool },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/debug/")]
pub struct DebugParamDiff {
    pub name: String,
    pub diff: DebugParamDiffKind,
}

/// 另一个后端上对应动作的情况
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/debug/")]
pub struct DebugOtherBackend {
    pub backend: BackendType,
    pub present: bool,
    /// 全部出入，文档页的对照表用
    pub diffs: Vec<DebugParamDiff>,
    /// 照这边的写法发到另一边会直接失败：同名参数类型大类不同，或一侧的必填参数另一侧没有
    pub breaking: bool,
}

/// 单个动作的完整说明
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/debug/")]
pub struct DebugActionSpec {
    pub name: String,
    pub aliases: Vec<String>,
    pub summary: String,
    pub description: Option<String>,
    pub category: DebugActionCategory,
    pub safety: DebugActionSafety,
    pub stream: bool,
    pub supported: bool,
    #[ts(type = "Record<string, unknown>")]
    pub params_schema: serde_json::Value,
    #[ts(type = "Record<string, unknown> | null")]
    pub returns_schema: Option<serde_json::Value>,
    /// SnowLuma 的 `returns` 是一段散文，没有 schema
    pub returns_text: Option<String>,
    #[ts(type = "unknown")]
    pub return_example: Option<serde_json::Value>,
    #[ts(type = "Array<unknown>")]
    pub examples: Vec<serde_json::Value>,
    pub error_examples: Vec<DebugErrorExample>,
    pub invariants: Vec<String>,
    pub other_backend: Option<DebugOtherBackend>,
    pub source: DebugCatalogSource,
}

/// 一个后端的动作目录
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/debug/")]
pub struct DebugCatalog {
    pub backend: BackendType,
    pub source: DebugCatalogSource,
    pub snapshot_version: String,
    pub actions: Vec<DebugActionSummary>,
}

// ---------------------------------------------------------------------------
// 调用
// ---------------------------------------------------------------------------

/// 这次调用是从界面哪里发起的，历史里据此过滤
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/debug/")]
pub enum DebugCallOrigin {
    Editor,
    Composer,
    Picker,
    /// MCP 服务（agent）发起的调用，和用户的一样进历史
    Mcp,
    Other,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/debug/")]
pub struct DebugCallRequest {
    /// 前端生成，用来取消和对账
    pub request_id: String,
    pub bot_id: String,
    pub channel: DebugChannelId,
    pub action: String,
    #[ts(type = "unknown")]
    pub params: serde_json::Value,
    pub timeout_ms: Option<u32>,
    pub origin: DebugCallOrigin,
}

/// 调用拿到了 OneBot 回包时的结果（回包里 retcode 非 0 也算拿到）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/debug/")]
pub struct DebugCallOutcome {
    pub ok: bool,
    pub status: String,
    #[ts(type = "number")]
    pub retcode: i64,
    #[ts(type = "unknown")]
    pub data: serde_json::Value,
    pub message: String,
    pub wording: String,
    /// 完整回包；`truncated` 为真时是前 256 KiB 的文本
    #[ts(type = "unknown")]
    pub raw: serde_json::Value,
    pub elapsed_ms: u32,
    pub channel: DebugChannelId,
    pub size_bytes: u32,
    pub truncated: bool,
}

/// 调用没拿到回包的原因
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/debug/")]
pub enum DebugError {
    BotNotFound,
    BotNotRunning,
    NotLoggedIn,
    ChannelUnavailable { reason: String },
    UpstreamTooOld,
    AuthFailed { status: u16 },
    Timeout { ms: u32 },
    Cancelled,
    Transport { message: String },
    InvalidParams { message: String },
    FeatureDisabled,
    Internal { message: String },
}

impl DebugError {
    /// 与序列化出来的 `kind` 一致的名字，历史摘要按它筛选，不必反序列化整条记录
    pub const fn kind_str(&self) -> &'static str {
        match self {
            Self::BotNotFound => "bot_not_found",
            Self::BotNotRunning => "bot_not_running",
            Self::NotLoggedIn => "not_logged_in",
            Self::ChannelUnavailable { .. } => "channel_unavailable",
            Self::UpstreamTooOld => "upstream_too_old",
            Self::AuthFailed { .. } => "auth_failed",
            Self::Timeout { .. } => "timeout",
            Self::Cancelled => "cancelled",
            Self::Transport { .. } => "transport",
            Self::InvalidParams { .. } => "invalid_params",
            Self::FeatureDisabled => "feature_disabled",
            Self::Internal { .. } => "internal",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/debug/")]
pub enum DebugCallResult {
    Ok { outcome: DebugCallOutcome },
    Err { error: DebugError },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/debug/")]
pub struct DebugCallResponse {
    pub request_id: String,
    pub result: DebugCallResult,
}

// ---------------------------------------------------------------------------
// 流式调用（分块上传 / 下载）
// ---------------------------------------------------------------------------

/// 流式调用的进度信封版本
pub const DEBUG_STREAM_VERSION: u32 = 1;

/// 参数里「这个字符串值要用本机文件替换」的标记前缀，后面直接跟本机绝对路径。
/// 前后端各持一份同一字面量（前端在 `core/domain/debug/streamActions.ts`）
pub const LOCAL_FILE_TOKEN_PREFIX: &str = "ncd-local-file://";

/// 流式调用进行到哪一步了；界面按它决定进度的名目
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/debug/")]
pub enum DebugStreamStage {
    /// 读本机文件、算校验
    Reading,
    /// 分块往 Bot 一侧传
    Uploading,
    /// 从 Bot 一侧收分块
    Downloading,
    /// 传输完成，正在发起目标动作
    Calling,
}

/// 流式调用推给前端的一拍进度，经 `Channel<DebugStreamProgress>` 逐拍推，`v` 为
/// [`DEBUG_STREAM_VERSION`]。字节数测不出来时（上游先给信息帧才给总量）`total_*` 为空，
/// 前端此时只显示已传量，不算百分比
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/debug/")]
pub struct DebugStreamProgress {
    pub v: u32,
    pub request_id: String,
    pub stage: DebugStreamStage,
    /// 正在传的文件名（分块下载在收到信息帧之前是空串）
    pub file_name: String,
    #[ts(type = "number")]
    pub done_bytes: u64,
    #[ts(type = "number | null")]
    pub total_bytes: Option<u64>,
    pub done_chunks: u32,
    pub total_chunks: Option<u32>,
}

/// 一处要用本机文件替换的参数：`params` 里所有值为 `token` 的字符串都换成文件传到
/// Bot 一侧之后的路径。`path` 同时编进 `token`（[`LOCAL_FILE_TOKEN_PREFIX`] 前缀 + 路径原文），
/// 后端按前缀拆出来，不另存映射
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/debug/")]
pub struct DebugLocalFile {
    pub path: String,
    pub token: String,
}

/// 一次流式调用（`onebot_debug_call_stream`）。与 [`DebugCallRequest`] 拆开是因为
/// 它多带一批本机文件占位、中途有进度要推
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/debug/")]
pub struct DebugStreamCallRequest {
    /// 前端生成，用来取消和对账
    pub request_id: String,
    pub bot_id: String,
    pub channel: DebugChannelId,
    pub action: String,
    #[ts(type = "unknown")]
    pub params: serde_json::Value,
    /// 需要先用本机文件替换的参数标记；空就是纯传输 / 纯调用
    #[serde(default)]
    pub local_files: Vec<DebugLocalFile>,
    pub timeout_ms: Option<u32>,
    pub origin: DebugCallOrigin,
}

// ---------------------------------------------------------------------------
// 事件流
// ---------------------------------------------------------------------------

/// 事件流里的一条调用记录（我们自己发的和上游旁路看到的都走这个）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/debug/")]
pub struct DebugCallRecord {
    /// 旁路看到的别人发起的调用没有 request_id
    pub request_id: Option<String>,
    pub origin: DebugCallOrigin,
    pub action: String,
    #[ts(type = "unknown")]
    pub params: serde_json::Value,
    pub ok: Option<bool>,
    #[ts(type = "number | null")]
    pub retcode: Option<i64>,
    pub elapsed_ms: Option<u32>,
    #[ts(type = "number | null")]
    pub message_id: Option<i64>,
    pub error: Option<String>,
    pub channel: Option<DebugChannelId>,
}

/// 事件接收器的连接状态
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "state", rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/debug/")]
pub enum DebugReceiverState {
    Connecting,
    Connected,
    Reconnecting { attempt: u32, retry_in_ms: u32 },
    Stopped { reason: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/debug/")]
pub enum DebugEventBody {
    /// 上游推来的一条 OneBot 11 事件
    Ob11 {
        #[ts(type = "Record<string, unknown>")]
        payload: serde_json::Value,
    },
    Call {
        record: DebugCallRecord,
    },
    /// 断线期间漏掉的一段时间（Unix 毫秒）
    Gap {
        #[ts(type = "number")]
        from_ms: u64,
        #[ts(type = "number")]
        to_ms: u64,
    },
    /// 缓冲写满，丢了 `count` 条
    Dropped {
        count: u32,
    },
    Receiver {
        state: DebugReceiverState,
        source: DebugChannelId,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/debug/")]
pub struct DebugEvent {
    /// 同一个 Bot 的事件流内单调递增
    #[ts(type = "number")]
    pub seq: u64,
    /// 到达时间（Unix 毫秒）
    #[ts(type = "number")]
    pub at_ms: u64,
    pub body: DebugEventBody,
}

/// 推给前端的一批事件，`v` 为 `DEBUG_EVENT_VERSION`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/debug/")]
pub struct DebugEventBatch {
    pub v: u32,
    pub bot_id: String,
    pub events: Vec<DebugEvent>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/debug/")]
pub struct DebugReceiverInfo {
    pub bot_id: String,
    pub source: DebugChannelId,
    pub state: DebugReceiverState,
    pub buffered: u32,
    #[ts(type = "number")]
    pub dropped_total: u64,
    /// 缓冲里最早一条的 seq，前端据此判断补拉从哪开始
    #[ts(type = "number")]
    pub first_seq: u64,
    /// 正在看这个 Bot 事件流的窗口数
    pub viewers: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/debug/")]
pub struct DebugSubscribeResponse {
    pub subscription_id: String,
    pub receiver: DebugReceiverInfo,
}

// ---------------------------------------------------------------------------
// 落盘：工作区、收藏夹、历史
// ---------------------------------------------------------------------------

/// 一个编辑器标签里的草稿
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/debug/")]
pub struct DebugRequestDraft {
    pub id: String,
    pub action: String,
    /// 用户输入的原文，可能不是合法 JSON，所以不解析
    pub params_text: String,
    pub timeout_ms: Option<u32>,
    pub channel: Option<DebugChannelId>,
}

/// 某个 Bot 上一次选的调用通道和事件通道
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/debug/")]
pub struct DebugChannelChoice {
    pub call: DebugChannelId,
    pub events: DebugChannelId,
}

/// 右栏显示聊天视图还是事件列表
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/debug/")]
pub enum DebugChatView {
    Chat,
    List,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
// 容器级 default：落盘文件是老版本写的、缺新字段时，缺的字段取默认值，不至于整份读不出来
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/debug/")]
pub struct DebugLayout {
    pub left_collapsed: bool,
    pub right_collapsed: bool,
    pub left_width: u16,
    pub right_width: u16,
    pub right_view: DebugChatView,
}

impl Default for DebugLayout {
    fn default() -> Self {
        Self {
            left_collapsed: false,
            right_collapsed: false,
            left_width: 240,
            right_width: 380,
            right_view: DebugChatView::Chat,
        }
    }
}

/// 调试台的工作区：重开应用后回到离开时的样子
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/debug/")]
pub struct DebugWorkspace {
    pub version: u32,
    pub tabs: Vec<DebugRequestDraft>,
    pub active_tab: Option<String>,
    /// 最近关掉的 10 个标签，Ctrl+Shift+T 用
    pub closed_tabs: Vec<DebugRequestDraft>,
    pub selected_bot: Option<String>,
    /// 以 `bot_id` 为键
    pub channel_choice: BTreeMap<String, DebugChannelChoice>,
    pub layout: DebugLayout,
    /// 最近用过的 20 个动作名
    pub recent_actions: Vec<String>,
}

impl Default for DebugWorkspace {
    fn default() -> Self {
        Self {
            version: 1,
            tabs: Vec::new(),
            active_tab: None,
            closed_tabs: Vec::new(),
            selected_bot: None,
            channel_choice: BTreeMap::new(),
            layout: DebugLayout::default(),
            recent_actions: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/debug/")]
pub struct DebugSavedFolder {
    pub id: String,
    pub name: String,
    pub order: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/debug/")]
pub struct DebugSavedRequest {
    pub id: String,
    pub name: String,
    pub folder_id: Option<String>,
    pub action: String,
    #[ts(type = "unknown")]
    pub params: serde_json::Value,
    pub channel: Option<DebugChannelId>,
    pub note: Option<String>,
    pub order: i32,
    #[ts(type = "number")]
    pub created_at_ms: u64,
    #[ts(type = "number")]
    pub updated_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/debug/")]
pub struct DebugCollections {
    pub version: u32,
    pub folders: Vec<DebugSavedFolder>,
    pub requests: Vec<DebugSavedRequest>,
}

impl Default for DebugCollections {
    fn default() -> Self {
        Self {
            version: 1,
            folders: Vec::new(),
            requests: Vec::new(),
        }
    }
}

/// 历史里的一条完整记录，令牌不会出现在里面
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/debug/")]
pub struct DebugHistoryEntry {
    pub id: String,
    #[ts(type = "number")]
    pub at_ms: u64,
    pub bot_id: String,
    pub bot_name: String,
    pub backend: BackendType,
    pub channel: DebugChannelId,
    pub origin: DebugCallOrigin,
    pub action: String,
    #[ts(type = "unknown")]
    pub params: serde_json::Value,
    /// 参数存盘时被瘦身过（超长字符串换成占位文字，或整体收成摘要）：
    /// 前端据此拦住「把占位文字当参数重放出去」
    #[serde(default)]
    pub params_truncated: bool,
    pub ok: bool,
    #[ts(type = "number | null")]
    pub retcode: Option<i64>,
    pub error: Option<DebugError>,
    pub elapsed_ms: u32,
    #[ts(type = "unknown")]
    pub response: Option<serde_json::Value>,
    /// 回包太大，`response` 只留了前一段
    pub response_truncated: bool,
}

/// 历史列表里的一行，不带参数和回包
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/debug/")]
pub struct DebugHistorySummary {
    pub id: String,
    #[ts(type = "number")]
    pub at_ms: u64,
    pub bot_id: String,
    pub bot_name: String,
    pub backend: BackendType,
    pub channel: DebugChannelId,
    pub origin: DebugCallOrigin,
    pub action: String,
    pub ok: bool,
    #[ts(type = "number | null")]
    pub retcode: Option<i64>,
    pub elapsed_ms: u32,
    /// `DebugError::kind_str` 的结果
    pub error_kind: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/debug/")]
pub struct DebugHistoryQuery {
    pub action: Option<String>,
    pub bot_id: Option<String>,
    pub ok: Option<bool>,
    pub text: Option<String>,
    pub limit: u32,
    pub offset: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/debug/")]
pub struct DebugHistoryPage {
    pub entries: Vec<DebugHistorySummary>,
    pub total: u32,
}

/// 落盘文件损坏时，原文件被挪到别处，用这个告诉界面
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/debug/")]
pub struct DebugStorageNotice {
    pub file: String,
    pub moved_to: String,
    pub reason: String,
}

// ---------------------------------------------------------------------------
// 令牌打码
// ---------------------------------------------------------------------------

/// 短于这个字符数的令牌整个遮住：短令牌露出首尾 4 个字符，剩下的已经没几位可猜了
const MASK_REVEAL_MIN_CHARS: usize = 12;

/// 令牌的展示形式：空串没有可显示的，不足 12 个字符全遮，其余保留首尾各 2 个字符。
/// 按字符而不是字节切，令牌里混进非 ASCII 字符也不会 panic
pub fn mask_token(token: &str) -> Option<String> {
    let count = token.chars().count();
    if count == 0 {
        return None;
    }
    if count < MASK_REVEAL_MIN_CHARS {
        return Some("***".to_owned());
    }
    let head: String = token.chars().take(2).collect();
    let tail: String = token.chars().skip(count - 2).collect();
    Some(format!("{head}***{tail}"))
}

#[cfg(test)]
mod tests {
    use serde::de::DeserializeOwned;

    use super::*;

    fn round_trip<T>(value: &T) -> T
    where
        T: Serialize + DeserializeOwned,
    {
        let json = serde_json::to_string(value).unwrap();
        serde_json::from_str(&json).unwrap()
    }

    #[test]
    fn channel_id_uses_kind_tag() {
        let id = DebugChannelId::Http { name: "a".into() };
        assert_eq!(
            serde_json::to_string(&id).unwrap(),
            r#"{"kind":"http","name":"a"}"#
        );
        assert_eq!(round_trip(&id), id);
        assert_eq!(
            serde_json::to_string(&DebugChannelId::Auto).unwrap(),
            r#"{"kind":"auto"}"#
        );
    }

    #[test]
    fn call_result_err_nests_error_kind() {
        let result = DebugCallResult::Err {
            error: DebugError::Timeout { ms: 5 },
        };
        assert_eq!(
            serde_json::to_string(&result).unwrap(),
            r#"{"kind":"err","error":{"kind":"timeout","ms":5}}"#
        );
        assert_eq!(round_trip(&result), result);
    }

    #[test]
    fn receiver_state_uses_state_tag() {
        let state = DebugReceiverState::Reconnecting {
            attempt: 1,
            retry_in_ms: 1000,
        };
        assert_eq!(
            serde_json::to_string(&state).unwrap(),
            r#"{"state":"reconnecting","attempt":1,"retry_in_ms":1000}"#
        );
        assert_eq!(round_trip(&state), state);
    }

    #[test]
    fn mask_token_keeps_only_edges() {
        assert_eq!(mask_token(""), None);
        assert_eq!(mask_token("abc"), Some("***".to_owned()));
        assert_eq!(mask_token("abcdefgh"), Some("***".to_owned()));
        // 11 个字符仍全遮，12 个才露首尾
        assert_eq!(mask_token("abcdefghijk"), Some("***".to_owned()));
        assert_eq!(mask_token("abcdefghijkl"), Some("ab***kl".to_owned()));
        // 多字节字符按字符数算，不会切在字节中间
        assert_eq!(mask_token("令牌令牌令牌令牌"), Some("***".to_owned()));
        assert_eq!(
            mask_token("令牌令牌令牌令牌令牌令牌"),
            Some("令牌***令牌".to_owned())
        );
    }

    #[test]
    fn channel_label_key_is_stable() {
        assert_eq!(DebugChannelId::Auto.label_key(), "auto");
        assert_eq!(DebugChannelId::Internal.label_key(), "internal");
        assert_eq!(
            DebugChannelId::Http { name: "srv".into() }.label_key(),
            "http:srv"
        );
        assert_eq!(
            DebugChannelId::Ws { name: "ws1".into() }.label_key(),
            "ws:ws1"
        );
    }

    #[test]
    fn error_kind_str_matches_serialized_kind() {
        let errors = [
            DebugError::BotNotFound,
            DebugError::BotNotRunning,
            DebugError::NotLoggedIn,
            DebugError::ChannelUnavailable { reason: "x".into() },
            DebugError::UpstreamTooOld,
            DebugError::AuthFailed { status: 401 },
            DebugError::Timeout { ms: 1 },
            DebugError::Cancelled,
            DebugError::Transport {
                message: "x".into(),
            },
            DebugError::InvalidParams {
                message: "x".into(),
            },
            DebugError::FeatureDisabled,
            DebugError::Internal {
                message: "x".into(),
            },
        ];
        for error in errors {
            let value = serde_json::to_value(&error).unwrap();
            assert_eq!(value["kind"], error.kind_str());
        }
    }

    #[test]
    fn workspace_default_round_trips() {
        let workspace = DebugWorkspace::default();
        assert_eq!(workspace.version, 1);
        assert!(workspace.tabs.is_empty());
        assert_eq!(workspace.layout.left_width, 240);
        assert_eq!(workspace.layout.right_width, 380);
        assert_eq!(workspace.layout.right_view, DebugChatView::Chat);
        assert_eq!(round_trip(&workspace), workspace);
        assert_eq!(DebugCollections::default().version, 1);
        assert_eq!(
            round_trip(&DebugCollections::default()),
            DebugCollections::default()
        );
    }

    #[test]
    fn channel_id_works_as_map_key() {
        let mut counts = std::collections::HashMap::new();
        counts.insert(DebugChannelId::Ws { name: "a".into() }, 1);
        assert_eq!(
            counts.get(&DebugChannelId::Ws { name: "a".into() }),
            Some(&1)
        );
    }

    #[test]
    fn event_batch_round_trips_with_value_payload() {
        let batch = DebugEventBatch {
            v: DEBUG_EVENT_VERSION,
            bot_id: "1".into(),
            events: vec![DebugEvent {
                seq: 7,
                at_ms: 1_700_000_000_000,
                body: DebugEventBody::Ob11 {
                    payload: serde_json::json!({"post_type": "message", "message_id": 9}),
                },
            }],
        };
        assert_eq!(round_trip(&batch), batch);
    }

    #[test]
    fn stream_request_tolerates_missing_local_files() {
        // 老的前端（或手写的调用）不带 local_files 字段也要能反序列化
        let request: DebugStreamCallRequest = serde_json::from_str(
            r#"{
                "request_id": "r1", "bot_id": "1",
                "channel": {"kind": "internal"}, "action": "upload_file_stream",
                "params": {}, "timeout_ms": null, "origin": "editor"
            }"#,
        )
        .unwrap();
        assert!(request.local_files.is_empty());
    }

    #[test]
    fn stream_progress_round_trips_with_optional_totals() {
        let progress = DebugStreamProgress {
            v: DEBUG_STREAM_VERSION,
            request_id: "r1".into(),
            stage: DebugStreamStage::Downloading,
            file_name: "a.png".into(),
            done_bytes: 4096,
            total_bytes: None,
            done_chunks: 4,
            total_chunks: None,
        };
        let json = serde_json::to_string(&progress).unwrap();
        assert!(json.contains(r#""stage":"downloading""#));
        assert!(json.contains(r#""total_bytes":null"#));
        assert_eq!(round_trip(&progress), progress);

        let file = DebugLocalFile {
            path: "C:\\tmp\\a.png".into(),
            token: format!("{LOCAL_FILE_TOKEN_PREFIX}C:\\tmp\\a.png"),
        };
        assert!(file.token.starts_with(LOCAL_FILE_TOKEN_PREFIX));
        assert_eq!(round_trip(&file), file);
    }
}
