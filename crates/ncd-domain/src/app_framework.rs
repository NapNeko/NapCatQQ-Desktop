//! 应用端框架轴（AppFramework）— 与协议 BackendType 正交。
//!
//! 应用实例不是协议 Bot：无 QQ 扫码/登录态语义，不进 BackendType / BotFlavor。
//! 具体框架（Karin / NoneBot2 …）各自提供 manifest + Integration；本文件只放跨边界数据。

use serde::{Deserialize, Serialize};
use std::fmt;
use ts_rs::TS;

use crate::bot_config::{WebsocketClientConfig, WebsocketServerConfig};
use crate::deployment_task::AppStoreResource;
use crate::ids::BotId;
use crate::kinds::RuntimeTarget;

/// 应用端框架标识（稳定字符串，如 "karin" / "nonebot2"）。
/// 禁止与 BackendType 变体混用。
// 不加 `#[serde(transparent)]`：ts-rs 解析不了会报 warning，而 newtype 在 serde_json 下本就按裸字符串收发（有 round-trip 测试锁定）。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AppFrameworkId(String);

impl AppFrameworkId {
    pub fn new(value: impl Into<String>) -> Self {
        let s = value.into();
        debug_assert!(!s.is_empty(), "AppFrameworkId must not be empty");
        Self(s)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for AppFrameworkId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<&str> for AppFrameworkId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

/// 应用端实例 id（控制台侧，非 QQ 号）。
// 同 AppFrameworkId，不加 `serde(transparent)`。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AppInstanceId(String);

impl AppInstanceId {
    pub fn new(value: impl Into<String>) -> Self {
        let s = value.into();
        debug_assert!(!s.is_empty(), "AppInstanceId must not be empty");
        Self(s)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for AppInstanceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<&str> for AppInstanceId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

/// 应用端 placement（与协议运行矩阵分表）。
/// 首发已开: 本机 Native + 远端 Native；RemoteDocker 只声明，矩阵未开。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum AppPlacement {
    /// 本机原生进程
    LocalNative,
    /// 远端主机原生进程（SSH 安装 / 启停 / 写配置）
    RemoteNative,
    /// 远端主机 Docker（未开）
    RemoteDocker,
}

impl AppPlacement {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LocalNative => "local_native",
            Self::RemoteNative => "remote_native",
            Self::RemoteDocker => "remote_docker",
        }
    }

    /// 由 host_id 推断 Native placement：`local` → 本机，`remote:<id>` → 远端
    pub fn native_for_host(host_id: &str) -> Self {
        if host_id.starts_with(REMOTE_HOST_ID_PREFIX) {
            Self::RemoteNative
        } else {
            Self::LocalNative
        }
    }
}

/// 应用端实例生命周期（控制台子集，非协议登录态）。对接状态另见 `AppInstance.link`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum AppInstanceState {
    /// 组件未安装（或安装中途失败）
    NotInstalled,
    /// 安装任务排队/执行中
    Installing,
    /// 已安装未运行
    Installed,
    /// 进程运行中
    Running,
    /// 曾运行、现已停止
    Stopped,
}

impl AppInstanceState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotInstalled => "not_installed",
            Self::Installing => "installing",
            Self::Installed => "installed",
            Self::Running => "running",
            Self::Stopped => "stopped",
        }
    }

    pub const fn is_installed(self) -> bool {
        matches!(self, Self::Installed | Self::Running | Self::Stopped)
    }
}

/// 协议 Bot 与应用端怎么连。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum OneBotLinkMode {
    /// 协议 Bot 作 WS 客户端，连应用端的 WS 服务
    ReverseWs,
    /// 协议 Bot 开 WS 服务端，应用端作客户端连过来（MaiBot 的 NapCat 适配器插件只会这样）。
    /// 听口在 Bot 侧，跨机时隧道方向和反向相反：把 Bot 的口挂到应用端那台机的回环上
    ForwardWs,
}

impl OneBotLinkMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ReverseWs => "reverse_ws",
            Self::ForwardWs => "forward_ws",
        }
    }
}

/// 组件页 host_id 口径：本机 `local`，远端 `remote:<server_id>`。
pub const LOCAL_HOST_ID: &str = "local";
pub const REMOTE_HOST_ID_PREFIX: &str = "remote:";

/// 从 host_id 取 server_id；本机为 None
pub fn server_id_of_host(host_id: &str) -> Option<&str> {
    host_id.strip_prefix(REMOTE_HOST_ID_PREFIX)
}

/// 协议 Bot 的 runtime_target 与应用实例 host_id 是否同机。
pub fn runtime_target_matches_host(target: &RuntimeTarget, host_id: &str) -> bool {
    match server_id_of_host(host_id) {
        None => target.is_local(),
        Some(server_id) => target.server_id() == Some(server_id),
    }
}

