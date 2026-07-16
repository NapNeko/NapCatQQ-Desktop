//! Windows 产品路径注册表(安装目录 / 数据目录)
//!
//! 机器级(MSI / 启动补写,可能需管理员):
//!   HKLM\SOFTWARE\NapCatQQ-Desktop
//!     InstallDir  REG_SZ  程序安装根(含 NapCatQQ-Desktop.exe)
//!     DataRoot    REG_SZ  默认数据根(首次安装写入;升级 NeverOverwrite)
//!
//! 用户级(数据目录迁移,无 UAC):
//!   HKCU\SOFTWARE\NapCatQQ-Desktop
//!     DataRoot    REG_SZ  用户迁移后的权威数据根
//!
//! 解析优先级见 bootstrap::resolve_data_root:
//!   NCD_DATA_ROOT → HKCU DataRoot → HKLM DataRoot → ProgramData 默认
//!
//! ensure 只补 HKLM 缺键,绝不覆盖已有 HKLM/HKCU。
//! 迁移成功只写 HKCU,实现无感换根。

use std::path::{Path, PathBuf};

/// 与 WiX `Software\NapCatQQ-Desktop` 一致(winreg 用 SOFTWARE 大写亦可)
pub const PRODUCT_REGISTRY_KEY: &str = r"SOFTWARE\NapCatQQ-Desktop";
pub const VALUE_INSTALL_DIR: &str = "InstallDir";
pub const VALUE_DATA_ROOT: &str = "DataRoot";

/// 开发/排障覆盖数据根;优先于注册表与默认 ProgramData
pub const DATA_ROOT_ENV: &str = "NCD_DATA_ROOT";

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProductPaths {
    pub install_dir: Option<PathBuf>,
    /// 机器级 HKLM DataRoot(MSI 默认)
    pub data_root: Option<PathBuf>,
    /// 用户级 HKCU DataRoot(迁移后;解析优先于 HKLM)
    pub user_data_root: Option<PathBuf>,
}

/// 路径是否可作 data_root / install_dir 登记:非空、绝对路径。
/// 不要求目录已存在(首次安装或迁移目标可能尚未创建)。
pub fn is_usable_absolute_path(path: &Path) -> bool {
    if path.as_os_str().is_empty() {
        return false;
    }
    let s = path.to_string_lossy();
    if s.trim().is_empty() || s.contains('\0') {
        return false;
    }
    path.is_absolute()
}

/// 去掉尾部空白与多余的路径分隔(注册表/MSI 属性常带尾 `\`)。
pub fn normalize_registered_path(path: PathBuf) -> PathBuf {
    let raw = path.to_string_lossy();
    let trimmed = raw.trim().trim_end_matches(['\\', '/']);
    if trimmed.is_empty() {
        return path;
    }
    PathBuf::from(trimmed)
}

/// 合并 HKLM + HKCU 产品路径(InstallDir 仅 HKLM)。
#[cfg(windows)]
pub fn read_product_paths() -> ProductPaths {
    let mut paths = read_product_paths_from_hklm().unwrap_or_default();
    paths.user_data_root = read_user_data_root();
    paths
}

#[cfg(not(windows))]
pub fn read_product_paths() -> ProductPaths {
    ProductPaths::default()
}

/// 权威 DataRoot 指针:用户迁移(HKCU)优先于机器默认(HKLM)。
#[cfg(windows)]
pub fn read_data_root() -> Option<PathBuf> {
    if let Some(user) = read_user_data_root() {
        return Some(user);
    }
    read_product_paths_from_hklm()
        .ok()
        .and_then(|p| p.data_root)
}

#[cfg(not(windows))]
pub fn read_data_root() -> Option<PathBuf> {
    None
}

#[cfg(windows)]
pub fn read_user_data_root() -> Option<PathBuf> {
    read_path_from_hive(Hive::CurrentUser, VALUE_DATA_ROOT)
}

#[cfg(not(windows))]
pub fn read_user_data_root() -> Option<PathBuf> {
    None
}

