//! NeoBot 上游事实（对照 PyPI `neobot-app` 1.2.0 与仓库 app/pyproject.toml、`docs/05-配置参考.md`，
//! 编码前核对，不凭记忆改键名）。
//!
//! - PyPI 包名就是导入名之外的发行名：`neobot-app`，控制台入口 `neobot = "neobot_app.cli:main"`
//!   （app/pyproject.toml:52-53）；`requires-python = ">=3.13"`
//! - **数据目录是 app/data（不是 data）**：core/paths.py 的 get_data_dir() 在非打包运行时
//!   返回「项目根/app/data」（没有 .git / pyproject.toml 祖先时回落到「cwd/app/data」），
//!   而桌面端就是以实例目录为 cwd 启动它的（进程认领也按 cwd 比对）。所以下面所有相对路径
//!   都以 app/data 打头；data/ 是另一套布局（打包运行「exe 目录/data」），只在识别已有项目时认。
//! - 面板是内置插件 `dashboard`，配置在数据目录下的 `plugins_data/dashboard/config.toml`（**不在** config.toml），
//!   默认 `host = "0.0.0.0"` / `port = 9981`，端口被占用时从 9981 起最多向后试 10 个；
//!   登录用面板密码，PBKDF2 哈希存 `plugins_data/dashboard/auth.json`
//! - OneBot 侧是**反向 WS 服务端**（应用端听口、协议 Bot 作客户端连过来）⇒ `OneBotLinkMode::ReverseWs`；
//!   键在 `[adapter]`：`mode`（默认 `"onebot"`）、`reverse_ws_host`（留空缺省 `0.0.0.0`）、
//!   `reverse_ws_port`（默认 0 = 未配置，缺省 8080）、`reverse_ws_access_token`（留空不校验）
//! - `requires-python >= 3.13`；本机/远端都跑原生进程
//!
//! 与上游「事实」相关的常量集中在这里，实现文件不重写这些字面量。

use ncd_domain::{
    AppFrameworkId, AppFrameworkManifest, AppPlacement, OneBotLinkMode, AppWebUiAuthKind,
};

pub const NEOBOT_FRAMEWORK_ID: &str = "neobot";
/// 与 `ComponentId::NeoBot` 的 serde 字面量一致
pub const NEOBOT_COMPONENT_ID: &str = "neobot";
pub const NEOBOT_REPO_URL: &str = "https://github.com/SuperQuail/NeoBot";
pub const NEOBOT_DOCS_URL: &str = "https://github.com/SuperQuail/NeoBot/tree/main/docs";

/// PyPI 发行名（不是导入名 `neobot_app`）
pub const PYPI_NEOBOT: &str = "neobot-app";

/// 实例解释器：上游 `requires-python = ">=3.13"`
pub const NEOBOT_PYTHON_REQUIRES: &str = "3.13";
/// 对 uv 组件的版本约束（与 `Component::requirements()` 逐字一致，registry 测试会核）
pub const NEOBOT_UV_VERSION_RANGE: &str = ">=0.4";

/// 面板（WebUI）默认口；`[dashboard].port` 的出厂值
pub const NEOBOT_DEFAULT_DASHBOARD_PORT: u16 = 9981;
/// OneBot 反向 WS 默认口；`[adapter].reverse_ws_port` 出厂值是 0（未配置），实际缺省落到这个
pub const NEOBOT_DEFAULT_ONEBOT_PORT: u16 = 8080;

/// 数据目录（实例目录内相对路径）：**app/data，不是 data**。
///
/// 见模块头：NeoBot 自己按 get_data_dir() 取「cwd/app/data」，桌面端又以实例目录为 cwd
/// 启动它，所以 PyPI 安装的实例数据都落在这一层。原先写成 data 会让桌面端读写一棵
/// NeoBot 从不看的树——对接写进去的端口/token 根本不生效，且导入已有项目也找不到配置。
pub const NEOBOT_DATA_DIR: &str = "app/data";
/// 另一套布局：打包运行（exe 目录/data）。识别已有项目时要认它，但桌面端按 NEOBOT_DATA_DIR 读写。
pub const NEOBOT_DATA_DIR_LEGACY: &str = "data";
pub const NEOBOT_CONFIG_TOML: &str = "app/data/config.toml";
pub const NEOBOT_DASHBOARD_CONFIG: &str = "app/data/plugins_data/dashboard/config.toml";
pub const NEOBOT_DASHBOARD_AUTH: &str = "app/data/plugins_data/dashboard/auth.json";
/// 打包布局下的同名文件：识别已有项目时要认，桌面端不按这些路径读写
pub const NEOBOT_CONFIG_TOML_LEGACY: &str = "data/config.toml";
pub const NEOBOT_DASHBOARD_CONFIG_LEGACY: &str = "data/plugins_data/dashboard/config.toml";
pub const NEOBOT_STDOUT_LOG: &str = ".ncd-neobot.log";