/// 对接拓扑。不看 `BackendType`：NapCat / SnowLuma 都走同一份 `websocket_clients`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppLinkTopology {
    SameHost,
    /// 本机协议 Bot → 远端应用：Desktop SSH `-L`
    LocalBotRemoteApp,
    /// 远端协议 Bot → 本机应用：Desktop SSH `-R`
    RemoteBotLocalApp,
    /// 远端协议 Bot → 另一台远端应用：应用机常驻 `ssh -R`（Desktop 只编排）
    RemoteBotRemoteApp,
}

pub fn classify_app_link(bot_target: &RuntimeTarget, app_host_id: &str) -> Option<AppLinkTopology> {
    if runtime_target_matches_host(bot_target, app_host_id) {
        return Some(AppLinkTopology::SameHost);
    }
    if bot_target.is_local() && server_id_of_host(app_host_id).is_some() {
        return Some(AppLinkTopology::LocalBotRemoteApp);
    }
    if !bot_target.is_local() && server_id_of_host(app_host_id).is_none() {
        return Some(AppLinkTopology::RemoteBotLocalApp);
    }
    if !bot_target.is_local() && server_id_of_host(app_host_id).is_some() {
        return Some(AppLinkTopology::RemoteBotRemoteApp);
    }
    None
}

/// `RuntimeTarget` → 实例 `host_id`（`local` / `remote:<id>`）
pub fn host_id_of_runtime_target(target: &RuntimeTarget) -> String {
    match target.server_id() {
        Some(id) => format!("{REMOTE_HOST_ID_PREFIX}{id}"),
        None => LOCAL_HOST_ID.to_string(),
    }
}

/// 把 `ws://host:port/path` 改到 `127.0.0.1:<port>`；路径保留（Karin / NoneBot 各自的 /onebot/v11/ws）。
pub fn rewrite_ws_loopback_port(url: &str, local_port: u16) -> Result<String, String> {
    let (scheme, rest) = if let Some(rest) = url.strip_prefix("wss://") {
        ("wss", rest)
    } else if let Some(rest) = url.strip_prefix("ws://") {
        ("ws", rest)
    } else {
        return Err("对接地址不是 WebSocket URL".into());
    };
    let (path, _authority) = match rest.split_once('/') {
        Some((authority, path)) => (format!("/{path}"), authority),
        None => (String::new(), rest),
    };
    if local_port == 0 {
        return Err("隧道本地端口无效".into());
    }
    Ok(format!("{scheme}://127.0.0.1:{local_port}{path}"))
}

/// `ws://host:port/path` 的主机 / 口 / 路径；导入认领已有对接时用来对 Bot 连接表
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WsUrlParts {
    pub host: String,
    pub port: u16,
    pub path: String,
}

/// 必须带显式端口,避免 80/443 默认口误配
pub fn parse_ws_url(url: &str) -> Option<WsUrlParts> {
    let raw = url.trim();
    let rest = raw
        .strip_prefix("wss://")
        .or_else(|| raw.strip_prefix("ws://"))?;
    let (authority, path) = match rest.split_once('/') {
        Some((a, p)) => (a, format!("/{p}")),
        None => (rest, String::new()),
    };
    if authority.is_empty() {
        return None;
    }
    let (host, port) = if let Some(inner) = authority.strip_prefix('[') {
        let (host, after) = inner.split_once(']')?;
        let port = after.strip_prefix(':')?.parse().ok()?;
        (host.to_string(), port)
    } else {
        let (host, port) = authority.rsplit_once(':')?;
        if host.is_empty() {
            return None;
        }
        (host.to_string(), port.parse().ok()?)
    };
    if host.is_empty() || port == 0 {
        return None;
    }
    Some(WsUrlParts { host, port, path })
}

pub fn is_loopback_host(host: &str) -> bool {
    let h = host
        .trim()
        .trim_matches(|c| c == '[' || c == ']')
        .to_ascii_lowercase();
    matches!(
        h.as_str(),
        "127.0.0.1" | "localhost" | "::1" | "0.0.0.0" | "::"
    )
}

/// 写入协议 Bot 的连接名前缀；全名 `ncd-app:<instance_id>`，重复对接按名替换，解绑按名删。
pub const APP_LINK_CONNECTION_PREFIX: &str = "ncd-app:";

/// 导入时认到正向 WS (应用连 Bot 的 wsServers),Bot 侧没有 ncd-app 连接可摘
pub const APP_LINK_ADOPTED_FORWARD: &str = "ncd-adopt-forward";

pub fn app_link_connection_name(instance_id: &AppInstanceId) -> String {
    format!("{APP_LINK_CONNECTION_PREFIX}{}", instance_id.as_str())
}

