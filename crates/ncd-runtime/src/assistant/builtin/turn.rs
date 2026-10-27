//! 串行完成模型与工具轮次，退出时结算所有已经声明的调用。

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use ncd_domain::assistant::{AssistantQuestion, AssistantToolCall};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use super::history::History;
use super::mcp::{McpClient, validate_arguments};
use super::model::{FinishReason, ModelClient, ModelReply, ToolCall, ToolSpec};
use super::{BuiltinRunContext, BuiltinSink};

const STEP_TEXT_LIMIT: usize = 32 * 1024;
const PRIVATE_CONTENT_LIMIT: usize = 128 * 1024;
const REASONING_LIMIT: usize = 64 * 1024;
const ARGUMENT_LIMIT: usize = 16 * 1024;
const RUN_BYTE_LIMIT: usize = 2 * 1024 * 1024;
const ASK_USER: &str = "ask_user";

pub(super) async fn run(
    mut context: BuiltinRunContext,
    history: &mut History,
    system_prompt: &str,
    input: &str,
    sink: Arc<dyn BuiltinSink>,
) -> Result<(), String> {
    if input.trim().is_empty() || input.len() > 16 * 1024 || system_prompt.len() > 64 * 1024 {
        return Err("内置助手输入超过限制".into());
    }
    if !(1..=12).contains(&context.profile.max_steps) || context.profile.request_timeout_secs == 0 {
        return Err("内置助手处理预算无效".into());
    }
    let cancel = context.cancelled.child_token();
    context.cancelled = cancel.clone();
    let timeout = Duration::from_secs(
        (context.profile.request_timeout_secs as u64)
            .saturating_mul(context.profile.max_steps as u64)
            .clamp(60, 1200),
    );
    let secrets = Secrets {
        key: context.api_key.clone(),
        bearer: context.connection.bearer_token.clone(),
    };
    let system = clean(&*sink, &secrets, system_prompt);
    history.sanitize(|text| clean_protocol(&secrets, text))?;
    history.begin(clean(&*sink, &secrets, input))?;
    sink.checkpoint(history).await?;
    let partial = Arc::new(Mutex::new(StreamingText::default()));
    let result = {
        let operation = run_inner(
            context,
            history,
            &system,
            Arc::clone(&sink),
            Arc::clone(&partial),
            &secrets,
        );
        tokio::pin!(operation);
        tokio::select! {
            biased;
            _ = cancel.cancelled() => Err("本轮诊断已停止；已提交操作的结果需重新核实".to_owned()),
            _ = tokio::time::sleep(timeout) => {
                cancel.cancel();
                Err("本轮诊断达到时间预算；已提交操作的结果需重新核实".to_owned())
            }
            result = &mut operation => result,
        }
    };
    if result.is_err() && history.pending().is_empty() {
        let mut partial = partial.lock().unwrap_or_else(PoisonError::into_inner);
        let text = clean(&*sink, &secrets, &partial.text);
        // 连接中断时 pending 仍可能是凭据前缀，不能当成安全文本归档。
        partial.pending.clear();
        drop(partial);
        if !text.is_empty() {
            let _ = history.append_assistant(text, None, Vec::new());
        }
    }
    let unsettled = history.settle_pending(
        "本轮已停止；已发出的操作可能提交，其余工具未执行。先核实状态，禁止自动重放。",
    );
    for id in unsettled {
        emit_tool(
            &*sink,
            id,
            "工具已停止",
            "failed",
            "本轮已停止；未确认的操作结果需重新核实",
        );
    }
    history.finish();
    sink.checkpoint(history).await?;
    result
}

struct Secrets {
    key: String,
    bearer: String,
}

