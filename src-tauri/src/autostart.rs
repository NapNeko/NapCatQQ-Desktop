//! 当前用户开机自启：Windows HKCU Run / Linux XDG autostart。
//!
//! 写 HKCU\Software\Microsoft\Windows\CurrentVersion\Run,不需要管理员/UAC。
//! 机器级 HKLM Run 或计划任务才要提权;本产品默认用户级即可,与「启动时仅托盘」
//! (uiModeOnStartup) 组合使用。
//!
//! 配置权威在 app-settings.json 的 launchOnStartup;本模块只负责把偏好落到注册表。
//! 启动时按配置双向 reconcile:开则刷新路径,关则删除本产品 Run 值,避免
//! 「JSON 已关但注册表仍在」导致仍开机自启。
//!
//! 可执行路径优先 MSI InstallDir 下的主程序,避免自更新/临时目录下 current_exe 漂移。

use std::path::{Path, PathBuf};

const AUTOSTART_ARG: &str = "--autostart";

/// 只将用户启动项视为登录自启；应用启动器和直接运行始终打开主窗口。
pub fn is_autostart_launch(args: impl IntoIterator<Item = impl AsRef<std::ffi::OsStr>>) -> bool {
    args.into_iter()
        .skip(1)
        .any(|arg| arg.as_ref() == AUTOSTART_ARG)
}

pub fn start_in_tray(mode: &ncd_domain::UiModeOnStartup, autostart: bool) -> bool {
    autostart && matches!(mode, ncd_domain::UiModeOnStartup::TrayOnly)
}

/// Run 键下的值名(与 productName 对齐,卸载/排障可按名查找)
pub const AUTOSTART_VALUE_NAME: &str = "NapCatQQ Desktop";

/// 安装树主程序文件名(与 Cargo [[bin]] / WiX Path 一致)
#[cfg(windows)]
const MAIN_EXE_NAME: &str = "NapCatQQ-Desktop.exe";

#[cfg(windows)]
const RUN_SUBKEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

/// Win32 ERROR_FILE_NOT_FOUND:删除不存在的值时视为已关闭
#[cfg(windows)]
const ERROR_FILE_NOT_FOUND: i32 = 2;

/// 按开关同步自启注册表项。
///
/// - enabled=true: 写入带引号的绝对路径
/// - enabled=false: 删除本产品值(不存在视为成功)
#[cfg(windows)]
pub fn apply_launch_on_startup(enabled: bool) -> Result<(), String> {
    if enabled {
        let exe = resolve_autostart_exe()?;
        set_run_value(&format_run_command(&exe))
    } else {
        remove_run_value()
    }
}

#[cfg(target_os = "linux")]
pub fn apply_launch_on_startup(enabled: bool) -> Result<(), String> {
    let entry = dirs::config_dir()
        .ok_or("无法确定用户配置目录")?
        .join("autostart/com.napcatqq.desktop.desktop");
    if !enabled {
        return match std::fs::remove_file(&entry) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(format!("删除自启动项失败: {error}")),
        };
    }
    // AppImage 的 current_exe 位于临时挂载目录，登录时应启动原始 AppImage。
    let exe = std::env::var_os("APPIMAGE")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .map(Ok)
        .unwrap_or_else(current_exe_path)?;
    let command = desktop_exec(&exe);
    let contents = format!(
        "[Desktop Entry]\nType=Application\nName=NapCatQQ Desktop\nExec={command}\nTerminal=false\n"
    );
    let directory = entry.parent().ok_or("自启动目录无效")?;
    std::fs::create_dir_all(directory).map_err(|e| format!("创建自启动目录失败: {e}"))?;
    std::fs::write(entry, contents).map_err(|e| format!("写入自启动项失败: {e}"))
}

