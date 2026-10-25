//! 有界 SSE 分帧与完整 Chat Completions 工具调用校验。

use std::collections::{BTreeMap, HashSet};

use serde_json::Value;

use super::model::{
    FinishReason, MAX_ARGUMENT_BYTES, MAX_REASONING_BYTES, MAX_TEXT_BYTES, MAX_TOOL_CALLS,
    ModelError, ModelReply, ToolCall, valid_identifier,
};

#[cfg(test)]
mod tests;

const MAX_SSE_LINE_BYTES: usize = 256 * 1024;
const MAX_SSE_EVENT_BYTES: usize = 256 * 1024;

#[derive(Default)]
pub(super) struct SseDecoder {
    line: Vec<u8>,
    data: Vec<u8>,
    has_data: bool,
    skip_lf: bool,
    seen_line: bool,
}

impl SseDecoder {
    pub(super) fn push(
        &mut self,
        bytes: &[u8],
        mut event: impl FnMut(&[u8]) -> Result<(), ModelError>,
    ) -> Result<(), ModelError> {
        for &byte in bytes {
            if self.skip_lf {
                self.skip_lf = false;
                if byte == b'\n' {
                    continue;
                }
            }
            match byte {
                b'\r' => {
                    self.end_line(&mut event)?;
                    self.skip_lf = true;
                }
                b'\n' => self.end_line(&mut event)?,
                _ => {
                    if self.line.len() >= MAX_SSE_LINE_BYTES {
                        return Err(ModelError::ResponseTooLarge);
                    }
                    self.line.push(byte);
                }
            }
        }
        Ok(())
    }

    fn end_line(
        &mut self,
        event: &mut impl FnMut(&[u8]) -> Result<(), ModelError>,
    ) -> Result<(), ModelError> {
        let mut line = std::str::from_utf8(&self.line).map_err(|_| ModelError::InvalidResponse)?;
        if !self.seen_line {
            line = line.strip_prefix('\u{feff}').unwrap_or(line);
            self.seen_line = true;
        }
        if line.is_empty() {
            if self.has_data {
                event(&self.data)?;
                self.data.clear();
                self.has_data = false;
            }
        } else if !line.starts_with(':') {
            let (field, value) = line.split_once(':').unwrap_or((line, ""));
            if field == "data" {
                let value = value.strip_prefix(' ').unwrap_or(value);
                let extra = usize::from(self.has_data);
                if self
                    .data
                    .len()
                    .saturating_add(extra)
                    .saturating_add(value.len())
                    > MAX_SSE_EVENT_BYTES
                {
                    return Err(ModelError::ResponseTooLarge);
                }
                if self.has_data {
                    self.data.push(b'\n');
                }
                self.data.extend_from_slice(value.as_bytes());
                self.has_data = true;
            }
        }
        self.line.clear();
        Ok(())
    }

    pub(super) fn finish(&self) -> Result<(), ModelError> {
        if !self.line.is_empty() || self.has_data {
            return Err(ModelError::IncompleteResponse);
        }
        Ok(())
    }
}

#[derive(Default)]
struct PendingTool {
    id: String,
    name: String,
    arguments: String,
}

#[derive(Default)]
struct ThinkFilter {
    pending: String,
    depth: usize,
    visible_bytes: usize,
}

impl ThinkFilter {
    fn push(&mut self, text: &str, on_text: &dyn Fn(&str)) {
        let mut visible = String::new();
        for ch in text.chars() {
            self.pending.push(ch);
            loop {
                let tag = self.pending.to_ascii_lowercase();
                if tag == "<think>" {
                    self.depth = self.depth.saturating_add(1);
                    self.pending.clear();
                    break;
                }
                if tag == "</think>" {
                    self.depth = self.depth.saturating_sub(1);
                    self.pending.clear();
                    break;
                }
                if "<think>".starts_with(&tag) || "</think>".starts_with(&tag) {
                    break;
                }
                let Some(first) = self.pending.chars().next() else {
                    break;
                };
                if self.depth == 0 {
                    visible.push(first);
                }
                self.pending.drain(..first.len_utf8());
                if self.pending.is_empty() {
                    break;
                }
            }
        }
        if !visible.is_empty() {
            self.visible_bytes += visible.len();
            on_text(&visible);
        }
    }