async fn run_inner(
    context: BuiltinRunContext,
    history: &mut History,
    system: &str,
    sink: Arc<dyn BuiltinSink>,
    partial: Arc<Mutex<StreamingText>>,
    secrets: &Secrets,
) -> Result<(), String> {
    let mcp = McpClient::new(&context.connection)?;
    let mut tools = mcp.discover(&context.cancelled).await?;
    tools.push(question_spec());
    let client = ModelClient::new_with_settings(
        &context.profile,
        &context.api_key,
        Duration::from_secs(context.profile.request_timeout_secs as u64),
    )
    .map_err(|error| error.to_string())?;
    let mut consumed = 0usize;
    for _ in 0..context.profile.max_steps {
        if context.cancelled.is_cancelled() {
            return Err("本轮诊断已停止".into());
        }
        let messages = history.messages(system);
        consumed = consumed.saturating_add(
            serde_json::to_vec(&messages)
                .map_err(|_| "组织模型会话失败")?
                .len(),
        );
        if consumed > RUN_BYTE_LIMIT {
            return Err("本轮诊断达到字节预算".into());
        }
        *partial.lock().unwrap_or_else(PoisonError::into_inner) = StreamingText::default();
        let on_text = {
            let sink = Arc::clone(&sink);
            let partial = Arc::clone(&partial);
            let secrets = Secrets {
                key: secrets.key.clone(),
                bearer: secrets.bearer.clone(),
            };
            Arc::new(move |text: &str| {
                let mut current = partial.lock().unwrap_or_else(PoisonError::into_inner);
                let text = current.push(text, &*sink, &secrets);
                drop(current);
                if !text.is_empty() {
                    sink.text(&text);
                }
            }) as Arc<dyn Fn(&str) + Send + Sync>
        };
        let reply = client
            .complete(&messages, &tools, on_text, &context.cancelled)
            .await
            .map_err(|error| error.to_string())?;
        if context.cancelled.is_cancelled() {
            return Err("本轮诊断已停止".into());
        }
        let tail = partial
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .flush(&*sink, secrets);
        if !tail.is_empty() {
            sink.text(&tail);
        }
        // 协议字段不能经过面向 UI 的截断、JSON 重排或思考标签过滤。
        let text = clean_protocol(secrets, &reply.text);
        let reasoning_content = reply
            .reasoning_content
            .as_deref()
            .map(|reasoning| clean_protocol(secrets, reasoning));
        if text.len() > PRIVATE_CONTENT_LIMIT
            || reasoning_content
                .as_ref()
                .is_some_and(|reasoning| reasoning.len() > REASONING_LIMIT)
        {
            return Err("模型回复超过本轮字节预算".into());
        }
        let prepared = match prepare_calls(&reply, &tools, history, &*sink, secrets) {
            Ok(prepared) => prepared,
            Err(error) => {
                *partial.lock().unwrap_or_else(PoisonError::into_inner) = StreamingText::default();
                history.append_assistant(text, reasoning_content, Vec::new())?;
                return Err(error);
            }
        };
        consumed = consumed
            .saturating_add(text.len())
            .saturating_add(reasoning_content.as_ref().map_or(0, String::len))
            .saturating_add(
                prepared
                    .iter()
                    .map(|call| call.history.arguments.len())
                    .sum::<usize>(),
            );
        if consumed > RUN_BYTE_LIMIT {
            return Err("本轮诊断达到字节预算，工具尚未执行".into());
        }
        let mut protocol = reply.protocol;
        if let Some(protocol) = &mut protocol {
            protocol.sanitize(|text| clean_protocol(secrets, text))?;
        }
        history.append_assistant_with_protocol(
            text,
            reasoning_content,
            prepared.iter().map(|call| call.history.clone()).collect(),
            protocol,
        )?;
        *partial.lock().unwrap_or_else(PoisonError::into_inner) = StreamingText::default();
        sink.checkpoint(history).await?;
        if prepared.is_empty() {
            return match reply.finish_reason {
                FinishReason::Stop => Ok(()),
                FinishReason::Length => Err("模型达到输出上限，本轮已停止".into()),
                FinishReason::ContentFilter => Err("模型拒绝处理本轮内容".into()),
                FinishReason::ToolCalls => Err("模型未返回完整工具调用".into()),
            };
        }
        for call in prepared {
            if context.cancelled.is_cancelled() {
                return Err("本轮诊断已停止".into());
            }
            emit_tool(
                &*sink,
                call.history.id.clone(),
                &call.history.name,
                "in_progress",
                &call.history.arguments,
            );
            let result = if call.original.name == ASK_USER {
                let args = clean_value(&*sink, secrets, call.args, 0);
                ask_user(&*sink, &args, &context.cancelled).await
            } else {
                mcp.call(&call.original.name, call.args, &context.cancelled)
                    .await
            };
            let (value, failed) = match result {
                Ok(value) => {
                    let failed = value["isError"] == true || value["cancelled"] == true;
                    (value, failed)
                }
                Err(error) => (json!({"error": error}), true),
            };
            let value = clean_value(&*sink, secrets, value, 0);
            let content = value.to_string();
            consumed = consumed.saturating_add(content.len());
            history.append_tool(&call.history.id, content.clone())?;
            emit_tool(
                &*sink,
                call.history.id,
                &call.history.name,
                if failed { "failed" } else { "completed" },
                &content,
            );
            // 宿主落盘确认之前不提交下一项操作，重启也不会自动重放未完成调用。
            sink.checkpoint(history).await?;
            if failed {
                return Err("诊断工具未完成，本轮已停止；请查看工具结果并核实状态".into());
            }
            if consumed > RUN_BYTE_LIMIT {
                return Err("本轮诊断达到字节预算".into());
            }
        }
    }
    Err("本轮达到工具处理步数上限，可以补充信息后继续".into())
}

