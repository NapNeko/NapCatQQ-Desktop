//! V2 MSI 安装目录孤儿清理(窄白名单)
//!
//! V2 PyInstaller onedir 在 `Program Files\NapCatQQ Desktop\_internal\` 下放
//! 整棵 Python/Qt 运行时。V3 近单 EXE,MSI MajorUpgrade 一般会卸掉旧产品,
//! 但文件锁/未登记残留时 `_internal` 可能还在。
//!
//! 另外 V2 运行时会把 SQLite 落到安装目录(工作目录常是 exe 旁):
//! `guild1.db` / `guild1.db-shm` / `guild1.db-wal`。这些不在 MSI 文件表里,
//! MajorUpgrade 也不会删,必须白名单清掉。
//!
//! MSI 侧已有 deferred 清理(见 `src-tauri/wix/`)。
//! 这里只在进程启动时对 **当前 exe 父目录** 做一次白名单兜底,失败只记日志,
//! 绝不碰 ProgramData / 用户配置。

use std::path::{Path, PathBuf};

/// 允许删除的安装目录子项(相对 exe 父目录)。只加确认无用的 V2 痕迹。
const LEGACY_ORPHAN_NAMES: &[&str] = &[
    "_internal",
    // 早期 V3 曾把 tray png 打进 resources；现已 embed，安装树不应再有 icons/
    "icons",
    "guild1.db",
    "guild1.db-shm",
    "guild1.db-wal",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyInstallCleanupReport {
    pub install_dir: PathBuf,
    pub removed: Vec<String>,
    pub failed: Vec<(String, String)>,
    pub skipped_reason: Option<String>,
}

/// 用 `current_exe` 父目录作为安装根;非 Windows 直接 skip。
pub fn purge_legacy_install_orphans() -> LegacyInstallCleanupReport {
    #[cfg(not(windows))]
    {
        return LegacyInstallCleanupReport {
            install_dir: PathBuf::new(),
            removed: Vec::new(),
            failed: Vec::new(),
            skipped_reason: Some("not windows".into()),
        };
    }

    #[cfg(windows)]
    {
        match std::env::current_exe() {
            Ok(exe) => match exe.parent() {
                Some(dir) => purge_legacy_install_orphans_in(dir),
                None => LegacyInstallCleanupReport {
                    install_dir: exe,
                    removed: Vec::new(),
                    failed: Vec::new(),
                    skipped_reason: Some("exe has no parent".into()),
                },
            },
            Err(err) => LegacyInstallCleanupReport {
                install_dir: PathBuf::new(),
                removed: Vec::new(),
                failed: Vec::new(),
                skipped_reason: Some(format!("current_exe: {err}")),
            },
        }
    }
}

/// 可测入口:只处理 `install_dir` 下白名单名字。
pub fn purge_legacy_install_orphans_in(install_dir: &Path) -> LegacyInstallCleanupReport {
    let mut report = LegacyInstallCleanupReport {
        install_dir: install_dir.to_path_buf(),
        removed: Vec::new(),
        failed: Vec::new(),
        skipped_reason: None,
    };

    if !install_dir.is_dir() {
        report.skipped_reason = Some("install_dir missing".into());
        return report;
    }

    for name in LEGACY_ORPHAN_NAMES {
        // 硬拒绝主程序名,防止名单被误改成自杀
        if name.eq_ignore_ascii_case("NapCatQQ-Desktop.exe") {
            report
                .failed
                .push(((*name).to_string(), "refusing main binary name".into()));
            continue;
        }
        let target = install_dir.join(name);
        if !target.exists() {
            continue;
        }
        let result = if target.is_dir() {
            std::fs::remove_dir_all(&target)
        } else {
            std::fs::remove_file(&target)
        };
        match result {
            Ok(()) => report.removed.push((*name).to_string()),
            Err(err) => report.failed.push(((*name).to_string(), err.to_string())),
        }
    }

    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn removes_internal_dir_only() {
        let root = tempfile::tempdir().unwrap();
        let internal = root.path().join("_internal");
        fs::create_dir_all(internal.join("PySide6")).unwrap();
        fs::write(internal.join("python312.dll"), b"x").unwrap();
        fs::write(root.path().join("NapCatQQ-Desktop.exe"), b"exe").unwrap();
        fs::create_dir_all(root.path().join("keep_me")).unwrap();

        let report = purge_legacy_install_orphans_in(root.path());
        assert_eq!(report.removed, vec!["_internal".to_string()]);
        assert!(report.failed.is_empty());
        assert!(!internal.exists());
        assert!(root.path().join("NapCatQQ-Desktop.exe").exists());
        assert!(root.path().join("keep_me").exists());
    }

    #[test]
    fn removes_guild_sqlite_sidecars() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("NapCatQQ-Desktop.exe"), b"exe").unwrap();
        fs::write(root.path().join("guild1.db"), b"db").unwrap();
        fs::write(root.path().join("guild1.db-shm"), b"shm").unwrap();
        fs::write(root.path().join("guild1.db-wal"), b"wal").unwrap();
        fs::write(root.path().join("keep.me"), b"x").unwrap();

        let report = purge_legacy_install_orphans_in(root.path());
        assert!(report.removed.contains(&"guild1.db".to_string()));
        assert!(report.removed.contains(&"guild1.db-shm".to_string()));
        assert!(report.removed.contains(&"guild1.db-wal".to_string()));
        assert!(!root.path().join("guild1.db").exists());
        assert!(root.path().join("keep.me").exists());
        assert!(root.path().join("NapCatQQ-Desktop.exe").exists());
    }

    #[test]
    fn removes_icons_dir() {
        let root = tempfile::tempdir().unwrap();
        let icons = root.path().join("icons");
        fs::create_dir_all(icons.join("tray")).unwrap();
        fs::write(icons.join("tray").join("tray-32.png"), b"x").unwrap();
        fs::write(root.path().join("NapCatQQ-Desktop.exe"), b"exe").unwrap();

        let report = purge_legacy_install_orphans_in(root.path());
        assert!(report.removed.contains(&"icons".to_string()));
        assert!(!icons.exists());
        assert!(root.path().join("NapCatQQ-Desktop.exe").exists());
    }

    #[test]
    fn noop_when_no_orphans() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("NapCatQQ-Desktop.exe"), b"exe").unwrap();
        let report = purge_legacy_install_orphans_in(root.path());
        assert!(report.removed.is_empty());
        assert!(report.failed.is_empty());
        assert!(report.skipped_reason.is_none());
    }
}
