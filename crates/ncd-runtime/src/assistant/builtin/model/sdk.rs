//! SDK 负责协议编解码，NCD 保留连接、安全预算和工具执行边界。

mod guard;
mod reply;
mod request;
#[cfg(test)]
mod tests;

use super::super::stream::ThinkFilter;
use super::{
    ChatMessage, FinishReason, MAX_TEXT_BYTES, ModelError, ModelReply, ToolSpec,
    endpoint_url_for_format, is_loopback, valid_identifier,
};
use futures_util::StreamExt;
use genai::{
    Client,
    adapter::AdapterKind,
    chat::{ChatOptions, ChatRequest, ChatStreamEvent},
};
use guard::ModelStreamGuard;
use ncd_domain::assistant::{AssistantApiFormat, AssistantBuiltinSettings};
pub(super) use reply::validate_protocol;
use reply::{finish_reply, sdk_error};
use rustls_platform_verifier::BuilderVerifierExt;
use serde_json::json;
use std::{collections::HashSet, sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;

pub(super) struct SdkClient {
    client: Client,
    endpoint: reqwest::Url,
    model: String,
    api_key: String,
    format: AssistantApiFormat,
    max_output_tokens: u32,
    timeout: Duration,
}

impl SdkClient {
    pub(super) fn new(
        settings: &AssistantBuiltinSettings,
        key: &str,
        timeout: Duration,
    ) -> Result<Self, ModelError> {
        let endpoint = endpoint_url_for_format(&settings.base_url, settings.api_format)?;
        let model = settings.model.trim();
        if !valid_identifier(model, 256) {
            return Err(ModelError::InvalidModel);
        }
        if settings.max_output_tokens == 0
            || settings.max_output_tokens > 131_072
            || timeout.is_zero()
            || timeout > Duration::from_secs(3600)
            || key.len() > 4096
            || key.chars().any(char::is_control)
        {
            return Err(ModelError::InvalidConfiguration);
        }
        // 单独指定既有 ring，避免额外编译 AWS-LC 或修改进程默认 provider。
        let mut tls = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .map_err(|_| ModelError::InvalidConfiguration)?
        .with_platform_verifier()
        .map_err(|_| ModelError::InvalidConfiguration)?
        .with_no_client_auth();
        // 预配置 TLS 不经过 reqwest 的 ALPN 默认值填充。
        tls.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
        let mut http = reqwest_model::Client::builder()
            .tls_backend_preconfigured(tls)
            .redirect(reqwest_model::redirect::Policy::none())
            .retry(reqwest_model::retry::never())
            .referer(false)
            .connect_timeout(timeout.min(Duration::from_secs(30)))
            .timeout(timeout);
        if is_loopback(&endpoint) {
            http = http.no_proxy();
        }
        let http = http.build().map_err(|_| ModelError::InvalidConfiguration)?;
        let client = Client::builder()
            .with_reqwest(http)
            .with_adapter_kind(adapter(settings.api_format))
            .build()
            .map_err(|_| ModelError::InvalidConfiguration)?;
        Ok(Self {
            client,
            endpoint,
            model: model.to_owned(),
            api_key: key.to_owned(),
            format: settings.api_format,
            max_output_tokens: settings.max_output_tokens,
            timeout,
        })
    }

    pub(super) async fn complete(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolSpec],
        on_text: Arc<dyn Fn(&str) + Send + Sync>,
        cancel: &CancellationToken,
    ) -> Result<ModelReply, ModelError> {
        if cancel.is_cancelled() {
            return Err(ModelError::Cancelled);
        }
        let request = self.request(messages, tools)?;
        let previous_call_ids = messages
            .iter()
            .filter_map(|message| match message {
                ChatMessage::Assistant { tool_calls, .. } => Some(tool_calls),
                _ => None,
            })
            .flatten()
            .map(|call| call.id.clone())
            .collect();
        tokio::select! {
            biased;
            _ = cancel.cancelled() => Err(ModelError::Cancelled),
            result = tokio::time::timeout(self.timeout, self.send(request, on_text, previous_call_ids)) => result.map_err(|_| ModelError::Timeout)?,
        }
    }

    async fn send(
        &self,
        request: ChatRequest,
        on_text: Arc<dyn Fn(&str) + Send + Sync>,
        previous_call_ids: HashSet<String>,
    ) -> Result<ModelReply, ModelError> {
        let guard = Arc::new(ModelStreamGuard::new(self.format));
        let mut options = ChatOptions::default()
            .with_max_tokens(self.max_output_tokens)
            .with_capture_content(true)
            .with_capture_tool_calls(true)
            .with_capture_reasoning_content(true)
            .with_normalize_reasoning_content(false)
            .with_stream_guard(guard.clone())
            .with_stream_header_timeout(self.timeout.min(Duration::from_secs(30)))
            .with_stream_read_timeout(self.timeout);
        if self.format != AssistantApiFormat::GeminiGenerateContent {
            // 转发服务的模型标识可能带命名空间，不能由 SDK 改写。
            options = options.with_extra_body(json!({"model":self.model}));
        }
        if self.format == AssistantApiFormat::OpenAiChatCompletions
            && self.model.eq_ignore_ascii_case("minimax-m3.1-flash")
        {
            options.max_tokens = None;
            options = options.with_extra_body(json!({"model":self.model,"reasoning_split":true,"reasoning_effort":"low","max_completion_tokens":self.max_output_tokens}));
        }
        let response = self
            .client
            .exec_chat_stream(self.target()?, request, Some(&options))
            .await
            .map_err(|error| sdk_error(error, &guard))?;
        let mut stream = response.stream;
        let mut filter = ThinkFilter::default();
        let mut bytes = 0usize;
        while let Some(event) = stream.next().await {
            let event = event.map_err(|error| sdk_error(error, &guard))?;
            if let Some(error) = guard.error() {
                return Err(error);
            }
            match event {
                ChatStreamEvent::Chunk(chunk) => {
                    bytes = bytes.saturating_add(chunk.content.len());
                    if bytes > MAX_TEXT_BYTES {
                        return Err(ModelError::ResponseTooLarge);
                    }
                    filter.push(&chunk.content, on_text.as_ref());
                }
                ChatStreamEvent::End(end) => {
                    let reply = finish_reply(self.format, end, &guard, &previous_call_ids)?;
                    filter.finish(on_text.as_ref());
                    if !filter.has_visible_text()
                        && reply.tool_calls.is_empty()
                        && reply.finish_reason == FinishReason::Stop
                    {
                        return Err(ModelError::InvalidResponse);
                    }
                    return Ok(reply);
                }
                _ => {}
            }
        }
        Err(guard.error().unwrap_or(ModelError::IncompleteResponse))
    }
}

fn adapter(format: AssistantApiFormat) -> AdapterKind {
    match format {
        AssistantApiFormat::OpenAiChatCompletions => AdapterKind::OpenAI,
        AssistantApiFormat::OpenAiResponses => AdapterKind::OpenAIResp,
        AssistantApiFormat::AnthropicMessages => AdapterKind::Anthropic,
        AssistantApiFormat::GeminiGenerateContent => AdapterKind::Gemini,
    }
}
