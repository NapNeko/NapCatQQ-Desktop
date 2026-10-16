//! 原生会话适配由宿主注入，值守状态机不拥有 QQ 生命周期。
use crate::NotifyBotTarget;
use async_trait::async_trait;
use ncd_domain::SnowLumaQrLoginSnapshot;
use ncd_relogin::Observation;
use std::sync::{Arc, OnceLock};

#[async_trait]
pub trait NativeWatchAdapter: Send + Sync {
    async fn observe(&self, bot: &NotifyBotTarget) -> Option<Observation>;
    async fn qr(&self, bot_id: &str, refresh: bool) -> Result<SnowLumaQrLoginSnapshot, String>;
    async fn close_qr(&self, bot_id: &str);
    async fn read_webui(&self, bot_id: &str, path: &str) -> Result<serde_json::Value, String>;
    fn process(&self, bot: &NotifyBotTarget) -> crate::probe::ProbeResult;
}

static ADAPTER: OnceLock<Arc<dyn NativeWatchAdapter>> = OnceLock::new();

pub fn install(adapter: Arc<dyn NativeWatchAdapter>) -> Result<(), String> {
    ADAPTER
        .set(adapter)
        .map_err(|_| "native watch adapter already installed".into())
}

pub fn adapter() -> Option<&'static Arc<dyn NativeWatchAdapter>> {
    ADAPTER.get()
}
