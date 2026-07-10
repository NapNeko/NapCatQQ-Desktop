//! data_root 布局 v1 路径权威。
//!
//! 业务模块只通过本模块拼路径,避免再散落 join("runtime") / join("config")。
//! 旧路径常量仅供发现、收敛与兼容读取。

use std::path::{Path, PathBuf};

/// 当前落盘布局版本。升高时启动收敛会再跑一遍(配合 force 或启发式)。
pub const LAYOUT_VERSION: u32 = 1;

/// 原子写同文件最多保留的 .bak.* 份数。
pub const MAX_JSON_BAK_FILES: usize = 3;

/// migration-backup 目录最多保留份数。
pub const MAX_MIGRATION_BACKUPS: usize = 5;

/// 桌面会话日志最多保留文件数(与按天清理叠加,取更严)。
pub const MAX_DESKTOP_LOG_FILES: usize = 30;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataPaths {
    root: PathBuf,
}

impl DataPaths {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn layout_version_path(&self) -> PathBuf {
        self.root.join("layout-version.json")
    }

    pub fn config_dir(&self) -> PathBuf {
        self.root.join("config")
    }

    pub fn bot_config_path(&self) -> PathBuf {
        self.config_dir().join("bot.json")
    }

    pub fn app_settings_path(&self) -> PathBuf {
        self.config_dir().join("app-settings.json")
    }

    pub fn app_config_path(&self) -> PathBuf {
        // 旧 QConfig 兼容文件名;新布局不再作为主设置源
        self.config_dir().join("config.json")
    }

    pub fn servers_path(&self) -> PathBuf {
        self.config_dir().join("servers.json")
    }

    pub fn migration_report_path(&self) -> PathBuf {
        self.config_dir().join("migration-report.json")
    }

    pub fn secrets_dir(&self) -> PathBuf {
        self.root.join("secrets")
    }

    pub fn ssh_keys_dir(&self) -> PathBuf {
        self.root.join("ssh_keys")
    }

    pub fn components_dir(&self) -> PathBuf {
        self.root.join("components")
    }

    pub fn napcat_install_dir(&self) -> PathBuf {
        self.components_dir().join("NapCatQQ")
    }

    pub fn napcat_config_dir(&self) -> PathBuf {
        self.napcat_install_dir().join("config")
    }

    pub fn snowluma_install_dir(&self) -> PathBuf {
        self.components_dir().join("SnowLuma")
    }

    pub fn snowluma_config_dir(&self) -> PathBuf {
        self.snowluma_install_dir().join("config")
    }

    pub fn state_dir(&self) -> PathBuf {
        self.root.join("state")
    }

    pub fn snowluma_data_dir(&self) -> PathBuf {
        self.state_dir().join("snowluma")
    }

    pub fn logs_dir(&self) -> PathBuf {
        self.root.join("logs")
    }

    pub fn desktop_log_dir(&self) -> PathBuf {
        self.logs_dir().join("desktop")
    }

    pub fn bot_log_dir(&self) -> PathBuf {
        self.logs_dir().join("bots")
    }

    pub fn cache_dir(&self) -> PathBuf {
        self.root.join("cache")
    }

    pub fn tmp_dir(&self) -> PathBuf {
        self.root.join("tmp")
    }

    pub fn migration_backup_dir(&self) -> PathBuf {
        self.tmp_dir().join("migration-backup")
    }

    pub fn output_dir(&self) -> PathBuf {
        self.root.join("output")
    }

    pub fn export_dir(&self) -> PathBuf {
        self.tmp_dir().join("exports")
    }

    // --- 旧布局(只读发现 / 收敛源) ---

    pub fn legacy_runtime_dir(&self) -> PathBuf {
        self.root.join("runtime")
    }

    pub fn legacy_runtime_config_dir(&self) -> PathBuf {
        self.legacy_runtime_dir().join("config")
    }

    pub fn legacy_bot_config_path(&self) -> PathBuf {
        self.legacy_runtime_config_dir().join("bot.json")
    }

    pub fn legacy_app_settings_path(&self) -> PathBuf {
        self.legacy_runtime_config_dir().join("app-settings.json")
    }

    pub fn legacy_app_config_path(&self) -> PathBuf {
        self.legacy_runtime_config_dir().join("config.json")
    }

    pub fn legacy_servers_path(&self) -> PathBuf {
        self.legacy_runtime_config_dir().join("servers.json")
    }

    pub fn legacy_napcat_install_dir(&self) -> PathBuf {
        self.legacy_runtime_dir().join("NapCatQQ")
    }

    pub fn legacy_snowluma_install_dir(&self) -> PathBuf {
        self.legacy_runtime_dir().join("SnowLuma")
    }

    pub fn legacy_bot_log_dir(&self) -> PathBuf {
        self.legacy_runtime_dir().join("log").join("bots")
    }

    pub fn legacy_desktop_log_dir(&self) -> PathBuf {
        self.root.join("log")
    }

    pub fn legacy_snowluma_data_dir(&self) -> PathBuf {
        self.root.join("snowluma")
    }

    pub fn legacy_migration_backup_dir(&self) -> PathBuf {
        self.legacy_runtime_dir()
            .join("tmp")
            .join("migration-backup")
    }
}

/// 读取 layout-version.json;缺失或损坏视为 0。
pub fn read_layout_version(root: &Path) -> u32 {
    let path = DataPaths::new(root).layout_version_path();
    let Ok(text) = std::fs::read_to_string(path) else {
        return 0;
    };
    let Ok(file) = serde_json::from_str::<LayoutVersionFile>(&text) else {
        return 0;
    };
    file.version
}

pub fn write_layout_version(root: &Path, version: u32) -> std::io::Result<()> {
    let paths = DataPaths::new(root);
    if let Some(parent) = paths.layout_version_path().parent() {
        std::fs::create_dir_all(parent)?;
    }
    let payload = LayoutVersionFile {
        version,
        consolidated_at: Some(chrono::Utc::now().to_rfc3339()),
    };
    let text = serde_json::to_string_pretty(&payload)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(paths.layout_version_path(), text)
}

/// layout-version.json 落盘结构(仅 marker,不进 IPC)。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct LayoutVersionFile {
    version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    consolidated_at: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_v1_paths_are_stable() {
        let p = DataPaths::new(PathBuf::from("D:/data"));
        assert_eq!(p.config_dir(), PathBuf::from("D:/data/config"));
        assert_eq!(
            p.bot_config_path(),
            PathBuf::from("D:/data/config/bot.json")
        );
        assert_eq!(
            p.napcat_install_dir(),
            PathBuf::from("D:/data/components/NapCatQQ")
        );
        assert_eq!(
            p.snowluma_data_dir(),
            PathBuf::from("D:/data/state/snowluma")
        );
        assert_eq!(p.desktop_log_dir(), PathBuf::from("D:/data/logs/desktop"));
        assert_eq!(p.bot_log_dir(), PathBuf::from("D:/data/logs/bots"));
    }
}
