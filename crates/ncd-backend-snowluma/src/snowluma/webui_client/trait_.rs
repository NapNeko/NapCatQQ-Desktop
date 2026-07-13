//! SnowLumaWebUiClient trait

use std::time::Duration;

use async_trait::async_trait;

use super::types::{
    AgreementsPayload, AuthState, HookProcessInfo, OneBotInstanceInfo, QqPortLoginInfo,
};
use crate::snowluma::error::SnowLumaWebUiError;

#[async_trait]
pub trait SnowLumaWebUiClient: Send + Sync {
    async fn wait_ready(
        &self,
        timeout: Duration,
        dead_check: Box<dyn Fn() -> bool + Send + Sync>,
    ) -> Result<(), SnowLumaWebUiError>;

    async fn login(&self) -> Result<(), SnowLumaWebUiError>;

    async fn logout(&self) -> Result<(), SnowLumaWebUiError>;

    async fn list_processes(&self) -> Result<Vec<HookProcessInfo>, SnowLumaWebUiError>;

    async fn list_qq_instances(&self) -> Result<Vec<OneBotInstanceInfo>, SnowLumaWebUiError>;

    async fn probe_process_login_info(
        &self,
        pid: u32,
    ) -> Result<Option<QqPortLoginInfo>, SnowLumaWebUiError>;

    async fn load_process(&self, pid: u32) -> Result<HookProcessInfo, SnowLumaWebUiError>;

    async fn unload_process(&self, pid: u32) -> Result<HookProcessInfo, SnowLumaWebUiError>;

    async fn get_auth_state(&self) -> Result<AuthState, SnowLumaWebUiError>;

    async fn get_agreements(&self) -> Result<AgreementsPayload, SnowLumaWebUiError>;

    async fn record_agreement_consent(&self, version: &str) -> Result<(), SnowLumaWebUiError>;

    async fn update_onebot_config(
        &self,
        uin: &str,
        config: &serde_json::Value,
    ) -> Result<bool, SnowLumaWebUiError>;
}
