//! NoneBot2 上游事实（来源：nonebot.dev 文档 + nonebot/nonebot2 仓库，编码前核对，不凭记忆）。
//!
//! - PyPI 包 `nonebot2[fastapi]`（ReverseDriver，默认驱动）+ `nonebot-adapter-onebot`；Python `>=3.9`
//! - 项目 = `pyproject.toml`（`[tool.nonebot]` 声明适配器 / 插件）+ `bot.py` 入口
//! - `.env` 只放 `ENVIRONMENT=prod`，其余进 `.env.prod`：`DRIVER=~fastapi`、`HOST`（默认 127.0.0.1）、
//!   `PORT`（默认 8080）、`ONEBOT_ACCESS_TOKEN`（别名 `ONEBOT_V11_ACCESS_TOKEN`）
//! - OneBot V11 反向 WS 路径：`/onebot/v11/`、`/onebot/v11/ws`、`/onebot/v11/ws/` 三者等价
//! - 没有 WebUI；正向 WS 需 `ONEBOT_WS_URLS`，首发不开
//! - 桌面端不引入 nb-cli：脚手架文件很少，直接写；依赖用 `uv sync` 装进实例目录 `.venv`

use ncd_domain::{AppFrameworkId, AppFrameworkManifest, AppPlacement, OneBotLinkMode};

pub const NONEBOT2_FRAMEWORK_ID: &str = "nonebot2";
/// 与 `ComponentId::NoneBot2` 的 serde 字面量一致
pub const NONEBOT2_COMPONENT_ID: &str = "nonebot2";
pub const NONEBOT2_DEFAULT_PORT: u16 = 8080;
pub const NONEBOT2_REPO_URL: &str = "https://github.com/nonebot/nonebot2";
pub const NONEBOT2_DOCS_URL: &str = "https://nonebot.dev";
/// `uv sync` 解析 Python 时的下限（NoneBot2 声明 >=3.9；3.10 起 typing 体验更好，且 uv 有托管包）
pub const NONEBOT2_PYTHON_REQUIRES: &str = ">=3.10,<3.14";
/// 对 uv 组件的版本约束（`--default-index` / 托管 Python 自 0.4 起稳定）
pub const NONEBOT2_UV_VERSION_RANGE: &str = ">=0.4";

/// PyPI 依赖（写进 pyproject.toml）
pub const PYPI_NONEBOT2: &str = "nonebot2[fastapi]";
pub const PYPI_ADAPTER_ONEBOT: &str = "nonebot-adapter-onebot";
/// `uv.lock` 里的规范包名（用于探测版本）
pub const LOCK_PACKAGE_NONEBOT2: &str = "nonebot2";

/// 实例目录内的相对路径
pub const NONEBOT2_PYPROJECT: &str = "pyproject.toml";
pub const NONEBOT2_UV_LOCK: &str = "uv.lock";
pub const NONEBOT2_BOT_PY: &str = "bot.py";
pub const NONEBOT2_ENV_FILE: &str = ".env";
pub const NONEBOT2_ENV_PROD_FILE: &str = ".env.prod";
pub const NONEBOT2_STDOUT_LOG: &str = ".ncd-nonebot2.log";

/// `.env.prod` 键
pub const ENV_DRIVER: &str = "DRIVER";
pub const ENV_HOST: &str = "HOST";
pub const ENV_PORT: &str = "PORT";
pub const ENV_ONEBOT_ACCESS_TOKEN: &str = "ONEBOT_ACCESS_TOKEN";
pub const DRIVER_FASTAPI: &str = "~fastapi";

/// 反向 WS 路径（三条等价，用带 ws 语义且与 Karin 一致的那条）
pub const NONEBOT2_REVERSE_WS_PATH: &str = "/onebot/v11/ws";

pub fn nonebot2_manifest() -> AppFrameworkManifest {
    AppFrameworkManifest {
        id: AppFrameworkId::new(NONEBOT2_FRAMEWORK_ID),
        display_name: "NoneBot2".to_string(),
        description: "Python 异步应用端，插件生态丰富；无 WebUI".to_string(),
        repo_url: Some(NONEBOT2_REPO_URL.to_string()),
        docs_url: Some(NONEBOT2_DOCS_URL.to_string()),
        supported_placements: vec![AppPlacement::LocalNative, AppPlacement::RemoteNative],
        default_port: NONEBOT2_DEFAULT_PORT,
        has_webui: false,
        link_modes: vec![OneBotLinkMode::ReverseWs],
        component_id: NONEBOT2_COMPONENT_ID.to_string(),
        runtime_component_ids: vec!["uv".to_string()],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_round_trips_and_matches_locked_facts() {
        let m = nonebot2_manifest();
        let json = serde_json::to_string(&m).unwrap();
        let back: AppFrameworkManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(back, m);
        assert_eq!(back.id.as_str(), "nonebot2");
        assert_eq!(back.default_port, 8080);
        assert!(!back.has_webui);
        assert_eq!(back.link_modes, vec![OneBotLinkMode::ReverseWs]);
        assert_eq!(
            back.supported_placements,
            vec![AppPlacement::LocalNative, AppPlacement::RemoteNative]
        );
    }
}
