//! ncd-watch: 远端主机侧监控（进程/容器探活 + Desktop 离线时 Webhook）
//!
//! 第一期不做启停、不监听端口。配置由 Desktop 经 SFTP 下发。

pub mod config;
pub mod edge;
pub mod present;
pub mod probe;
pub mod run;
pub mod webhook;

pub use config::{
    DesktopPresentFile, NotifyBotTarget, NotifyConfig, WatchConfig, WatchPaths, WatchRoot,
};
pub use edge::{EdgeAction, EdgeTracker};
pub use present::desktop_is_present;
pub use probe::{ProbeKind, ProbeResult, ProbeStatus, Prober};
pub use run::{RunOnceOutcome, run_loop, run_once};
pub use webhook::send_watch_webhooks;
