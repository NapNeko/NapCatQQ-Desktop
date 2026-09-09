//! NoneBot 配置文件布局（官方：https://nonebot.dev/docs/appendices/config）。
//!
//! 优先级：`nonebot.init` > 系统环境变量 > `.env` / `.env.{ENVIRONMENT}`。
//! `ENVIRONMENT` 从 `.env` 读（大小写不敏感），缺省 `prod`，再加载 `.env.{ENVIRONMENT}` 覆盖。
//! 桌面端探测远程项目时只看项目文件，不读本机进程环境。

use super::manifest::{NONEBOT2_ENV_FILE, NONEBOT2_ENV_PROD_FILE};
use crate::env_file::EnvFile;

pub const DEFAULT_ENVIRONMENT: &str = "prod";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoneBotEnvLayout {
    pub environment: String,
    /// 类型化 / 对接写入的相对路径
    pub write_rel: String,
}

impl NoneBotEnvLayout {
    pub fn is_overlay(&self) -> bool {
        self.write_rel != NONEBOT2_ENV_FILE
    }
}

/// `base` = `.env` 原文；`overlay_exists` = `.env.{environment}` 是否在磁盘上。
pub fn layout_from_base(base: Option<&str>, overlay_exists: impl Fn(&str) -> bool) -> NoneBotEnvLayout {
    let environment = environment_from_dotenv(base.unwrap_or(""));
    let overlay_rel = format!(".env.{environment}");
    let write_rel = if overlay_exists(&overlay_rel) {
        overlay_rel
    } else {
        NONEBOT2_ENV_FILE.to_string()
    };
    NoneBotEnvLayout {
        environment,
        write_rel,
    }
}

pub fn environment_from_dotenv(text: &str) -> String {
    let env = EnvFile::parse(text);
    env.get_ci("ENVIRONMENT")
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| DEFAULT_ENVIRONMENT.to_string())
}

pub fn overlay_rel(environment: &str) -> String {
    format!(".env.{environment}")
}

/// 官方脚手架把具体配置放 `.env.prod`；真实项目常只有一份 `.env`。
pub fn is_standard_overlay_rel(rel: &str) -> bool {
    rel == NONEBOT2_ENV_PROD_FILE || rel.starts_with(".env.")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn official_default_is_prod_and_uses_env_when_no_overlay() {
        let layout = layout_from_base(Some("HOST=127.0.0.1\nPORT=13120\n"), |_| false);
        assert_eq!(layout.environment, "prod");
        assert_eq!(layout.write_rel, ".env");
        assert!(!layout.is_overlay());
    }

    #[test]
    fn ncd_scaffold_writes_env_prod() {
        let layout = layout_from_base(Some("ENVIRONMENT=prod\n"), |rel| rel == ".env.prod");
        assert_eq!(layout.write_rel, ".env.prod");
        assert!(layout.is_overlay());
    }

    #[test]
    fn environment_dev_uses_env_dev_when_present() {
        let layout = layout_from_base(Some("ENVIRONMENT=dev\n"), |rel| rel == ".env.dev");
        assert_eq!(layout.environment, "dev");
        assert_eq!(layout.write_rel, ".env.dev");
    }

    #[test]
    fn environment_is_case_insensitive_key() {
        assert_eq!(environment_from_dotenv("environment=staging\n"), "staging");
    }
}
