use std::collections::HashMap;

use serde::de;
use serde::{Deserialize, Deserializer, Serialize};
use ts_rs::TS;

use crate::kinds::RuntimeTarget;
use crate::snowluma_start_mode::SnowLumaStartMode;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum BackendType {
    #[default]
    NapCat,
    SnowLuma,
}

/// Bot 的启动方式:直接在主机上跑原生进程,还是用 Docker 容器跑
///
/// 与 RuntimeTarget(在哪台主机)正交:Native/Docker 决定"怎么跑",
/// runtime_target 决定"在哪跑"两者组合,比如 Server(id)+Docker =
/// 用 SSH 在远端跑 docker compose
///
/// 默认 Native 保证旧配置(无此字段)反序列化后行为不变
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum DeploymentType {
    /// 直接在主机上 spawn 原生进程(现有默认路径)
    #[default]
    Native,
    /// 用 docker compose 起容器
    Docker,
}

impl DeploymentType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Native => "native",
            Self::Docker => "docker",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum TimeUnit {
    #[serde(rename = "m")]
    Minute,
    #[serde(rename = "h")]
    #[default]
    Hour,
    #[serde(rename = "d")]
    Day,
    #[serde(rename = "mon")]
    Month,
    #[serde(rename = "year")]
    Year,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum LogLevel {
    Debug,
    Info,
    Error,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum WsRole {
    Api,
    Event,
    #[default]
    Universal,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum MessagePostFormat {
    #[default]
    Array,
    String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
#[ts(as = "u8")]
pub enum O3HookMode {
    Off,
    #[default]
    On,
}

impl From<O3HookMode> for u8 {
    fn from(value: O3HookMode) -> Self {
        match value {
            O3HookMode::Off => 0,
            O3HookMode::On => 1,
        }
    }
}

impl TryFrom<u8> for O3HookMode {
    type Error = String;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Off),
            1 => Ok(Self::On),
            other => Err(format!("invalid o3HookMode: {other}")),
        }
    }
}

impl Serialize for O3HookMode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_u8(u8::from(*self))
    }
}

