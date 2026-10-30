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
pub mod git;
pub mod host_download;
pub mod napcat;
pub mod ncd_watch;
pub mod nodejs;
pub mod novnc;
pub mod pkg_install_stream;
pub mod qq;
pub mod qq_deps;
pub mod qq_linux_pid;
pub mod redis;
pub mod remote_qq_entry;
pub mod requirement;
pub mod snowluma;
pub mod traits;
pub mod types;
pub mod uv;
pub mod vcredist;
pub mod windows_agent;
mod windows_install;
pub mod windows_watch;

pub use context::{ActionCtx, ProgressEvent, ProgressKind, ProgressLogLevel};
pub use desktop_self::DesktopSelfComponent;
pub use download::DownloadHelper;
pub use error::ActionError;
pub use git::{GitComponent, MINGIT_DEFAULT_VERSION};
pub use napcat::NapCatComponent;
pub use ncd_watch::{
    NcdWatchComponent, ncd_watch_asset_name, ncd_watch_bin_path, ncd_watch_release_download_url,
    ncd_watch_release_download_url_for_tag,
};
pub use nodejs::NodeJsComponent;
pub use novnc::NoVncComponent;
pub use qq::{
    MANAGED_QQ_DIR_NAME, QQ_PCCONFIG_URL, QQ_REGISTRY_SUBKEY, QQComponent, managed_qq_current_dir,
    managed_qq_root, probe_linux_qq_latest, probe_windows_qq_latest,
};
pub use qq_linux_pid::{LINUX_QQ_PROCESS_PROBE, linux_qq_running_pid_script};
pub use redis::RedisComponent;

pub use remote_qq_entry::{
    QQ_MAIN_NAPCAT_INJECT, QQ_MAIN_NATIVE, load_napcat_js_path, napcat_mjs_path,
    qq_package_json_path, set_remote_qq_package_main,
};
pub use requirement::{
    DependencyNode, DependencyPlan, DependencyTarget, HostPackageGroup, Requirement,
    RequirementPhase, RequirementStatus, RuntimeReadiness, VersionReq, all_versions_match,
};
pub use snowluma::SnowLumaComponent;
pub use traits::{Action, Component};
pub use types::{
    ComponentCategory, ComponentDetectResult, ComponentId, ComponentInfo, DetectOutcome,
    DetectedVersion, LaunchArgs, SupportedTarget, UninstallSupport, UnusableInstall, VerifyReport,
};
pub use uv::{UV_DEFAULT_VERSION, UvComponent};
pub use vcredist::VcRedistComponent;
pub use windows_agent::WindowsAgentComponent;
pub use windows_watch::WindowsNcdWatchComponent;