/// 连接名是否由应用端对接写入
pub fn is_app_link_connection_name(name: &str) -> bool {
    name.starts_with(APP_LINK_CONNECTION_PREFIX)
}

/// 应用端框架清单（静态，由各框架模块给出；UI 直接消费，不做派生）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AppFrameworkManifest {
    pub id: AppFrameworkId,
    /// UI 显示名（"Karin" / "NoneBot2"）
    pub display_name: String,
    /// 一行简介
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub docs_url: Option<String>,
    /// 已开的 placement 子集
    pub supported_placements: Vec<AppPlacement>,
    /// 框架惯例端口（Karin 7777 / NoneBot 8080）。新建实例不再使用，只作文档兼容。
    #[ts(type = "number")]
    pub default_port: u16,
    /// 是否有可打开的 WebUI
    pub has_webui: bool,
    /// 支持的对接拓扑；首位为推荐
    pub link_modes: Vec<OneBotLinkMode>,
    /// 承载安装/探测的组件 wire 字面量（如 "karin"），与 ComponentId 的 serde 字面量一致
    pub component_id: String,
    /// 运行时依赖组件的 wire 字面量（如 "nodejs" / "uv"）；组件页据此展示「依赖是否就绪」。
    /// 真相仍是 Component::requirements()，注册表测试保证两者一致
    #[serde(default)]
    pub runtime_component_ids: Vec<String>,
    /// 商店 Tab 对应的资源；空 = 无商店。UI 按此挂 Tab，不写框架名。
    #[serde(default)]
    pub store_resources: Vec<AppStoreResource>,
    /// 新建实例是否展示「一并安装渲染器」
    #[serde(default)]
    pub has_install_renderer: bool,
    /// WebUI 登录方式；决定新建对话框是否收账号密码、打开时弹不弹账号框
    #[serde(default)]
    pub webui_auth: AppWebUiAuthKind,
    /// 上游要求用户同意的条款；非空时新建对话框必须勾选同意，启动前也会核对是否改过
    #[serde(default)]
    pub terms: Vec<AppTermsDoc>,
}

/// 一份上游条款（MaiBot 的 EULA / 隐私条款）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AppTermsDoc {
    /// 稳定 id（"eula" / "privacy"）
    pub id: String,
    pub title: String,
    /// 上游原文；新建时本地还没有文件，只能给链接
    pub url: String,
}

/// 已装实例里还没同意（或更新后改过）的一份条款，带实例目录里的原文
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AppPendingTerms {
    pub id: String,
    pub title: String,
    pub url: String,
    pub text: String,
}

/// WebUI 怎么登录。Karin 是单个 `HTTP_AUTH_KEY`；AstrBot 是用户名 + 密码（落盘只有哈希）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum AppWebUiAuthKind {
    #[default]
    None,
    Key,
    UserPassword,
}

/// 用户名密码类 WebUI 的账号视图（打开 WebUI 时弹给用户）。
/// 密码只有桌面端自己设过才知道；导入的实例或用户在 WebUI 改过后就只剩用户名。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AppWebUiAccount {
    pub username: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub password: Option<String>,
    /// 记住的密码与落盘哈希是否仍一致；None = 没记密码或落盘没有哈希（首启才生成）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub password_matches: Option<bool>,
    /// 实例已停止时才允许桌面端重置密码（运行中改文件会被进程覆盖）
    pub can_reset: bool,
}

/// 对接时将改动的应用端文件（预览用，只描述不带内容）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AppConfigWrite {
    /// 应用端文件相对实例目录的路径（如 ".env"）
    pub path: String,
    /// 人话摘要（如 "写入 WS_SERVER_AUTH_KEY / HTTP_PORT=7777"）
    pub summary: String,
}

/// 对接时往协议 Bot 里写的那一条连接，name 固定 `ncd-app:<instance_id>`。
/// 反向进 `connect.websocketClients`，正向进 `connect.websocketServers`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum OneBotLinkEndpoint {
    WsClient(WebsocketClientConfig),
    WsServer(WebsocketServerConfig),
}

impl OneBotLinkEndpoint {
    pub fn mode(&self) -> OneBotLinkMode {
        match self {
            Self::WsClient(_) => OneBotLinkMode::ReverseWs,
            Self::WsServer(_) => OneBotLinkMode::ForwardWs,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Self::WsClient(c) => &c.base.name,
            Self::WsServer(s) => &s.base.name,
        }
    }

    pub fn token(&self) -> &str {
        match self {
            Self::WsClient(c) => &c.base.token,
            Self::WsServer(s) => &s.base.token,
        }
    }

    pub fn as_ws_client(&self) -> Option<&WebsocketClientConfig> {
        match self {
            Self::WsClient(c) => Some(c),
            Self::WsServer(_) => None,
        }
    }

