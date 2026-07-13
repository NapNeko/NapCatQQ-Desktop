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
//! 权威清理在 MSI elevated deferred CA(`src-tauri/wix/v2-orphan-cleanup.wxs`)。
//! 进程启动兜底优先读 HKLM InstallDir,再退回 current_exe 父目录。
//! Program Files 对普通用户只读:Access Denied 只记日志,不阻断启动,
//! 也不碰 ProgramData / 用户配置。
//!
//! 路径安全(防「空路径/盘符根递归删」类事故):
//! - 只删 install_dir 下固定白名单相对名,从不删 install_dir 本身
//! - 拒绝空路径、相对路径、盘符根(C:\)、过浅路径
//! - 白名单名不得含分隔符 / `.` / `..`
//! - 目标必须仍落在 install_dir 之下
//!
//! 另:3.0.0 曾把 HKCU 产品键写成字面量 `Software{{product_name}}`
//! (Handlebars 吃掉 `\`)。启动时删掉该脏键(HKCU,无需管理员)。

use std::path::{Component, Path, PathBuf};

/// 允许删除的安装目录子项(相对安装根)。只加确认无用的 V2 痕迹。
const LEGACY_ORPHAN_NAMES: &[&str] = &[
    "_internal",
    // 早期 V3 曾把 tray png 打进 resources；现已 embed，安装树不应再有 icons/
    "icons",
    "guild1.db",
    "guild1.db-shm",
    "guild1.db-wal",
];

const MAIN_EXE_NAME: &str = "NapCatQQ-Desktop.exe";

/// 3.0.0 模板转义错误留下的字面量键(不是 `Software\NapCatQQ Desktop`)
const BROKEN_HKCU_PRODUCT_KEY: &str = r"Software{{product_name}}";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyInstallCleanupReport {
    pub install_dir: PathBuf,
    pub removed: Vec<String>,
    pub failed: Vec<(String, String)>,
    pub skipped_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BrokenRegistryCleanupReport {
    /// 是否发现并尝试处理脏键
    pub found: bool,
    pub removed: bool,
    pub error: Option<String>,
}

/// 解析安装根并清理白名单孤儿;非 Windows 直接 skip。
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
        match resolve_install_dir_for_cleanup() {
            Ok(dir) => purge_legacy_install_orphans_in(&dir),
            Err(reason) => LegacyInstallCleanupReport {
                install_dir: PathBuf::new(),
                removed: Vec::new(),
                failed: Vec::new(),
                skipped_reason: Some(reason),
            },
        }
    }
}

/// 优先 HKLM InstallDir(且含主程序),否则 current_exe 父目录。
#[cfg(windows)]
fn resolve_install_dir_for_cleanup() -> Result<PathBuf, String> {
    if let Some(from_reg) = crate::product_registry::read_install_dir() {
        let candidate = crate::product_registry::normalize_registered_path(from_reg);
        if is_safe_install_dir(&candidate) && candidate.join(MAIN_EXE_NAME).is_file() {
            return Ok(candidate);
        }
    }

    let exe = std::env::current_exe().map_err(|e| format!("current_exe: {e}"))?;
    let dir = exe
        .parent()
        .ok_or_else(|| "exe has no parent".to_string())?
        .to_path_buf();
    if !is_safe_install_dir(&dir) {
        return Err(format!(
            "current_exe parent rejected as unsafe install_dir: {}",
            dir.display()
        ));
    }
    Ok(dir)
}

/// 删除 3.0.0 误写的 `HKCU\Software{{product_name}}`(若存在)。
/// 不需要管理员;失败只返回 error 字符串。
#[cfg(windows)]
pub fn purge_broken_hkcu_product_key() -> BrokenRegistryCleanupReport {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;

    let mut report = BrokenRegistryCleanupReport::default();
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    match hkcu.open_subkey(BROKEN_HKCU_PRODUCT_KEY) {
        Ok(_) => report.found = true,
        Err(_) => return report,
    }
    match hkcu.delete_subkey_all(BROKEN_HKCU_PRODUCT_KEY) {
        Ok(()) => report.removed = true,
        Err(err) => report.error = Some(err.to_string()),
    }
    report
}

#[cfg(not(windows))]
pub fn purge_broken_hkcu_product_key() -> BrokenRegistryCleanupReport {
    BrokenRegistryCleanupReport::default()
}

/// 安装根是否允许作为删除锚点。
///
/// 拒绝空/相对/盘符根/过浅路径,避免「路径解析失败 → 落到 C:\ → 递归删」类事故。
/// 不要求目录已存在(调用方再查 is_dir)。
pub(crate) fn is_safe_install_dir(path: &Path) -> bool {
    if path.as_os_str().is_empty() {
        return false;
    }
    let s = path.to_string_lossy();
    let trimmed = s.trim().trim_end_matches(['\\', '/']);
    if trimmed.is_empty() || trimmed.contains('\0') {
        return false;
    }
    if !path.is_absolute() {
        return false;
    }
    // 盘符根: C:\ / C: / \\?\C:\
    if is_drive_root(path) {
        return false;
    }
    // 至少要有一层目录名(Program Files\App 或更深),不能是 \\server\share 根
    let normal_components = path
        .components()
        .filter(|c| matches!(c, Component::Normal(_)))
        .count();
    if normal_components < 1 {
        return false;
    }
    // 路径字符串过短几乎只能是根
    if trimmed.chars().count() < 4 {
        return false;
    }
    true
}

