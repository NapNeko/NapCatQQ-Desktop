// 运行截图快捷键隔离测试，不连接真实账号。
#[cfg(windows)]
#[path = "screenshot_shortcut_smoke/support.rs"]
mod support;
#[cfg(windows)]
pub(crate) use support::{AppState, chat_window, native_panel, webview_scheduler, windows_ui};

fn main() {
    #[cfg(windows)]
    support::run();
    #[cfg(not(windows))]
    eprintln!("原生截图快捷键测试仅支持 Windows");
}