    pub fn as_ws_server(&self) -> Option<&WebsocketServerConfig> {
        match self {
            Self::WsServer(s) => Some(s),
            Self::WsClient(_) => None,
        }
    }
}

/// 对接计划：预览与应用共用同一份，UI 展示「将写入 Bot 的连接」与「将改动的应用端文件」。
/// `mode` 与 `connection` 的种类一致（反向 = WsClient，正向 = WsServer），由各框架 `plan_link` 保证
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct OneBotLinkPlan {
    pub mode: OneBotLinkMode,
    pub instance_id: AppInstanceId,
    #[ts(type = "string")]
    pub bot_id: BotId,
    pub connection: OneBotLinkEndpoint,
    pub app_side_writes: Vec<AppConfigWrite>,
    pub access_token: String,
}

/// 已生效的对接记录（落在 AppInstance 上）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AppLinkRecord {
    #[ts(type = "string")]
    pub bot_id: BotId,
    pub mode: OneBotLinkMode,
    pub connection_name: String,
    #[ts(type = "number")]
    pub linked_at_ms: u64,
    /// 两台远端常驻隧道的 loopback 听口：反向开在 Bot 机上（Bot 连它），正向开在应用机上（应用端连它）；
    /// 桌面端握着的隧道 / 同机为 None
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub resident_forward_port: Option<u16>,
}

/// 应用端实例快照（控制台列表/状态用；持久化在 data_root/config/app-instances.json）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AppInstance {
    pub id: AppInstanceId,
    pub framework_id: AppFrameworkId,
    pub display_name: String,
    pub placement: AppPlacement,
    /// `local` / `remote:<server_id>`
    pub host_id: String,
    /// 实例目录（HostPath POSIX 字面量）
    pub install_dir: String,
    /// 应用端监听端口（反向 WS / WebUI 共用）
    #[ts(type = "number")]
    pub port: u16,
    pub state: AppInstanceState,
    /// 已对接的协议 Bot；未对接为 None
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub link: Option<AppLinkRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub installed_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub last_error: Option<String>,
    #[serde(default)]
    #[ts(type = "number")]
    pub created_at_ms: u64,
    /// Karin：创建时是否一并装 `@karinjs/plugin-puppeteer`。旧快照缺字段视为 true。
    #[serde(default = "default_true")]
    pub install_renderer: bool,
    /// 桌面端脚手架新建 vs 领养已有目录。旧快照缺字段视为 Created。
    #[serde(default)]
    pub origin: AppInstanceOrigin,
    /// 开机/桌面端启动时自动启动该实例。旧快照缺字段视为 true（兼容语义：已安装即启用）。
    #[serde(default = "default_true")]
    pub auto_start: bool,
}

fn default_true() -> bool {
    true
}

/// 实例从哪来。导入的目录禁止脚手架重写，删除默认不拆项目。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum AppInstanceOrigin {
    #[default]
    Created,
    Imported,
}

impl AppInstanceOrigin {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Imported => "imported",
        }
    }

    pub const fn is_imported(self) -> bool {
        matches!(self, Self::Imported)
    }
}

impl AppInstance {
    pub fn server_id(&self) -> Option<&str> {
        server_id_of_host(&self.host_id)
    }

    pub fn is_local(&self) -> bool {
        self.server_id().is_none()
    }
}

/// 新建实例请求（UI → command → 编排）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct CreateAppInstanceRequest {
    pub framework_id: AppFrameworkId,
    pub host_id: String,
    pub display_name: String,
    /// None 则随机分配高位端口，并避开同机已有实例（本机再探 bind）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub port: Option<u16>,
    /// 实例根（HostPath POSIX 或本机 Windows 路径，由编排层规范化）。None = 默认根。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub install_dir: Option<String>,
    /// 仅 Karin 有意义；None 视为 true。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub install_renderer: Option<bool>,
    /// 仅 `webui_auth = user_password` 的框架有意义；None 用框架默认用户名
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub webui_username: Option<String>,
    /// None / 空 = 桌面端按框架口令策略随机生成
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub webui_password: Option<String>,
    /// 开机/桌面端启动时自动启动该实例。**默认 false**：新建的实例不随桌面端启动。
    ///
    /// 与 `AppInstance::auto_start` 的 default_true 不冲突：那个是为了旧快照的兼容
    /// （字段出现之前建的实例按「已安装即启用」处理），这里说的是**新建时**取什么值。
    /// 想让它跟着桌面端起，在详情页把那个开关打开即可。
    #[serde(default)]
    pub auto_start: bool,
    /// 用户已勾选同意 manifest 里的上游条款；框架没有条款时忽略
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub accept_terms: Option<bool>,
}

