// 常驻重登所需的本机端点；不保存 Desktop 的隧道端口。
use ncd_domain::relogin::ReloginSettings;
use serde::{Deserialize, Serialize};

pub const RELOGIN_CAPABILITY: &str = "onebot_relogin_v1";

#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct WatchRunIntent {
    pub running: bool,
    pub reset_generation: String,
}

#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct WatchReloginStatus {
    pub v: u32,
    pub updated_at: i64,
    pub configuration: String,
    pub connected_messengers: Vec<String>,
}

pub fn fingerprint(config: &crate::config::NotifyConfig) -> Result<String, serde_json::Error> {
    use sha2::{Digest, Sha256};
    let bytes = serde_json::to_vec(config)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

pub fn owns(config: &crate::config::NotifyConfig, bot: &str) -> bool {
    config.onebot.enabled
        && config.onebot.relogin.settings.monitors(bot)
        && !config.onebot.relogin.event_endpoints.is_empty()
        && config.bots.iter().any(|b| b.bot_id == bot && b.enabled)
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct WatchReloginConfig {
    pub settings: ReloginSettings,
    pub login_targets: Vec<WatchLoginTarget>,
    pub event_endpoints: Vec<WatchEventEndpoint>,
}

impl std::fmt::Debug for WatchReloginConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WatchReloginConfig")
            .field("settings", &self.settings)
            .field("login_targets", &self.login_targets.len())
            .field("event_endpoints", &self.event_endpoints.len())
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct WatchLoginTarget {
    pub bot_id: String,
    pub webui_port: u16,
    pub webui_secret: String,
    pub expected_pid: Option<u32>,
    pub pid_file: Option<String>,
    pub qr_enabled: bool,
    pub container_name: Option<String>,
    pub container_webui_port: Option<u16>,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct WatchEventEndpoint {
    pub bot_id: String,
    pub port: u16,
    pub path: String,
    pub token: String,
    pub container_name: Option<String>,
}
