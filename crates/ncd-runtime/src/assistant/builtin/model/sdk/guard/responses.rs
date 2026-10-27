//! Responses 增量、工具参数和完整 response.completed 校验。

use super::*;

impl State {
    pub(super) fn responses(&mut self, value: &Value) -> Result<(), ModelError> {
        let kind = value["type"].as_str().ok_or(ModelError::InvalidResponse)?;
        for field in ["output_index", "content_index"] {
            if value.get(field).is_some() {
                index(value, field)?;
            }
        }
        match kind {
            "response.created"
            | "response.in_progress"
            | "response.content_part.added"
            | "response.content_part.done" => {}
            "response.output_item.added" => {
                let item = &value["item"];
                match item["type"].as_str() {
                    Some("function_call") => {
                        let index = index(value, "output_index")?;
                        if self.blocks.contains_key(&index) {
                            return Err(ModelError::InvalidToolCall);
                        }
                        self.tool(
                            Some(
                                item["call_id"]
                                    .as_str()
                                    .ok_or(ModelError::InvalidToolCall)?,
                            ),
                            &item["name"],
                        )?;
                        self.blocks.insert(
                            index,
                            Block {
                                kind: "tool".into(),
                                ..Default::default()
                            },
                        );
                    }
                    Some("message") | Some("reasoning") => {}
                    _ => return Err(ModelError::InvalidResponse),
                }
            }
            "response.function_call_arguments.delta" => {
                let index = index(value, "output_index")?;
                let block = self
                    .blocks
                    .get_mut(&index)
                    .filter(|block| !block.closed)
                    .ok_or(ModelError::InvalidToolCall)?;
                let delta = value["delta"].as_str().ok_or(ModelError::InvalidToolCall)?;
                if block.arguments.len().saturating_add(delta.len()) > MAX_ARGUMENT_BYTES {
                    return Err(ModelError::ResponseTooLarge);
                }
                block.arguments.push_str(delta);
            }
            "response.function_call_arguments.done" => {
                arguments(
                    value["arguments"]
                        .as_str()
                        .ok_or(ModelError::InvalidToolCall)?,
                )?;
            }
            "response.output_item.done" => {
                if value["item"]["type"] == "function_call" {
                    let index = index(value, "output_index")?;
                    let block = self
                        .blocks
                        .get_mut(&index)
                        .ok_or(ModelError::InvalidToolCall)?;
                    arguments(
                        value["item"]["arguments"]
                            .as_str()
                            .ok_or(ModelError::InvalidToolCall)?,
                    )?;
                    block.closed = true;
                }
                if value["item"]["type"] == "reasoning" {
                    self.signature(&value["item"]["encrypted_content"])?;
                }
            }
            "response.output_text.delta" => self.text(
                value["delta"].as_str().ok_or(ModelError::InvalidResponse)?,
                false,
            )?,
            "response.reasoning_summary_text.delta" | "response.reasoning_text.delta" => self
                .text(
                    value["delta"].as_str().ok_or(ModelError::InvalidResponse)?,
                    true,
                )?,
            "response.output_text.done"
            | "response.reasoning_summary_text.done"
            | "response.reasoning_summary_part.added"
            | "response.reasoning_summary_part.done"
            | "response.reasoning_text.done" => {}
            "response.completed" => {
                if value["response"]["status"] != "completed"
                    || self.blocks.values().any(|block| !block.closed)
                {
                    return Err(ModelError::IncompleteResponse);
                }
                if let Some(output) = value["response"].get("output") {
                    let output = output.as_array().ok_or(ModelError::InvalidResponse)?;
                    if output.len() > 64 {
                        return Err(ModelError::ResponseTooLarge);
                    }
                    for item in output {
                        if item["type"] == "function_call" {
                            if !item["call_id"]
                                .as_str()
                                .is_some_and(|id| valid_identifier(id, 256))
                                || !item["name"]
                                    .as_str()
                                    .is_some_and(|name| valid_identifier(name, 128))
                            {
                                return Err(ModelError::InvalidToolCall);
                            }
                            arguments(
                                item["arguments"]
                                    .as_str()
                                    .ok_or(ModelError::InvalidToolCall)?,
                            )?;
                        }
                    }
                }
                self.done = true;
            }
            "response.incomplete" | "response.failed" | "error" => {
                return Err(ModelError::IncompleteResponse);
            }
            _ => return Err(ModelError::InvalidResponse),
        }
        Ok(())
    }
}
