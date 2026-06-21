//! 本地子进程启动时的跨平台修饰（目前仅 Windows 需要隐藏控制台窗口）。

/// 在 Windows 上为子进程加上 CREATE_NO_WINDOW，避免启动时闪多个 cmd 黑框。
/// 非 Windows 平台为空操作。
pub fn hide_console_window(cmd: &mut tokio::process::Command) {
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    {
        let _ = cmd;
    }
}