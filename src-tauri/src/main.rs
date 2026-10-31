#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(target_os = "linux")]
mod linux_webview;

fn main() {
    #[cfg(target_os = "linux")]
    // SAFETY: 进程入口尚未初始化 Tauri、GTK 或任何工作线程。
    unsafe {
        linux_webview::configure_before_threads();
    }
    ncd_tauri::run();
}
