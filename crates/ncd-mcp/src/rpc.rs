//! JSON-RPC 2.0 帧：initialize / ping / tools/list / tools/call，外加通知吞掉。
//! 返回 None 表示这是通知，HTTP 层按 202 收尾；其余情况返回应答帧。

use serde_json::{Value, json};

use crate::gate::Decision;
use crate::server::ServerCtx;
use crate::tools::{self, ToolError};

/// 本服务说出的协议版本；客户端报来老版本也照常服务（用到的语义一样）
pub(crate) const PROTOCOL_VERSION: &str = "2025-06-18";
pub(crate) const SERVER_NAME: &str = "napcatqq-desktop-onebot-debug";

const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;

fn error_frame(id: Value, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message },
    })
}

fn result_frame(id: Value, result: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result,
    })
}

/// 工具结果包装：text 里放 JSON 字符串（老客户端只看 content），structuredContent 给新客户端
fn tool_ok_frame(id: Value, value: Value) -> Value {
    let text = serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string());
    result_frame(
        id,
        json!({
            "content": [{ "type": "text", "text": text }],
            "structuredContent": value,
        }),
    )
}

fn tool_err_frame(id: Value, error: ToolError) -> Value {
    let mut result = json!({
        "content": [{ "type": "text", "text": error.message }],
        "isError": true,
    });
    if let Some(structured) = error.structured {
        result["structuredContent"] = structured;
    }
    result_frame(id, result)
}

/// 单帧处理。通知（没有 id）一律不回应；frame 不是对象按 Invalid Request 处理
pub(crate) async fn handle_frame(ctx: &ServerCtx, frame: &Value) -> Option<Value> {
    let Some(object) = frame.as_object() else {
        return Some(error_frame(Value::Null, INVALID_REQUEST, "帧必须是对象"));
    };
    let id = object.get("id").cloned();
    let method = object.get("method").and_then(Value::as_str);
    let Some(method) = method else {
        return id.map(|id| error_frame(id, INVALID_REQUEST, "缺 method"));
    };
    // 通知没有 id，不回包
    let Some(id) = id else {
        return None;
    };
    if frame.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return Some(error_frame(id, INVALID_REQUEST, "jsonrpc 必须是 \"2.0\""));
    }
    let params = object.get("params").cloned().unwrap_or(Value::Null);
    Some(match method {
        "initialize" => result_frame(
            id,
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": { "tools": { "listChanged": false } },
                "serverInfo": { "name": SERVER_NAME, "version": env!("CARGO_PKG_VERSION") },
            }),
        ),
        "ping" => result_frame(id, json!({})),
        "tools/list" => result_frame(
            id,
            json!({
                "tools": tools::tool_defs()
                    .iter()
                    .map(|def| json!({
                        "name": def.name,
                        "description": def.description,
                        "inputSchema": def.input_schema,
                    }))
                    .collect::<Vec<_>>(),
            }),
        ),
        "tools/call" => call_tool(ctx, id, &params).await,
        _ if method.starts_with("notifications/") => return None,
        _ => error_frame(id, METHOD_NOT_FOUND, "未知方法"),
    })
}

/// 入流的整体：可能是单个请求，也可能是一批。全通知时返回 None（HTTP 202）
pub(crate) async fn handle_body(ctx: &ServerCtx, body: &Value) -> Option<Value> {
    match body {
        Value::Array(frames) => {
            if frames.is_empty() {
                return Some(error_frame(Value::Null, INVALID_REQUEST, "空批次"));
            }
            let mut answers = Vec::new();
            for frame in frames {
                if let Some(answer) = handle_frame(ctx, frame).await {
                    answers.push(answer);
                }
            }
            if answers.is_empty() {
                None
            } else {
                Some(Value::Array(answers))
            }
        }
        other => handle_frame(ctx, other).await,
    }
}

/// JSON 都没法解析时的应答（谈不上 id）
pub(crate) fn parse_error_frame() -> Value {
    error_frame(Value::Null, PARSE_ERROR, "不是合法的 JSON-RPC 帧")
}