struct PreparedCall {
    original: ToolCall,
    history: ToolCall,
    args: Value,
}

fn prepare_calls(
    reply: &ModelReply,
    tools: &[ToolSpec],
    history: &History,
    sink: &dyn BuiltinSink,
    secrets: &Secrets,
) -> Result<Vec<PreparedCall>, String> {
    if reply.tool_calls.len() > 4
        || (!reply.tool_calls.is_empty() && !matches!(reply.finish_reason, FinishReason::ToolCalls))
    {
        return Err("模型工具调用未完整结束或超过单步上限，未执行工具".into());
    }
    let mut seen = history.call_ids();
    let mut prepared = Vec::new();
    for call in &reply.tool_calls {
        if call.id.trim().is_empty()
            || call.id.len() > 256
            || call.id.chars().any(char::is_control)
            || clean(sink, secrets, &call.id) != call.id
            || !seen.insert(call.id.clone())
            || call.arguments.len() > ARGUMENT_LIMIT
        {
            return Err("模型工具编号或参数无效，未执行工具".into());
        }
        let tool = tools
            .iter()
            .find(|tool| tool.name == call.name)
            .ok_or("模型请求了未提供的工具，未执行工具")?;
        let args: Value = serde_json::from_str(&call.arguments)
            .map_err(|_| "模型工具参数不是完整 JSON，未执行工具")?;
        if !args.is_object() || !validate_arguments(&tool.parameters, &args) {
            return Err("模型工具参数不符合工具约束，未执行工具".into());
        }
        if call.name == ASK_USER && question_schema(&args).is_none() {
            return Err("助手不能索取密码、密钥或令牌，未执行本批工具".into());
        }
        let safe_args = clean_value(sink, secrets, args.clone(), 0).to_string();
        prepared.push(PreparedCall {
            original: call.clone(),
            history: ToolCall {
                id: call.id.clone(),
                name: call.name.clone(),
                arguments: safe_args,
            },
            args,
        });
    }
    Ok(prepared)
}

fn question_spec() -> ToolSpec {
    ToolSpec {
        name: ASK_USER.into(),
        description:
            "仅在缺少用户掌握的事实时提出一个具体问题。可直接从 NCD 读取的状态不要询问用户。".into(),
        parameters: json!({"type":"object", "properties": {
            "question":{"type":"string","minLength":1,"maxLength":256},
            "options":{"type":"array","maxItems":6,
                "items":{"type":"string","minLength":1,"maxLength":128}}
        },"required":["question"],"additionalProperties":false}),
    }
}

async fn ask_user(
    sink: &dyn BuiltinSink,
    args: &Value,
    cancel: &CancellationToken,
) -> Result<Value, String> {
    let schema = question_schema(args).ok_or("助手不能索取凭据或无界补充信息")?;
    let fields = super::super::events::question_fields(&schema).ok_or("追问内容无效")?;
    let question = AssistantQuestion {
        id: String::new(),
        title: "补充诊断信息".into(),
        fields,
        created_at_ms: chrono::Utc::now().timestamp_millis() as f64,
    };
    tokio::select! {
        biased;
        _ = cancel.cancelled() => Err("本轮诊断已停止".into()),
        answer = sink.question(question) => {
            if answer.cancelled { Ok(json!({"cancelled": true})) }
            else { super::super::events::answer_content(&schema, &answer)
                .map(|content| json!({"cancelled": false, "values":content}))
                .ok_or_else(|| "补充信息不符合问题约束".into()) }
        },
    }
}