    fn finish(&mut self, on_text: &dyn Fn(&str)) {
        if self.depth == 0 && !self.pending.is_empty() {
            self.visible_bytes += self.pending.len();
            on_text(&self.pending);
        }
        self.pending.clear();
    }
}

#[derive(Default)]
pub(super) struct CompletionParser {
    text: String,
    reasoning_content: Option<String>,
    visible: ThinkFilter,
    tools: BTreeMap<usize, PendingTool>,
    finish_reason: Option<FinishReason>,
    done: bool,
}

impl CompletionParser {
    pub(super) fn event(&mut self, bytes: &[u8], on_text: &dyn Fn(&str)) -> Result<(), ModelError> {
        if self.done {
            return Err(ModelError::InvalidResponse);
        }
        let text = std::str::from_utf8(bytes)
            .map_err(|_| ModelError::InvalidResponse)?
            .trim();
        if text == "[DONE]" {
            if self.finish_reason.is_none() {
                return Err(ModelError::IncompleteResponse);
            }
            self.visible.finish(on_text);
            self.done = true;
            return Ok(());
        }
        let value: Value = serde_json::from_str(text).map_err(|_| ModelError::InvalidResponse)?;
        self.ingest(&value, false, on_text)
    }

    pub(super) fn is_done(&self) -> bool {
        self.done
    }

    fn ingest(
        &mut self,
        value: &Value,
        full: bool,
        on_text: &dyn Fn(&str),
    ) -> Result<(), ModelError> {
        let object = value.as_object().ok_or(ModelError::InvalidResponse)?;
        if object.contains_key("error") {
            return Err(ModelError::Provider);
        }
        let choices = object
            .get("choices")
            .and_then(Value::as_array)
            .ok_or(ModelError::InvalidResponse)?;
        if choices.is_empty() && !full && object.get("usage").is_some_and(Value::is_object) {
            return Ok(());
        }
        if choices.len() != 1 || self.finish_reason.is_some() {
            return Err(ModelError::InvalidResponse);
        }
        let choice = &choices[0];
        if choice["index"].as_u64() != Some(0) {
            return Err(ModelError::InvalidResponse);
        }
        let payload = choice
            .get(if full { "message" } else { "delta" })
            .ok_or(ModelError::InvalidResponse)?;
        if !payload.is_object() {
            return Err(ModelError::InvalidResponse);
        }
        if let Some(role) = payload.get("role") {
            if role != "assistant" {
                return Err(ModelError::InvalidResponse);
            }
        }
        if let Some(content) = payload.get("content").filter(|content| !content.is_null()) {
            let content = content.as_str().ok_or(ModelError::InvalidResponse)?;
            if self.text.len().saturating_add(content.len()) > MAX_TEXT_BYTES {
                return Err(ModelError::ResponseTooLarge);
            }
            self.text.push_str(content);
            if !content.is_empty() {
                self.visible.push(content, on_text);
            }
        }
        // 推理原文只保留在私有协议历史，多轮工具请求依赖它；不能送到文本回调。
        if let Some(reasoning) = payload
            .get("reasoning_content")
            .filter(|reasoning| !reasoning.is_null())
        {
            let reasoning = reasoning.as_str().ok_or(ModelError::InvalidResponse)?;
            let accumulated = self.reasoning_content.get_or_insert_with(String::new);
            if accumulated.len().saturating_add(reasoning.len()) > MAX_REASONING_BYTES {
                return Err(ModelError::ResponseTooLarge);
            }
            accumulated.push_str(reasoning);
        }
        if let Some(calls) = payload.get("tool_calls").filter(|calls| !calls.is_null()) {
            let calls = calls.as_array().ok_or(ModelError::InvalidToolCall)?;
            if calls.len() > MAX_TOOL_CALLS {
                return Err(ModelError::ResponseTooLarge);
            }
            for (position, call) in calls.iter().enumerate() {
                let index = if full {
                    position
                } else {
                    let index = call
                        .get("index")
                        .and_then(Value::as_u64)
                        .ok_or(ModelError::InvalidToolCall)?;
                    usize::try_from(index).map_err(|_| ModelError::InvalidToolCall)?
                };
                self.append_tool(index, call)?;
            }
        }
        if let Some(reason) = choice
            .get("finish_reason")
            .filter(|reason| !reason.is_null())
        {
            self.finish_reason = Some(match reason.as_str() {
                Some("stop") => FinishReason::Stop,
                Some("tool_calls") => FinishReason::ToolCalls,
                Some("length") => FinishReason::Length,
                Some("content_filter") => FinishReason::ContentFilter,
                _ => return Err(ModelError::InvalidResponse),
            });
        }
        Ok(())
    }

