//! ncd-watch: 远端主机侧监控（进程/容器探活 + Desktop 离线时 Webhook）
//!
//! 第一期不做启停、不监听端口。配置由 Desktop 经 SFTP 下发。
//!
//! 依赖面:
//! - 默认 / `daemon`: 完整探活 + Webhook + CLI(远端 musl 二进制)
//! - `default-features = false`: 仅 config schema,供 Desktop 写 notify.json / present

pub mod config;

#[cfg(feature = "daemon")]
pub mod edge;
#[cfg(feature = "daemon")]
pub mod present;
#[cfg(feature = "daemon")]
pub mod probe;
#[cfg(feature = "daemon")]
pub mod run;
#[cfg(feature = "daemon")]
pub mod webhook;

pub use config::{
    DesktopPresentFile, NotifyBotTarget, NotifyConfig, WatchConfig, WatchPaths, WatchRoot,
};

#[cfg(feature = "daemon")]
pub use edge::{EdgeAction, EdgeTracker};
#[cfg(feature = "daemon")]
pub use present::desktop_is_present;
#[cfg(feature = "daemon")]
pub use probe::{ProbeKind, ProbeResult, ProbeStatus, Prober};
#[cfg(feature = "daemon")]
pub use run::{RunOnceOutcome, run_loop, run_once};
#[cfg(feature = "daemon")]
pub use webhook::send_watch_webhooks;
