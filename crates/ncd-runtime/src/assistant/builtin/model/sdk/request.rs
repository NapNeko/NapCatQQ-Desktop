//! 将私有会话投影为 SDK 请求，按显式协议构建用户端点。

use super::super::super::stream::ThinkFilter;
use super::super::{
    ChatMessage, MAX_ARGUMENT_BYTES, MAX_REASONING_BYTES, MAX_REQUEST_BYTES, MAX_TEXT_BYTES,
    MAX_TOOL_CALLS, ModelError, ToolSpec, valid_identifier,
};
use super::{SdkClient, adapter, validate_protocol};
use genai::{
    Headers, ModelIden, ServiceTarget,
    chat::{
        ChatMessage as SdkMessage, ChatRequest, ContentPart, MessageContent, Tool,
        ToolCall as SdkToolCall, ToolResponse,
    },
    resolver::{AuthData, Endpoint},
};
use ncd_domain::assistant::AssistantApiFormat;
use serde_json::Value;
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
};

impl SdkClient {
    pub(super) fn request(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolSpec],
    ) -> Result<ChatRequest, ModelError> {
        if messages.is_empty()
            || messages.len() > 256
            || tools.len() > 64
            || serde_json::to_vec(messages)
                .map_err(|_| ModelError::InvalidConfiguration)?
                .len()
                > MAX_REQUEST_BYTES
        {
            return Err(ModelError::RequestTooLarge);
        }
        let mut call_names = HashMap::new();
        let mut historical_calls = HashSet::new();
        let mut output = Vec::new();
        for message in messages {
            let next = match message {
                ChatMessage::System { content } => {
                    check_text(content)?;
                    SdkMessage::system(content.clone())
                }
                ChatMessage::User { content } => {
                    check_text(content)?;
                    SdkMessage::user(content.clone())
                }
                ChatMessage::Tool {
                    tool_call_id,
                    content,
                } => {
                    check_text(content)?;
                    let name = call_names
                        .get(tool_call_id)
                        .ok_or(ModelError::InvalidToolCall)?;
                    if historical_calls.contains(tool_call_id) {
                        let text = format!(
                            "历史工具结果（仅供上下文，不会重新执行）\n工具：{name}\n{content}"
                        );
                        check_text(&text)?;
                        SdkMessage::user(text)
                    } else {
                        SdkMessage::from(
                            ToolResponse::new(tool_call_id.clone(), content.clone())
                                .with_fn_name(name),
                        )
                    }
                }
                ChatMessage::Assistant {
                    content,
                    reasoning_content,
                    tool_calls,
                    protocol,
                } => {
                    check_text(content)?;
                    if reasoning_content
                        .as_ref()
                        .is_some_and(|text| text.len() > MAX_REASONING_BYTES)
                        || tool_calls.len() > MAX_TOOL_CALLS
                    {
                        return Err(ModelError::RequestTooLarge);
                    }
                    let mut ids = HashSet::new();
                    let calls = tool_calls
                        .iter()
                        .map(|call| {
                            if !valid_identifier(&call.id, 256)
                                || !valid_identifier(&call.name, 128)
                                || !ids.insert(call.id.clone())
                                || call.arguments.len() > MAX_ARGUMENT_BYTES
                            {
                                return Err(ModelError::InvalidToolCall);
                            }
                            let args: Value = serde_json::from_str(&call.arguments)
                                .map_err(|_| ModelError::InvalidToolCall)?;
                            if !args.is_object() {
                                return Err(ModelError::InvalidToolCall);
                            }
                            if call_names
                                .insert(call.id.clone(), call.name.clone())
                                .is_some()
                            {
                                return Err(ModelError::InvalidToolCall);
                            }
                            Ok(SdkToolCall {
                                call_id: call.id.clone(),
                                fn_name: call.name.clone(),
                                fn_arguments: args,
                                thought_signatures: None,
                            })
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    if let Some(protocol) = protocol
                        .as_ref()
                        .filter(|value| value.format == self.format)
                    {
                        validate_protocol(protocol, tool_calls)?;
                        SdkMessage::assistant(protocol.content.clone())
                    } else if protocol.is_none()
                        && self.format == AssistantApiFormat::OpenAiChatCompletions
                    {
                        let mut parts = vec![ContentPart::Text(content.clone())];
                        parts.extend(calls.into_iter().map(ContentPart::ToolCall));
                        SdkMessage::assistant(MessageContent::from_parts(parts))
                            .with_reasoning_content(reasoning_content.clone())
                    } else {
                        // 旧协议的调用与结果只是历史数据，不能重建成目标协议的原生调用。
                        let mut visible = visible_history(content);
                        for call in tool_calls {
                            historical_calls.insert(call.id.clone());
                            visible.push_str(&format!(
                                "\n历史工具调用：{}（仅供上下文，不会重新执行）",
                                call.name
                            ));
                        }
                        if visible.trim().is_empty() {
                            continue;
                        }
                        check_text(&visible)?;
                        SdkMessage::assistant(visible)
                    }
                }
            };
            output.push(next);
        }
        let mut names = HashSet::new();
        let definitions = tools
            .iter()
            .map(|tool| {
                if !valid_identifier(&tool.name, 128)
                    || !names.insert(tool.name.clone())
                    || tool.description.len() > 8192
                    || !tool.parameters.is_object()
                    || tool.parameters.to_string().len() > 64 * 1024
                {
                    return Err(ModelError::InvalidToolCall);
                }
                Ok(Tool::new(tool.name.clone())
                    .with_description(tool.description.clone())
                    .with_schema(tool.parameters.clone()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ChatRequest::new(output)
            .with_tools(definitions)
            .with_store(false))
    }

    pub(super) fn target(&self) -> Result<ServiceTarget, ModelError> {
        let mut url = self.endpoint.clone();
        if self.format == AssistantApiFormat::GeminiGenerateContent {
            if !url.path().ends_with(":streamGenerateContent") {
                let model = self.model.strip_prefix("models/").unwrap_or(&self.model);
                url.path_segments_mut()
                    .map_err(|_| ModelError::InvalidEndpoint)?
                    .pop_if_empty()
                    .push("models")
                    .push(&format!("{model}:streamGenerateContent"));
            }
            url.query_pairs_mut().append_pair("alt", "sse");
        }
        let mut headers = Headers::from(("content-type", "application/json"));
        headers.merge(("accept", "text/event-stream"));
        if !self.api_key.is_empty() {
            match self.format {
                AssistantApiFormat::AnthropicMessages => {
                    headers.merge(("x-api-key", self.api_key.clone()));
                }
                AssistantApiFormat::GeminiGenerateContent => {
                    headers.merge(("x-goog-api-key", self.api_key.clone()));
                }
                _ => {
                    headers.merge(("authorization", format!("Bearer {}", self.api_key)));
                }
            }
        }
        if self.format == AssistantApiFormat::AnthropicMessages {
            headers.merge(("anthropic-version", "2023-06-01"));
        }
        Ok(ServiceTarget {
            endpoint: Endpoint::from_owned(format!(
                "{}/",
                self.endpoint.origin().ascii_serialization()
            )),
            auth: AuthData::RequestOverride {
                url: url.to_string(),
                headers,
            },
            model: ModelIden::new(adapter(self.format), self.model.clone()),
        })
    }
}

fn check_text(text: &str) -> Result<(), ModelError> {
    if text.len() > MAX_TEXT_BYTES {
        Err(ModelError::RequestTooLarge)
    } else {
        Ok(())
    }
}

fn visible_history(content: &str) -> String {
    let visible = RefCell::new(String::new());
    let append = |text: &str| visible.borrow_mut().push_str(text);
    let mut filter = ThinkFilter::default();
    filter.push(content, &append);
    filter.finish(&append);
    visible.into_inner()
}