/// 用户迁移成功后写入 HKCU(覆盖);无需管理员。
#[cfg(windows)]
pub fn write_user_data_root(path: &Path) -> Result<PathBuf, String> {
    let data_norm = normalize_registered_path(path.to_path_buf());
    if !is_usable_absolute_path(&data_norm) {
        return Err("DataRoot: path is not a usable absolute path".into());
    }
    write_value_hive(Hive::CurrentUser, VALUE_DATA_ROOT, &data_norm)?;
    Ok(data_norm)
}

#[cfg(not(windows))]
pub fn write_user_data_root(_path: &Path) -> Result<PathBuf, String> {
    Err("user DataRoot registry is only supported on Windows".into())
}

/// 清除用户级 DataRoot(回退到 HKLM/默认);迁移失败排障用。
#[cfg(windows)]
pub fn clear_user_data_root() -> Result<(), String> {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, KEY_WRITE};

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key = match hkcu.open_subkey_with_flags(PRODUCT_REGISTRY_KEY, KEY_WRITE) {
        Ok(k) => k,
        Err(_) => return Ok(()),
    };
    match key.delete_value(VALUE_DATA_ROOT) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err.to_string()),
    }
}

#[cfg(not(windows))]
pub fn clear_user_data_root() -> Result<(), String> {
    Ok(())
}

#[cfg(windows)]
pub fn read_install_dir() -> Option<PathBuf> {
    read_product_paths_from_hklm()
        .ok()
        .and_then(|p| p.install_dir)
}

#[cfg(not(windows))]
pub fn read_install_dir() -> Option<PathBuf> {
    None
}

/// 启动兜底:缺 HKLM `InstallDir` / `DataRoot` 时补写,已有值不改。
/// 不读写 HKCU(用户迁移指针只由 migrate 写入)。
/// HKLM 写失败(无管理员权限等)只记结果,不阻断启动。
#[cfg(windows)]
pub fn ensure_product_paths_registered(data_root: &Path) -> EnsureRegistryReport {
    let mut report = EnsureRegistryReport::default();
    let machine = read_product_paths_from_hklm().unwrap_or_default();

    let install_dir = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|p| p.to_path_buf()))
        .map(normalize_registered_path)
        .filter(|p| is_usable_absolute_path(p));

    if machine.install_dir.is_none() {
        if let Some(dir) = install_dir.as_ref() {
            match write_value_hive(Hive::LocalMachine, VALUE_INSTALL_DIR, dir) {
                Ok(()) => {
                    report.wrote_install_dir = true;
                    report.install_dir = Some(dir.clone());
                }
                Err(err) => report.errors.push(format!("{VALUE_INSTALL_DIR}: {err}")),
            }
        } else {
            report
                .errors
                .push("InstallDir: cannot resolve current_exe parent".into());
        }
    } else {
        report.install_dir = machine.install_dir;
    }

    let data_norm = normalize_registered_path(data_root.to_path_buf());
    if machine.data_root.is_none() {
        if is_usable_absolute_path(&data_norm) {
            match write_value_hive(Hive::LocalMachine, VALUE_DATA_ROOT, &data_norm) {
                Ok(()) => {
                    report.wrote_data_root = true;
                    report.data_root = Some(data_norm);
                }
                Err(err) => report.errors.push(format!("{VALUE_DATA_ROOT}: {err}")),
            }
        } else {
            report
                .errors
                .push("DataRoot: resolved path is not usable".into());
        }
    } else {
        report.data_root = machine.data_root;
    }

    report.user_data_root = read_user_data_root();
    report
}

