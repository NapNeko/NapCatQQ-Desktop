//! 内置助手的模型协议请求、私有历史与固定错误边界。

use std::{net::IpAddr, sync::Arc, time::Duration};

use ncd_domain::assistant::{AssistantApiFormat, AssistantBuiltinSettings};
#[cfg(test)]
use reqwest::{Client, Url, header::HeaderValue};
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[cfg(test)]
use serde_json::json;
use tokio_util::sync::CancellationToken;

mod endpoints;
#[cfg(test)]
mod legacy;
mod sdk;
#[cfg(test)]
pub(in crate::assistant) use endpoints::endpoint_url;
pub(in crate::assistant) use endpoints::endpoint_url_for_format;
#[cfg(test)]
use legacy::wire_message;

#[cfg(test)]
mod tests;

pub(super) const MAX_ARGUMENT_BYTES: usize = 32 * 1024;
pub(super) const MAX_TEXT_BYTES: usize = 128 * 1024;
pub(super) const MAX_REASONING_BYTES: usize = 64 * 1024;
pub(super) const MAX_TOOL_CALLS: usize = 16;
const MAX_REQUEST_BYTES: usize = 1024 * 1024;
const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "lowercase")]
pub(in crate::assistant) enum ChatMessage {
    System {
        content: String,
    },
    User {
        content: String,
    },
    Assistant {
        content: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reasoning_content: Option<String>,
        #[serde(default)]
        tool_calls: Vec<ToolCall>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        protocol: Option<ProtocolContent>,
    },
    Tool {
        tool_call_id: String,
        content: String,
    },
}

#[derive(Clone, Serialize, Deserialize)]
pub(in crate::assistant) struct ProtocolContent {
    pub format: AssistantApiFormat,
    pub content: genai::chat::MessageContent,
}

impl ProtocolContent {
    pub(super) fn validate(&self, calls: &[ToolCall]) -> Result<(), ModelError> {
        sdk::validate_protocol(self, calls)
    }

