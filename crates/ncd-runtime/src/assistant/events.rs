//! 将 ACP 通知与反向请求收窄为诊断助手事件。

use std::sync::atomic::Ordering;
use std::sync::{Arc, Weak};

use ncd_domain::assistant::*;
use serde_json::{Map, Value, json};
use tokio::sync::mpsc;

use super::acp::{AcpEvent, AcpProcess};
use super::session::{Session, lock, now_ms};

pub(super) async fn run(
    session: Weak<Session>,
    process: Arc<AcpProcess>,
    epoch: u64,
    mut events: mpsc::Receiver<AcpEvent>,
) {
    while let Some(event) = events.recv().await {
        let Some(session) = session.upgrade() else {
            process.shutdown().await;
            return;
        };
        if session.epoch.load(Ordering::Acquire) != epoch {
            return;
        }
        match event {
            AcpEvent::Notification { method, params } => {
                if method == "session/update"
                    && session.accepting_updates.load(Ordering::Acquire)
                    && owns_session(&session, &params)
                {
                    project_update(&session, epoch, &params["update"]);
                } else if method == "$/cancel_request" {
                    cancel_request(&session, epoch, &params);
                }
            }
            AcpEvent::Request { id, method, params } => {
                let Some((key, cancel)) = session.register_reverse(epoch, &id) else {
                    let _ = process
                        .respond_error(id, -32602, "反向请求已撤销或超过上限")
                        .await;
                    continue;
                };
                let process = Arc::clone(&process);
                tokio::spawn(async move {
                    let _guard = ReverseGuard {
                        session: Arc::clone(&session),
                        key,
                    };
                    if !owns_session(&session, &params)
                        || session.epoch.load(Ordering::Acquire) != epoch
                    {
                        let _ = process.respond_error(id, -32602, "会话无效或已撤销").await;
                        return;
                    }
                    let cancelled_id = id.clone();
                    let work = async {
                        match method.as_str() {
                            "session/request_permission" => {
                                let result = permission(&session, epoch, &params).await;
                                let _ = process.respond(id, result).await;
                            }
                            "elicitation/create" | "session/elicitation/create" => {
                                let result = question(&session, epoch, &params).await;
                                let _ = process.respond(id, result).await;
                            }
                            _ => {
                                let _ = process
                                    .respond_error(id, -32601, "NCD 未提供文件或终端操作")
                                    .await;
                            }
                        }
                    };
                    tokio::select! {
                        biased;
                        _ = cancel.cancelled() => { let _ = process.respond_error(cancelled_id, -32800, "请求已取消").await; },
                        _ = work => {},
                    }
                });
            }
            AcpEvent::Closed { reason } => {
                let _connecting = session.connecting.lock().await;
                if session.epoch.load(Ordering::Acquire) != epoch {
                    return;
                }
                let closed_epoch = session.advance_epoch();
                session.accepting_updates.store(false, Ordering::Release);
                session.revoke();
                session.state_at(
                    closed_epoch,
                    AssistantSessionState::Disconnected,
                    Some(reason),
                );
                session.host.close(&session.id()).await;
                let live = session.live.lock().await.take();
                if let Some(live) = live {
                    live.process.shutdown().await;
                }
                return;
            }
        }
    }
}

pub(super) fn cancel_request(session: &Session, epoch: u64, params: &Value) {
    if let Some(id) = params.get("requestId").or_else(|| params.get("id")) {
        session.cancel_reverse(epoch, id);
    }
}

struct ReverseGuard {
    session: Arc<Session>,
    key: String,
}
impl Drop for ReverseGuard {
    fn drop(&mut self) {
        self.session.finish_reverse(&self.key);
    }
}

fn owns_session(session: &Session, params: &Value) -> bool {
    let data = lock(&session.data);
    params
        .get("sessionId")
        .and_then(Value::as_str)
        .zip(data.stored.remote_session_id.as_deref())
        .is_some_and(|(actual, expected)| actual == expected)
}

fn project_update(session: &Session, epoch: u64, update: &Value) {
    match update["sessionUpdate"].as_str().unwrap_or_default() {
        "agent_message_chunk" => {
            if update["content"]["type"] == "text" {
                if let Some(text) = update["content"]["text"].as_str() {
                    session.delta(epoch, update["messageId"].as_str(), text);
                }
            }
        }
        "tool_call" | "tool_call_update" => {
            let Some(id) = update["toolCallId"].as_str() else {
                return;
            };
            let mut details = Vec::new();
            if let Some(input) = update.get("rawInput") {
                details.push(input.to_string());
            }
            if let Some(content) = update["content"].as_array() {
                for item in content.iter().take(12) {
                    if let Some(text) = item["content"]["text"]
                        .as_str()
                        .or_else(|| item["text"].as_str())
                    {
                        details.push(text.to_owned());
                    }
                }
            }
            session.tool(
                epoch,
                AssistantToolCall {
                    id: id.to_owned(),
                    title: update["title"].as_str().unwrap_or_default().to_owned(),
                    status: update["status"]
                        .as_str()
                        .unwrap_or("in_progress")
                        .chars()
                        .take(32)
                        .collect(),
                    details: details.join("\n"),
                    created_at_ms: now_ms(),
                },
            );
        }
        _ => {}
    }
}