fn is_drive_root(path: &Path) -> bool {
    let mut comps = path.components();
    match (comps.next(), comps.next(), comps.next()) {
        // C:\
        (Some(Component::Prefix(_)), Some(Component::RootDir), None) => true,
        // \
        (Some(Component::RootDir), None, _) => true,
        _ => {
            // 规范化后 parent 为 None 且有 root 也视为根
            path.parent().is_none_or(|p| p.as_os_str().is_empty())
                && path
                    .components()
                    .any(|c| matches!(c, Component::RootDir | Component::Prefix(_)))
                && path
                    .components()
                    .filter(|c| matches!(c, Component::Normal(_)))
                    .count()
                    == 0
        }
    }
}

/// 白名单项是否为安全的单层相对名(无分隔符、非 . / ..)。
pub(crate) fn is_safe_orphan_name(name: &str) -> bool {
    if name.is_empty() || name == "." || name == ".." {
        return false;
    }
    if name.contains('/') || name.contains('\\') || name.contains('\0') {
        return false;
    }
    if name.eq_ignore_ascii_case(MAIN_EXE_NAME) {
        return false;
    }
    // 拒绝绝对路径片段
    if Path::new(name).is_absolute() {
        return false;
    }
    Path::new(name)
        .components()
        .all(|c| matches!(c, Component::Normal(_)))
        && Path::new(name).components().count() == 1
}

/// 可测入口:只处理 `install_dir` 下白名单名字。
pub fn purge_legacy_install_orphans_in(install_dir: &Path) -> LegacyInstallCleanupReport {
    let mut report = LegacyInstallCleanupReport {
        install_dir: install_dir.to_path_buf(),
        removed: Vec::new(),
        failed: Vec::new(),
        skipped_reason: None,
    };

    if !is_safe_install_dir(install_dir) {
        report.skipped_reason = Some(format!(
            "install_dir rejected as unsafe: {}",
            install_dir.display()
        ));
        return report;
    }

    if !install_dir.is_dir() {
        report.skipped_reason = Some("install_dir missing".into());
        return report;
    }

    for name in LEGACY_ORPHAN_NAMES {
        if !is_safe_orphan_name(name) {
            report
                .failed
                .push(((*name).to_string(), "refusing unsafe orphan name".into()));
            continue;
        }
        let target = install_dir.join(name);
        // join 后必须仍是 install_dir 的直接子项(防将来名单被改成 ..\..)
        if target.parent() != Some(install_dir) {
            report
                .failed
                .push(((*name).to_string(), "target escaped install_dir".into()));
            continue;
        }
        if !target.exists() {
            continue;
        }
        // 永不删除安装根自身
        if target == install_dir {
            report
                .failed
                .push(((*name).to_string(), "refusing to delete install_dir".into()));
            continue;
        }
        let result = if target.is_dir() {
            std::fs::remove_dir_all(&target)
        } else {
            std::fs::remove_file(&target)
        };
        match result {
            Ok(()) => report.removed.push((*name).to_string()),
            Err(err) => {
                // Program Files 下 Users 只有 RX:Access Denied 是预期,等 MSI elevated CA
                let msg = if err.kind() == std::io::ErrorKind::PermissionDenied {
                    format!("{err} (need elevated MSI purge under Program Files)")
                } else {
                    err.to_string()
                };
                report.failed.push(((*name).to_string(), msg));
            }
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

    #[test]
    fn broken_hkcu_key_constant_is_literal_template_bug() {
        // 必须是字面量 Software{{product_name}},不是展开后的产品名
        assert_eq!(BROKEN_HKCU_PRODUCT_KEY, "Software{{product_name}}");
        assert!(!BROKEN_HKCU_PRODUCT_KEY.contains('\\'));
    }

    #[test]
    fn rejects_drive_root_and_empty_install_dir() {
        assert!(!is_safe_install_dir(Path::new("")));
        assert!(!is_safe_install_dir(Path::new(".")));
        assert!(!is_safe_install_dir(Path::new("relative\\app")));
        #[cfg(windows)]
        {
            // raw string 不能以 \ 结尾,用普通转义
            assert!(!is_safe_install_dir(Path::new("C:\\")));
            assert!(!is_safe_install_dir(Path::new("C:/")));
            assert!(!is_safe_install_dir(Path::new("D:\\")));
            // 至少一层目录名才允许
            assert!(is_safe_install_dir(Path::new(
                r"C:\Program Files\NapCatQQ Desktop"
            )));
        }
    }

    #[test]
    fn rejects_unsafe_orphan_names() {
        assert!(!is_safe_orphan_name(""));
        assert!(!is_safe_orphan_name("."));
        assert!(!is_safe_orphan_name(".."));
        assert!(!is_safe_orphan_name(r"..\Windows"));
        assert!(!is_safe_orphan_name(r"foo\bar"));
        assert!(!is_safe_orphan_name("NapCatQQ-Desktop.exe"));
        assert!(is_safe_orphan_name("_internal"));
        assert!(is_safe_orphan_name("guild1.db"));
    }

    #[test]
    fn purge_skips_unsafe_install_dir_without_deleting() {
        let report = purge_legacy_install_orphans_in(Path::new("C:\\"));
        assert!(report.skipped_reason.is_some());
        assert!(report.removed.is_empty());
    }
}
