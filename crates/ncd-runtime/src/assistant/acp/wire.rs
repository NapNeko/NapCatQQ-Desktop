//! ACP stdio 帧的解析与长度限制。

use std::io::{self, Write};

use serde_json::{Value, json};
use tokio::io::{AsyncBufRead, AsyncBufReadExt};

pub(super) const MAX_FRAME_BYTES: usize = 1024 * 1024;
const MAX_METHOD_BYTES: usize = 256;
const MAX_ID_BYTES: usize = 256;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(super) enum RequestId {
    Null,
    Number(i64),
    String(String),
}

impl RequestId {
    pub(super) fn parse(value: &Value) -> Result<Self, String> {
        match value {
            Value::Null => Ok(Self::Null),
            Value::Number(number) => number
                .as_i64()
                .map(Self::Number)
                .ok_or_else(|| "Agent 请求编号必须是整数".into()),
            Value::String(text) if text.len() <= MAX_ID_BYTES => Ok(Self::String(text.clone())),
            _ => Err("Agent 请求编号无效".into()),
        }
    }

    pub(super) fn value(&self) -> Value {
        match self {
            Self::Null => Value::Null,
            Self::Number(number) => json!(number),
            Self::String(text) => json!(text),
        }
    }
}

pub(super) enum Incoming {
    Notification {
        method: String,
        params: Value,
    },
    Request {
        id: RequestId,
        method: String,
        params: Value,
    },
    Response {
        id: RequestId,
        result: Result<Value, String>,
    },
}

pub(super) fn validate_method(method: &str) -> Result<(), String> {
    if method.is_empty() || method.len() > MAX_METHOD_BYTES || method.chars().any(char::is_control)
    {
        return Err("Agent 方法名无效".into());
    }
    Ok(())
}

pub(super) fn encode(value: Value) -> Result<Vec<u8>, String> {
    let mut buffer = FrameBuffer(Vec::with_capacity(4096));
    serde_json::to_writer(&mut buffer, &value).map_err(|_| "Agent 消息超过长度限制".to_string())?;
    buffer.0.push(b'\n');
    Ok(buffer.0)
}

struct FrameBuffer(Vec<u8>);

impl Write for FrameBuffer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.0.len().saturating_add(bytes.len()) > MAX_FRAME_BYTES {
            return Err(io::Error::other("ACP frame limit exceeded"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(super) fn parse(bytes: &[u8]) -> Result<Incoming, String> {
    let value: Value =
        serde_json::from_slice(bytes).map_err(|_| "Agent 输出不是有效的 JSON 消息".to_string())?;
    let object = value
        .as_object()
        .ok_or_else(|| "Agent 消息必须是对象".to_string())?;
    if object.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return Err("Agent 消息的 JSON-RPC 版本无效".into());
    }
    if let Some(method) = object.get("method") {
        if object.contains_key("result") || object.contains_key("error") {
            return Err("Agent 请求混入了响应字段".into());
        }
        let method = method
            .as_str()
            .ok_or_else(|| "Agent 方法名无效".to_string())?;
        validate_method(method)?;
        let params = object.get("params").cloned().unwrap_or(Value::Null);
        if !params.is_null() && !params.is_object() && !params.is_array() {
            return Err("Agent 请求参数无效".into());
        }
        return match object.get("id") {
            Some(id) => Ok(Incoming::Request {
                id: RequestId::parse(id)?,
                method: method.to_owned(),
                params,
            }),
            None => Ok(Incoming::Notification {
                method: method.to_owned(),
                params,
            }),
        };
    }
    let id = object
        .get("id")
        .ok_or_else(|| "Agent 响应缺少请求编号".to_string())?;
    let id = RequestId::parse(id)?;
    let result = match (object.get("result"), object.get("error")) {
        (Some(result), None) => Ok(result.clone()),
        (None, Some(error)) => {
            let error = error
                .as_object()
                .ok_or_else(|| "Agent 错误响应无效".to_string())?;
            let code = error
                .get("code")
                .and_then(Value::as_i64)
                .ok_or_else(|| "Agent 错误响应缺少错误码".to_string())?;
            if !error.get("message").is_some_and(Value::is_string) {
                return Err("Agent 错误响应缺少说明".into());
            }
            // 上游错误可能回显凭据；已核实的固定前缀只能映射为本地提示，不能透传尾部或 data。
            Err(match code {
                -32000 => format!(
                    "Agent 返回需要认证，请在该 Agent 中检查登录状态后重试（错误码 {code}）"
                ),
                -32603 if error.get("message").and_then(Value::as_str).is_some_and(|message|
                    message.to_ascii_lowercase().contains("insufficient balance")
                ) => format!("Agent 报告模型服务余额不足，请在对应服务中检查额度后重试（错误码 {code}）"),
                -32603 if error.get("message").and_then(Value::as_str).is_some_and(|message| message.starts_with(
                    "Internal error: turn failed: llm-deepseek: no API key for provider route \"deepseek-official\"; "
                )) => format!("DSH 的 deepseek-official 模型路由缺少 API Key，请先在 DSH 中配置该路由后重试（错误码 {code}）"),
                _ => format!("Agent 请求失败（错误码 {code}）"),
            })
        }
        _ => return Err("Agent 响应必须包含一个结果或错误".into()),
    };
    Ok(Incoming::Response { id, result })
}

pub(super) async fn read_frame<R: AsyncBufRead + Unpin>(
    reader: &mut R,
) -> Result<Option<Vec<u8>>, String> {
    let mut line = Vec::new();
    loop {
        let available = reader
            .fill_buf()
            .await
            .map_err(|_| "读取 Agent 输出失败".to_string())?;
        if available.is_empty() {
            return if line.is_empty() {
                Ok(None)
            } else {
                Err("Agent 消息未完整结束".into())
            };
        }
        let newline = available.iter().position(|byte| *byte == b'\n');
        let take = newline.unwrap_or(available.len());
        if line.len().saturating_add(take) > MAX_FRAME_BYTES {
            return Err("Agent 输出超过单条消息限制".into());
        }
        line.extend_from_slice(&available[..take]);
        reader.consume(take + usize::from(newline.is_some()));
        if newline.is_some() {
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            if line.is_empty() {
                return Err("Agent 输出了空消息".into());
            }
            return Ok(Some(line));
        }
    }
}