    pub(super) fn sanitize(&mut self, redact: impl Fn(&str) -> String) -> Result<(), String> {
        fn clean(
            value: Value,
            redact: &impl Fn(&str) -> String,
            depth: usize,
        ) -> Result<Value, String> {
            if depth > 32 {
                return Err("内置助手私有协议内容嵌套过深".into());
            }
            Ok(match value {
                Value::String(text) => Value::String(redact(&text)),
                Value::Array(items) => Value::Array(
                    items
                        .into_iter()
                        .map(|value| clean(value, redact, depth + 1))
                        .collect::<Result<_, _>>()?,
                ),
                Value::Object(items) => Value::Object(
                    items
                        .into_iter()
                        .map(|(key, value)| Ok((redact(&key), clean(value, redact, depth + 1)?)))
                        .collect::<Result<_, String>>()?,
                ),
                value => value,
            })
        }
        let value =
            serde_json::to_value(&self.content).map_err(|_| "无法组织内置助手私有协议历史")?;
        self.content = serde_json::from_value(clean(value, &redact, 0)?)
            .map_err(|_| "内置助手私有协议历史无效")?;
        Ok(())
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub(in crate::assistant) struct ToolSpec {
    pub(in crate::assistant) name: String,
    pub(in crate::assistant) description: String,
    pub(in crate::assistant) parameters: Value,
}

#[derive(Clone, Serialize, Deserialize)]
pub(in crate::assistant) struct ToolCall {
    pub(in crate::assistant) id: String,
    pub(in crate::assistant) name: String,
    pub(in crate::assistant) arguments: String,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::assistant) enum FinishReason {
    Stop,
    ToolCalls,
    Length,
    ContentFilter,
}

pub(in crate::assistant) struct ModelReply {
    pub(in crate::assistant) text: String,
    pub(in crate::assistant) reasoning_content: Option<String>,
    pub(in crate::assistant) tool_calls: Vec<ToolCall>,
    pub(in crate::assistant) finish_reason: FinishReason,
    pub(in crate::assistant) protocol: Option<ProtocolContent>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, thiserror::Error)]
pub(in crate::assistant) enum ModelError {
    #[error("模型接口地址无效，请填写 HTTPS 或本机 HTTP 地址，不能包含账号、查询参数或片段")]
    InvalidEndpoint,
    #[error("请填写有效的模型名称")]
    InvalidModel,
    #[error("模型请求设置无效，请检查令牌、输出上限和超时时间")]
    InvalidConfiguration,
    #[error("模型请求内容超过限制，请新建诊断会话后重试")]
    RequestTooLarge,
    #[error("模型请求已停止")]
    Cancelled,
    #[error("模型请求超时，已停止本次请求，请检查接口后手动重试")]
    Timeout,
    #[error("无法完成模型连接，已停止本次请求，请检查网络和接口设置")]
    Network,
    #[error("模型接口返回 HTTP {0}，已停止本次请求，请检查接口配置后重试")]
    Http(u16),
    #[error("模型接口返回了错误，已停止本次请求，请检查该接口的服务状态")]
    Provider,
    #[error("模型响应格式不受支持，已停止本次请求")]
    InvalidResponse,
    #[error("模型响应未完整结束，已停止本次请求，未执行未完成的工具调用")]
    IncompleteResponse,
    #[error("模型响应超过安全限制，已停止本次请求")]
    ResponseTooLarge,
    #[error("模型返回了无效的工具调用，未执行该调用")]
    InvalidToolCall,
}

fn is_loopback(url: &reqwest::Url) -> bool {
    url.host_str().is_some_and(|host| {
        host.eq_ignore_ascii_case("localhost")
            || host
                .trim_matches(['[', ']'])
                .parse::<IpAddr>()
                .is_ok_and(|ip| ip.is_loopback())
    })
}

pub(in crate::assistant) struct ModelClient {
    sdk: Option<sdk::SdkClient>,
    #[cfg(test)]
    client: Client,
    #[cfg(test)]
    endpoint: Url,
    #[cfg(test)]
    model: String,
    #[cfg(test)]
    authorization: Option<HeaderValue>,
    #[cfg(test)]
    max_output_tokens: u32,
    #[cfg(test)]
    timeout: Duration,
}

impl ModelClient {
    pub(in crate::assistant) fn new_with_settings(
        settings: &AssistantBuiltinSettings,
        api_key: &str,
        timeout: Duration,
    ) -> Result<Self, ModelError> {
        let sdk = sdk::SdkClient::new(settings, api_key, timeout)?;
        #[cfg(test)]
        {
            let mut client = Self::new(
                &settings.base_url,
                &settings.model,
                api_key,
                settings.max_output_tokens,
                timeout,
            )?;
            client.sdk = Some(sdk);
            Ok(client)
        }
        #[cfg(not(test))]
        {
            Ok(Self { sdk: Some(sdk) })
        }
    }

    pub(in crate::assistant) async fn complete(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolSpec],
        on_text: Arc<dyn Fn(&str) + Send + Sync>,
        cancel: &CancellationToken,
    ) -> Result<ModelReply, ModelError> {
        if let Some(sdk) = &self.sdk {
            return sdk.complete(messages, tools, on_text, cancel).await;
        }
        #[cfg(test)]
        {
            if cancel.is_cancelled() {
                return Err(ModelError::Cancelled);
            }
            let body = self.request_body(messages, tools)?;
            tokio::select! {
                biased;
                _ = cancel.cancelled() => Err(ModelError::Cancelled),
                result = tokio::time::timeout(self.timeout, self.send(body, on_text)) =>
                    result.map_err(|_| ModelError::Timeout)?,
            }
        }
        #[cfg(not(test))]
        Err(ModelError::InvalidConfiguration)
    }
}

pub(super) fn valid_identifier(value: &str, max: usize) -> bool {
    !value.is_empty() && value.len() <= max && !value.chars().any(char::is_control)
}
