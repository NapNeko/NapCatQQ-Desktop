//! 内置助手的一次性 Chat Completions 请求与安全错误边界。

use std::{collections::HashSet, net::IpAddr, sync::Arc, time::Duration};

use reqwest::{
    Client, Url,
    header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderValue},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use super::stream::{CompletionParser, SseDecoder};

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
    },
    Tool {
        tool_call_id: String,
        content: String,
    },
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

pub(in crate::assistant) fn endpoint_url(base_url: &str) -> Result<Url, ModelError> {
    let input = base_url.trim();
    if input.is_empty()
        || input.len() > 4096
        || base_url.chars().any(char::is_control)
        || input.contains('\\')
    {
        return Err(ModelError::InvalidEndpoint);
    }
    let authority = input
        .split_once("://")
        .map(|(_, rest)| rest.split(['/', '?', '#']).next().unwrap_or_default());
    let mut url = Url::parse(input).map_err(|_| ModelError::InvalidEndpoint)?;
    if authority.is_none_or(|authority| authority.contains('@'))
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !url.has_host()
    {
        return Err(ModelError::InvalidEndpoint);
    }
    if url.scheme() != "https" && !(url.scheme() == "http" && is_loopback(&url)) {
        return Err(ModelError::InvalidEndpoint);
    }
    let path = url.path().trim_end_matches('/');
    let path = if path.is_empty() {
        "/v1/chat/completions".to_owned()
    } else if path.ends_with("/v1") {
        format!("{path}/chat/completions")
    } else {
        path.to_owned()
    };
    url.set_path(&path);
    Ok(url)
}

fn is_loopback(url: &Url) -> bool {
    url.host_str().is_some_and(|host| {
        host.eq_ignore_ascii_case("localhost")
            || host
                .trim_matches(['[', ']'])
                .parse::<IpAddr>()
                .is_ok_and(|ip| ip.is_loopback())
    })
}

pub(in crate::assistant) struct ModelClient {
    client: Client,
    endpoint: Url,
    model: String,
    authorization: Option<HeaderValue>,
    max_output_tokens: u32,
    timeout: Duration,
}

impl ModelClient {
    pub(in crate::assistant) fn new(
        base_url: &str,
        model: &str,
        api_key: &str,
        max_output_tokens: u32,
        timeout: Duration,
    ) -> Result<Self, ModelError> {
        let endpoint = endpoint_url(base_url)?;
        let model = model.trim();
        if model.is_empty() || model.len() > 256 || model.chars().any(char::is_control) {
            return Err(ModelError::InvalidModel);
        }
        if max_output_tokens == 0
            || max_output_tokens > 131_072
            || timeout.is_zero()
            || timeout > Duration::from_secs(3600)
            || api_key.len() > 4096
            || api_key.chars().any(char::is_control)
        {
            return Err(ModelError::InvalidConfiguration);
        }
        let authorization = if api_key.is_empty() {
            None
        } else {
            let mut value = HeaderValue::from_str(&format!("Bearer {api_key}"))
                .map_err(|_| ModelError::InvalidConfiguration)?;
            value.set_sensitive(true);
            Some(value)
        };
        let mut builder = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .referer(false)
            .connect_timeout(timeout.min(Duration::from_secs(30)))
            .timeout(timeout);
        if is_loopback(&endpoint) {
            builder = builder.no_proxy();
        }
        let client = builder
            .build()
            .map_err(|_| ModelError::InvalidConfiguration)?;
        Ok(Self {
            client,
            endpoint,
            model: model.to_owned(),
            authorization,
            max_output_tokens,
            timeout,
        })
    }

