//! Karin 上游事实（来源：.references/Karin，node-karin 1.17，编码前锁定，不凭记忆）。
//!
//! - npm 包 `node-karin`，Node `>=18`；项目脚手架 = `pnpm init` → `pnpm install node-karin` → `karin init`
//! - `karin init` 生成 `.env`（HTTP_ENABLE / HTTP_PORT=7777 / HTTP_HOST / HTTP_AUTH_KEY / WS_SERVER_AUTH_KEY …）、
//!   `@karinjs/config/*.json`、`index.mjs`；上游 create-karin 也是改 `.env` 里的两把 key
//! - OneBot 反向 WS：`adapter.json` `onebot.ws_server.enable` 默认 true；接受路径 `/`、`/onebot/v11/ws`；
//!   鉴权 = `.env` `WS_SERVER_AUTH_KEY`（Karin 监听 .env 变化会热断开重鉴权）
//! - WebUI：`http://<host>:<HTTP_PORT>/web`，登录用 `HTTP_AUTH_KEY`
//! - 直启入口 `node_modules/node-karin/dist/start/app.mjs` 可独立运行（index.mjs 只是 fork 包装）

use ncd_domain::{AppFrameworkId, AppFrameworkManifest, AppPlacement, OneBotLinkMode};

pub const KARIN_FRAMEWORK_ID: &str = "karin";
/// 与 `ComponentId::Karin` 的 serde 字面量一致
pub const KARIN_COMPONENT_ID: &str = "karin";
pub const KARIN_NPM_PACKAGE: &str = "node-karin";
pub const KARIN_NODE_VERSION_RANGE: &str = ">=18";
pub const KARIN_DEFAULT_PORT: u16 = 7777;
pub const KARIN_REPO_URL: &str = "https://github.com/KarinJS/Karin";
pub const KARIN_DOCS_URL: &str = "https://karinjs.com";

/// 实例目录内的相对路径
pub const KARIN_ENV_FILE: &str = ".env";
pub const KARIN_ADAPTER_JSON: &str = "@karinjs/config/adapter.json";
pub const KARIN_ENTRY_INDEX: &str = "index.mjs";
pub const KARIN_PACKAGE_JSON: &str = "node_modules/node-karin/package.json";
pub const KARIN_DIRECT_ENTRY: &str = "node_modules/node-karin/dist/start/app.mjs";
pub const KARIN_CLI_ENTRY: &str = "node_modules/node-karin/dist/cli/index.mjs";
/// 桌面端接管的日志文件（实例目录内；Karin 自身的 @karinjs/logs 另算）
pub const KARIN_STDOUT_LOG: &str = ".ncd-karin.log";
pub const KARIN_PID_FILE: &str = ".ncd-karin.pid";

/// `.env` 键
pub const ENV_HTTP_ENABLE: &str = "HTTP_ENABLE";
pub const ENV_HTTP_PORT: &str = "HTTP_PORT";
pub const ENV_HTTP_HOST: &str = "HTTP_HOST";
pub const ENV_HTTP_AUTH_KEY: &str = "HTTP_AUTH_KEY";
pub const ENV_WS_SERVER_AUTH_KEY: &str = "WS_SERVER_AUTH_KEY";

/// 反向 WS 路径（Karin 同时接受 `/`，用带语义的那条）
pub const KARIN_REVERSE_WS_PATH: &str = "/onebot/v11/ws";
pub const KARIN_WEBUI_PATH: &str = "/web";

pub fn karin_manifest() -> AppFrameworkManifest {
    AppFrameworkManifest {
        id: AppFrameworkId::new(KARIN_FRAMEWORK_ID),
        display_name: "Karin".to_string(),
        description: "Node.js 插件化应用端，自带 WebUI 与插件市场".to_string(),
        repo_url: Some(KARIN_REPO_URL.to_string()),
        docs_url: Some(KARIN_DOCS_URL.to_string()),
        supported_placements: vec![AppPlacement::LocalNative, AppPlacement::RemoteNative],
        default_port: KARIN_DEFAULT_PORT,
        has_webui: true,
        link_modes: vec![OneBotLinkMode::ReverseWs],
        component_id: KARIN_COMPONENT_ID.to_string(),
        runtime_component_ids: vec!["nodejs".to_string()],
        store_resources: vec![ncd_domain::AppStoreResource::Plugin],
        has_install_renderer: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_round_trips_and_matches_locked_facts() {
        let m = karin_manifest();
        let json = serde_json::to_string(&m).unwrap();
        let back: AppFrameworkManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(back, m);
        assert_eq!(back.id.as_str(), "karin");
        assert_eq!(back.default_port, 7777);
        assert!(back.has_webui);
        assert_eq!(back.link_modes, vec![OneBotLinkMode::ReverseWs]);
        assert_eq!(
            back.supported_placements,
            vec![AppPlacement::LocalNative, AppPlacement::RemoteNative]
        );
        assert!(!back.supported_placements.contains(&AppPlacement::RemoteDocker));
        assert!(back.has_install_renderer);
        assert_eq!(back.store_resources, vec![ncd_domain::AppStoreResource::Plugin]);
    }
}