fn question_schema(args: &Value) -> Option<Value> {
    fn sensitive(text: &str) -> bool {
        let lower = text.to_ascii_lowercase();
        let normalized = lower.replace(['_', '-', ' '], "");
        [
            "password",
            "passwd",
            "apikey",
            "accesstoken",
            "privatekey",
            "secret",
            "authorization",
            "cookie",
            "bearer",
        ]
        .iter()
        .any(|word| normalized.contains(word))
            || lower
                .split(|character: char| !character.is_ascii_alphanumeric())
                .any(|word| matches!(word, "key" | "token"))
            || ["密码", "密钥", "令牌", "私钥", "口令"]
                .iter()
                .any(|word| text.contains(word))
    }
    let question = args["question"].as_str().filter(|text| {
        !text.trim().is_empty() && text.chars().count() <= 256 && !sensitive(text)
    })?;
    let mut answer = json!({"type":"string", "title":question,"minLength":1,"maxLength":2048});
    if let Some(options) = args.get("options") {
        let options = options.as_array().filter(|items| items.len() <= 6)?;
        let mut seen = std::collections::HashSet::new();
        for option in options {
            let text = option.as_str().filter(|text| {
                !text.is_empty() && text.chars().count() <= 128 && !sensitive(text)
            })?;
            if !seen.insert(text) {
                return None;
            }
        }
        if !options.is_empty() {
            answer["enum"] = Value::Array(options.clone());
        }
    }
    Some(json!({"type":"object", "properties":{"answer":answer},
        "required":["answer"], "additionalProperties":false}))
}

fn emit_tool(sink: &dyn BuiltinSink, id: String, title: &str, status: &str, details: &str) {
    sink.tool(AssistantToolCall {
        id,
        title: sink.redact(title),
        status: status.into(),
        details: sink.redact(details),
        created_at_ms: chrono::Utc::now().timestamp_millis() as f64,
    });
}

fn clean(sink: &dyn BuiltinSink, secrets: &Secrets, text: &str) -> String {
    sink.redact(&clean_protocol(secrets, text))
}

fn clean_protocol(secrets: &Secrets, text: &str) -> String {
    let mut text = text.to_owned();
    for secret in [&secrets.key, &secrets.bearer] {
        if !secret.is_empty() {
            text = text.replace(secret, "[REDACTED]");
        }
    }
    text
}

fn clean_value(sink: &dyn BuiltinSink, secrets: &Secrets, value: Value, depth: usize) -> Value {
    if depth > 20 {
        return json!("[内容超过嵌套限制]");
    }
    match value {
        Value::String(text) => {
            if let Ok(nested @ (Value::Object(_) | Value::Array(_))) = serde_json::from_str(&text) {
                Value::String(clean_value(sink, secrets, nested, depth + 1).to_string())
            } else {
                Value::String(clean(sink, secrets, &text))
            }
        }
        Value::Array(items) => Value::Array(
            items
                .into_iter()
                .map(|item| clean_value(sink, secrets, item, depth + 1))
                .collect(),
        ),
        Value::Object(object) => Value::Object(
            object
                .into_iter()
                .map(|(key, value)| {
                    (
                        clean(sink, secrets, &key),
                        clean_value(sink, secrets, value, depth + 1),
                    )
                })
                .collect(),
        ),
        value => value,
    }
}

fn prefix(text: &str, max: usize) -> &str {
    let mut end = text.len().min(max);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

#[derive(Default)]
struct StreamingText {
    pending: String,
    text: String,
}

impl StreamingText {
    fn push(&mut self, chunk: &str, sink: &dyn BuiltinSink, secrets: &Secrets) -> String {
        self.pending.push_str(chunk);
        self.pending = clean(sink, secrets, &self.pending);
        // 留住可能跨 SSE chunk 的凭据前缀，后续片段到达前不把它发送到 UI。
        let mut keep = 0usize;
        for secret in [&secrets.key, &secrets.bearer] {
            for length in 1..secret.len().min(self.pending.len().saturating_add(1)) {
                if secret.is_char_boundary(length)
                    && self.pending.is_char_boundary(self.pending.len() - length)
                    && self.pending.ends_with(&secret[..length])
                {
                    keep = keep.max(length);
                }
            }
        }
        let ready = self.pending[..self.pending.len() - keep].to_owned();
        self.pending = self.pending[self.pending.len() - keep..].to_owned();
        self.record(&ready)
    }

    fn flush(&mut self, sink: &dyn BuiltinSink, secrets: &Secrets) -> String {
        let tail = clean(sink, secrets, &std::mem::take(&mut self.pending));
        self.record(&tail)
    }

    fn record(&mut self, text: &str) -> String {
        let text = prefix(text, STEP_TEXT_LIMIT.saturating_sub(self.text.len()));
        self.text.push_str(text);
        text.to_owned()
    }
}
