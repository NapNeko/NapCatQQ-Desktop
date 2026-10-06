//! NeoBot 上游事实（对照 PyPI `neobot-app` 1.2.0 与仓库 app/pyproject.toml、`docs/05-配置参考.md`，
//! 编码前核对，不凭记忆改键名）。
//!
//! - PyPI 包名就是导入名之外的发行名：`neobot-app`，控制台入口 `neobot = "neobot_app.cli:main"`
//!   （app/pyproject.toml:52-53）；`requires-python = ">=3.13"`
//! - **数据目录是 app/data（不是 data）**：core/paths.py 的 get_data_dir() 先认环境变量
//!   `NEOBOT_DATA_DIR`，非打包运行时返回「项目根/app/data」（项目根 = site-packages 往上
//!   第一个有 .git 的祖先，否则最外层有 pyproject.toml 的祖先，都没有才是 cwd）。桌面端启动时
//!   用环境变量把它钉到「实例目录/app/data」；data/ 是打包运行（exe 目录/data）的布局，桌面端不用。
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
    AppFrameworkId, AppFrameworkManifest, AppPlacement, AppWebUiAuthKind, OneBotLinkMode,
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
/// 上游 get_data_dir() 的「项目根」是从 site-packages 往上找 .git / pyproject.toml，
/// 实例目录放在某个 git 仓库下面时会跑到那个祖先的 app/data。所以启动时用
/// `NEOBOT_DATA_DIR` 环境变量把它钉在实例目录里，下面的相对路径才一定成立。
pub const NEOBOT_DATA_DIR: &str = "app/data";
/// 上游 .env 同理按项目根算，一并钉住
pub const NEOBOT_ENV_FILE: &str = "app/.env";
/// 上游 core/paths.py 认的环境变量名
pub const ENV_NEOBOT_DATA_DIR: &str = "NEOBOT_DATA_DIR";
pub const ENV_NEOBOT_ENV_FILE: &str = "NEOBOT_ENV_FILE";
pub const NEOBOT_CONFIG_TOML: &str = "app/data/config.toml";
pub const NEOBOT_DASHBOARD_CONFIG: &str = "app/data/plugins_data/dashboard/config.toml";
pub const NEOBOT_DASHBOARD_AUTH: &str = "app/data/plugins_data/dashboard/auth.json";
/// 旧布局的 config.toml：打包运行（exe 目录/data），以及 3.0.x 早期桌面端误写的那份。
/// 桌面端不按它读写，只用于导入时给出明确报错、启动时把旧对接搬过去
pub const NEOBOT_CONFIG_TOML_LEGACY: &str = "data/config.toml";
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
