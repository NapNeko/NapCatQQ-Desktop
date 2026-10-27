//! 保存完整工具调用与结果，只按完整旧轮次回收历史。

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use super::model::{ChatMessage, ProtocolContent, ToolCall};

pub(super) const HISTORY_LIMIT: usize = 192 * 1024;
pub(super) const TOOL_RESULT_RESERVE: usize = 34 * 1024;

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(in crate::assistant) struct History {
    turns: Vec<Turn>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Turn {
    messages: Vec<ChatMessage>,
    complete: bool,
}

impl History {
    pub(in crate::assistant) fn sanitize(
        &mut self,
        redact: impl Fn(&str) -> String,
    ) -> Result<(), String> {
        self.validate()?;
        for message in self.turns.iter_mut().flat_map(|turn| &mut turn.messages) {
            match message {
                ChatMessage::System { content }
                | ChatMessage::User { content }
                | ChatMessage::Assistant { content, .. } => {
                    *content = redact(content);
                }
                ChatMessage::Tool { content, .. } => {
                    *content = redact_json_text(content, &redact, 0);
                }
            }
            match message {
                ChatMessage::Assistant {
                    reasoning_content,
                    tool_calls,
                    protocol,
                    ..
                } => {
                    if let Some(reasoning) = reasoning_content {
                        *reasoning = redact(reasoning);
                    }
                    for call in tool_calls {
                        call.id = redact(&call.id);
                        call.name = redact(&call.name);
                        call.arguments = redact_json_text(&call.arguments, &redact, 0);
                    }
                    if let Some(protocol) = protocol {
                        protocol.sanitize(&redact)?;
                    }
                }
                ChatMessage::Tool { tool_call_id, .. } => *tool_call_id = redact(tool_call_id),
                _ => {}
            }
        }
        self.validate()
    }

    pub(super) fn begin(&mut self, input: String) -> Result<(), String> {
        self.validate()?;
        self.settle_pending("上次处理已中断；此工具结果未知，禁止自动重放操作。");
        self.finish();
        self.turns.push(Turn {
            messages: vec![ChatMessage::User { content: input }],
            complete: false,
        });
        self.prune()
    }

    pub(super) fn messages(&self, system: &str) -> Vec<ChatMessage> {
        let mut messages = vec![ChatMessage::System {
            content: system.to_owned(),
        }];
        messages.extend(
            self.turns
                .iter()
                .flat_map(|turn| turn.messages.iter())
                .cloned(),
        );
        messages
    }

    pub(super) fn call_ids(&self) -> HashSet<String> {
        self.turns
            .iter()
            .flat_map(|turn| &turn.messages)
            .filter_map(|message| match message {
                ChatMessage::Assistant { tool_calls, .. } => Some(tool_calls),
                _ => None,
            })
            .flatten()
            .map(|call| call.id.clone())
            .collect()
    }

    pub(super) fn append_assistant(
        &mut self,
        content: String,
        reasoning_content: Option<String>,
        tool_calls: Vec<ToolCall>,
    ) -> Result<(), String> {
        self.append_assistant_with_protocol(content, reasoning_content, tool_calls, None)
    }

    pub(super) fn append_assistant_with_protocol(
        &mut self,
        content: String,
        reasoning_content: Option<String>,
        tool_calls: Vec<ToolCall>,
        protocol: Option<ProtocolContent>,
    ) -> Result<(), String> {
        if !self.pending().is_empty() {
            return Err("会话中仍有未结算的工具调用".into());
        }
        if content.len() > 128 * 1024
            || reasoning_content
                .as_ref()
                .is_some_and(|reasoning| reasoning.len() > 64 * 1024)
        {
            return Err("内置助手协议消息超过限制".into());
        }
        let reserve = tool_calls.len().saturating_mul(TOOL_RESULT_RESERVE);
        if let Some(protocol) = &protocol {
            protocol
                .validate(&tool_calls)
                .map_err(|_| "内置助手私有协议历史无效")?;
        }
        let message = ChatMessage::Assistant {
            content,
            reasoning_content,
            tool_calls,
            protocol,
        };
        let current = self.turns.last().ok_or("诊断轮次尚未开始")?;
        let size = encoded_size(current)?.saturating_add(encoded_size(&message)?);
        if size.saturating_add(reserve).saturating_add(1024) > HISTORY_LIMIT {
            return Err("本轮诊断达到会话字节预算，工具尚未执行".into());
        }
        self.turns
            .last_mut()
            .ok_or("诊断轮次尚未开始")?
            .messages
            .push(message);
        self.prune()
    }

    pub(super) fn append_tool(&mut self, id: &str, content: String) -> Result<(), String> {
        if self.pending().first().is_none_or(|pending| pending != id) {
            return Err("工具结果与等待中的调用不匹配".into());
        }
        let message = ChatMessage::Tool {
            tool_call_id: id.to_owned(),
            content,
        };
        if encoded_size(&message)? > TOOL_RESULT_RESERVE {
            return Err("诊断工具结果超过会话预算".into());
        }
        self.turns
            .last_mut()
            .ok_or("诊断轮次尚未开始")?
            .messages
            .push(message);
        self.prune()
    }

    pub(super) fn pending(&self) -> Vec<String> {
        let Some(turn) = self.turns.last() else {
            return Vec::new();
        };
        let mut pending = Vec::new();
        for message in &turn.messages {
            match message {
                ChatMessage::Assistant { tool_calls, .. } => {
                    pending = tool_calls.iter().map(|call| call.id.clone()).collect();
                }
                ChatMessage::Tool { tool_call_id, .. } => {
                    if pending.first() == Some(tool_call_id) {
                        pending.remove(0);
                    }
                }
                _ => {}
            }
        }
        pending
    }

    pub(super) fn settle_pending(&mut self, reason: &str) -> Vec<String> {
        let pending = self.pending();
        if let Some(turn) = self.turns.last_mut() {
            for id in &pending {
                turn.messages.push(ChatMessage::Tool {
                    tool_call_id: id.clone(),
                    content: serde_json::json!({"error": reason}).to_string(),
                });
            }
        }
        pending
    }

    pub(super) fn finish(&mut self) {
        if self.pending().is_empty() {
            if let Some(turn) = self.turns.last_mut() {
                turn.complete = true;
            }
        }
    }

    pub(in crate::assistant) fn validate(&self) -> Result<(), String> {
        if self.turns.len() > 200
            || encoded_size(self)? > HISTORY_LIMIT
            || self
                .turns
                .iter()
                .map(|turn| turn.messages.len())
                .sum::<usize>()
                > 200
        {
            return Err("内置助手会话档案超过限制".into());
        }
        let mut seen = HashSet::new();
        for (index, turn) in self.turns.iter().enumerate() {
            if !matches!(turn.messages.first(), Some(ChatMessage::User { .. })) {
                return Err("内置助手会话缺少完整用户轮次".into());
            }
            let mut pending = Vec::new();
            for (position, message) in turn.messages.iter().enumerate() {
                match message {
                    ChatMessage::User { .. } if position == 0 => {}
                    ChatMessage::Assistant {
                        content,
                        reasoning_content,
                        tool_calls,
                        protocol,
                    } if pending.is_empty() => {
                        if let Some(protocol) = protocol {
                            protocol
                                .validate(tool_calls)
                                .map_err(|_| "内置助手私有协议历史无效")?;
                        }
                        if content.len() > 128 * 1024
                            || reasoning_content
                                .as_ref()
                                .is_some_and(|reasoning| reasoning.len() > 64 * 1024)
                        {
                            return Err("内置助手协议消息超过限制".into());
                        }
                        if tool_calls.len() > 4 {
                            return Err("内置助手会话的工具调用超过限制".into());
                        }
                        for call in tool_calls {
                            if call.id.trim().is_empty()
                                || call.id.len() > 256
                                || call.id.chars().any(char::is_control)
                                || !seen.insert(call.id.as_str())
                            {
                                return Err("内置助手会话的工具编号重复".into());
                            }
                            if call.name.is_empty()
                                || call.name.len() > 128
                                || call.name.chars().any(char::is_control)
                                || call.arguments.len() > 16 * 1024
                            {
                                return Err("内置助手会话的工具参数超过限制".into());
                            }
                            let args: serde_json::Value = serde_json::from_str(&call.arguments)
                                .map_err(|_| "内置助手会话的工具参数损坏")?;
                            if !args.is_object() {
                                return Err("内置助手会话的工具参数不是对象".into());
                            }
                            pending.push(call.id.as_str());
                        }
                    }
                    ChatMessage::Tool { tool_call_id, .. }
                        if pending.first().copied() == Some(tool_call_id.as_str()) =>
                    {
                        pending.remove(0);
                    }
                    _ => return Err("内置助手会话的工具调用与结果配对损坏".into()),
                }
            }
            if (!pending.is_empty() && turn.complete)
                || (!turn.complete && index + 1 != self.turns.len())
            {
                return Err("内置助手会话包含未结算的旧轮次".into());
            }
        }
        Ok(())
    }

    fn prune(&mut self) -> Result<(), String> {
        while encoded_size(self)? > HISTORY_LIMIT
            || self
                .turns
                .iter()
                .map(|turn| turn.messages.len())
                .sum::<usize>()
                > 200
        {
            if self.turns.len() <= 1 || !self.turns[0].complete {
                return Err("本轮诊断达到会话字节预算".into());
            }
            self.turns.remove(0);
        }
        Ok(())
    }
}

fn encoded_size(value: &impl Serialize) -> Result<usize, String> {
    serde_json::to_vec(value)
        .map(|bytes| bytes.len())
        .map_err(|_| "序列化内置助手历史失败".into())
}

fn redact_json_text(text: &str, redact: &impl Fn(&str) -> String, depth: usize) -> String {
    if depth <= 16 {
        if let Ok(value @ (serde_json::Value::Object(_) | serde_json::Value::Array(_))) =
            serde_json::from_str(text)
        {
            return redact_json(value, redact, depth + 1).to_string();
        }
    }
    redact(text)
}

fn redact_json(
    value: serde_json::Value,
    redact: &impl Fn(&str) -> String,
    depth: usize,
) -> serde_json::Value {
    use serde_json::Value;
    if depth > 20 {
        return Value::String("[内容超过嵌套限制]".into());
    }
    match value {
        Value::String(text) => Value::String(redact_json_text(&text, redact, depth + 1)),
        Value::Array(items) => Value::Array(
            items
                .into_iter()
                .map(|item| redact_json(item, redact, depth + 1))
                .collect(),
        ),
        Value::Object(object) => Value::Object(
            object
                .into_iter()
                .map(|(key, value)| (redact(&key), redact_json(value, redact, depth + 1)))
                .collect(),
        ),
        value => value,
    }
}
