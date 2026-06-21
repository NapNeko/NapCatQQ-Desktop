// Windows Toast:显式 AppUserModelID,避免 notify-rust 在 target 目录下回退 PowerShell

use std::path::Path;

#[cfg(windows)]
pub fn prepare_windows_toast_identity(app: &tauri::AppHandle) {
    let aumid = app.config().identifier.clone();
    if set_process_app_user_model_id(&aumid).is_err() {
        tracing::warn!(%aumid, "SetCurrentProcessExplicitAppUserModelID failed");
    }

    let Ok(exe) = tauri::utils::platform::current_exe() else {
        return;
    };
    if !exe_in_cargo_output(&exe) {
        return;
    }

    let product = app
        .config()
        .product_name
        .clone()
        .unwrap_or_else(|| "NapCatQQ Desktop".to_string());
    let exe_for_thread = exe.clone();
    std::thread::spawn(move || {
        if let Err(err) = ensure_start_menu_shortcut_hidden(&exe_for_thread, &product) {
            tracing::debug!(%err, "start menu shortcut for toast AUMID skipped");
        }
    });
}

#[cfg(windows)]
pub fn show_desktop_toast(aumid: &str, headline: &str, body: &str) {
    use tauri_winrt_notification::Toast;
    if let Err(err) = Toast::new(aumid)
        .title(headline)
        .text1(body)
        .show()
    {
        tracing::warn!(%err, "winrt toast show failed");
    }
}

#[cfg(not(windows))]
pub fn show_desktop_toast(_aumid: &str, _headline: &str, _body: &str) {}

#[cfg(windows)]
fn exe_in_cargo_output(exe: &Path) -> bool {
    let Some(exe_dir) = exe.parent() else {
        return false;
    };
    let dir = exe_dir.display().to_string();
    dir.contains("\\target\\debug")
        || dir.contains("\\target\\release")
        || dir.contains("/target/debug")
        || dir.contains("/target/release")
}

#[cfg(windows)]
#[allow(unsafe_code)] // Windows FFI: SetCurrentProcessExplicitAppUserModelID 必须用 unsafe
fn set_process_app_user_model_id(aumid: &str) -> Result<(), windows::core::Error> {
    use windows::core::HSTRING;
    use windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;
    unsafe { SetCurrentProcessExplicitAppUserModelID(&HSTRING::from(aumid)) }
}

#[cfg(windows)]
fn ensure_start_menu_shortcut_hidden(exe: &Path, display_name: &str) -> std::io::Result<()> {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let appdata = std::env::var("APPDATA").map_err(std::io::Error::other)?;
    let programs = Path::new(&appdata).join("Microsoft/Windows/Start Menu/Programs");
    std::fs::create_dir_all(&programs)?;
    let safe_name = display_name.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_");
    let lnk = programs.join(format!("{safe_name}.lnk"));
    if lnk.is_file() {
        return Ok(());
    }

    let target = exe.display().to_string().replace('\'', "''");
    let work_dir = exe
        .parent()
        .map(|p| p.display().to_string().replace('\'', "''"))
        .unwrap_or_default();
    let lnk_esc = lnk.display().to_string().replace('\'', "''");

    let ps = format!(
        "$s = (New-Object -ComObject WScript.Shell).CreateShortcut('{lnk_esc}'); \
         $s.TargetPath = '{target}'; \
         $s.WorkingDirectory = '{work_dir}'; \
         $s.Description = '{safe_name}'; \
         $s.Save()"
    );

    let status = std::process::Command::new("powershell")
        .creation_flags(CREATE_NO_WINDOW)
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &ps,
        ])
        .status()?;
    if !status.success() {
        return Err(std::io::Error::other(format!(
            "powershell shortcut exit: {status}"
        )));
    }
    tracing::info!("created Start Menu shortcut for toast AppUserModelID");
    Ok(())
}

#[cfg(not(windows))]
pub fn prepare_windows_toast_identity(_app: &tauri::AppHandle) {}