async fn call_tool(ctx: &ServerCtx, id: Value, params: &Value) -> Value {
    let name = params.get("name").and_then(Value::as_str).unwrap_or("");
    if !tools::tool_exists(name) {
        return error_frame(id, INVALID_PARAMS, "未知工具");
    }
    let args = params
        .get("arguments")
        .filter(|a| a.is_object())
        .cloned()
        .unwrap_or_else(|| json!({}));

    let safety = tools::effective_safety(&ctx.tools, name, &args).await;
    match ctx.tools.gate.decide(name, safety, &args) {
        Decision::Allow => match tools::dispatch(&ctx.tools, name, args).await {
            Ok(value) => tool_ok_frame(id, value),
            Err(error) => tool_err_frame(id, error),
        },
        Decision::NeedConfirm { token } => tool_err_frame(
            id,
            ToolError {
                message: format!(
                    "「{name}」是有副作用的操作：把 confirm_token 填进 arguments 重放同一调用即视为确认执行"
                ),
                structured: Some(json!({
                    "requireConfirmation": true,
                    "confirmToken": token,
                    "expiresInSec": crate::gate::CHALLENGE_TTL_SECS,
                })),
            },
        ),
        Decision::Reject { reason } => tool_err_frame(
            id,
            ToolError {
                message: reason,
                structured: Some(json!({"rejected": true})),
            },
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::tests_support::test_server_ctx;

    async fn answer(method: &str, params: Value) -> Value {
        let ctx = test_server_ctx();
        handle_body(
            &ctx,
            json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params}),
        )
        .await
        .expect("带 id 的请求一定有应答")
    }

    #[tokio::test]
    async fn initialize_and_ping() {
        let init = answer("initialize", json!({"protocolVersion": "2025-03-26"})).await;
        assert_eq!(init["id"], 1);
        assert_eq!(init["result"]["protocolVersion"], PROTOCOL_VERSION);
        assert_eq!(init["result"]["serverInfo"]["name"], SERVER_NAME);

        let ping = answer("ping", json!({})).await;
        assert_eq!(ping["result"], json!({}));
    }

    #[tokio::test]
    async fn tools_list_matches_defs() {
        let list = answer("tools/list", json!({})).await;
        let tools = list["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), crate::tools::tool_defs().len());
        assert_eq!(tools[0]["name"], "list_targets");
        assert_eq!(tools[0]["inputSchema"]["type"], "object");
    }

    #[tokio::test]
    async fn unknown_method_and_unknown_tool() {
        let nope = answer("no/such", json!({})).await;
        assert_eq!(nope["error"]["code"], METHOD_NOT_FOUND);

        let nope = answer("tools/call", json!({"name": "wat"})).await;
        assert_eq!(nope["error"]["code"], INVALID_PARAMS);
    }

    #[tokio::test]
    async fn notifications_get_no_answer() {
        let ctx = test_server_ctx();
        let out = handle_body(
            &ctx,
            json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
        )
        .await;
        assert!(out.is_none());
        // 整批都是通知：整体也没应答（HTTP 202）
        let out = handle_body(
            &ctx,
            json!([
                {"jsonrpc": "2.0", "method": "notifications/initialized"},
                {"jsonrpc": "2.0", "method": "notifications/cancelled", "params": {}},
            ]),
        )
        .await;
        assert!(out.is_none());
    }

    #[tokio::test]
    async fn batch_mixed_request_and_notification() {
        let ctx = test_server_ctx();
        let out = handle_body(
            &ctx,
            json!([
                {"jsonrpc": "2.0", "id": 7, "method": "ping"},
                {"jsonrpc": "2.0", "method": "notifications/initialized"},
                {"jsonrpc": "2.0", "id": 8, "method": "tools/list"},
            ]),
        )
        .await
        .expect("批次里有请求就有应答");
        let answers = out.as_array().unwrap();
        assert_eq!(answers.len(), 2);
        assert_eq!(answers[0]["id"], 7);
        assert_eq!(answers[1]["id"], 8);
    }

    #[tokio::test]
    async fn side_effect_tool_returns_challenge_then_replay_runs() {
        let ctx = test_server_ctx();
        let first = handle_body(
            &ctx,
            json!({
                "jsonrpc": "2.0", "id": 1, "method": "tools/call",
                "params": {"name": "set_enabled", "arguments": {"enabled": false}},
            }),
        )
        .await
        .unwrap();
        assert_eq!(first["result"]["isError"], true);
        let token = first["result"]["structuredContent"]["confirmToken"]
            .as_str()
            .unwrap()
            .to_owned();
        assert_eq!(
            first["result"]["structuredContent"]["requireConfirmation"],
            true
        );

        let replay = handle_body(
            &ctx,
            json!({
                "jsonrpc": "2.0", "id": 2, "method": "tools/call",
                "params": {"name": "set_enabled", "arguments": {"enabled": false, "confirm_token": token}},
            }),
        )
        .await
        .unwrap();
        // 真的执行了：结果是 set_enabled 的确认
        assert_eq!(replay["result"]["structuredContent"]["enabled"], false);
        assert!(replay["result"]["isError"].is_null());
    }

    #[tokio::test]
    async fn dangerous_tool_rejected_without_flag() {
        let ctx = test_server_ctx();
        let out = handle_body(
            &ctx,
            json!({
                "jsonrpc": "2.0", "id": 1, "method": "tools/call",
                "params": {"name": "clear_history", "arguments": {}},
            }),
        )
        .await
        .unwrap();
        assert_eq!(out["result"]["isError"], true);
        assert_eq!(out["result"]["structuredContent"]["rejected"], true);
    }
}
