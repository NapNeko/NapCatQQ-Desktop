//! Windows 产品路径注册表(安装目录 / 数据目录)
//!
//! 契约键(64 位本机,管理员安装):
//!   HKLM\SOFTWARE\NapCatQQ-Desktop
//!     InstallDir  REG_SZ  程序安装根(含 NapCatQQ-Desktop.exe)
//!     DataRoot    REG_SZ  数据根(配置/组件/日志;可迁移)
//!
//! MSI 安装时写入默认值;启动时若缺键则尽量补写(旧 MSI / 注册表被清)。
//! 已有 DataRoot 绝不覆盖,以便后续「迁移数据目录」只改指针。
//! 卸载 CA / 排障工具读同一套键;业务配置仍只落在 DataRoot 文件树。
//!
//! 非 Windows 编译为空操作,保持 resolve 路径单一入口。

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
    pub data_root: Option<PathBuf>,
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

#[cfg(windows)]
pub fn read_product_paths() -> ProductPaths {
    read_product_paths_from_hklm().unwrap_or_default()
}

#[cfg(not(windows))]
pub fn read_product_paths() -> ProductPaths {
    ProductPaths::default()
}

#[cfg(windows)]
pub fn read_data_root() -> Option<PathBuf> {
    read_product_paths().data_root
}

#[cfg(not(windows))]
pub fn read_data_root() -> Option<PathBuf> {
    None
}

#[cfg(windows)]
pub fn read_install_dir() -> Option<PathBuf> {
    read_product_paths().install_dir
}

#[cfg(not(windows))]
pub fn read_install_dir() -> Option<PathBuf> {
    None
}

/// 启动兜底:缺 `InstallDir` / `DataRoot` 时补写,已有值不改。
/// HKLM 写失败(无管理员权限等)只记结果,不阻断启动。
#[cfg(windows)]
pub fn ensure_product_paths_registered(data_root: &Path) -> EnsureRegistryReport {
    let mut report = EnsureRegistryReport::default();
    let current = read_product_paths();

    let install_dir = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|p| p.to_path_buf()))
        .map(normalize_registered_path)
        .filter(|p| is_usable_absolute_path(p));

    if current.install_dir.is_none() {
        if let Some(dir) = install_dir.as_ref() {
            match write_value(VALUE_INSTALL_DIR, dir) {
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
        report.install_dir = current.install_dir;
    }

    let data_norm = normalize_registered_path(data_root.to_path_buf());
    if current.data_root.is_none() {
        if is_usable_absolute_path(&data_norm) {
            match write_value(VALUE_DATA_ROOT, &data_norm) {
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
        report.data_root = current.data_root;
    }

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
    pub wrote_install_dir: bool,
    pub wrote_data_root: bool,
    pub errors: Vec<String>,
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
    })
}

#[cfg(windows)]
fn read_path_value(key: &winreg::RegKey, name: &str) -> Option<PathBuf> {
    let raw: String = key.get_value(name).ok()?;
    let path = normalize_registered_path(PathBuf::from(raw));
    is_usable_absolute_path(&path).then_some(path)
}

#[cfg(windows)]
fn write_value(name: &str, path: &Path) -> Result<(), String> {
    use winreg::RegKey;
    use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_WRITE};

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let (key, _) = hklm
        .create_subkey_with_flags(PRODUCT_REGISTRY_KEY, KEY_WRITE)
        .map_err(|e| e.to_string())?;
    let value = path.to_string_lossy().into_owned();
    key.set_value(name, &value).map_err(|e| e.to_string())
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
}