    pub(in crate::assistant) async fn complete(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolSpec],
        on_text: Arc<dyn Fn(&str) + Send + Sync>,
        cancel: &CancellationToken,
    ) -> Result<ModelReply, ModelError> {
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

    fn request_body(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolSpec],
    ) -> Result<Vec<u8>, ModelError> {
        if messages.is_empty() || messages.len() > 256 || tools.len() > 64 {
            return Err(ModelError::RequestTooLarge);
        }
        let messages = messages
            .iter()
            .map(wire_message)
            .collect::<Result<Vec<_>, _>>()?;
        let mut names = HashSet::new();
        let mut definitions = Vec::new();
        for tool in tools {
            if !valid_identifier(&tool.name, 128)
                || !names.insert(&tool.name)
                || tool.description.len() > 8192
                || !tool.parameters.is_object()
                || tool.parameters.to_string().len() > 64 * 1024
            {
                return Err(ModelError::InvalidToolCall);
            }
            definitions.push(json!({"type":"function", "function":{
                "name":tool.name, "description":tool.description, "parameters":tool.parameters
            }}));
        }
        let mut body = json!({"model":self.model, "messages":messages, "stream":true});
        if self.model.eq_ignore_ascii_case("minimax-m3.1-flash") {
            body["reasoning_split"] = json!(true);
            body["reasoning_effort"] = json!("low");
            body["max_completion_tokens"] = json!(self.max_output_tokens);
        } else {
            body["max_tokens"] = json!(self.max_output_tokens);
        }
        if !definitions.is_empty() {
            body["tools"] = Value::Array(definitions);
        }
        let body = serde_json::to_vec(&body).map_err(|_| ModelError::InvalidConfiguration)?;
        if body.len() > MAX_REQUEST_BYTES {
            return Err(ModelError::RequestTooLarge);
        }
        Ok(body)
    }

    async fn send(
        &self,
        body: Vec<u8>,
        on_text: Arc<dyn Fn(&str) + Send + Sync>,
    ) -> Result<ModelReply, ModelError> {
        let mut request = self
            .client
            .post(self.endpoint.clone())
            .header(ACCEPT, "text/event-stream")
            .header(CONTENT_TYPE, "application/json")
            .body(body);
        if let Some(authorization) = &self.authorization {
            request = request.header(AUTHORIZATION, authorization.clone());
        }
        let mut response = request.send().await.map_err(network_error)?;
        if !response.status().is_success() {
            return Err(ModelError::Http(response.status().as_u16()));
        }
        if response
            .content_length()
            .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
        {
            return Err(ModelError::ResponseTooLarge);
        }
        let mime = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.split(';').next())
            .map(str::trim)
            .ok_or(ModelError::InvalidResponse)?;
        let is_json = mime.eq_ignore_ascii_case("application/json");
        if !is_json && !mime.eq_ignore_ascii_case("text/event-stream") {
            return Err(ModelError::InvalidResponse);
        }
        let mut total = 0usize;
        let mut body = Vec::new();
        let mut decoder = SseDecoder::default();
        let mut parser = CompletionParser::default();
        while let Some(chunk) = response.chunk().await.map_err(network_error)? {
            total = total.saturating_add(chunk.len());
            if total > MAX_RESPONSE_BYTES {
                return Err(ModelError::ResponseTooLarge);
            }
            if is_json {
                body.extend_from_slice(&chunk);
            } else {
                decoder.push(&chunk, |event| parser.event(event, on_text.as_ref()))?;
                if parser.is_done() {
                    break;
                }
            }
        }
        if is_json {
            CompletionParser::json(&body, on_text.as_ref())
        } else {
            decoder.finish()?;
            parser.finish()
        }
    }
}

fn network_error(error: reqwest::Error) -> ModelError {
    if error.is_timeout() {
        ModelError::Timeout
    } else {
        ModelError::Network
    }
}

pub(super) fn valid_identifier(value: &str, max: usize) -> bool {
    !value.is_empty() && value.len() <= max && !value.chars().any(char::is_control)
}

fn wire_message(message: &ChatMessage) -> Result<Value, ModelError> {
    let (content, result) = match message {
        ChatMessage::System { content } => (content, json!({"role":"system", "content":content})),
        ChatMessage::User { content } => (content, json!({"role":"user", "content":content})),
        ChatMessage::Tool {
            tool_call_id,
            content,
        } => {
            if !valid_identifier(tool_call_id, 256) {
                return Err(ModelError::InvalidToolCall);
            }
            (
                content,
                json!({"role":"tool", "tool_call_id":tool_call_id, "content":content}),
            )
        }
        ChatMessage::Assistant {
            content,
            reasoning_content,
            tool_calls,
        } => {
            if reasoning_content
                .as_ref()
                .is_some_and(|reasoning| reasoning.len() > MAX_REASONING_BYTES)
            {
                return Err(ModelError::RequestTooLarge);
            }
            if tool_calls.len() > MAX_TOOL_CALLS {
                return Err(ModelError::InvalidToolCall);
            }
            let mut ids = HashSet::new();
            let mut calls = Vec::new();
            for call in tool_calls {
                if !valid_identifier(&call.id, 256)
                    || !valid_identifier(&call.name, 128)
                    || !ids.insert(&call.id)
                    || call.arguments.len() > MAX_ARGUMENT_BYTES
                    || !serde_json::from_str::<Value>(&call.arguments)
                        .is_ok_and(|args| args.is_object())
                {
                    return Err(ModelError::InvalidToolCall);
                }
                calls.push(json!({"id":call.id, "type":"function", "function":{"name":call.name,"arguments":call.arguments}}));
            }
            let mut value = json!({"role":"assistant", "content":content});
            if let Some(reasoning) = reasoning_content {
                value["reasoning_content"] = json!(reasoning);
            }
            if !calls.is_empty() {
                value["tool_calls"] = Value::Array(calls);
            }
            (content, value)
        }
    };
    if content.len() > MAX_TEXT_BYTES {
        return Err(ModelError::RequestTooLarge);
    }
    Ok(result)
}
