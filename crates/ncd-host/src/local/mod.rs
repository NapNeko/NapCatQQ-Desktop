//! 本地主机实装与按当前平台选择的统一入口。

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "linux")]
mod pty_linux;
#[cfg(target_os = "linux")]
pub use linux::LocalLinuxHost;

#[cfg(windows)]
pub mod windows;

#[cfg(windows)]
pub(crate) mod elevate;

#[cfg(windows)]
pub(crate) mod pty_windows;

#[cfg(windows)]
pub use pty_windows::conpty_available;
#[cfg(windows)]
pub use windows::LocalWindowsHost;

#[cfg(any(windows, target_os = "linux"))]
pub fn local_host() -> std::sync::Arc<dyn crate::Host> {
    #[cfg(windows)]
    let host = LocalWindowsHost::new();
    #[cfg(target_os = "linux")]
    let host = LocalLinuxHost::new();
    std::sync::Arc::new(host)
}

#[cfg(any(windows, target_os = "linux"))]
pub fn local_host_for_data_root(root: &std::path::Path) -> std::sync::Arc<dyn crate::Host> {
    #[cfg(windows)]
    {
        let _ = root;
        local_host()
    }
    #[cfg(target_os = "linux")]
    std::sync::Arc::new(LocalLinuxHost::new().with_managed_root(root))
}
