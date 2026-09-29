//! Koishi 上游事实（对照 `.references/koishi/*`：boilerplate v1.16.x、koishi 4.18.11、
//! adapter-onebot 6.9.4，不凭记忆改键名）。
//!
//! - 官方整包：`koishijs/boilerplate` 的 Release 附件 `boilerplate-<tag>-<os>-<arch>-node<N>.zip`，
//!   带 node_modules、`.yarn/releases/yarn-*.cjs` 和 `.yarnrc.yml`，解开就能 `yarn start`
//! - `yarn start` = `koishi start`：daemon 进程 fork 出 worker；worker 退出码 51 表示重启，daemon 会重拉
//! - 配置 `koishi.yml`：`plugins` 是入口分组，键是 `名字:标识`，`~` 前缀表示停用，`$` 开头的是元信息；
//!   loader 首启给缺标识的键补 6 位随机标识并整份重写（js-yaml，不留注释），运行中不盯文件
//! - 端口在 `server` 插件（默认 5140，`maxPort` 在口被占时往上顺延），`host` 默认 127.0.0.1
//! - 控制台就在同一个口的 `/`，数据走 `/status` WebSocket；上游默认不装登录（`~auth`）
//! - OneBot 适配器 `koishi-plugin-adapter-onebot` 不在整包里；ws-reverse 在 server 上开一条路径，
//!   只认请求头 `X-Self-ID` 和 `selfId` 对得上，不验 token

use ncd_domain::{
    AppFrameworkId, AppFrameworkManifest, AppPlacement, AppStoreResource, AppWebUiAuthKind,
    OneBotLinkMode,
};

pub const KOISHI_FRAMEWORK_ID: &str = "koishi";
/// 与 `ComponentId::Koishi` 的 serde 字面量一致
pub const KOISHI_COMPONENT_ID: &str = "koishi";
pub const KOISHI_REPO_URL: &str = "https://github.com/koishijs/koishi";
pub const KOISHI_DOCS_URL: &str = "https://koishi.chat/zh-CN/";
pub const BOILERPLATE_REPO: &str = "koishijs/boilerplate";
pub const KOISHI_NODE_VERSION_RANGE: &str = ">=18";
pub const KOISHI_DEFAULT_PORT: u16 = 5140;

/// GitHub API 不通时按平台装的版本：v1.16.1 没有 Windows 附件
pub const PINNED_WINDOWS_TAG: &str = "v1.16.0";
pub const PINNED_LINUX_TAG: &str = "v1.16.1";
/// 附件名里的 Node 大版本；只用来拼回落地址，挑附件时不看它
pub const PINNED_NODE_MAJOR: &str = "20";

pub const KOISHI_YML: &str = "koishi.yml";
pub const PACKAGE_JSON: &str = "package.json";
pub const YARNRC: &str = ".yarnrc.yml";
pub const KOISHI_CORE_PACKAGE_JSON: &str = "node_modules/koishi/package.json";
pub const BOILERPLATE_PACKAGE_NAME: &str = "@koishijs/boilerplate";
pub const KOISHI_STDOUT_LOG: &str = ".ncd-koishi.log";
pub const STAGE_DIR: &str = ".ncd-stage";

pub const ONEBOT_ADAPTER_PACKAGE: &str = "koishi-plugin-adapter-onebot";
pub const ONEBOT_ADAPTER_NAME: &str = "adapter-onebot";
/// 桌面端写的那条 OneBot 适配器；一个实例只对接一个 Bot，标识固定
pub const LINK_IDENT: &str = "ncd-link";
/// 独占的反向 WS 路径：同一路径上先注册的层先接，和用户自己配的 `/onebot` 分开
pub const KOISHI_REVERSE_WS_PATH: &str = "/onebot/ncd";
/// 对接条目放进这个分组（上游模板自带）；用户删了就放根上
pub const ADAPTER_GROUP_IDENT: &str = "adapter";

/// 控制台数据通道（`@koishijs/plugin-console` 的 `apiPath` 默认值）
pub const CONSOLE_API_PATH: &str = "/status";

/// 更新换整包时原样留下的顶层条目：配置、数据、环境变量、用户自己的插件目录和桌面端标记
pub const PRESERVED_ON_UPDATE: &[&str] = &[
    KOISHI_YML,
    "data",
    ".env",
    ".env.local",
    "plugins",
    "external",
    KOISHI_STDOUT_LOG,
    STAGE_DIR,
    crate::node_tooling::NODE_MARKER_FILE,
    ".ncd-app.pid",
];

/// 桌面端和控制台都靠它们活着，商店里不给卸、不给停
pub const LOCKED_PACKAGES: &[&str] = &[
    "koishi",
    "@koishijs/plugin-server",
    "@koishijs/plugin-console",
    "@koishijs/plugin-config",
    "@koishijs/plugin-market",
    "@koishijs/plugin-logger",
    ONEBOT_ADAPTER_PACKAGE,
];

pub fn koishi_manifest() -> AppFrameworkManifest {
    AppFrameworkManifest {
        id: AppFrameworkId::new(KOISHI_FRAMEWORK_ID),
        display_name: "Koishi".to_string(),
        description: "跨平台聊天机器人框架，插件市场有几千个插件，自带控制台".to_string(),
        repo_url: Some(KOISHI_REPO_URL.to_string()),
        docs_url: Some(KOISHI_DOCS_URL.to_string()),
        supported_placements: vec![AppPlacement::LocalNative, AppPlacement::RemoteNative],
        default_port: KOISHI_DEFAULT_PORT,
        has_webui: true,
        link_modes: vec![OneBotLinkMode::ReverseWs],
        component_id: KOISHI_COMPONENT_ID.to_string(),
        runtime_component_ids: vec!["nodejs".to_string()],
        store_resources: vec![AppStoreResource::Plugin],
        has_install_renderer: false,
        webui_auth: AppWebUiAuthKind::None,
        terms: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_round_trips_and_matches_locked_facts() {
        let m = koishi_manifest();
        let json = serde_json::to_string(&m).unwrap();
        let back: AppFrameworkManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(back, m);
        assert_eq!(back.id.as_str(), "koishi");
        assert_eq!(back.default_port, 5140);
        assert_eq!(back.link_modes, vec![OneBotLinkMode::ReverseWs]);
        assert_eq!(back.webui_auth, AppWebUiAuthKind::None, "上游默认不装登录");
        assert_eq!(back.runtime_component_ids, vec!["nodejs".to_string()]);
        assert!(back.terms.is_empty());
    }
}