async fn permission(session: &Session, epoch: u64, params: &Value) -> Value {
    let Some(raw_options) = params["options"]
        .as_array()
        .filter(|options| options.len() <= 16)
    else {
        return json!({"outcome": {"outcome": "cancelled"}});
    };
    let options = raw_options
        .iter()
        .filter_map(|option| {
            let id = option["optionId"].as_str().filter(|id| id.len() <= 256)?;
            let kind = option["kind"].as_str()?;
            if !matches!(
                kind,
                "allow_once" | "allow_always" | "reject_once" | "reject_always"
            ) {
                return None;
            }
            Some(AssistantPermissionOption {
                id: id.to_owned(),
                kind: kind.to_owned(),
                label: option["name"]
                    .as_str()
                    .unwrap_or(kind)
                    .chars()
                    .take(256)
                    .collect(),
            })
        })
        .collect::<Vec<_>>();
    if options.is_empty() {
        return json!({"outcome": {"outcome": "cancelled"}});
    }
    let tool = &params["toolCall"];
    let request = AssistantPermissionRequest {
        id: String::new(),
        source: AssistantPermissionSource::Agent,
        title: tool["title"]
            .as_str()
            .unwrap_or("Agent 请求操作权限")
            .to_owned(),
        details: tool
            .get("rawInput")
            .map(Value::to_string)
            .unwrap_or_default(),
        options,
        created_at_ms: now_ms(),
    };
    match session.request_permission_at(epoch, request).await {
        Ok(Some(option_id)) => json!({"outcome": {"outcome": "selected", "optionId": option_id}}),
        _ => json!({"outcome": {"outcome": "cancelled"}}),
    }
}

async fn question(session: &Session, epoch: u64, params: &Value) -> Value {
    if params.get("mode").is_some_and(|mode| mode != "form") {
        return json!({"action": "decline"});
    }
    let schema = params
        .get("requestedSchema")
        .or_else(|| params.get("schema"))
        .unwrap_or(&Value::Null);
    let Some(fields) = question_fields(schema) else {
        return json!({"action": "decline"});
    };
    let answer = session
        .ask_question(
            epoch,
            AssistantQuestion {
                id: String::new(),
                title: session
                    .host
                    .redact(params["message"].as_str().unwrap_or("需要补充信息")),
                fields,
                created_at_ms: now_ms(),
            },
        )
        .await;
    if answer.cancelled {
        return json!({"action": "cancel"});
    }
    match answer_content(schema, &answer) {
        Some(content) => json!({"action": "accept", "content": content}),
        None => json!({"action": "decline"}),
    }
}

pub(super) fn question_fields(schema: &Value) -> Option<Vec<AssistantQuestionField>> {
    if schema["type"] != "object" {
        return None;
    }
    let properties = schema["properties"].as_object()?;
    if properties.len() > 24 {
        return None;
    }
    let required = schema["required"].as_array();
    properties
        .iter()
        .map(|(name, field)| {
            if name.len() > 128 {
                return None;
            }
            let kind = field["type"].as_str()?;
            if !matches!(kind, "string" | "number" | "integer" | "boolean") {
                return None;
            }
            let title = field["title"].as_str().unwrap_or(name);
            let normalized = name.to_ascii_lowercase().replace(['_', '-'], "");
            if field["format"] == "password"
                || (kind == "string"
                    && ([
                        "password",
                        "passwd",
                        "apikey",
                        "accesstoken",
                        "secret",
                        "authorization",
                        "cookie",
                    ]
                    .iter()
                    .any(|part| normalized.contains(part))
                        || ["密码", "密钥", "令牌"]
                            .iter()
                            .any(|part| title.contains(part))))
            {
                return None;
            }
            let options = match field.get("enum") {
                Some(Value::Array(values)) if values.len() <= 64 => values
                    .iter()
                    .map(|value| value.as_str().map(str::to_owned))
                    .collect::<Option<Vec<_>>>()?,
                None => Vec::new(),
                _ => return None,
            };
            Some(AssistantQuestionField {
                name: name.clone(),
                title: field["title"]
                    .as_str()
                    .unwrap_or(name)
                    .chars()
                    .take(256)
                    .collect(),
                description: field["description"]
                    .as_str()
                    .map(|text| text.chars().take(1024).collect()),
                kind: kind.to_owned(),
                required: required
                    .is_some_and(|required| required.iter().any(|value| value == name)),
                options,
            })
        })
        .collect()
}

pub(super) fn answer_content(
    schema: &Value,
    answer: &AssistantQuestionAnswer,
) -> Option<Map<String, Value>> {
    let fields = question_fields(schema)?;
    if answer
        .values
        .keys()
        .any(|name| !fields.iter().any(|field| &field.name == name))
    {
        return None;
    }
    let mut content = Map::new();
    for field in fields {
        let Some(text) = answer
            .values
            .get(&field.name)
            .filter(|text| !text.is_empty())
        else {
            if field.required {
                return None;
            }
            continue;
        };
        let constraints = &schema["properties"][&field.name];
        let value = match field.kind.as_str() {
            "boolean" => Value::Bool(text.parse().ok()?),
            "integer" => Value::Number(text.parse::<i64>().ok()?.into()),
            "number" => {
                serde_json::Number::from_f64(text.parse::<f64>().ok()?).map(Value::Number)?
            }
            _ => {
                if !field.options.is_empty() && !field.options.contains(text) {
                    return None;
                }
                if constraints["minLength"]
                    .as_u64()
                    .is_some_and(|min| (text.chars().count() as u64) < min)
                    || constraints["maxLength"]
                        .as_u64()
                        .is_some_and(|max| (text.chars().count() as u64) > max)
                {
                    return None;
                }
                Value::String(text.clone())
            }
        };
        if let Some(number) = value.as_f64() {
            if constraints["minimum"]
                .as_f64()
                .is_some_and(|min| number < min)
                || constraints["maximum"]
                    .as_f64()
                    .is_some_and(|max| number > max)
            {
                return None;
            }
        }
        content.insert(field.name, value);
    }
    Some(content)
}
