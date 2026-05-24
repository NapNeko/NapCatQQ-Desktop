//! `ncd-host`:NapCatQQ-Desktop 的"主机"抽象层。
//!
//! 蓝图 §6:`Host` trait 把"一台机器"(本地 Windows / 远端 Linux / 远端 Windows
//! stub / 未来的 macOS / Docker / Agent)抽成统一接口,所有"装组件 / 跑命令 /
//! 传文件 / 开隧道 / 提权"操作都通过它完成。
//!
//! ## 设计意图
//!
//! - **解决 legacy 痛点**:legacy 把"本地"和"远端"做成两套完全独立的实装(installation/
//!   vs remote/),同一段逻辑要写两遍。`Host` trait 让上层代码(`ncd-component` 中的
//!   `NapCatComponent::install` 等)只关心"我要做什么",不关心"在哪台机器上"。
//!
//! - **Component × Host × Action 三维模型**(蓝图 §5):本 crate 提供 Host 维度,
//!   `ncd-component` 提供 Component 维度,`ncd-deploy` 编排 Action。
//!
//! ## 当前状态(M3.1)
//!
//! - ✅ `Host` / `HostShell` / `PackageManager` trait 定义
//! - ✅ `HostPath` / `HostCommand` / `CommandOutput` / `HostProcess` 跨平台数据类型
//! - ✅ `HostError` 错误体系
//! - ⏳ `LocalWindowsHost` 实装(M3.2)
//! - ⏳ `RemoteLinuxHost` 实装(M3.3,引入 russh)
//! - ⏳ `RemoteWindowsHost` stub(M3.4,接口预留)
//!
//! ## 跨平台铁律(蓝图 §6 + §10.4)
//!
//! - 所有路径用 [`HostPath`] 表达,内部统一 POSIX 风格,落地时由各 Host 实装做转换
//! - 所有命令用 [`HostCommand`] 构建,shell escape 委托给 [`HostShell`]
//! - 各 OS 差异由 [`Host::os`] / [`Host::pkg_manager`] / [`Host::shell`] 暴露,
//!   Component 内部 `match host.os() { ... }` 决策

pub mod command;
pub mod error;
pub mod host;
pub mod local;
pub mod package_manager;
pub mod path;
pub mod process;
pub mod shell;

pub use command::{CommandOutput, HostCommand};
pub use error::HostError;
pub use host::{Arch, Host, Locality, Os};
pub use package_manager::{PackageInfo, PackageManager, PackageManagerKind};
pub use path::{ArchiveKind, DirEntry, HostPath, PathStyle};
pub use process::{ExitStatus, HostProcess, ProcessId};
pub use shell::{HostShell, ShellKind};