#[cfg(not(windows))]
pub fn ensure_product_paths_registered(_data_root: &Path) -> EnsureRegistryReport {
    EnsureRegistryReport::default()
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EnsureRegistryReport {
    pub install_dir: Option<PathBuf>,
    pub data_root: Option<PathBuf>,
    pub user_data_root: Option<PathBuf>,
    pub wrote_install_dir: bool,
    pub wrote_data_root: bool,
    pub errors: Vec<String>,
}

#[cfg(windows)]
#[derive(Clone, Copy)]
enum Hive {
    LocalMachine,
    CurrentUser,
}

#[cfg(windows)]
fn read_product_paths_from_hklm() -> Result<ProductPaths, String> {
    use winreg::RegKey;
    use winreg::enums::HKEY_LOCAL_MACHINE;

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let key = match hklm.open_subkey(PRODUCT_REGISTRY_KEY) {
        Ok(k) => k,
        Err(_) => return Ok(ProductPaths::default()),
    };

    let install_dir = read_path_value(&key, VALUE_INSTALL_DIR);
    let data_root = read_path_value(&key, VALUE_DATA_ROOT);
    Ok(ProductPaths {
        install_dir,
        data_root,
        user_data_root: None,
    })
}

#[cfg(windows)]
fn read_path_from_hive(hive: Hive, name: &str) -> Option<PathBuf> {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};

    let root = match hive {
        Hive::LocalMachine => RegKey::predef(HKEY_LOCAL_MACHINE),
        Hive::CurrentUser => RegKey::predef(HKEY_CURRENT_USER),
    };
    let key = root.open_subkey(PRODUCT_REGISTRY_KEY).ok()?;
    read_path_value(&key, name)
}

#[cfg(windows)]
fn read_path_value(key: &winreg::RegKey, name: &str) -> Option<PathBuf> {
    let raw: String = key.get_value(name).ok()?;
    let path = normalize_registered_path(PathBuf::from(raw));
    is_usable_absolute_path(&path).then_some(path)
}

#[cfg(windows)]
fn write_value_hive(hive: Hive, name: &str, path: &Path) -> Result<(), String> {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_WRITE};

    let root = match hive {
        Hive::LocalMachine => RegKey::predef(HKEY_LOCAL_MACHINE),
        Hive::CurrentUser => RegKey::predef(HKEY_CURRENT_USER),
    };
    let (key, _) = root
        .create_subkey_with_flags(PRODUCT_REGISTRY_KEY, KEY_WRITE)
        .map_err(|e| e.to_string())?;
    let value = path.to_string_lossy().into_owned();
    key.set_value(name, &value).map_err(|e| e.to_string())
}

/// 纯函数:在已读出的候选上选权威 DataRoot(便于单测,不碰真注册表)。
pub fn select_data_root_pointer(
    env: Option<PathBuf>,
    user: Option<PathBuf>,
    machine: Option<PathBuf>,
) -> Option<PathBuf> {
    for candidate in [env, user, machine] {
        if let Some(path) = candidate {
            let path = normalize_registered_path(path);
            if is_usable_absolute_path(&path) {
                return Some(path);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_relative_and_empty() {
        assert!(!is_usable_absolute_path(Path::new("")));
        assert!(!is_usable_absolute_path(Path::new("relative\\foo")));
        assert!(!is_usable_absolute_path(Path::new("data")));
    }

    #[test]
    #[cfg(windows)]
    fn accepts_windows_absolute() {
        assert!(is_usable_absolute_path(Path::new(
            r"C:\ProgramData\NapCatQQ Desktop"
        )));
        assert!(is_usable_absolute_path(Path::new(
            r"D:\Migrated\NapCatQQ Desktop"
        )));
    }

    #[test]
    fn normalize_strips_trailing_slash() {
        let p = normalize_registered_path(PathBuf::from(r"C:\Program Files\NapCatQQ Desktop\"));
        assert_eq!(p, PathBuf::from(r"C:\Program Files\NapCatQQ Desktop"));
    }

    #[test]
    fn select_prefers_env_then_user_then_machine() {
        let env = PathBuf::from(r"C:\env-root");
        let user = PathBuf::from(r"C:\user-root");
        let machine = PathBuf::from(r"C:\machine-root");
        assert_eq!(
            select_data_root_pointer(Some(env.clone()), Some(user.clone()), Some(machine.clone())),
            Some(env)
        );
        assert_eq!(
            select_data_root_pointer(None, Some(user.clone()), Some(machine.clone())),
            Some(user)
        );
        assert_eq!(
            select_data_root_pointer(None, None, Some(machine.clone())),
            Some(machine)
        );
        assert_eq!(select_data_root_pointer(None, None, None), None);
    }

    #[test]
    fn select_skips_unusable_env() {
        let user = PathBuf::from(r"C:\user-root");
        assert_eq!(
            select_data_root_pointer(Some(PathBuf::from("relative")), Some(user.clone()), None),
            Some(user)
        );
    }
}
