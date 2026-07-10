//! watch.json / notify.json / desktop_present 与路径布局

use std::path::{Path, PathBuf};

use ncd_domain::{OfflineEmailSettings, OfflineWebhookChannel, default_webhook_body_template};
use serde::{Deserialize, Serialize};

/// 当前配置协议版本;破坏性变更时递增
pub const WATCH_PROTOCOL_V1: u32 = 1;

/// 默认 Desktop 心跳 TTL(秒):超过则视为 Desktop 离线,watch 可发 Webhook
pub const DEFAULT_DESKTOP_PRESENT_TTL_SECS: u32 = 90;

/// 默认探活间隔(秒)
pub const DEFAULT_PROBE_INTERVAL_SECS: u32 = 15;

/// 默认掉线边沿防抖(秒);0 表示不防抖
pub const DEFAULT_DEBOUNCE_SECS: u32 = 0;

fn default_protocol() -> u32 {
    WATCH_PROTOCOL_V1
}

fn default_probe_interval() -> u32 {
    DEFAULT_PROBE_INTERVAL_SECS
}

fn default_present_ttl() -> u32 {
    DEFAULT_DESKTOP_PRESENT_TTL_SECS
}

fn default_debounce() -> u32 {
    DEFAULT_DEBOUNCE_SECS
}

fn default_true() -> bool {
    true
}

fn default_features() -> Vec<String> {
    vec![
        "process_watch".to_string(),
        "docker_watch".to_string(),
        "webhook".to_string(),
        "login_watch".to_string(),
        "email".to_string(),
    ]
}

/// 安装根下的标准路径
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchPaths {
    pub root: PathBuf,
    pub bin_dir: PathBuf,
    pub config_dir: PathBuf,
    pub state_dir: PathBuf,
    pub log_dir: PathBuf,
    pub watch_json: PathBuf,
    pub notify_json: PathBuf,
    pub desktop_present: PathBuf,
    pub edge_state: PathBuf,
}

impl WatchPaths {
    pub fn from_root(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        let config_dir = root.join("config");
        let state_dir = root.join("state");
        Self {
            bin_dir: root.join("bin"),
            watch_json: config_dir.join("watch.json"),
            notify_json: config_dir.join("notify.json"),
            desktop_present: state_dir.join("desktop_present"),
            edge_state: state_dir.join("edge_state.json"),
            config_dir,
            state_dir,
            log_dir: root.join("logs"),
            root,
        }
    }

    /// 默认 `~/ncd-watch`(HOME 缺失时用当前目录下 ncd-watch)
    pub fn default_home() -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        Self::from_root(home.join("ncd-watch"))
    }
}

/// 兼容旧称呼
pub type WatchRoot = WatchPaths;

/// `config/watch.json` — 本机运行参数(可由 Desktop 下发或手工改)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchConfig {
    #[serde(default = "default_protocol")]
    pub protocol: u32,
    #[serde(default = "default_features")]
    pub features: Vec<String>,
    /// 探活周期秒
    #[serde(default = "default_probe_interval")]
    pub probe_interval_secs: u32,
    /// desktop_present 新鲜度阈值秒
    #[serde(default = "default_present_ttl")]
    pub desktop_present_ttl_secs: u32,
    /// 同一 bot 掉线边沿最短间隔秒
    #[serde(default = "default_debounce")]
    pub debounce_secs: u32,
    /// Desktop 在线(present 未过期)时是否仍发 Webhook;默认 false
    #[serde(default)]
    pub notify_while_desktop_present: bool,
}

impl Default for WatchConfig {
    fn default() -> Self {
        Self {
            protocol: WATCH_PROTOCOL_V1,
            features: default_features(),
            probe_interval_secs: DEFAULT_PROBE_INTERVAL_SECS,
            desktop_present_ttl_secs: DEFAULT_DESKTOP_PRESENT_TTL_SECS,
            debounce_secs: DEFAULT_DEBOUNCE_SECS,
            notify_while_desktop_present: false,
        }
    }
}