/// `app/data/config.toml` 的 `[adapter]` 键
pub const KEY_ADAPTER: &str = "adapter";
pub const KEY_ADAPTER_MODE: &str = "mode";
pub const KEY_REVERSE_WS_HOST: &str = "reverse_ws_host";
pub const KEY_REVERSE_WS_PORT: &str = "reverse_ws_port";
pub const KEY_REVERSE_WS_ACCESS_TOKEN: &str = "reverse_ws_access_token";
pub const ADAPTER_MODE_ONEBOT: &str = "onebot";

/// 面板配置文件的键
pub const KEY_DASHBOARD_HOST: &str = "host";
pub const KEY_DASHBOARD_PORT: &str = "port";

pub fn neobot_manifest() -> AppFrameworkManifest {
    AppFrameworkManifest {
        id: AppFrameworkId::new(NEOBOT_FRAMEWORK_ID),
        display_name: "NeoBot".to_string(),
        description: "Python 应用端，主打有活人感的聊天，自带 WebUI".to_string(),
        repo_url: Some(NEOBOT_REPO_URL.to_string()),
        docs_url: Some(NEOBOT_DOCS_URL.to_string()),
        supported_placements: vec![AppPlacement::LocalNative, AppPlacement::RemoteNative],
        default_port: NEOBOT_DEFAULT_ONEBOT_PORT,
        has_webui: true,
        link_modes: vec![OneBotLinkMode::ReverseWs],
        component_id: NEOBOT_COMPONENT_ID.to_string(),
        runtime_component_ids: vec!["uv".to_string()],
        store_resources: Vec::new(),
        has_install_renderer: false,
        // 面板密码由用户在面板上首次设置；桌面端读不到 NeoBot 的密码存储格式
        // （plugins_data/dashboard/auth.json 由面板自己管理），所以不接管账号，
        // 只负责把面板口告诉前端好开隧道。
        webui_auth: AppWebUiAuthKind::None,
        terms: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_round_trips_and_matches_locked_facts() {
        let m = neobot_manifest();
        let json = serde_json::to_string(&m).unwrap();
        let back: AppFrameworkManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(back, m);
        assert_eq!(back.id.as_str(), "neobot");
        assert_eq!(back.component_id, "neobot");
        assert_eq!(back.default_port, 8080);
        assert!(back.has_webui);
        assert_eq!(back.link_modes, vec![OneBotLinkMode::ReverseWs]);
        assert_eq!(
            back.supported_placements,
            vec![AppPlacement::LocalNative, AppPlacement::RemoteNative]
        );
        assert_eq!(back.runtime_component_ids, vec!["uv".to_string()]);
        assert_eq!(back.webui_auth, AppWebUiAuthKind::None);
        assert!(!back.has_install_renderer);
        assert!(
            back.store_resources.is_empty(),
            "NeoBot 面板没有插件市场索引：装第三方插件是贴一个 GitHub 仓库地址（见 dashboard/api.py 的 plugins_install），而桌面端商店要的是「市场 URL + 解析器」，形状对不上，所以这里声明为空"
        );
        assert!(back.terms.is_empty());
    }

    /// 简介要写明自带 WebUI：卡片上就靠这一行让用户知道能开面板。
    /// （其余框架同样写法：NoneBot2 写「无 WebUI」，Karin / AstrBot / MaiBot 写「自带 WebUI」。）
    #[test]
    fn description_states_webui_support() {
        let m = neobot_manifest();
        assert!(
            m.description.contains("WebUI"),
            "简介要写明 WebUI 支持情况：{}",
            m.description
        );
        // 声明了 has_webui 就必须在简介里说清，否则用户不知道能开面板
        assert!(m.has_webui);
    }

    /// 面板口与 OneBot 口是两个口；设计上依赖这一条（webui_port() 与 listen_port() 不同）
    #[test]
    fn dashboard_and_onebot_ports_are_distinct() {
        assert_ne!(NEOBOT_DEFAULT_DASHBOARD_PORT, NEOBOT_DEFAULT_ONEBOT_PORT);
    }
}
