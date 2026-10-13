// Desktop 与 watch 共用的重登事件和指令编排。
mod command;
mod engine;
mod ledger;
#[cfg(test)]
mod tests;
pub use command::{Command, Incoming, parse_event};
pub use engine::ReloginManager;
pub use ledger::atomic_write;
pub use ledger::{AccountState, Observation};

use async_trait::async_trait;
use base64::Engine;
use ncd_domain::{SnowLumaQrLoginSnapshot, SnowLumaQrLoginState};

#[derive(Clone, PartialEq, Eq)]
pub struct QrReply {
    pub generation: String,
    pub snapshot: SnowLumaQrLoginSnapshot,
}

impl QrReply {
    pub fn image(&self) -> Option<String> {
        if self.snapshot.state != SnowLumaQrLoginState::WaitingForQr {
            return None;
        }
        let payload = self.snapshot.qrcode_payload.as_deref()?;
        let base64 = payload.strip_prefix("data:image/png;base64,")?;
        if base64.len() > 512 * 1024 {
            return None;
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(base64)
            .ok()?;
        bytes
            .starts_with(b"\x89PNG\r\n\x1a\n")
            .then(|| format!("base64://{base64}"))
    }
}

#[derive(Clone)]
pub struct Outgoing {
    pub text: String,
    pub image: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Delivery {
    Sent {
        message_id: String,
    },
    Rejected,
    /// 已交给网络但没有可信回执；自动换路会重复发送。
    Unknown,
}

#[async_trait]
pub trait ReloginPort: Send + Sync {
    async fn observe(&self, bot_id: &str) -> Option<Observation>;
    async fn can_send(&self, bot_id: &str) -> bool {
        self.observe(bot_id)
            .await
            .is_some_and(|o| o.state == AccountState::Online)
    }
    async fn qr(&self, bot_id: &str, refresh: bool) -> Result<QrReply, String>;
    async fn qr_is_current(&self, bot_id: &str, qr: &QrReply) -> bool {
        self.observe(bot_id)
            .await
            .is_some_and(|o| o.state == AccountState::Offline && o.generation == qr.generation)
    }
    async fn send(&self, messenger: &str, recipient: u64, message: Outgoing) -> Delivery;
    async fn send_group(&self, _messenger: &str, _group: u64, _text: String) -> Delivery {
        Delivery::Rejected
    }
    async fn fallback(&self, _bot_id: &str) {}
    async fn authorized(&self, _bot_id: &str, _administrator: Option<u64>) -> bool {
        true
    }
    async fn close_qr(&self, bot_id: &str);
}