/// 「原始文件」里展示的 **Bot 侧** 配置文件（只读）。
///
/// 与应用端自己的 AppConfigDocument 分开：那些相对实例目录读写、可编辑；
/// 这些在**协议 Bot 的配置目录**里（对接时 Desktop 写进去的 WS 客户端连接），
/// 所属主机与路径根都不一样，混进同一套「相对路径」语义只会出错，所以单独一类。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AppLinkBotDocument {
    /// 文件名，例如 onebot11_10001.json
    pub name: String,
    /// 绝对路径（给人看，显示在标题旁）
    pub path: String,
    /// 文件内容；文件还没生成时为 None
    pub text: Option<String>,
}

/// 探测已有项目目录（导入前）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AppProjectProbe {
    pub framework_id: AppFrameworkId,
    pub path: String,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub version: Option<String>,
    /// 类型化配置实际读写的相对路径（NoneBot：`.env` 或 `.env.{ENVIRONMENT}`）
    pub env_rel_path: String,
    pub environment: String,
    /// 现在就能启动（依赖已同步）
    pub ready: bool,
    /// 目录里已有该框架进程
    pub running: bool,
    /// 将停用的 systemd 单元等；导入后由桌面端接管启停
    #[serde(default)]
    pub supervisors: Vec<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
    /// 已有协议 Bot 的反向 / 正向 WS 能唯一对上时填 QQ 号
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "string")]
    pub detected_bot_id: Option<BotId>,
}

/// 领养已有项目（UI → command → 编排）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct ImportAppInstanceRequest {
    pub framework_id: AppFrameworkId,
    pub host_id: String,
    /// 项目根（HostPath POSIX 或本机 Windows 路径）
    pub path: String,
    pub display_name: String,
}

/// 配置文件格式（原始文件 Tab 决定预检与高亮）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum AppConfigFormat {
    Json,
    DotEnv,
    Toml,
    Yaml,
}

/// 应用端实例的一个可编辑配置文件（框架适配器声明；原始文件 Tab 与类型化读写共用）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AppConfigDocument {
    /// 稳定 id（如 "env" / "adapter"）
    pub id: String,
    /// UI 显示名
    pub label: String,
    /// 相对实例目录的路径（POSIX 写法）
    pub rel_path: String,
    pub format: AppConfigFormat,
    /// 应用端自己会热加载该文件；false 表示改完要重启实例
    pub hot_reload: bool,
}

/// 配置文件原文 + 版本号（版本号 = 内容哈希；写回需带 base_revision 防覆盖）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AppConfigText {
    pub doc_id: String,
    pub text: String,
    /// 文件不存在时为 `"missing"`
    pub revision: String,
}

/// 插件配置表单里一个字段的类型。`Json` 是兜底：框架 schema 里桌面端不认的类型按 JSON 源码编辑。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum AppPluginConfigFieldKind {
    String,
    Text,
    Int,
    Float,
    Bool,
    List,
    Object,
    Json,
}

/// 插件配置表单字段（由框架 schema 翻译；默认值不过 IPC，缺文件时由后端按 schema 物化到文档）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AppPluginConfigField {
    pub key: String,
    pub kind: AppPluginConfigFieldKind,
    pub label: String,
    #[serde(default)]
    pub hint: String,
    #[serde(default)]
    pub obvious_hint: bool,
    #[serde(default)]
    pub secret: bool,
    #[serde(default)]
    pub options: Vec<String>,
    /// `Object` 的子字段
    #[serde(default)]
    pub items: Vec<AppPluginConfigField>,
}

/// 插件配置表单：绑定到哪份文档 + 字段树。None 表示该插件只能改原文。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AppPluginConfigSchema {
    pub doc_id: String,
    pub fields: Vec<AppPluginConfigField>,
}

/// 校验问题（`path` 用 JSON pointer 风格定位到字段，如 "env/http_port" / "groups/2/mode"）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AppConfigIssue {
    pub path: String,
    pub message: String,
}

impl AppConfigIssue {
    pub fn new(path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            message: message.into(),
        }
    }
}

/// 打开应用端 WebUI 时返回给前端的入口。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AppInstanceWebUi {
    pub url: String,
    /// Karin `HTTP_AUTH_KEY`；空则前端不写剪贴板。
    pub auth_key: String,
    /// 用户名密码类 WebUI（AstrBot）的账号；None = 该框架不是账号密码登录
    #[ts(optional)]
    pub account: Option<AppWebUiAccount>,
}

