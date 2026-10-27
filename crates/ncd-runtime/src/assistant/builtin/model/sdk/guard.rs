//! 在 SDK 分配工具索引与解释结束事件之前校验原始协议帧。

mod anthropic;
mod gemini;
mod responses;

use super::super::super::stream::CompletionParser;
use super::super::{
    MAX_ARGUMENT_BYTES, MAX_REASONING_BYTES, MAX_REQUEST_BYTES, MAX_RESPONSE_BYTES, MAX_TEXT_BYTES,
    MAX_TOOL_CALLS, ModelError, ModelReply, valid_identifier,
};
use genai::chat::ChatStreamGuard;
use ncd_domain::assistant::AssistantApiFormat;
use serde_json::Value;
use std::{
    collections::{BTreeMap, HashSet},
    sync::{Mutex, PoisonError},
};

pub(super) struct ModelStreamGuard {
    format: AssistantApiFormat,
    state: Mutex<State>,
}

impl std::fmt::Debug for ModelStreamGuard {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        fmt.write_str("ModelStreamGuard")
    }
}

#[derive(Default)]
struct State {
    chat: CompletionParser,
    chat_reply: Option<ModelReply>,
    error: Option<ModelError>,
    done: bool,
    reason: bool,
    text_bytes: usize,
    reasoning_bytes: usize,
    signatures_bytes: usize,
    blocks: BTreeMap<usize, Block>,
    seen_ids: HashSet<String>,
    tools: usize,
    gemini_calls: Vec<(String, Option<String>)>,
}

#[derive(Default)]
struct Block {
    kind: String,
    arguments: String,
    closed: bool,
}

impl ModelStreamGuard {
    pub(super) fn new(format: AssistantApiFormat) -> Self {
        Self {
            format,
            state: Mutex::new(State::default()),
        }
    }
    pub(super) fn error(&self) -> Option<ModelError> {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .error
    }
    pub(super) fn completed(&self) -> Result<(), ModelError> {
        let state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(error) = state.error {
            Err(error)
        } else if state.done {
            Ok(())
        } else {
            Err(ModelError::IncompleteResponse)
        }
    }
    pub(super) fn take_chat_reply(&self) -> Option<ModelReply> {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .chat_reply
            .take()
    }
    pub(super) fn gemini_calls(&self) -> Vec<(String, Option<String>)> {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .gemini_calls
            .clone()
    }
    fn reject(&self, error: ModelError) -> genai::Error {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .error = Some(error);
        genai::Error::StreamGuardRejected {
            reason: "NCD model protocol guard rejected input",
        }
    }
}

impl ChatStreamGuard for ModelStreamGuard {
    fn max_response_bytes(&self) -> usize {
        MAX_RESPONSE_BYTES
    }
    fn max_frame_bytes(&self) -> usize {
        256 * 1024
    }
    fn validate_request(&self, payload: &Value) -> genai::Result<()> {
        if serde_json::to_vec(payload).map_or(true, |value| value.len() > MAX_REQUEST_BYTES) {
            return Err(self.reject(ModelError::RequestTooLarge));
        }
        Ok(())
    }
    fn validate_content_type(&self, value: &str) -> genai::Result<()> {
        if !value
            .split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .eq_ignore_ascii_case("text/event-stream")
        {
            return Err(self.reject(ModelError::InvalidResponse));
        }
        Ok(())
    }
    fn validate_eof(&self) -> genai::Result<()> {
        self.completed().map_err(|error| self.reject(error))
    }
    fn validate_frame(&self, event: Option<&str>, data: &str) -> genai::Result<()> {
        if data.trim().is_empty() {
            return Ok(());
        }
        let result = {
            let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
            if let Some(error) = state.error {
                Err(error)
            } else if state.done {
                Err(ModelError::InvalidResponse)
            } else if self.format == AssistantApiFormat::OpenAiChatCompletions {
                state.chat.event(data.as_bytes(), &|_| {}).and_then(|_| {
                    if state.chat.is_done() {
                        state.chat_reply = Some(std::mem::take(&mut state.chat).finish()?);
                        state.done = true;
                    }
                    Ok(())
                })
            } else {
                state.frame(self.format, event, data)
            }
        };
        result.map_err(|error| self.reject(error))
    }
}

