//! TRSS-Yunzai 上游事实（来源：.references/TRSS-Yunzai @ 3.1.3，编码前锁定，不凭记忆）。
//!
//! - 仓库 `TimeRainStarSky/Yunzai`，`package.json` name = `trss-yunzai`，ESM，入口 `app.js`
//! - `node app.js daemon`：守护进程循环拉 `node app.js start`，子进程退出码 255 才停；
//!   群里 `#重启` 在 Windows 上就是子进程退出再被拉起（直接 `node .` 会 detached 拉新进程）
//! - 配置：`config/default_config/*.yaml` 首启复制到 `config/config/`，读取时两份浅合并，
//!   chokidar 监听，改了 5 秒后生效
//! - HTTP / WS 同一个口：`server.yaml` 的 `port`（默认 2536），`listen(port)` 不带地址，监听全部网卡；
//!   鉴权 `server.yaml` 的 `auth`（请求头名 → 值，头或同名 query 任一对上即可，每一条都要对上）
//! - OneBotv11 反向 WS 路径 `/OneBotv11`；`/exit` 只收回环来源的请求，收到就走退出流程（先存 Redis）
//! - Redis：`redis.yaml` 地址连不上且 host 是 127.0.0.1 时 `spawn(path, ["--port", port])`，cwd 是实例目录
//! - 依赖里 engines 最高的是 puppeteer `>=22.12.0`；README 写的 23.11 只为 `process.execve` 原地重启

use ncd_domain::{
    AppFrameworkId, AppFrameworkManifest, AppPlacement, AppStoreResource, OneBotLinkMode,
};

pub const YUNZAI_FRAMEWORK_ID: &str = "yunzai";
/// 与 `ComponentId::Yunzai` 的 serde 字面量一致
pub const YUNZAI_COMPONENT_ID: &str = "yunzai";
pub const YUNZAI_PACKAGE_NAME: &str = "trss-yunzai";
pub const YUNZAI_NODE_VERSION_RANGE: &str = ">=22.12.0";
pub const YUNZAI_DEFAULT_PORT: u16 = 2536;
pub const YUNZAI_REPO_URL: &str = "https://github.com/TimeRainStarSky/Yunzai";
pub const YUNZAI_DOCS_URL: &str = "https://github.com/TimeRainStarSky/Yunzai/tree/docs";

/// 本体的几个源（顺序无所谓，装的时候并发 ls-remote 挑最先应答的）。
/// `git.trss.me` 只是跳到 GitHub，不单列
pub const YUNZAI_GIT_SOURCES: &[&str] = &[
    "https://gitee.com/TimeRainStarSky/Yunzai",
    "https://github.com/TimeRainStarSky/Yunzai",
    "https://gitcode.com/TimeRainStarSky/Yunzai",
];

/// 实例目录内的相对路径
pub const YUNZAI_ENTRY: &str = "app.js";
pub const YUNZAI_PACKAGE_JSON: &str = "package.json";
pub const YUNZAI_CONFIG_DIR: &str = "config/config";
pub const YUNZAI_DEFAULT_CONFIG_DIR: &str = "config/default_config";
pub const YUNZAI_PLUGINS_DIR: &str = "plugins";
/// 单文件插件的落点（上游 loader 对不带 index.js 的目录逐个加载 .js，这个目录还热加载）
pub const YUNZAI_JS_PLUGIN_DIR: &str = "plugins/example";
/// 上游自带、不算商店插件的目录
pub const YUNZAI_BUILTIN_PLUGIN_DIRS: &[&str] = &["adapter", "example", "other", "system"];
/// 桌面端接管的 stdout 日志（上游自己还往 logs/ 写一份）
pub const YUNZAI_STDOUT_LOG: &str = ".ncd-yunzai.log";

/// `config/config/` 下的文件名（不带 .yaml），与 default_config 一一对应
pub const YUNZAI_CONFIG_NAMES: &[&str] = &[
    "bot", "other", "group", "server", "redis", "renderer", "db", "milky", "satori",
];

pub const YUNZAI_ONEBOT_PATH: &str = "/OneBotv11";
/// NapCat 反向 WS 带的鉴权头；server.yaml 的 auth 里写这一条
pub const YUNZAI_AUTH_HEADER: &str = "Authorization";

pub fn yunzai_manifest() -> AppFrameworkManifest {
    AppFrameworkManifest {
        id: AppFrameworkId::new(YUNZAI_FRAMEWORK_ID),
        display_name: "TRSS-Yunzai".to_string(),
        description: "云崽 Node.js 应用端，喵喵插件那一套生态".to_string(),
        repo_url: Some(YUNZAI_REPO_URL.to_string()),
        docs_url: Some(YUNZAI_DOCS_URL.to_string()),
        supported_placements: vec![AppPlacement::LocalNative, AppPlacement::RemoteNative],
        default_port: YUNZAI_DEFAULT_PORT,
        has_webui: false,
        link_modes: vec![OneBotLinkMode::ReverseWs],
        component_id: YUNZAI_COMPONENT_ID.to_string(),
        runtime_component_ids: vec!["git".to_string(), "nodejs".to_string(), "redis".to_string()],
        store_resources: vec![AppStoreResource::Plugin],
        has_install_renderer: true,
        webui_auth: ncd_domain::AppWebUiAuthKind::None,
        terms: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_round_trips_and_matches_locked_facts() {
        let m = yunzai_manifest();
        let json = serde_json::to_string(&m).unwrap();
        let back: AppFrameworkManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(back, m);
        assert_eq!(back.id.as_str(), "yunzai");
        assert_eq!(back.component_id, "yunzai");
        assert_eq!(back.default_port, 2536);
        assert!(!back.has_webui);
        assert_eq!(back.link_modes, vec![OneBotLinkMode::ReverseWs]);
        assert_eq!(back.store_resources, vec![AppStoreResource::Plugin]);
        assert!(back.has_install_renderer);
        assert!(back.terms.is_empty());
        assert!(
            !back
                .supported_placements
                .contains(&AppPlacement::RemoteDocker)
        );
    }

    #[test]
    fn config_names_cover_upstream_default_config() {
        // .references/TRSS-Yunzai/config/default_config 里的九份
        let mut names = YUNZAI_CONFIG_NAMES.to_vec();
        names.sort_unstable();
        assert_eq!(
            names,
            vec![
                "bot", "db", "group", "milky", "other", "redis", "renderer", "satori", "server"
            ]
        );
    }
}
