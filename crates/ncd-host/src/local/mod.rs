//! 本地主机实装(`LocalWindowsHost` / 未来的 `LocalLinuxHost` / `LocalMacOsHost`)。
//!
//! 蓝图 §6.3 的 "实装与预留矩阵":
//! - **M3.2**:`LocalWindowsHost`(本节)
//! - 后续:`LocalLinuxHost` / `LocalMacOsHost` 由 `#[cfg(target_os = ...)]` 启用

#[cfg(windows)]
pub mod windows;

#[cfg(windows)]
pub use windows::LocalWindowsHost;
