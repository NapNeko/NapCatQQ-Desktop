//! SDK 终止结果和私有续轮校验，错误只转换为固定分类。

use super::super::{
    FinishReason, MAX_ARGUMENT_BYTES, MAX_REASONING_BYTES, MAX_TEXT_BYTES, MAX_TOOL_CALLS,
    ModelError, ModelReply, ProtocolContent, ToolCall, valid_identifier,
};
use super::guard::ModelStreamGuard;
use genai::chat::{ContentPart, StopReason, StreamEnd};
use ncd_domain::assistant::AssistantApiFormat;
use serde_json::Value;
use std::collections::HashSet;

pub(super) fn finish_reply(
    format: AssistantApiFormat,
    end: StreamEnd,
    guard: &ModelStreamGuard,
    previous_call_ids: &HashSet<String>,
) -> Result<ModelReply, ModelError> {
    guard.completed()?;
    let mut content = end.captured_content.unwrap_or_default();
    if format == AssistantApiFormat::GeminiGenerateContent {
        unique_gemini_call_ids(&mut content, guard, previous_call_ids)?;
    }
    if !content.contains_reasoning_content() {
        if let Some(reasoning) = &end.captured_reasoning_content {
            content.push(ContentPart::ReasoningContent(reasoning.clone()));
        }
    }
    let mut calls = Vec::new();
    for part in content.iter_mut() {
        if let ContentPart::ToolCall(call) = part {
            if let Value::String(arguments) = &call.fn_arguments {
                call.fn_arguments =
                    serde_json::from_str(arguments).map_err(|_| ModelError::InvalidToolCall)?;
            }
            if !call.fn_arguments.is_object() {
                return Err(ModelError::InvalidToolCall);
            }
            calls.push(ToolCall {
                id: call.call_id.clone(),
                name: call.fn_name.clone(),
                arguments: call.fn_arguments.to_string(),
            });
        }
    }
    let reason = match end.captured_stop_reason {
        Some(StopReason::Completed(_)) | Some(StopReason::StopSequence(_)) => {
            if calls.is_empty() {
                FinishReason::Stop
            } else {
                FinishReason::ToolCalls
            }
        }
        Some(StopReason::ToolCall(_)) => FinishReason::ToolCalls,
        Some(StopReason::MaxTokens(_)) if calls.is_empty() => FinishReason::Length,
        Some(StopReason::ContentFilter(_)) if calls.is_empty() => FinishReason::ContentFilter,
        _ => return Err(ModelError::IncompleteResponse),
    };
    let protocol = ProtocolContent { format, content };
    validate_protocol(&protocol, &calls)?;
    if let Some(mut reply) = guard.take_chat_reply() {
        validate_protocol(&protocol, &reply.tool_calls)?;
        reply.protocol = Some(protocol);
        Ok(reply)
    } else {
        let text = protocol.content.texts().join("");
        if text.is_empty() && calls.is_empty() && reason == FinishReason::Stop {
            return Err(ModelError::InvalidResponse);
        }
        Ok(ModelReply {
            text,
            reasoning_content: end.captured_reasoning_content,
            tool_calls: calls,
            finish_reason: reason,
            protocol: Some(protocol),
        })
    }
}

fn unique_gemini_call_ids(
    content: &mut genai::chat::MessageContent,
    guard: &ModelStreamGuard,
    previous_call_ids: &HashSet<String>,
) -> Result<(), ModelError> {
    let identities = guard.gemini_calls();
    let mut reserved = previous_call_ids.clone();
    for (_, native_id) in &identities {
        if let Some(id) = native_id {
            if !reserved.insert(id.clone()) {
                return Err(ModelError::InvalidToolCall);
            }
        }
    }
    let mut identities = identities.into_iter();
    for part in content.iter_mut() {
        let ContentPart::ToolCall(call) = part else {
            continue;
        };
        let (name, native_id) = identities.next().ok_or(ModelError::InvalidToolCall)?;
        if call.fn_name != name {
            return Err(ModelError::InvalidToolCall);
        }
        if let Some(id) = native_id {
            if call.call_id != id {
                return Err(ModelError::InvalidToolCall);
            }
        } else {
            // 保持 SDK 的合成格式：它会省略虚构的 Gemini 原生 id；编号需避开私有历史。
            // 随机起点也避免复用已从私有上下文裁掉、仍显示在会话中的旧调用编号。
            let mut index = rand::random::<usize>();
            loop {
                let id = format!("call#{name}#{index}");
                if reserved.insert(id.clone()) {
                    call.call_id = id;
                    break;
                }
                index = index.checked_add(1).ok_or(ModelError::InvalidToolCall)?;
            }
        }
    }
    if identities.next().is_some() {
        return Err(ModelError::InvalidToolCall);
    }
    Ok(())
}

