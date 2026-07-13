//! SnowLuma WebUI HTTP 客户端
//!
//! - `types`: 强类型 payload
//! - `trait_`: [`SnowLumaWebUiClient`]
//! - `client`: [`ReqwestSnowLumaWebUiClient`]

mod client;
mod trait_;
mod types;

pub use client::{
    ReqwestSnowLumaWebUiClient, ReqwestSnowLumaWebUiClientFactory, snowluma_error_requires_consent,
};
pub use trait_::SnowLumaWebUiClient;
pub use types::{
    AgreementDoc, AgreementsPayload, AuthState, HookProcessInfo, HookProcessStatus,
    OneBotInstanceInfo, QqPortLoginInfo,
};

// 测试 `use super::*` 对齐旧单体模块内可见性
// 仅 re-export 测试实际用到的符号
#[cfg(test)]
pub(crate) use crate::snowluma::error::SnowLumaWebUiError;
#[cfg(test)]
pub(crate) use client::{ordered_candidates, validate_host};
#[cfg(test)]
pub(crate) use std::time::Duration;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
