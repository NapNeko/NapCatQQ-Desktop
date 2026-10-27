//! 旧版有界 HTTP 请求只保留为测试对照。

use std::collections::HashSet;

use reqwest::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE};

use super::super::stream::{CompletionParser, SseDecoder};
use super::*;

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
            sdk: None,
            client,
            endpoint,
            model: model.to_owned(),
            authorization,
            max_output_tokens,
            timeout,
        })
    }

    pub(super) fn request_body(
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

    pub(super) async fn send(
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

pub(super) fn wire_message(message: &ChatMessage) -> Result<Value, ModelError> {
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
            ..
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
