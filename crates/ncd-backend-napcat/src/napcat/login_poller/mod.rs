//! NapCat WebUI 登录轮询组件
//!
//! 暴露 [NapCatLoginPoller] 与其依赖:[PollerConfig],[PollerDeps],
//! [RestartHandle]。状态转移见 `transitions`；主循环见 `loop_`。

mod loop_;
mod transitions;
mod types;

pub use types::{NapCatLoginPoller, PollerConfig, PollerDeps, RestartHandle};

// 测试文件用 `use super::*` 对齐旧单体模块可见性
// 仅 re-export 测试实际用到的符号,避免 unused_imports 噪声
#[cfg(test)]
pub(crate) use loop_::{adjust_status_interval, do_auth_refresh, do_status_poll};
#[cfg(test)]
pub(crate) use transitions::{apply_login_status, apply_online_status};
#[cfg(test)]
pub(crate) use types::{LoginState, PollerCommand};

#[cfg(test)]
pub(crate) use crate::napcat::offline_notifier::OfflineNotifier;
#[cfg(test)]
pub(crate) use crate::napcat::webui_client::{
    CheckLoginStatusData, GetQQLoginInfoData, NapCatWebUiClient, NapCatWebUiError,
};
#[cfg(test)]
pub(crate) use async_trait::async_trait;
#[cfg(test)]
pub(crate) use ncd_domain::domain_event::DomainEvent;
#[cfg(test)]
pub(crate) use ncd_domain::ids::BotId;
#[cfg(test)]
pub(crate) use ncd_domain::napcat_events::NapCatLoginInvalidationReason;
#[cfg(test)]
pub(crate) use ncd_traits::events::{BroadcastEventBus, EventBus};
#[cfg(test)]
pub(crate) use std::sync::Arc;
#[cfg(test)]
pub(crate) use std::time::Duration;
#[cfg(test)]
pub(crate) use tokio::sync::mpsc;
#[cfg(test)]
pub(crate) use tokio::time::{MissedTickBehavior, interval};
#[cfg(test)]
pub(crate) use tokio_util::sync::CancellationToken;

#[cfg(test)]
#[path = "tests_unit.rs"]
mod tests;

#[cfg(test)]
#[path = "tests_transition.rs"]
mod transition_tests;

#[cfg(test)]
#[path = "tests_property.rs"]
mod property_tests;