pub(in crate::assistant) fn validate_protocol(
    protocol: &ProtocolContent,
    calls: &[ToolCall],
) -> Result<(), ModelError> {
    if serde_json::to_vec(&protocol.content)
        .map_err(|_| ModelError::InvalidResponse)?
        .len()
        > 256 * 1024
        || protocol.content.parts().len() > 128
        || calls.len() > MAX_TOOL_CALLS
    {
        return Err(ModelError::ResponseTooLarge);
    }
    let mut seen = HashSet::new();
    let mut matched = 0;
    for part in protocol.content.parts() {
        match part {
            ContentPart::Text(text) if text.len() <= MAX_TEXT_BYTES => {}
            ContentPart::ReasoningContent(text) | ContentPart::ThoughtSignature(text)
                if text.len() <= MAX_REASONING_BYTES => {}
            ContentPart::Custom(part)
                if protocol.format == AssistantApiFormat::AnthropicMessages
                    && part.typ() == Some("redacted_thinking")
                    && part.data.to_string().len() <= MAX_REASONING_BYTES => {}
            ContentPart::ToolCall(call) => {
                if !valid_identifier(&call.call_id, 256)
                    || !valid_identifier(&call.fn_name, 128)
                    || !seen.insert(call.call_id.clone())
                    || !call.fn_arguments.is_object()
                    || call.fn_arguments.to_string().len() > MAX_ARGUMENT_BYTES
                {
                    return Err(ModelError::InvalidToolCall);
                }
                if !calls.iter().any(|expected| {
                    expected.id == call.call_id
                        && expected.name == call.fn_name
                        && serde_json::from_str::<Value>(&expected.arguments)
                            .ok()
                            .as_ref()
                            == Some(&call.fn_arguments)
                }) {
                    return Err(ModelError::InvalidToolCall);
                }
                if call.thought_signatures.as_ref().is_some_and(|items| {
                    items.len() > 16
                        || items.iter().map(String::len).sum::<usize>() > MAX_REASONING_BYTES
                }) {
                    return Err(ModelError::ResponseTooLarge);
                }
                matched += 1;
            }
            _ => return Err(ModelError::InvalidResponse),
        }
    }
    if matched != calls.len() {
        return Err(ModelError::InvalidToolCall);
    }
    Ok(())
}

pub(super) fn sdk_error(error: genai::Error, guard: &ModelStreamGuard) -> ModelError {
    if let Some(error) = guard.error() {
        return error;
    }
    match error {
        genai::Error::HttpError { status, .. } => ModelError::Http(status.as_u16()),
        genai::Error::StreamGuardRejected { .. } => ModelError::ResponseTooLarge,
        genai::Error::WebStream { error, .. } => {
            if let Some(genai::Error::HttpError { status, .. }) =
                error.downcast_ref::<genai::Error>()
            {
                ModelError::Http(status.as_u16())
            } else if matches!(
                error.downcast_ref::<genai::Error>(),
                Some(genai::Error::StreamGuardRejected { .. })
            ) {
                ModelError::ResponseTooLarge
            } else if error
                .downcast_ref::<reqwest_model::Error>()
                .is_some_and(|error| error.is_timeout())
                || error.downcast_ref::<genai::webc::StreamTimeout>().is_some()
            {
                ModelError::Timeout
            } else {
                ModelError::Network
            }
        }
        genai::Error::ChatResponse { .. } => ModelError::Provider,
        genai::Error::StreamParse { .. } => ModelError::InvalidResponse,
        _ => ModelError::InvalidConfiguration,
    }
}
