//! Anthropic 消息、内容块和显式 message_stop 校验。

use super::*;

impl State {
    pub(super) fn anthropic(
        &mut self,
        event: Option<&str>,
        value: &Value,
    ) -> Result<(), ModelError> {
        let kind = value["type"].as_str().ok_or(ModelError::InvalidResponse)?;
        if event.is_some_and(|event| event != kind) {
            return Err(ModelError::InvalidResponse);
        }
        match kind {
            "ping" => {}
            "message_start" => {
                if value["message"]["role"] != "assistant" {
                    return Err(ModelError::InvalidResponse);
                }
            }
            "content_block_start" => {
                let index = index(value, "index")?;
                if self.blocks.contains_key(&index)
                    || self.blocks.values().any(|block| !block.closed)
                {
                    return Err(ModelError::InvalidResponse);
                }
                let block = &value["content_block"];
                let kind = block["type"].as_str().ok_or(ModelError::InvalidResponse)?;
                match kind {
                    "tool_use" => {
                        self.tool(
                            Some(block["id"].as_str().ok_or(ModelError::InvalidToolCall)?),
                            &block["name"],
                        )?;
                        object(&block["input"])?;
                    }
                    "text" => {
                        if let Some(text) = block["text"].as_str() {
                            self.text(text, false)?;
                        }
                    }
                    "thinking" => {
                        if let Some(text) = block["thinking"].as_str() {
                            self.text(text, true)?;
                        }
                        self.signature(&block["signature"])?;
                    }
                    "redacted_thinking" => self.signature(&block["data"])?,
                    _ => return Err(ModelError::InvalidResponse),
                }
                self.blocks.insert(
                    index,
                    Block {
                        kind: kind.to_owned(),
                        arguments: String::new(),
                        closed: false,
                    },
                );
            }
            "content_block_delta" => {
                let index = index(value, "index")?;
                let block = self
                    .blocks
                    .get(&index)
                    .filter(|block| !block.closed)
                    .ok_or(ModelError::InvalidResponse)?;
                let kind = block.kind.clone();
                let delta = &value["delta"];
                match (kind.as_str(), delta["type"].as_str()) {
                    ("tool_use", Some("input_json_delta")) => {
                        let text = delta["partial_json"]
                            .as_str()
                            .ok_or(ModelError::InvalidToolCall)?;
                        let block = self
                            .blocks
                            .get_mut(&index)
                            .ok_or(ModelError::InvalidResponse)?;
                        if block.arguments.len().saturating_add(text.len()) > MAX_ARGUMENT_BYTES {
                            return Err(ModelError::ResponseTooLarge);
                        }
                        block.arguments.push_str(text);
                    }
                    ("text", Some("text_delta")) => self.text(
                        delta["text"].as_str().ok_or(ModelError::InvalidResponse)?,
                        false,
                    )?,
                    ("thinking", Some("thinking_delta")) => self.text(
                        delta["thinking"]
                            .as_str()
                            .ok_or(ModelError::InvalidResponse)?,
                        true,
                    )?,
                    ("thinking", Some("signature_delta")) => self.signature(&delta["signature"])?,
                    _ => return Err(ModelError::InvalidResponse),
                }
            }
            "content_block_stop" => {
                let index = index(value, "index")?;
                let block = self
                    .blocks
                    .get_mut(&index)
                    .filter(|block| !block.closed)
                    .ok_or(ModelError::InvalidResponse)?;
                if block.kind == "tool_use" && !block.arguments.is_empty() {
                    arguments(&block.arguments)?;
                }
                block.closed = true;
            }
            "message_delta" => {
                stop_reason(value["delta"]["stop_reason"].as_str())?;
                self.reason = true;
            }
            "message_stop" => {
                if !self.reason || self.blocks.values().any(|block| !block.closed) {
                    return Err(ModelError::IncompleteResponse);
                }
                self.done = true;
            }
            "error" => return Err(ModelError::Provider),
            _ => return Err(ModelError::InvalidResponse),
        }
        Ok(())
    }
}
