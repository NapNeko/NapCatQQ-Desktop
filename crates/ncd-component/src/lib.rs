//! ncd-component:NapCatQQ-Desktop 的"组件"抽象
//!
//! 把"装什么"(NapCat / SnowLuma / QQ / Node / noVNC / Desktop 自身 / ncd-watch)
//! 抽成统一的 [Component] trait,与 [ncd_host::Host] trait 正交
//!
//! 与 ncd-host(WHERE,提供 Host trait),ncd-deploy(VERB,负责把
//! Component 列表 + Host 编排成 DeployPlan)合起来构成 Component × Host ×
//! Action 三维抽象,本 crate 是中间一层
//!
//! 核心设计原则:
//! - Component 不知道在哪台机器,只描述"我是什么,URL,SHA256,装在哪个目录,
//!   启动命令长啥样"
//! - Host 不知道在装什么,只提供 read_file / spawn / extract_archive 等能力
//! - ncd-deploy 负责编排:把 Component 列表 + Host 拼成 DeployPlan

pub mod context;
pub mod desktop_self;
pub mod download;
pub mod error;
pub mod host_download;
pub mod napcat;
pub mod ncd_watch;
pub mod nodejs;
pub mod novnc;
pub mod pkg_install_stream;
pub mod qq;
pub mod qq_deps;
pub mod remote_qq_entry;
pub mod snowluma;
pub mod traits;
pub mod types;

pub use context::{ActionCtx, ProgressEvent, ProgressKind, ProgressLogLevel};
pub use desktop_self::DesktopSelfComponent;
pub use download::DownloadHelper;
pub use error::ActionError;
pub use host_download::download_url_to_host_with_progress;
pub use napcat::NapCatComponent;
pub use ncd_watch::{
    NcdWatchComponent, discover_local_ncd_watch_binary, ncd_watch_asset_name,
    ncd_watch_musl_target, ncd_watch_release_download_url, ncd_watch_release_download_url_for_tag,
    ncd_watch_version_from_tag, normalize_detected_version, probe_remote_arch,
};
pub use nodejs::NodeJsComponent;
pub use novnc::NoVncComponent;
pub use pkg_install_stream::run_pkg_command_with_progress;
pub use qq::{
    LinuxQqRelease, NCLATEST_QQ_VER_URL, QQ_PCCONFIG_URL, QQ_URL_SIGN_URL, QQComponent,
    probe_linux_qq_latest, probe_windows_qq_latest,
};

pub use remote_qq_entry::{QQ_MAIN_NAPCAT_INJECT, QQ_MAIN_NATIVE, set_remote_qq_package_main};
pub use snowluma::SnowLumaComponent;
pub use traits::{Action, Component};
pub use types::{
    ComponentCategory, ComponentDetectResult, ComponentId, ComponentInfo, DetectedVersion,
    LaunchArgs, SupportedTarget, VerifyReport,
};

// Linux 包管理器,novnc / qq_deps 共用
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PkgMgr {
    Apt,
    Dnf,
}

// sh -c 命令手动拼路径时用,委托 ncd_host::BashShell 的单引号转义
pub(crate) fn shell_quote(s: &str) -> String {
    use ncd_host::HostShell;
    ncd_host::shell::BashShell.escape(s)
}
