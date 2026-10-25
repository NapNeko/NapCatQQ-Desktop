//! 诊断会话与宿主工具的边界，凭据不经过桌面 IPC。

use std::sync::Arc;

use async_trait::async_trait;
use ncd_domain::assistant::{AssistantEvent, AssistantPermissionRequest};
use tokio_util::sync::CancellationToken;

pub trait AssistantEventSink: Send + Sync {
    fn send(&self, event: &AssistantEvent) -> bool;
}

#[async_trait]
pub trait AssistantInteraction: Send + Sync {
    async fn request_permission(
        &self,
        request: AssistantPermissionRequest,
    ) -> Result<Option<String>, String>;
}

pub struct AssistantToolConnection {
    pub url: String,
    pub bearer_token: String,
}

pub struct AssistantContext {
    pub bot_name: String,
    pub summary: String,
}

#[async_trait]
pub trait AssistantToolHost: Send + Sync {
    async fn context(&self, bot_id: &str) -> Result<AssistantContext, String>;

    async fn open(
        &self,
        session_id: &str,
        bot_id: &str,
        interaction: Arc<dyn AssistantInteraction>,
        cancelled: CancellationToken,
    ) -> Result<AssistantToolConnection, String>;

    async fn close(&self, session_id: &str);

    fn redact(&self, text: &str) -> String;
}