#[cfg(target_os = "linux")]
fn desktop_exec(exe: &Path) -> String {
    // Exec 先经过 desktop-entry 字符串反转义，再解析带引号的参数。
    let escaped = exe
        .to_string_lossy()
        .replace('\\', r"\\\\")
        .replace('"', r#"\\\""#)
        .replace('`', r"\\`")
        .replace('$', r"\\$")
        .replace('%', "%%")
        .replace('\n', r"\n")
        .replace('\r', r"\r");
    format!("\"{escaped}\" {AUTOSTART_ARG}")
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn apply_launch_on_startup(_enabled: bool) -> Result<(), String> {
    Ok(())
}

/// 启动 reconcile:以 app-settings 为准收敛 HKCU Run。
///
/// - enabled=true: 刷新为本机稳定 exe 路径
/// - enabled=false: 删除本产品 Run 值(只动 AUTOSTART_VALUE_NAME)
///
/// 失败只记日志,不阻断启动。
#[cfg(any(windows, target_os = "linux"))]
pub fn reconcile_launch_on_startup(enabled: bool) {
    if let Err(err) = apply_launch_on_startup(enabled) {
        tracing::warn!(
            target: "ncd_tauri::autostart",
            enabled,
            error = %err,
            "failed to reconcile user autostart entry"
        );
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn reconcile_launch_on_startup(_enabled: bool) {}

/// 解析写入 Run 的 exe:优先 InstallDir 主程序,否则 current_exe。
#[cfg(windows)]
fn resolve_autostart_exe() -> Result<PathBuf, String> {
    if let Some(from_install) = install_dir_main_exe() {
        return Ok(from_install);
    }
    current_exe_path()
}

#[cfg(windows)]
fn install_dir_main_exe() -> Option<PathBuf> {
    let dir = crate::product_registry::read_install_dir()?;
    let candidate = dir.join(MAIN_EXE_NAME);
    if candidate.is_file() && candidate.is_absolute() {
        Some(candidate)
    } else {
        None
    }
}

fn current_exe_path() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| format!("current_exe: {e}"))?;
    if !exe.is_absolute() {
        return Err(format!("current_exe is not absolute: {}", exe.display()));
    }
    Ok(exe)
}

/// Run 值:路径含空格时必须整体加引号
#[cfg(any(windows, test))]
fn format_run_command(exe: &Path) -> String {
    let s = exe.to_string_lossy();
    if s.contains(' ') || s.contains('\t') {
        format!("\"{s}\" {AUTOSTART_ARG}")
    } else {
        format!("{s} {AUTOSTART_ARG}")
    }
}

#[cfg(windows)]
fn set_run_value(command: &str) -> Result<(), String> {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, KEY_SET_VALUE};

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (key, _) = hkcu
        .create_subkey_with_flags(RUN_SUBKEY, KEY_SET_VALUE)
        .map_err(|e| format!("open HKCU\\{RUN_SUBKEY}: {e}"))?;
    // winreg: ToRegValue for &str,无需再 to_string
    key.set_value(AUTOSTART_VALUE_NAME, &command)
        .map_err(|e| format!("set {AUTOSTART_VALUE_NAME}: {e}"))?;
    Ok(())
}

#[cfg(windows)]
fn remove_run_value() -> Result<(), String> {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, KEY_SET_VALUE};

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let key = match hkcu.open_subkey_with_flags(RUN_SUBKEY, KEY_SET_VALUE) {
        Ok(k) => k,
        Err(_) => return Ok(()),
    };
    match key.delete_value(AUTOSTART_VALUE_NAME) {
        Ok(()) => Ok(()),
        Err(err) => {
            // 只认 NotFound / ERROR_FILE_NOT_FOUND,不依赖中英文 Display 文案
            if err.kind() == std::io::ErrorKind::NotFound
                || err.raw_os_error() == Some(ERROR_FILE_NOT_FOUND)
            {
                Ok(())
            } else {
                Err(format!("delete {AUTOSTART_VALUE_NAME}: {err}"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn format_run_command_quotes_when_path_has_spaces() {
        let cmd = format_run_command(Path::new(r"C:\Program Files\NapCatQQ Desktop\app.exe"));
        assert_eq!(
            cmd,
            r#""C:\Program Files\NapCatQQ Desktop\app.exe" --autostart"#
        );
    }

    #[test]
    fn format_run_command_leaves_simple_paths_unquoted() {
        let cmd = format_run_command(Path::new(r"C:\Apps\ncd\app.exe"));
        assert_eq!(cmd, r"C:\Apps\ncd\app.exe --autostart");
    }

    #[test]
    fn format_run_command_quotes_when_path_has_tab() {
        let cmd = format_run_command(Path::new("C:\\Apps\\\tbad\\app.exe"));
        assert!(
            cmd.starts_with('"') && cmd.ends_with("\" --autostart"),
            "cmd={cmd}"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn desktop_exec_quotes_appimage_and_marks_login_autostart() {
        assert_eq!(
            desktop_exec(Path::new("/home/user/My Apps/NapCatQQ 100%.AppImage")),
            "\"/home/user/My Apps/NapCatQQ 100%%.AppImage\" --autostart"
        );
    }

    #[test]
    fn manual_start_opens_window_even_with_tray_only_preference() {
        for mode in [
            ncd_domain::UiModeOnStartup::Normal,
            ncd_domain::UiModeOnStartup::TrayOnly,
        ] {
            assert!(!start_in_tray(&mode, is_autostart_launch(["app"])));
        }
    }

    #[test]
    fn login_autostart_obeys_window_preference() {
        let autostart = is_autostart_launch(["app", "--autostart"]);
        assert!(autostart);
        assert!(start_in_tray(
            &ncd_domain::UiModeOnStartup::TrayOnly,
            autostart
        ));
        assert!(!start_in_tray(
            &ncd_domain::UiModeOnStartup::Normal,
            autostart
        ));
    }

    #[test]
    fn autostart_marker_must_be_a_separate_argument() {
        assert!(!is_autostart_launch(["/apps/--autostart"]));
        assert!(!is_autostart_launch(["app", "--autostart=false"]));
    }
}