impl State {
    fn frame(
        &mut self,
        format: AssistantApiFormat,
        event: Option<&str>,
        data: &str,
    ) -> Result<(), ModelError> {
        if data.trim().is_empty() {
            return Ok(());
        }
        let value: Value = serde_json::from_str(data).map_err(|_| ModelError::InvalidResponse)?;
        if !value.is_object() {
            return Err(ModelError::InvalidResponse);
        }
        if value.get("error").is_some() {
            return Err(ModelError::Provider);
        }
        match format {
            AssistantApiFormat::OpenAiResponses => self.responses(&value),
            AssistantApiFormat::AnthropicMessages => self.anthropic(event, &value),
            AssistantApiFormat::GeminiGenerateContent => self.gemini(&value),
            AssistantApiFormat::OpenAiChatCompletions => Err(ModelError::InvalidConfiguration),
        }
    }
    fn text(&mut self, text: &str, reasoning: bool) -> Result<(), ModelError> {
        let total = if reasoning {
            &mut self.reasoning_bytes
        } else {
            &mut self.text_bytes
        };
        *total = total.saturating_add(text.len());
        if *total
            > if reasoning {
                MAX_REASONING_BYTES
            } else {
                MAX_TEXT_BYTES
            }
        {
            return Err(ModelError::ResponseTooLarge);
        }
        Ok(())
    }
    fn signature(&mut self, value: &Value) -> Result<(), ModelError> {
        if value.is_null() {
            return Ok(());
        }
        let text = value.as_str().ok_or(ModelError::InvalidResponse)?;
        self.signatures_bytes = self.signatures_bytes.saturating_add(text.len());
        if self.signatures_bytes > MAX_REASONING_BYTES {
            return Err(ModelError::ResponseTooLarge);
        }
        Ok(())
    }
    fn tool(&mut self, id: Option<&str>, name: &Value) -> Result<(), ModelError> {
        name.as_str()
            .filter(|name| valid_identifier(name, 128))
            .ok_or(ModelError::InvalidToolCall)?;
        if let Some(id) = id {
            if !valid_identifier(id, 256) || !self.seen_ids.insert(id.to_owned()) {
                return Err(ModelError::InvalidToolCall);
            }
        }
        self.tools += 1;
        if self.tools > MAX_TOOL_CALLS {
            return Err(ModelError::ResponseTooLarge);
        }
        Ok(())
    }
}

fn index(value: &Value, field: &str) -> Result<usize, ModelError> {
    value[field]
        .as_u64()
        .filter(|index| *index < 64)
        .and_then(|index| usize::try_from(index).ok())
        .ok_or(ModelError::InvalidToolCall)
}
fn arguments(value: &str) -> Result<(), ModelError> {
    if value.len() > MAX_ARGUMENT_BYTES {
        return Err(ModelError::ResponseTooLarge);
    }
    let value: Value = serde_json::from_str(value).map_err(|_| ModelError::InvalidToolCall)?;
    object(&value)
}
fn object(value: &Value) -> Result<(), ModelError> {
    if !value.is_object() {
        Err(ModelError::InvalidToolCall)
    } else if value.to_string().len() > MAX_ARGUMENT_BYTES {
        Err(ModelError::ResponseTooLarge)
    } else {
        Ok(())
    }
}
fn stop_reason(reason: Option<&str>) -> Result<(), ModelError> {
    match reason {
        Some(
            "end_turn" | "tool_use" | "max_tokens" | "stop_sequence" | "refusal" | "STOP"
            | "MAX_TOKENS" | "SAFETY" | "RECITATION" | "BLOCKLIST" | "PROHIBITED_CONTENT" | "SPII"
            | "IMAGE_SAFETY",
        ) => Ok(()),
        _ => Err(ModelError::InvalidResponse),
    }
}
