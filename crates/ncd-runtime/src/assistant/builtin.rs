//! 内置模型仅通过当前诊断会话的受限 MCP 调用 NCD。

mod history;
mod mcp;
mod model;
mod stream;
#[cfg(test)]
mod tests;
mod turn;

use std::sync::Arc;

use async_trait::async_trait;
use ncd_domain::assistant::{
    AssistantBuiltinSettings, AssistantQuestion, AssistantQuestionAnswer, AssistantToolCall,
};
use ncd_traits::assistant::AssistantToolConnection;
use tokio_util::sync::CancellationToken;

pub(super) use history::History;
pub(super) use model::endpoint_url;

pub(super) struct BuiltinRunContext {
    pub profile: AssistantBuiltinSettings,
    pub api_key: String,
    pub connection: AssistantToolConnection,
    pub cancelled: CancellationToken,
}

#[async_trait]
pub(super) trait BuiltinSink: Send + Sync {
    fn text(&self, text: &str);
    fn tool(&self, tool: AssistantToolCall);
    fn redact(&self, text: &str) -> String;
    async fn question(&self, question: AssistantQuestion) -> AssistantQuestionAnswer;
    async fn checkpoint(&self, history: &History) -> Result<(), String>;
}

pub(super) async fn run(
    context: BuiltinRunContext,
    history: &mut History,
    system_prompt: &str,
    input: &str,
    sink: Arc<dyn BuiltinSink>,
) -> Result<(), String> {
    turn::run(context, history, system_prompt, input, sink).await
}