/// 配置读写命令的结构化错误：前端按 `kind` 分流（冲突 → 重载/覆盖对话框；校验 → 定位字段）。
/// 其它命令仍返回 String，这里只在需要分流的地方升级。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum AppConfigErrorKind {
    Conflict,
    Invalid,
    Unsupported,
    NotRunning,
    Auth,
    Unreachable,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AppConfigError {
    pub kind: AppConfigErrorKind,
    pub message: String,
    pub issues: Vec<AppConfigIssue>,
}

/// 从协议 Bot 导出的 OneBot HTTP 出口（正向 HTTP 对接的输入；反向 WS 走 OneBotLinkPlan）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct OneBotEndpointExport {
    /// 协议侧 Bot id（通常 QQ 号字符串）
    #[ts(type = "string")]
    pub bot_id: BotId,
    /// 协议后端展示名 napcat / snowluma（不是 AppFrameworkId）
    pub protocol_backend: String,
    /// HTTP 根地址，如 http://127.0.0.1:3000
    pub base_url: String,
    pub access_token: String,
    pub host: String,
    #[ts(type = "number")]
    pub port: u16,
    /// 协议 Bot 的 runtime_target（本机 / 某 server）
    #[ts(type = "string")]
    pub runtime_target: RuntimeTarget,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bot_config::BackendType;

    #[test]
    fn backend_type_is_protocol_only_not_app_framework() {
        // 应用端不得塞进 BackendType；变体集合即协议轴。
        let variants = [BackendType::NapCat, BackendType::SnowLuma];
        assert_eq!(variants.len(), 2);
        let _ = AppFrameworkId::new("karin");
        let _ = AppFrameworkId::new("nonebot2");
    }

    #[test]
    fn app_placement_wire_literals() {
        assert_eq!(AppPlacement::LocalNative.as_str(), "local_native");
        assert_eq!(AppPlacement::RemoteNative.as_str(), "remote_native");
        assert_eq!(AppPlacement::RemoteDocker.as_str(), "remote_docker");
        assert_eq!(
            serde_json::to_value(AppPlacement::RemoteNative).unwrap(),
            "remote_native"
        );
        assert_eq!(
            AppPlacement::native_for_host("local"),
            AppPlacement::LocalNative
        );
        assert_eq!(
            AppPlacement::native_for_host("remote:srv-1"),
            AppPlacement::RemoteNative
        );
    }

    #[test]
    fn id_newtypes_serialize_as_bare_json_strings() {
        // 无 serde(transparent) 也必须是裸字符串，否则前端 `type X = string` 绑定失真。
        let framework = AppFrameworkId::new("karin");
        assert_eq!(serde_json::to_value(&framework).unwrap(), "karin");
        assert_eq!(
            serde_json::from_str::<AppFrameworkId>("\"karin\"").unwrap(),
            framework
        );

        let instance = AppInstanceId::new("app-1");
        assert_eq!(serde_json::to_value(&instance).unwrap(), "app-1");
        assert_eq!(
            serde_json::from_str::<AppInstanceId>("\"app-1\"").unwrap(),
            instance
        );
    }

    #[test]
    fn link_connection_name_is_prefixed_and_recognizable() {
        let name = app_link_connection_name(&AppInstanceId::new("k1"));
        assert_eq!(name, "ncd-app:k1");
        assert!(is_app_link_connection_name(&name));
        assert!(!is_app_link_connection_name("my-ws"));
    }

    #[test]
    fn same_host_rule_matches_local_and_server() {
        assert!(runtime_target_matches_host(&RuntimeTarget::Local, "local"));
        assert!(!runtime_target_matches_host(
            &RuntimeTarget::Local,
            "remote:a"
        ));
        let srv = RuntimeTarget::server("a");
        assert!(runtime_target_matches_host(&srv, "remote:a"));
        assert!(!runtime_target_matches_host(&srv, "remote:b"));
        assert!(!runtime_target_matches_host(&srv, "local"));
    }

    #[test]
    fn classify_allows_local_bot_to_remote_app() {
        assert_eq!(
            classify_app_link(&RuntimeTarget::Local, "local"),
            Some(AppLinkTopology::SameHost)
        );
        assert_eq!(
            classify_app_link(&RuntimeTarget::Local, "remote:vps"),
            Some(AppLinkTopology::LocalBotRemoteApp)
        );
        assert_eq!(
            classify_app_link(&RuntimeTarget::server("vps"), "local"),
            Some(AppLinkTopology::RemoteBotLocalApp)
        );
        assert_eq!(
            classify_app_link(&RuntimeTarget::server("a"), "remote:b"),
            Some(AppLinkTopology::RemoteBotRemoteApp)
        );
    }

    #[test]
    fn classify_same_remote_host_is_not_p2() {
        assert_eq!(
            classify_app_link(&RuntimeTarget::server("vps"), "remote:vps"),
            Some(AppLinkTopology::SameHost)
        );
    }

    #[test]
    fn parse_ws_url_requires_explicit_port() {
        let p = parse_ws_url("ws://127.0.0.1:3001/onebot/v11/ws").unwrap();
        assert_eq!(p.host, "127.0.0.1");
        assert_eq!(p.port, 3001);
        assert_eq!(p.path, "/onebot/v11/ws");
        assert_eq!(parse_ws_url("ws://127.0.0.1/onebot").map(|p| p.port), None);
        assert_eq!(
            parse_ws_url("ws://[::1]:8080/onebot/v11/ws").unwrap(),
            WsUrlParts {
                host: "::1".into(),
                port: 8080,
                path: "/onebot/v11/ws".into(),
            }
        );
        assert!(is_loopback_host("127.0.0.1"));
        assert!(is_loopback_host("LOCALHOST"));
        assert!(is_loopback_host("[::1]"));
        assert!(!is_loopback_host("160.30.231.138"));
    }

    #[test]
    fn rewrite_ws_keeps_framework_path() {
        assert_eq!(
            rewrite_ws_loopback_port("ws://127.0.0.1:32100/onebot/v11/ws", 47011).unwrap(),
            "ws://127.0.0.1:47011/onebot/v11/ws"
        );
        assert_eq!(
            rewrite_ws_loopback_port("ws://127.0.0.1:8080/", 9).unwrap(),
            "ws://127.0.0.1:9/"
        );
    }

    #[test]
    fn app_instance_round_trips_and_omits_empty_optionals() {
        let inst = AppInstance {
            id: AppInstanceId::new("k1"),
            framework_id: AppFrameworkId::new("karin"),
            display_name: "Karin 主实例".into(),
            placement: AppPlacement::LocalNative,
            host_id: "local".into(),
            install_dir: "/c/ProgramData/NapCatQQ Desktop/apps/karin/k1".into(),
            port: 7777,
            state: AppInstanceState::Installed,
            link: None,
            installed_version: Some("1.17.0".into()),
            last_error: None,
            created_at_ms: 1,
            install_renderer: true,
            origin: AppInstanceOrigin::Created,
            auto_start: true,
        };
        let json = serde_json::to_string(&inst).unwrap();
        assert!(!json.contains("\"link\""));
        assert!(!json.contains("last_error"));
        let back: AppInstance = serde_json::from_str(&json).unwrap();
        assert_eq!(back, inst);
        assert!(back.is_local());
        assert_eq!(back.server_id(), None);
    }

    #[test]
    fn link_record_omits_resident_port_and_accepts_legacy_json() {
        let rec = AppLinkRecord {
            bot_id: BotId::new("10001"),
            mode: OneBotLinkMode::ReverseWs,
            connection_name: "ncd-app:k1".into(),
            linked_at_ms: 1,
            resident_forward_port: None,
        };
        let json = serde_json::to_string(&rec).unwrap();
        assert!(!json.contains("resident_forward_port"));
        let legacy: AppLinkRecord = serde_json::from_str(
            r#"{"bot_id":"10001","mode":"reverse_ws","connection_name":"ncd-app:k1","linked_at_ms":1}"#,
        )
        .unwrap();
        assert_eq!(legacy, rec);
        let with_port = AppLinkRecord {
            resident_forward_port: Some(21001),
            ..rec.clone()
        };
        let v = serde_json::to_value(&with_port).unwrap();
        assert_eq!(v["resident_forward_port"], 21001);
    }

    #[test]
    fn forward_link_record_round_trips_next_to_legacy_reverse() {
        assert_eq!(OneBotLinkMode::ForwardWs.as_str(), "forward_ws");
        let rec: AppLinkRecord = serde_json::from_str(
            r#"{"bot_id":"10001","mode":"forward_ws","connection_name":"ncd-app:m1","linked_at_ms":1}"#,
        )
        .unwrap();
        assert_eq!(rec.mode, OneBotLinkMode::ForwardWs);
        assert_eq!(serde_json::to_value(rec.mode).unwrap(), "forward_ws");
    }

    fn base(name: &str, token: &str) -> crate::bot_config::NetworkBaseFields {
        crate::bot_config::NetworkBaseFields {
            enable: true,
            name: name.into(),
            message_post_format: crate::bot_config::MessagePostFormat::Array,
            token: token.into(),
            debug: false,
        }
    }

    #[test]
    fn link_endpoint_is_tagged_by_kind_and_keeps_wire_fields_flat() {
        let server = OneBotLinkEndpoint::WsServer(WebsocketServerConfig {
            base: base("ncd-app:m1", "tok"),
            host: "127.0.0.1".into(),
            port: 23001,
            report_self_message: false,
            enable_force_push_event: true,
            heart_interval: 30000,
            path: "/".into(),
            role: Default::default(),
        });
        let v = serde_json::to_value(&server).unwrap();
        assert_eq!(v["kind"], "ws_server");
        assert_eq!(v["name"], "ncd-app:m1");
        assert_eq!(v["port"], 23001);
        assert_eq!(v["heartInterval"], 30000);
        let back: OneBotLinkEndpoint = serde_json::from_value(v).unwrap();
        assert_eq!(back, server);
        assert_eq!(back.mode(), OneBotLinkMode::ForwardWs);
        assert_eq!(back.name(), "ncd-app:m1");
        assert_eq!(back.token(), "tok");
        assert!(back.as_ws_client().is_none());

        let client = OneBotLinkEndpoint::WsClient(WebsocketClientConfig {
            base: base("ncd-app:k1", "t2"),
            url: "ws://127.0.0.1:7777/onebot/v11/ws".into(),
            report_self_message: false,
            heart_interval: 30000,
            reconnect_interval: 30000,
            role: Default::default(),
        });
        let v = serde_json::to_value(&client).unwrap();
        assert_eq!(v["kind"], "ws_client");
        assert_eq!(v["url"], "ws://127.0.0.1:7777/onebot/v11/ws");
        let back: OneBotLinkEndpoint = serde_json::from_value(v).unwrap();
        assert_eq!(back.mode(), OneBotLinkMode::ReverseWs);
        assert_eq!(
            back.as_ws_client().map(|c| c.url.as_str()),
            Some("ws://127.0.0.1:7777/onebot/v11/ws")
        );
    }

    #[test]
    fn create_request_optional_dir_and_renderer_round_trip() {
        let req = CreateAppInstanceRequest {
            framework_id: AppFrameworkId::new("karin"),
            host_id: "local".into(),
            display_name: "k".into(),
            port: None,
            install_dir: Some("/d/bots/karin-main".into()),
            install_renderer: Some(false),
            webui_username: None,
            webui_password: None,
            auto_start: true,
            accept_terms: None,
        };
        let v = serde_json::to_value(&req).unwrap();
        assert_eq!(v["install_dir"], "/d/bots/karin-main");
        assert_eq!(v["install_renderer"], false);
        assert!(v.get("webui_password").is_none());
        assert!(v.get("accept_terms").is_none(), "旧前端不发这个字段也能解");
        let back: CreateAppInstanceRequest = serde_json::from_value(v).unwrap();
        assert_eq!(back, req);
    }

    #[test]
    fn app_instance_missing_install_renderer_defaults_true() {
        let json = r#"{
            "id":"k1","framework_id":"karin","display_name":"K",
            "placement":"local_native","host_id":"local",
            "install_dir":"/c/apps/k1","port":7777,"state":"installed",
            "created_at_ms":1
        }"#;
        let inst: AppInstance = serde_json::from_str(json).unwrap();
        assert!(inst.install_renderer);
        assert_eq!(inst.origin, AppInstanceOrigin::Created);
        assert!(inst.auto_start); // 旧 JSON 缺字段回落到 default_true
    }

    /// 新建实例默认**不**随桌面端启动。与上面那条不冲突：那条说的是旧快照缺字段
    /// 时的兼容回落（视为 true），这条说的是新建时取什么值。
    #[test]
    fn create_request_missing_auto_start_defaults_false() {
        let json = r#"{
            "framework_id":"neobot","display_name":"Luna","host_id":"local",
            "install_dir":"/c/apps/luna"
        }"#;
        let req: CreateAppInstanceRequest = serde_json::from_str(json).unwrap();
        assert!(!req.auto_start, "新建时不该默认跟着桌面端启动");
    }

    #[test]
    fn import_request_round_trips() {
        let req = ImportAppInstanceRequest {
            framework_id: AppFrameworkId::new("nonebot2"),
            host_id: "remote:s1".into(),
            path: "/root/game-qqbot/bot-xiuxian".into(),
            display_name: "荒境修仙".into(),
        };
        let v = serde_json::to_value(&req).unwrap();
        let back: ImportAppInstanceRequest = serde_json::from_value(v).unwrap();
        assert_eq!(back, req);
    }

    #[test]
    fn onebot_export_round_trips_json() {
        let export = OneBotEndpointExport {
            bot_id: BotId::new("10001"),
            protocol_backend: "napcat".into(),
            base_url: "http://127.0.0.1:3000".into(),
            access_token: "t".into(),
            host: "127.0.0.1".into(),
            port: 3000,
            runtime_target: RuntimeTarget::Local,
        };
        let v = serde_json::to_value(&export).unwrap();
        let back: OneBotEndpointExport = serde_json::from_value(v).unwrap();
        assert_eq!(back.port, 3000);
        assert_eq!(back.bot_id.as_str(), "10001");
    }
}