impl WatchConfig {
    pub fn load_or_default(path: &Path) -> Result<Self, String> {
        if !path.is_file() {
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(path).map_err(|e| format!("read watch.json: {e}"))?;
        serde_json::from_str(&text).map_err(|e| format!("parse watch.json: {e}"))
    }

    pub fn clamp(mut self) -> Self {
        if self.probe_interval_secs == 0 {
            self.probe_interval_secs = 1;
        }
        if self.desktop_present_ttl_secs < 15 {
            self.desktop_present_ttl_secs = 15;
        }
        self
    }
}

/// 单个被监控 Bot
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifyBotTarget {
    /// 与 Desktop BotId 一致,通常是 QQ 号字符串
    pub bot_id: String,
    #[serde(default)]
    pub qq_id: u64,
    #[serde(default)]
    pub bot_name: String,
    /// napcat | snowluma
    #[serde(default = "default_backend")]
    pub backend: String,
    /// native | docker
    #[serde(default = "default_deployment")]
    pub deployment: String,
    /// Docker 容器名;空则按 ncbot-<qq_id> 推导
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container_name: Option<String>,
    /// Native: 可选 pid 文件路径(相对或绝对)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pid_file: Option<String>,
    /// Native: 进程名子串匹配(如 QQ / napcat);空则仅 pid_file
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_match: Option<String>,
    /// NapCat WebUI 在远端本机的端口(Docker 为 host 映射口;Native 为实际监听口)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub webui_port: Option<u16>,
    /// NapCat WebUI token(换 Bearer);仅写 0600 的 notify.json
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub webui_token: Option<String>,
    /// 是否监控该 bot
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_backend() -> String {
    "napcat".to_string()
}

fn default_deployment() -> String {
    "native".to_string()
}

impl NotifyBotTarget {
    pub fn resolved_container_name(&self) -> String {
        if let Some(name) = self
            .container_name
            .as_ref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            return name.to_string();
        }
        let qq = if self.qq_id > 0 {
            self.qq_id.to_string()
        } else {
            self.bot_id.clone()
        };
        // 与 ncd-deploy::bot_docker_container_name 对齐;缺 containerName 时按 backend 猜
        let prefix = if self.backend.eq_ignore_ascii_case("snowluma") {
            "slbot"
        } else {
            "ncbot"
        };
        format!("{prefix}-{qq}")
    }

    pub fn is_docker(&self) -> bool {
        self.deployment.eq_ignore_ascii_case("docker")
    }
}

/// `config/notify.json` — Desktop 下发的监控目标 + Webhook 通道
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifyConfig {
    #[serde(default = "default_protocol")]
    pub protocol: u32,
    /// 配置来源 server_id(诊断用)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_id: Option<String>,
    #[serde(default)]
    pub bots: Vec<NotifyBotTarget>,
    /// 与 Desktop OfflineWebhookChannel 字段对齐的子集
    #[serde(default)]
    pub webhooks: Vec<OfflineWebhookChannel>,
    /// 对齐 poller.offline_webhook_notice;缺省 true 兼容旧 notify.json
    #[serde(default = "default_true")]
    pub webhook_enabled: bool,
    /// 对齐 poller.offline_email_notice
    #[serde(default)]
    pub email_enabled: bool,
    /// 对齐 offline_notify_behavior.notify_on_recovered
    #[serde(default)]
    pub notify_on_recovered: bool,
    /// 对齐 Desktop OfflineEmailSettings;email_enabled 时使用
    #[serde(default)]
    pub email: OfflineEmailSettings,
}

impl Default for NotifyConfig {
    fn default() -> Self {
        Self {
            protocol: WATCH_PROTOCOL_V1,
            server_id: None,
            bots: Vec::new(),
            webhooks: Vec::new(),
            webhook_enabled: true,
            email_enabled: false,
            notify_on_recovered: false,
            email: OfflineEmailSettings::default(),
        }
    }
}

impl NotifyConfig {
    pub fn load_or_default(path: &Path) -> Result<Self, String> {
        if !path.is_file() {
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(path).map_err(|e| format!("read notify.json: {e}"))?;
        serde_json::from_str(&text).map_err(|e| format!("parse notify.json: {e}"))
    }

    pub fn enabled_bots(&self) -> impl Iterator<Item = &NotifyBotTarget> {
        self.bots.iter().filter(|b| b.enabled)
    }

    pub fn enabled_webhooks(&self) -> Vec<&OfflineWebhookChannel> {
        self.webhooks
            .iter()
            .filter(|c| c.enabled && !c.url.trim().is_empty())
            .collect()
    }

    /// 示例配置(文档/测试用)
    pub fn example() -> Self {
        Self {
            protocol: WATCH_PROTOCOL_V1,
            server_id: Some("example-server".into()),
            bots: vec![NotifyBotTarget {
                bot_id: "10001".into(),
                qq_id: 10001,
                bot_name: "demo".into(),
                backend: "napcat".into(),
                deployment: "docker".into(),
                container_name: None,
                pid_file: None,
                process_match: None,
                webui_port: None,
                webui_token: None,
                enabled: true,
            }],
            webhooks: vec![OfflineWebhookChannel {
                id: "default".into(),
                name: "default".into(),
                enabled: true,
                url: "https://example.invalid/hook".into(),
                secret: String::new(),
                method: "POST".into(),
                body_template: default_webhook_body_template(),
            }],
            webhook_enabled: true,
            email_enabled: false,
            notify_on_recovered: false,
            email: OfflineEmailSettings::default(),
        }
    }
}

/// `state/desktop_present` 文件内容(JSON);也可用纯数字 unix 秒
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopPresentFile {
    /// Unix 秒(UTC)
    pub updated_at_unix: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub desktop_version: Option<String>,
}

impl DesktopPresentFile {
    pub fn now() -> Self {
        Self {
            updated_at_unix: chrono::Utc::now().timestamp(),
            desktop_version: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn watch_config_round_trip() {
        let c = WatchConfig::default();
        let raw = serde_json::to_string_pretty(&c).unwrap();
        let back: WatchConfig = serde_json::from_str(&raw).unwrap();
        assert_eq!(c, back);
        assert!(raw.contains("probeIntervalSecs"));
    }

    #[test]
    fn notify_config_round_trip() {
        let c = NotifyConfig::example();
        let raw = serde_json::to_string_pretty(&c).unwrap();
        let back: NotifyConfig = serde_json::from_str(&raw).unwrap();
        assert_eq!(c.bots[0].bot_id, back.bots[0].bot_id);
        assert_eq!(back.bots[0].resolved_container_name(), "ncbot-10001");
    }

    #[test]
    fn resolved_container_name_uses_backend_prefix() {
        let mut sl = NotifyConfig::example().bots.remove(0);
        sl.backend = "snowluma".into();
        sl.container_name = None;
        assert_eq!(sl.resolved_container_name(), "slbot-10001");

        sl.container_name = Some("custom-box".into());
        assert_eq!(sl.resolved_container_name(), "custom-box");
    }

    #[test]
    fn paths_layout() {
        let p = WatchPaths::from_root("/tmp/ncd-watch");
        assert_eq!(
            p.watch_json,
            PathBuf::from("/tmp/ncd-watch/config/watch.json")
        );
        assert_eq!(
            p.desktop_present,
            PathBuf::from("/tmp/ncd-watch/state/desktop_present")
        );
    }
}