    fn append_tool(&mut self, index: usize, call: &Value) -> Result<(), ModelError> {
        if index >= MAX_TOOL_CALLS || !call.is_object() {
            return Err(ModelError::InvalidToolCall);
        }
        if call.get("type").is_some_and(|kind| kind != "function") {
            return Err(ModelError::InvalidToolCall);
        }
        let tool = self.tools.entry(index).or_default();
        append_fragment(&mut tool.id, call.get("id"), 256)?;
        if let Some(function) = call.get("function").filter(|function| !function.is_null()) {
            if !function.is_object() {
                return Err(ModelError::InvalidToolCall);
            }
            append_fragment(&mut tool.name, function.get("name"), 128)?;
            append_fragment(
                &mut tool.arguments,
                function.get("arguments"),
                MAX_ARGUMENT_BYTES,
            )?;
        }
        Ok(())
    }

    pub(super) fn finish(self) -> Result<ModelReply, ModelError> {
        if !self.done {
            return Err(ModelError::IncompleteResponse);
        }
        let finish_reason = self.finish_reason.ok_or(ModelError::IncompleteResponse)?;
        if finish_reason != FinishReason::ToolCalls && !self.tools.is_empty() {
            return Err(ModelError::IncompleteResponse);
        }
        if finish_reason == FinishReason::ToolCalls && self.tools.is_empty() {
            return Err(ModelError::InvalidToolCall);
        }
        let mut ids = HashSet::new();
        let mut calls = Vec::new();
        for (position, (index, tool)) in self.tools.into_iter().enumerate() {
            if position != index
                || !valid_identifier(&tool.id, 256)
                || !valid_identifier(&tool.name, 128)
                || !ids.insert(tool.id.clone())
                || !serde_json::from_str::<Value>(&tool.arguments)
                    .is_ok_and(|args| args.is_object())
            {
                return Err(ModelError::InvalidToolCall);
            }
            calls.push(ToolCall {
                id: tool.id,
                name: tool.name,
                arguments: tool.arguments,
            });
        }
        if self.visible.visible_bytes == 0
            && calls.is_empty()
            && finish_reason == FinishReason::Stop
        {
            return Err(ModelError::InvalidResponse);
        }
        Ok(ModelReply {
            text: self.text,
            reasoning_content: self.reasoning_content,
            tool_calls: calls,
            finish_reason,
        })
    }

    pub(super) fn json(bytes: &[u8], on_text: &dyn Fn(&str)) -> Result<ModelReply, ModelError> {
        let value: Value =
            serde_json::from_slice(bytes).map_err(|_| ModelError::InvalidResponse)?;
        let mut parser = Self::default();
        parser.ingest(&value, true, on_text)?;
        if parser.finish_reason.is_none() {
            return Err(ModelError::IncompleteResponse);
        }
        parser.visible.finish(on_text);
        parser.done = true;
        parser.finish()
    }
}

fn append_fragment(
    target: &mut String,
    value: Option<&Value>,
    limit: usize,
) -> Result<(), ModelError> {
    let Some(value) = value.filter(|value| !value.is_null()) else {
        return Ok(());
    };
    let value = value.as_str().ok_or(ModelError::InvalidToolCall)?;
    if target.len().saturating_add(value.len()) > limit {
        return Err(ModelError::ResponseTooLarge);
    }
    target.push_str(value);
    Ok(())
}
