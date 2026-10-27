//! Gemini 单候选、工具对象和显式 finishReason 校验。

use super::*;

impl State {
    pub(super) fn gemini(&mut self, value: &Value) -> Result<(), ModelError> {
        if value["promptFeedback"]["blockReason"].as_str().is_some() {
            return Err(ModelError::Provider);
        }
        let candidates = value["candidates"]
            .as_array()
            .ok_or(ModelError::InvalidResponse)?;
        if candidates.len() != 1 {
            return Err(ModelError::InvalidResponse);
        }
        let candidate = &candidates[0];
        if candidate
            .get("index")
            .is_some_and(|index| index.as_u64() != Some(0))
        {
            return Err(ModelError::InvalidResponse);
        }
        if let Some(parts) = candidate["content"]["parts"].as_array() {
            if parts.len() > 64 {
                return Err(ModelError::ResponseTooLarge);
            }
            for part in parts {
                if let Some(text) = part["text"].as_str() {
                    self.text(text, part["thought"] == true)?;
                }
                self.signature(&part["thoughtSignature"])?;
                if let Some(call) = part.get("functionCall") {
                    self.tool(call["id"].as_str(), &call["name"])?;
                    object(&call["args"])?;
                    self.gemini_calls.push((
                        call["name"]
                            .as_str()
                            .ok_or(ModelError::InvalidToolCall)?
                            .to_owned(),
                        call["id"].as_str().map(str::to_owned),
                    ));
                }
                if part.get("text").is_none()
                    && part.get("functionCall").is_none()
                    && part.get("thoughtSignature").is_none()
                {
                    return Err(ModelError::InvalidResponse);
                }
            }
        }
        if let Some(reason) = candidate["finishReason"].as_str() {
            stop_reason(Some(reason))?;
            self.done = true;
        }
        Ok(())
    }
}
