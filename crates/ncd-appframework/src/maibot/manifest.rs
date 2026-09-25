//! MaiBot 上游事实（对照 `.references/MaiBot` 1.2.5 与 `.references/MaiBot-Napcat-Adapter` v1.4.0，
//! 不凭记忆改键名）。
//!
//! - 只有源码仓库，不发 PyPI；`uv sync --locked --no-dev --no-install-project` 装依赖，`python bot.py` 起
//! - bot.py 是 Runner，拉 Worker 子进程（`MAIBOT_WORKER_PROCESS=1`），Worker 退出码 42 由 Runner 重拉
//! - 主配置 `config/bot_config.toml`，`[inner].version` 必填（常量 `CONFIG_VERSION` 在 `src/config/config.py`）；
//!   WebUI `[webui].port` 默认 8001；旧版消息服务 `[maim_message].ws_server_port` 默认 8000，总会监听
//! - WebUI token 在 `data/webui.json`；`token_source = "configured"` 才不会每次启动重新生成
//! - 启动时比对 `EULA.md` / `PRIVACY.md` 的 md5 与 `eula.confirmed` / `privacy.confirmed`，不一致就等 stdin
//! - NapCat 适配器是插件（`plugins/MaiBot-Napcat-Adapter`），作客户端连 NapCat 正向 WS；
//!   `config.toml` 的 `[plugin].enabled` 默认 false，连接在 `[napcat_server]` host / port / token

use ncd_domain::{
    AppFrameworkId, AppFrameworkManifest, AppPlacement, AppTermsDoc, AppWebUiAuthKind,
    OneBotLinkMode,
};

pub const MAIBOT_FRAMEWORK_ID: &str = "maibot";
/// 与 `ComponentId::MaiBot` 的 serde 字面量一致
pub const MAIBOT_COMPONENT_ID: &str = "maibot";
pub const MAIBOT_REPO: &str = "Mai-with-u/MaiBot";
pub const MAIBOT_REPO_URL: &str = "https://github.com/Mai-with-u/MaiBot";
pub const MAIBOT_DOCS_URL: &str = "https://docs.mai-mai.org";
pub const ADAPTER_REPO: &str = "Mai-with-u/MaiBot-Napcat-Adapter";
/// 3.12 / 3.13 的 wheel 都齐；和 AstrBot 用同一个，uv 托管的解释器能共用
pub const MAIBOT_PYTHON_REQUIRES: &str = "3.12";
pub const MAIBOT_UV_VERSION_RANGE: &str = ">=0.4";
/// GitHub API 不通、或挑不出兼容组合时装的已验证组合
pub const PINNED_MAIBOT_TAG: &str = "1.2.5";
pub const PINNED_ADAPTER_TAG: &str = "v1.4.0";

pub const MAIBOT_DEFAULT_WEBUI_PORT: u16 = 8001;

pub const BOT_PY: &str = "bot.py";
pub const PYPROJECT: &str = "pyproject.toml";
pub const PYPROJECT_NAME: &str = "MaiBot";
pub const BOT_CONFIG: &str = "config/bot_config.toml";
pub const MODEL_CONFIG: &str = "config/model_config.toml";
/// 版本常量的出处；种子配置的 `[inner].version` 从这里读，写低了会触发 0.x 升级确认
pub const CONFIG_PY: &str = "src/config/config.py";
pub const WEBUI_JSON: &str = "data/webui.json";
pub const EULA_MD: &str = "EULA.md";
pub const PRIVACY_MD: &str = "PRIVACY.md";
pub const EULA_CONFIRMED: &str = "eula.confirmed";
pub const PRIVACY_CONFIRMED: &str = "privacy.confirmed";
/// 官方镜像也放在这个目录名下；上游按清单认插件，目录名本身不重要
pub const ADAPTER_DIR: &str = "plugins/MaiBot-Napcat-Adapter";
pub const ADAPTER_CONFIG: &str = "plugins/MaiBot-Napcat-Adapter/config.toml";
pub const ADAPTER_MANIFEST_FILE: &str = "_manifest.json";
/// 适配器 `SUPPORTED_CONFIG_VERSION`；不一致适配器拒绝连接
pub const ADAPTER_CONFIG_VERSION: &str = "0.1.0";
pub const MAIBOT_STDOUT_LOG: &str = ".ncd-maibot.log";
/// 解压暂存目录，放在实例目录里才能和目标同盘改名
pub const STAGE_DIR: &str = ".ncd-stage";

/// 更新换代码时原样留下的顶层条目：用户数据、配置、插件、日志、venv、协议确认、桌面端标记
pub const PRESERVED_ON_UPDATE: &[&str] = &[
    "config",
    "data",
    "plugins",
    "logs",
    ".venv",
    EULA_CONFIRMED,
    PRIVACY_CONFIRMED,
    MAIBOT_STDOUT_LOG,
    STAGE_DIR,
    crate::uv_tooling::UV_MARKER_FILE,
    ".ncd-app.pid",
];

pub const EULA_URL: &str = "https://github.com/Mai-with-u/MaiBot/blob/main/EULA.md";
pub const PRIVACY_URL: &str = "https://github.com/Mai-with-u/MaiBot/blob/main/PRIVACY.md";

pub fn maibot_manifest() -> AppFrameworkManifest {
    AppFrameworkManifest {
        id: AppFrameworkId::new(MAIBOT_FRAMEWORK_ID),
        display_name: "MaiBot".to_string(),
        description: "麦麦，大模型驱动的拟人聊天应用端，自带 WebUI".to_string(),
        repo_url: Some(MAIBOT_REPO_URL.to_string()),
        docs_url: Some(MAIBOT_DOCS_URL.to_string()),
        supported_placements: vec![AppPlacement::LocalNative, AppPlacement::RemoteNative],
        default_port: MAIBOT_DEFAULT_WEBUI_PORT,
        has_webui: true,
        link_modes: vec![OneBotLinkMode::ForwardWs],
        component_id: MAIBOT_COMPONENT_ID.to_string(),
        runtime_component_ids: vec!["uv".to_string()],
        store_resources: Vec::new(),
        has_install_renderer: false,
        webui_auth: AppWebUiAuthKind::Key,
        terms: vec![
            AppTermsDoc {
                id: "eula".to_string(),
                title: "MaiBot 最终用户许可协议".to_string(),
                url: EULA_URL.to_string(),
            },
            AppTermsDoc {
                id: "privacy".to_string(),
                title: "MaiBot 用户隐私条款".to_string(),
                url: PRIVACY_URL.to_string(),
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_round_trips_and_matches_locked_facts() {
        let m = maibot_manifest();
        let json = serde_json::to_string(&m).unwrap();
        let back: AppFrameworkManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(back, m);
        assert_eq!(back.id.as_str(), "maibot");
        assert_eq!(back.link_modes, vec![OneBotLinkMode::ForwardWs]);
        assert_eq!(back.webui_auth, AppWebUiAuthKind::Key);
        assert!(back.store_resources.is_empty(), "插件市场在 MaiBot 自己的 WebUI 里");
        let ids: Vec<&str> = back.terms.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, vec!["eula", "privacy"]);
    }
}
