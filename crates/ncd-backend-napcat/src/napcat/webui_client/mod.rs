//! NapCat WebUI HTTP 客户端

mod client;
mod error;
mod payloads;
mod trait_;

pub use client::ReqwestNapCatWebUiClient;
pub use error::NapCatWebUiError;
pub use payloads::{
    AuthLoginData, AuthLoginRequest, AuthLoginResponse, CheckLoginStatusData,
    CheckLoginStatusResponse, GetQQLoginInfoData, GetQQLoginInfoResponse,
};
pub use trait_::NapCatWebUiClient;

// 测试 `use super::*` 需要的额外符号（已 pub use 的不必再 re-export）
#[cfg(test)]
pub(crate) use async_trait::async_trait;
#[cfg(test)]
pub(crate) use sha2::{Digest, Sha256};
#[cfg(test)]
pub(crate) use std::time::Duration;

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
