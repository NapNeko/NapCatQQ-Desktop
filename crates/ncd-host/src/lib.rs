//! `ncd-host`:NapCatQQ-Desktop 的"主机"抽象层。
//!
//! `Host` trait 把"一台机器"(本地 Windows / 远端 Linux / 远端 Windows stub /
//! 未来的 macOS / Docker / Agent)抽成统一接口,所有"装组件 / 跑命令 / 传文件 /
//! 开隧道 / 提权"操作都通过它完成。
//!
//! 设计意图是消除 legacy "本地"和"远端"两套完全独立实装(installation/ vs remote/)
//! 同段逻辑写两遍的问题:`Host` trait 让上层代码(`ncd-component` 中的
//! `NapCatComponent::install` 等)只关心"我要做什么",不关心"在哪台机器上"。
//!
//! 与 `ncd-component`(Component 维度)、`ncd-deploy`(编排 Action)合起来构成
//! Component × Host × Action 三维模型。
//!
//! 实装清单:
//! - `Host` / `HostShell` / `PackageManager` trait 定义
//! - `HostPath` / `HostCommand` / `CommandOutput` / `HostProcess` 跨平台数据类型
//! - `HostError` 错误体系
//! - `LocalWindowsHost` 实装(`local::windows`,`#[cfg(windows)]`)
//! - `RemoteLinuxHost` 实装(基于 russh / russh-sftp)
//! - `RemoteWindowsHost` 接口预留 stub,所有方法返回 `HostError::Unsupported`
//!
//! 跨平台约束:
//! - 所有路径用 [`HostPath`] 表达,内部统一 POSIX 风格,落地时由各 Host 实装做转换
//! - 所有命令用 [`HostCommand`] 构建,shell escape 委托给 [`HostShell`]
//! - 各 OS 差异由 [`Host::os`] / [`Host::pkg_manager`] / [`Host::shell`] 暴露,
//!   Component 内部 `match host.os() { ... }` 决策

pub mod apt_lock;
pub mod command;
pub mod error;
pub mod host;
pub mod local;
pub mod package_manager;
pub mod pkg_output;
pub mod path;
pub mod process;
pub mod remote;
pub mod shell;
pub mod stream_chunk;
pub mod subprocess;

pub use apt_lock::{
    dpkg_lock_wait_preamble_sh, host_command_wrap_dpkg_wait_for_apt,
    output_indicates_dpkg_lock_hold, wrap_sh_script_with_dpkg_wait,
};
pub use command::{CommandOutput, HostCommand};
pub use error::HostError;
pub use host::{Arch, Host, Locality, Os, StreamSource};
pub use package_manager::{PackageInfo, PackageManager, PackageManagerKind};
pub use pkg_output::{
    fallback_percent_from_line_no, parse_pkg_mgr_line, truncate_pkg_line, PkgLineParse,
    PkgMgrFamily, PkgPhase,
};
pub use path::{ArchiveKind, DirEntry, HostPath, PathStyle};
pub use process::{ExitStatus, HostProcess, ProcessId};
pub use shell::{HostShell, ShellKind};
pub use subprocess::hide_console_window;