impl<'de> Deserialize<'de> for O3HookMode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = u8::deserialize(deserializer)?;
        Self::try_from(value).map_err(de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AutoRestartSchedule {
    #[serde(default)]
    pub enable: bool,
    #[serde(default)]
    pub time_unit: TimeUnit,
    #[serde(default = "default_auto_restart_duration")]
    pub duration: u32,
}

impl Default for AutoRestartSchedule {
    fn default() -> Self {
        Self {
            enable: false,
            time_unit: TimeUnit::Hour,
            duration: default_auto_restart_duration(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct BypassConfig {
    #[serde(default)]
    pub hook: bool,
    #[serde(default)]
    pub window: bool,
    #[serde(default)]
    pub module: bool,
    #[serde(default)]
    pub process: bool,
    #[serde(default)]
    pub container: bool,
    #[serde(default)]
    pub js: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct BotBasicConfig {
    pub name: String,
    #[serde(rename = "QQID")]
    #[ts(type = "number")]
    pub qq_id: u64,
    #[serde(default)]
    #[serde(rename = "musicSignUrl")]
    pub music_sign_url: String,
    #[serde(default)]
    #[serde(rename = "autoRestartSchedule")]
    pub auto_restart_schedule: AutoRestartSchedule,
    #[serde(default)]
    #[serde(rename = "offlineAutoRestart")]
    pub offline_auto_restart: bool,
    #[serde(default = "default_runtime_target")]
    #[ts(type = "string")]
    pub runtime_target: RuntimeTarget,
    #[serde(default)]
    pub backend_type: BackendType,
    #[serde(default, rename = "deploymentType")]
    #[ts(rename = "deploymentType")]
    pub deployment_type: DeploymentType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(rename = "snowlumaStartMode")]
    #[ts(optional, rename = "snowlumaStartMode")]
    pub snowluma_start_mode: Option<SnowLumaStartMode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct NetworkBaseFields {
    #[serde(default = "default_true")]
    pub enable: bool,
    pub name: String,
    #[serde(default)]
    #[serde(rename = "messagePostFormat")]
    pub message_post_format: MessagePostFormat,
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub debug: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct HttpServerConfig {
    #[serde(flatten)]
    pub base: NetworkBaseFields,
    pub host: String,
    pub port: u16,
    #[serde(default)]
    #[serde(rename = "enableCors")]
    pub enable_cors: bool,
    #[serde(default)]
    #[serde(rename = "enableWebsocket")]
    pub enable_websocket: bool,
    #[serde(default = "default_root_path")]
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct HttpSseServerConfig {
    #[serde(flatten)]
    pub base: NetworkBaseFields,
    pub host: String,
    pub port: u16,
    #[serde(default)]
    #[serde(rename = "enableCors")]
    pub enable_cors: bool,
    #[serde(default)]
    #[serde(rename = "enableWebsocket")]
    pub enable_websocket: bool,
    #[serde(default)]
    #[serde(rename = "reportSelfMessage")]
    pub report_self_message: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct HttpClientConfig {
    #[serde(flatten)]
    pub base: NetworkBaseFields,
    pub url: String,
    #[serde(default)]
    #[serde(rename = "reportSelfMessage")]
    pub report_self_message: bool,
    #[serde(default)]
    #[serde(rename = "timeoutMs")]
    #[ts(optional)]
    pub timeout_ms: Option<u32>,
}

impl Serialize for HttpClientConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;
        let field_count = 7 + usize::from(self.timeout_ms.is_some());
        let mut map = serializer.serialize_map(Some(field_count))?;
        // flatten base
        map.serialize_entry("enable", &self.base.enable)?;
        map.serialize_entry("name", &self.base.name)?;
        map.serialize_entry("messagePostFormat", &self.base.message_post_format)?;
        map.serialize_entry("token", &self.base.token)?;
        map.serialize_entry("debug", &self.base.debug)?;
        // own fields
        map.serialize_entry("url", &self.url)?;
        map.serialize_entry("reportSelfMessage", &self.report_self_message)?;
        if let Some(timeout) = self.timeout_ms {
            map.serialize_entry("timeoutMs", &timeout)?;
        }
        map.end()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct WebsocketServerConfig {
    #[serde(flatten)]
    pub base: NetworkBaseFields,
    pub host: String,
    pub port: u16,
    #[serde(default)]
    #[serde(rename = "reportSelfMessage")]
    pub report_self_message: bool,
    #[serde(default)]
    #[serde(rename = "enableForcePushEvent")]
    pub enable_force_push_event: bool,
    #[serde(default = "default_heart_interval")]
    #[serde(rename = "heartInterval")]
    pub heart_interval: u32,
    #[serde(default = "default_root_path")]
    pub path: String,
    #[serde(default)]
    pub role: WsRole,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct WebsocketClientConfig {
    #[serde(flatten)]
    pub base: NetworkBaseFields,
    pub url: String,
    #[serde(default)]
    #[serde(rename = "reportSelfMessage")]
    pub report_self_message: bool,
    #[serde(default = "default_heart_interval")]
    #[serde(rename = "heartInterval")]
    pub heart_interval: u32,
    #[serde(default = "default_reconnect_interval")]
    #[serde(rename = "reconnectInterval")]
    pub reconnect_interval: u32,
    #[serde(default)]
    pub role: WsRole,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct ConnectConfig {
    #[serde(default)]
    #[serde(rename = "httpServers")]
    pub http_servers: Vec<HttpServerConfig>,
    #[serde(default)]
    #[serde(rename = "httpSseServers")]
    pub http_sse_servers: Vec<HttpSseServerConfig>,
    #[serde(default)]
    #[serde(rename = "httpClients")]
    pub http_clients: Vec<HttpClientConfig>,
    #[serde(default)]
    #[serde(rename = "websocketServers")]
    pub websocket_servers: Vec<WebsocketServerConfig>,
    #[serde(default)]
    #[serde(rename = "websocketClients")]
    pub websocket_clients: Vec<WebsocketClientConfig>,
    #[serde(default)]
    #[ts(type = "Array<unknown>")]
    pub plugins: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AdvancedConfig {
    #[serde(default)]
    #[serde(rename = "autoStart")]
    pub auto_start: bool,
    #[serde(default)]
    #[serde(rename = "offlineNotice")]
    pub offline_notice: bool,
    #[serde(default)]
    #[serde(rename = "parseMultMsg")]
    pub parse_mult_msg: bool,
    #[serde(default)]
    #[serde(rename = "packetServer")]
    pub packet_server: String,
    #[serde(default = "default_packet_backend")]
    #[serde(rename = "packetBackend")]
    pub packet_backend: String,
    #[serde(default)]
    #[serde(rename = "enableLocalFile2Url")]
    pub enable_local_file_to_url: bool,
    #[serde(default)]
    #[serde(rename = "fileLog")]
    pub file_log: bool,
    #[serde(default = "default_true")]
    #[serde(rename = "consoleLog")]
    pub console_log: bool,
    #[serde(default = "default_file_log_level")]
    #[serde(rename = "fileLogLevel")]
    pub file_log_level: LogLevel,
    #[serde(default = "default_console_log_level")]
    #[serde(rename = "consoleLogLevel")]
    pub console_log_level: LogLevel,
    #[serde(default = "default_o3_hook_mode")]
    #[serde(rename = "o3HookMode")]
    pub o3_hook_mode: O3HookMode,
    #[serde(default)]
    pub bypass: BypassConfig,
}

impl Default for AdvancedConfig {
    fn default() -> Self {
        Self {
            auto_start: false,
            offline_notice: false,
            parse_mult_msg: false,
            packet_server: String::new(),
            packet_backend: default_packet_backend(),
            enable_local_file_to_url: false,
            file_log: false,
            console_log: true,
            file_log_level: default_file_log_level(),
            console_log_level: default_console_log_level(),
            o3_hook_mode: default_o3_hook_mode(),
            bypass: BypassConfig::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct StatusCommandConfig {
    #[serde(default = "default_status_command_enabled")]
    pub enabled: bool,
    #[serde(default)]
    pub swallow: bool,
    #[serde(default = "default_status_command_cooldown")]
    #[serde(rename = "cooldownSeconds")]
    pub cooldown_seconds: u32,
}

impl Default for StatusCommandConfig {
    fn default() -> Self {
        Self {
            enabled: default_status_command_enabled(),
            swallow: false,
            cooldown_seconds: default_status_command_cooldown(),
        }
    }
}

fn default_status_command_enabled() -> bool {
    true
}

fn default_status_command_cooldown() -> u32 {
    5
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct BotConfig {
    pub bot: BotBasicConfig,
    pub connect: ConnectConfig,
    pub advanced: AdvancedConfig,
    /// SnowLuma onebot_<uin>.json 的 statusCommand;NapCat 不序列化
    #[serde(default, rename = "statusCommand", skip_serializing_if = "Option::is_none")]
    #[ts(optional, rename = "statusCommand")]
    pub status_command: Option<StatusCommandConfig>,
}

impl BotConfig {
    pub fn validate(&self) -> Result<(), BotConfigError> {
        if self.bot.qq_id == 0 {
            return Err(BotConfigError::InvalidQqId(self.bot.qq_id));
        }

        let mut seen_names = HashMap::new();
        for config in &self.connect.http_servers {
            check_connect_name(&mut seen_names, &config.base.name)?;
            if config.port == 0 {
                return Err(BotConfigError::InvalidPort(config.port));
            }
        }
        for config in &self.connect.http_sse_servers {
            check_connect_name(&mut seen_names, &config.base.name)?;
            if config.port == 0 {
                return Err(BotConfigError::InvalidPort(config.port));
            }
        }
        for config in &self.connect.http_clients {
            check_connect_name(&mut seen_names, &config.base.name)?;
            if config.url.trim().is_empty() {
                return Err(BotConfigError::InvalidUrl(config.url.clone()));
            }
        }
        for config in &self.connect.websocket_servers {
            check_connect_name(&mut seen_names, &config.base.name)?;
            if config.port == 0 {
                return Err(BotConfigError::InvalidPort(config.port));
            }
        }
        for config in &self.connect.websocket_clients {
            check_connect_name(&mut seen_names, &config.base.name)?;
            if config.url.trim().is_empty() {
                return Err(BotConfigError::InvalidUrl(config.url.clone()));
            }
        }

        Ok(())
    }

    /// 校验 backend_type × deployment_type × runtime_target 运行矩阵是否当前支持
    ///
    /// 与 validate() 分开:validate() 管"配置本身是否自洽",这里管"这套运行组合当前
    /// 能不能跑"放在保存 / 导入边界调用,把不支持的组合在入口就拒掉,而不是留到
    /// 启动阶段才报错不放进 validate() 是因为有些链路(如生命周期路由健壮性测试)
    /// 需要存下"会被运行期拒绝"的配置当前矩阵限制:
    /// - Docker + 本机:不支持(Docker Desktop 安装链路太麻烦,本机只走直接运行)
    /// - 直接运行 + 远端 SSH:已支持(NapCat / SnowLuma Native,见 remote_native_launch / remote_snowluma)
    pub fn validate_runtime_matrix(&self) -> Result<(), BotConfigError> {
        let is_docker = matches!(self.bot.deployment_type, DeploymentType::Docker);
        let is_local = self.bot.runtime_target.is_local();

        if is_docker && is_local {
            return Err(BotConfigError::UnsupportedRuntimeMatrix(
                "本机暂不支持 Docker 部署,请改为「直接运行」或把运行宿主切换为远程 SSH 主机"
                    .to_string(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BotConfigError {
    #[error("invalid QQID: {0}")]
    InvalidQqId(u64),
    #[error("unsupported runtime matrix: {0}")]
    UnsupportedRuntimeMatrix(String),
    #[error("bot name cannot be empty")]
    EmptyName,
    #[error("duplicate connect config name: {0}")]
    DuplicateConnectName(String),
    #[error("duplicate QQID: {0}")]
    DuplicateQqId(u64),
    #[error("invalid port: {0}")]
    InvalidPort(u16),
    #[error("invalid URL: {0}")]
    InvalidUrl(String),
    #[error("bot config migration failed: {0}")]
    Migration(String),
    #[error("bot config JSON failed: {0}")]
    Json(String),
    #[error("bot config storage failed: {0}")]
    Storage(String),
}

impl From<crate::errors::MigrationError> for BotConfigError {
    fn from(error: crate::errors::MigrationError) -> Self {
        Self::Migration(error.to_string())
    }
}

impl From<crate::errors::ConfigError> for BotConfigError {
    fn from(error: crate::errors::ConfigError) -> Self {
        match error {
            crate::errors::ConfigError::Json(message) => Self::Json(message),
            other => Self::Storage(other.to_string()),
        }
    }
}

impl From<serde_json::Error> for BotConfigError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error.to_string())
    }
}

fn check_connect_name(
    seen_names: &mut HashMap<String, String>,
    name: &str,
) -> Result<(), BotConfigError> {
    let original = name.trim();
    if original.is_empty() {
        return Ok(());
    }

    let normalized = original.to_lowercase();
    if let Some(existing) = seen_names.get(&normalized) {
        return Err(BotConfigError::DuplicateConnectName(existing.clone()));
    }

    seen_names.insert(normalized, original.to_string());
    Ok(())
}

fn default_true() -> bool {
    true
}

fn default_runtime_target() -> RuntimeTarget {
    RuntimeTarget::Local
}

fn default_auto_restart_duration() -> u32 {
    6
}

fn default_root_path() -> String {
    "/".to_string()
}

fn default_heart_interval() -> u32 {
    30000
}

fn default_reconnect_interval() -> u32 {
    30000
}

fn default_packet_backend() -> String {
    "auto".to_string()
}

fn default_file_log_level() -> LogLevel {
    LogLevel::Debug
}

fn default_console_log_level() -> LogLevel {
    LogLevel::Info
}

fn default_o3_hook_mode() -> O3HookMode {
    O3HookMode::On
}

#[cfg(test)]
mod snowluma_start_mode_tests {
    //! BotBasicConfig.snowluma_start_mode 字节级 round-trip 锁定
    //! 字段约定( / / ):
    //! - JSON key 必须是驼峰 snowlumaStartMode(与 legacy autoRestartSchedule
    //! 等已有字段保持驼峰一致)
    //! - 字段 Option<SnowLumaStartMode> 默认值为 None,缺省时禁止出现
    //! 在序列化输出中(skip_serializing_if = "Option::is_none"),保证
    //! 纯 NapCat 用户的配置不会引入新字段
    //! - SnowLumaStartMode 复用 snowluma::launch_plan 已有 enum,通过
    //! #[serde(tag = "mode", rename_all = "snake_case")] 序列化
    //! 三个用例分别覆盖 None / ColdStart / HotStart,任一字段 / 字面量漂移
    //! 都会让对应测试失败
    use super::*;

    fn make_basic_config(start_mode: Option<SnowLumaStartMode>) -> BotBasicConfig {
        BotBasicConfig {
            name: "test-bot".to_string(),
            qq_id: 10001,
            music_sign_url: String::new(),
            auto_restart_schedule: AutoRestartSchedule::default(),
            offline_auto_restart: false,
            runtime_target: RuntimeTarget::Local,
            backend_type: BackendType::SnowLuma,
            deployment_type: DeploymentType::Native,
            snowluma_start_mode: start_mode,
        }
    }

    /// 缺省(None)时序列化不得出现 snowlumaStartMode key
    /// 反序列化忽略缺省字段后字段值仍为 None
    #[test]
    fn snowluma_start_mode_none_is_omitted_in_serialization() {
        let config = make_basic_config(None);

        let json = serde_json::to_string(&config).expect("serialize None");
        assert!(
            !json.contains("snowlumaStartMode"),
            "None 模式不应出现 snowlumaStartMode key，实际 JSON: {json}"
        );

        let decoded: BotBasicConfig = serde_json::from_str(&json).expect("deserialize None");
        assert_eq!(decoded, config);
        assert_eq!(decoded.snowluma_start_mode, None);
    }

    /// Some(ColdStart) 序列化形态:{"snowlumaStartMode":{"mode":"cold_start"}, ...}
    #[test]
    fn snowluma_start_mode_cold_start_is_byte_stable() {
        let config = make_basic_config(Some(SnowLumaStartMode::ColdStart));

        let json = serde_json::to_string(&config).expect("serialize ColdStart");
        assert!(
            json.contains(r#""snowlumaStartMode":{"mode":"cold_start"}"#),
            "缺少 ColdStart 字面量，实际 JSON: {json}"
        );

        let decoded: BotBasicConfig = serde_json::from_str(&json).expect("deserialize ColdStart");
        assert_eq!(decoded, config);
        assert_eq!(
            decoded.snowluma_start_mode,
            Some(SnowLumaStartMode::ColdStart)
        );

        // 二次序列化字节等价
        let json_again = serde_json::to_string(&decoded).expect("re-serialize ColdStart");
        assert_eq!(json.as_bytes(), json_again.as_bytes());
    }

    /// Some(HotStart) 序列化形态:
    /// {"snowlumaStartMode":{"mode":"hot_start"}, ...}
    /// HotStart 不再带 attach_pid 字段,PID 由 backend 在 start 时自动按 qq_id 匹配
    #[test]
    fn snowluma_start_mode_hot_start_is_byte_stable() {
        let config = make_basic_config(Some(SnowLumaStartMode::HotStart));

        let json = serde_json::to_string(&config).expect("serialize HotStart");
        assert!(
            json.contains(r#""snowlumaStartMode":{"mode":"hot_start"}"#),
            "缺少 HotStart 字面量，实际 JSON: {json}"
        );

        let decoded: BotBasicConfig = serde_json::from_str(&json).expect("deserialize HotStart");
        assert_eq!(decoded, config);
        assert_eq!(decoded.snowluma_start_mode, Some(SnowLumaStartMode::HotStart));

        let json_again = serde_json::to_string(&decoded).expect("re-serialize HotStart");
        assert_eq!(json.as_bytes(), json_again.as_bytes());
    }

    /// deployment_type 序列化为 camelCase key + lowercase 值,默认 Native
    #[test]
    fn deployment_type_serializes_camel_lowercase() {
        let mut config = make_basic_config(None);
        config.deployment_type = DeploymentType::Docker;
        let json = serde_json::to_string(&config).expect("serialize");
        assert!(
            json.contains(r#""deploymentType":"docker""#),
            "缺少 deploymentType:docker 字面量,实际: {json}"
        );
        let decoded: BotBasicConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(decoded.deployment_type, DeploymentType::Docker);
    }

    /// 旧配置(无 deploymentType key)反序列化后默认 Native,保证 0 成本升级
    #[test]
    fn deployment_type_absent_defaults_to_native() {
        // 一份不含 deploymentType 的最小 BotBasicConfig JSON
        let legacy = r#"{
            "name": "old-bot",
            "QQID": 10001,
            "musicSignUrl": "",
            "autoRestartSchedule": {"enable": false, "duration": 1, "time_unit": "h"},
            "offlineAutoRestart": false,
            "runtime_target": "local",
            "backend_type": "napcat"
        }"#;
        let decoded: BotBasicConfig =
            serde_json::from_str(legacy).expect("旧配置应可反序列化");
        assert_eq!(decoded.deployment_type, DeploymentType::Native);
    }
}

#[cfg(test)]
mod runtime_matrix_tests {
    use super::*;

    fn cfg(
        backend: BackendType,
        deployment: DeploymentType,
        target: RuntimeTarget,
    ) -> BotConfig {
        BotConfig {
            bot: BotBasicConfig {
                name: "t".to_string(),
                qq_id: 10001,
                music_sign_url: String::new(),
                auto_restart_schedule: AutoRestartSchedule::default(),
                offline_auto_restart: false,
                runtime_target: target,
                backend_type: backend,
                deployment_type: deployment,
                snowluma_start_mode: None,
            },
            connect: ConnectConfig::default(),
            advanced: AdvancedConfig::default(),
            status_command: None,
        }
    }

    #[test]
    fn supported_matrices_pass() {
        // 直接运行 + 本机 + NapCat:历史默认
        cfg(BackendType::NapCat, DeploymentType::Native, RuntimeTarget::Local)
            .validate_runtime_matrix()
            .unwrap();
        // 直接运行 + 本机 + SnowLuma
        cfg(BackendType::SnowLuma, DeploymentType::Native, RuntimeTarget::Local)
            .validate_runtime_matrix()
            .unwrap();
        // Docker + 远端 + NapCat:远端容器化主路径
        cfg(
            BackendType::NapCat,
            DeploymentType::Docker,
            RuntimeTarget::server("remote-a"),
        )
        .validate_runtime_matrix()
        .unwrap();
        // Docker + 远端 + SnowLuma
        cfg(
            BackendType::SnowLuma,
            DeploymentType::Docker,
            RuntimeTarget::server("remote-a"),
        )
        .validate_runtime_matrix()
        .unwrap();
        // 直接运行 + 远端 + NapCat / SnowLuma(SSH Native)
        cfg(
            BackendType::NapCat,
            DeploymentType::Native,
            RuntimeTarget::server("remote-a"),
        )
        .validate_runtime_matrix()
        .unwrap();
        cfg(
            BackendType::SnowLuma,
            DeploymentType::Native,
            RuntimeTarget::server("remote-a"),
        )
        .validate_runtime_matrix()
        .unwrap();
    }

    #[test]
    fn docker_on_local_is_rejected() {
        let err = cfg(BackendType::NapCat, DeploymentType::Docker, RuntimeTarget::Local)
            .validate_runtime_matrix()
            .unwrap_err();
        assert!(matches!(err, BotConfigError::UnsupportedRuntimeMatrix(_)));
    }

    #[test]
    fn docker_with_snowluma_on_remote_is_allowed() {
        cfg(
            BackendType::SnowLuma,
            DeploymentType::Docker,
            RuntimeTarget::server("remote-a"),
        )
        .validate_runtime_matrix()
        .unwrap();
    }

}
