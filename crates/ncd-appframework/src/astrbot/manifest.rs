//! AstrBot 上游事实（对照 `.references/AstrBot`，不凭记忆改键名）。
//!
//! - 工作目录 `astrbot init -y` 后 `astrbot run`；CLI `-p` 改的是 WebUI 口
//! - 权威配置 `data/cmd_config.json`；WebUI `dashboard.port` 默认 6185
//! - OneBot v11 落盘 `type=aiocqhttp`，反向 WS 路径 `/ws`，默认口 6199
//! - 官方不做平台市场；商店只代管 `data/plugins/`

use ncd_domain::{
    AppFrameworkId, AppFrameworkManifest, AppPlacement, AppStoreResource, AppWebUiAuthKind,
    OneBotLinkMode,
};

pub const ASTRBOT_FRAMEWORK_ID: &str = "astrbot";
/// 与 `ComponentId::AstrBot` 的 serde 字面量一致
pub const ASTRBOT_COMPONENT_ID: &str = "astrbot";
pub const ASTRBOT_DEFAULT_PORT: u16 = 6199;
pub const ASTRBOT_DEFAULT_DASHBOARD_PORT: u16 = 6185;
pub const ASTRBOT_REPO_URL: &str = "https://github.com/AstrBotDevs/AstrBot";
pub const ASTRBOT_DOCS_URL: &str = "https://docs.astrbot.app";
pub const ASTRBOT_PYTHON_REQUIRES: &str = "3.12";
pub const ASTRBOT_UV_VERSION_RANGE: &str = ">=0.4";
pub const PYPI_ASTRBOT: &str = "astrbot";

pub const ASTRBOT_CMD_CONFIG: &str = "data/cmd_config.json";
pub const ASTRBOT_SHARED_PREFS: &str = "data/shared_preferences.json";
pub const ASTRBOT_PLUGINS_DIR: &str = "data/plugins";
pub const ASTRBOT_STDOUT_LOG: &str = ".ncd-astrbot.log";
pub const ASTRBOT_REVERSE_WS_PATH: &str = "/ws";

pub const PLATFORM_TYPE_AIOCQHTTP: &str = "aiocqhttp";
pub const KEY_WS_REVERSE_HOST: &str = "ws_reverse_host";
pub const KEY_WS_REVERSE_PORT: &str = "ws_reverse_port";
pub const KEY_WS_REVERSE_TOKEN: &str = "ws_reverse_token";

pub fn astrbot_manifest() -> AppFrameworkManifest {
    AppFrameworkManifest {
        id: AppFrameworkId::new(ASTRBOT_FRAMEWORK_ID),
        display_name: "AstrBot".to_string(),
        description: "Python 应用端，自带 WebUI；Desktop 只对接 OneBot v11".to_string(),
        repo_url: Some(ASTRBOT_REPO_URL.to_string()),
        docs_url: Some(ASTRBOT_DOCS_URL.to_string()),
        supported_placements: vec![AppPlacement::LocalNative, AppPlacement::RemoteNative],
        default_port: ASTRBOT_DEFAULT_PORT,
        has_webui: true,
        link_modes: vec![OneBotLinkMode::ReverseWs],
        component_id: ASTRBOT_COMPONENT_ID.to_string(),
        runtime_component_ids: vec!["uv".to_string()],
        store_resources: vec![AppStoreResource::Plugin],
        has_install_renderer: false,
        webui_auth: AppWebUiAuthKind::UserPassword,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_round_trips_and_matches_locked_facts() {
        let m = astrbot_manifest();
        let json = serde_json::to_string(&m).unwrap();
        let back: AppFrameworkManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(back, m);
        assert_eq!(back.id.as_str(), "astrbot");
        assert_eq!(back.default_port, 6199);
        assert!(back.has_webui);
        assert_eq!(back.link_modes, vec![OneBotLinkMode::ReverseWs]);
        assert_eq!(back.store_resources, vec![AppStoreResource::Plugin]);
        assert!(!back.has_install_renderer);
        assert_eq!(back.webui_auth, AppWebUiAuthKind::UserPassword);
    }
}
