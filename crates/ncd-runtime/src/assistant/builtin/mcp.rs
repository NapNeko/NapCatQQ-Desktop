//! 只连接宿主交付的回环 MCP，不继承代理或跟随重定向。

use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use futures_util::StreamExt;
use ncd_traits::assistant::AssistantToolConnection;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use super::model::ToolSpec;

const CATALOG_LIMIT: usize = 128 * 1024;
pub(super) const RESULT_LIMIT: usize = 16 * 1024;
const MCP_VERSION: &str = "2025-11-25";
const NCD_TOOLS: &[&str] = &[
    "diagnose_bot",
    "get_bot_runtime",
    "tail_bot_logs",
    "list_channels",
    "list_actions",
    "describe_action",
    "search_actions",
    "get_security_policy",
    "call_action",
    "start_bot",
    "stop_bot",
    "restart_bot",
];

pub(super) struct McpClient {
    client: reqwest::Client,
    url: reqwest::Url,
    next_id: AtomicU64,
}

impl McpClient {
    pub(super) fn new(connection: &AssistantToolConnection) -> Result<Self, String> {
        let url = reqwest::Url::parse(&connection.url).map_err(|_| "诊断工具地址无效")?;
        if url.scheme() != "http"
            || url.host_str() != Some("127.0.0.1")
            || url.port().is_none_or(|port| port == 0)
            || url.path() != "/mcp"
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err("内置助手只能使用宿主提供的本机诊断工具连接".into());
        }
        if connection.bearer_token.is_empty()
            || connection.bearer_token.len() > 512
            || connection.bearer_token.chars().any(char::is_control)
        {
            return Err("诊断工具授权无效".into());
        }
        let mut headers = HeaderMap::new();
        let mut bearer = HeaderValue::from_str(&format!("Bearer {}", connection.bearer_token))
            .map_err(|_| "诊断工具授权无效")?;
        bearer.set_sensitive(true);
        headers.insert(AUTHORIZATION, bearer);
        headers.insert(
            "mcp-protocol-version",
            HeaderValue::from_static(MCP_VERSION),
        );
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .default_headers(headers)
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(115))
            .build()
            .map_err(|_| "无法建立诊断工具客户端")?;
        Ok(Self {
            client,
            url,
            next_id: AtomicU64::new(1),
        })
    }

    pub(super) async fn discover(
        &self,
        cancel: &CancellationToken,
    ) -> Result<Vec<ToolSpec>, String> {
        let initialized = self.request("initialize", json!({
            "protocolVersion": MCP_VERSION, "capabilities": {},
            "clientInfo": {"name": "napcatqq-desktop-builtin", "version": env!("CARGO_PKG_VERSION")}
        }), CATALOG_LIMIT, cancel).await?;
        if initialized["protocolVersion"] != MCP_VERSION
            || !initialized["capabilities"]["tools"].is_object()
        {
            return Err("诊断工具未协商到受支持的协议".into());
        }
        self.post(
            json!({"jsonrpc":"2.0", "method":"notifications/initialized"}),
            1024,
            true,
            cancel,
        )
        .await?;
        let catalog = self
            .request("tools/list", json!({}), CATALOG_LIMIT, cancel)
            .await?;
        parse_catalog(&catalog)
    }

    pub(super) async fn call(
        &self,
        name: &str,
        args: Value,
        cancel: &CancellationToken,
    ) -> Result<Value, String> {
        if !NCD_TOOLS.contains(&name) || !args.is_object() {
            return Err("诊断工具调用不在允许范围".into());
        }
        let result = self
            .request(
                "tools/call",
                json!({"name":name,"arguments":args}),
                RESULT_LIMIT,
                cancel,
            )
            .await?;
        if !result.is_object()
            || !result.get("content").is_some_and(|value| {
                value.as_array().is_some_and(|items| {
                    items.len() <= 32
                        && items
                            .iter()
                            .all(|item| item["type"] == "text" && item["text"].is_string())
                })
            })
            || result
                .get("isError")
                .is_some_and(|value| !value.is_boolean())
        {
            return Err("诊断工具返回了不受支持的结果；操作结果需重新核实".into());
        }
        Ok(result)
    }

    async fn request(
        &self,
        method: &str,
        params: Value,
        limit: usize,
        cancel: &CancellationToken,
    ) -> Result<Value, String> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let response = self
            .post(
                json!({"jsonrpc":"2.0", "id":id,
            "method":method, "params":params}),
                limit,
                false,
                cancel,
            )
            .await?;
        if response["jsonrpc"] != "2.0"
            || response["id"].as_u64() != Some(id)
            || response.get("error").is_some()
            || response.get("result").is_none()
        {
            return Err("诊断工具协议失败；已提交操作的结果需重新核实".into());
        }
        Ok(response["result"].clone())
    }

    async fn post(
        &self,
        body: Value,
        limit: usize,
        notification: bool,
        cancel: &CancellationToken,
    ) -> Result<Value, String> {
        let operation = async {
            let response = self
                .client
                .post(self.url.clone())
                .header("accept", "application/json")
                .json(&body)
                .send()
                .await
                .map_err(|_| "诊断工具连接失败；已提交操作的结果需重新核实")?;
            if notification && matches!(response.status().as_u16(), 202 | 204) {
                return Ok(Value::Null);
            }
            if !response.status().is_success() {
                return Err("诊断工具拒绝请求；已提交操作的结果需重新核实".into());
            }
            if response
                .content_length()
                .is_some_and(|length| length > limit as u64)
            {
                return Err("诊断工具结果超过限制；操作结果需重新核实".into());
            }
            let mut stream = response.bytes_stream();
            let mut bytes = Vec::new();
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(|_| "诊断工具响应中断；操作结果需重新核实")?;
                if bytes.len().saturating_add(chunk.len()) > limit {
                    return Err("诊断工具结果超过限制；操作结果需重新核实".into());
                }
                bytes.extend_from_slice(&chunk);
            }
            if notification && bytes.is_empty() {
                return Ok(Value::Null);
            }
            serde_json::from_slice(&bytes)
                .map_err(|_| "诊断工具响应无效；操作结果需重新核实".into())
        };
        tokio::select! {
            biased;
            _ = cancel.cancelled() => Err("本轮诊断已停止；已提交操作的结果需重新核实".into()),
            result = operation => result,
        }
    }
}

fn parse_catalog(catalog: &Value) -> Result<Vec<ToolSpec>, String> {
    if catalog
        .get("nextCursor")
        .is_some_and(|cursor| !cursor.is_null())
    {
        return Err("诊断工具目录超过本轮支持范围".into());
    }
    let entries = catalog["tools"]
        .as_array()
        .filter(|tools| !tools.is_empty() && tools.len() <= 32)
        .ok_or("诊断工具目录无效")?;
    let mut names = HashSet::new();
    let mut tools = Vec::new();
    for entry in entries {
        let name = entry["name"].as_str().ok_or("诊断工具名称无效")?;
        if !NCD_TOOLS.contains(&name) || !names.insert(name) {
            return Err("诊断工具目录包含未知或重复工具".into());
        }
        let description = entry["description"]
            .as_str()
            .filter(|text| text.len() <= 8192)
            .ok_or("诊断工具说明无效")?;
        let schema = &entry["inputSchema"];
        if schema["type"] != "object" || !supported_schema(schema, 0) {
            return Err("诊断工具参数结构不受支持".into());
        }
        tools.push(ToolSpec {
            name: name.to_owned(),
            description: description.to_owned(),
            parameters: schema.clone(),
        });
    }
    Ok(tools)
}

fn supported_schema(schema: &Value, depth: usize) -> bool {
    let Some(object) = schema.as_object().filter(|_| depth <= 12) else {
        return false;
    };
    const KEYS: &[&str] = &[
        "type",
        "properties",
        "required",
        "additionalProperties",
        "items",
        "oneOf",
        "anyOf",
        "allOf",
        "enum",
        "const",
        "minimum",
        "maximum",
        "minLength",
        "maxLength",
        "minItems",
        "maxItems",
        "description",
        "title",
        "default",
        "$schema",
    ];
    if object.keys().any(|key| !KEYS.contains(&key.as_str())) {
        return false;
    }
    if let Some(kind) = object.get("type") {
        if !matches!(
            kind.as_str(),
            Some("object" | "array" | "string" | "integer" | "number" | "boolean" | "null")
        ) {
            return false;
        }
    }
    if object.get("required").is_some_and(|value| {
        value
            .as_array()
            .is_none_or(|items| items.len() > 128 || items.iter().any(|item| !item.is_string()))
    }) || object.get("enum").is_some_and(|value| {
        value
            .as_array()
            .is_none_or(|items| items.is_empty() || items.len() > 256)
    }) {
        return false;
    }
    for keyword in ["minimum", "maximum"] {
        if object.get(keyword).is_some_and(|value| !value.is_number()) {
            return false;
        }
    }
    for keyword in ["minLength", "maxLength", "minItems", "maxItems"] {
        if object
            .get(keyword)
            .is_some_and(|value| value.as_u64().is_none())
        {
            return false;
        }
    }
    if let Some(properties) = object.get("properties") {
        let Some(properties) = properties.as_object() else {
            return false;
        };
        if !properties
            .values()
            .all(|schema| supported_schema(schema, depth + 1))
        {
            return false;
        }
    }
    for keyword in ["oneOf", "anyOf", "allOf"] {
        if let Some(choices) = object.get(keyword) {
            let Some(choices) = choices
                .as_array()
                .filter(|choices| !choices.is_empty() && choices.len() <= 16)
            else {
                return false;
            };
            if !choices
                .iter()
                .all(|schema| supported_schema(schema, depth + 1))
            {
                return false;
            }
        }
    }
    for keyword in ["items", "additionalProperties"] {
        if let Some(value) = object.get(keyword) {
            if keyword == "additionalProperties" && value.is_boolean() {
                continue;
            }
            if !supported_schema(value, depth + 1) {
                return false;
            }
        }
    }
    true
}

pub(super) fn validate_arguments(schema: &Value, args: &Value) -> bool {
    validate_value(schema, args, 0)
}

fn validate_value(schema: &Value, value: &Value, depth: usize) -> bool {
    if depth > 16 {
        return false;
    }
    for keyword in ["oneOf", "anyOf", "allOf"] {
        if let Some(choices) = schema[keyword].as_array() {
            let matches = choices
                .iter()
                .filter(|choice| validate_value(choice, value, depth + 1))
                .count();
            if (keyword == "oneOf" && matches != 1)
                || (keyword == "anyOf" && matches == 0)
                || (keyword == "allOf" && matches != choices.len())
            {
                return false;
            }
        }
    }
    if schema
        .get("const")
        .is_some_and(|expected| expected != value)
        || schema["enum"]
            .as_array()
            .is_some_and(|choices| !choices.contains(value))
    {
        return false;
    }
    let typed = match schema["type"].as_str() {
        Some("object") => value.is_object(),
        Some("array") => value.is_array(),
        Some("string") => value.is_string(),
        Some("integer") => value.is_i64() || value.is_u64(),
        Some("number") => value.is_number(),
        Some("boolean") => value.is_boolean(),
        Some("null") => value.is_null(),
        None => true,
        _ => false,
    };
    if !typed {
        return false;
    }
    if let Some(number) = value.as_f64() {
        if schema["minimum"]
            .as_f64()
            .is_some_and(|minimum| number < minimum)
            || schema["maximum"]
                .as_f64()
                .is_some_and(|maximum| number > maximum)
        {
            return false;
        }
    }
    if let Some(text) = value.as_str() {
        let length = text.chars().count() as u64;
        if schema["minLength"].as_u64().is_some_and(|min| length < min)
            || schema["maxLength"].as_u64().is_some_and(|max| length > max)
        {
            return false;
        }
    }
    if let Some(items) = value.as_array() {
        let length = items.len() as u64;
        if schema["minItems"].as_u64().is_some_and(|min| length < min)
            || schema["maxItems"].as_u64().is_some_and(|max| length > max)
        {
            return false;
        }
        if let Some(item_schema) = schema.get("items") {
            if !items
                .iter()
                .all(|item| validate_value(item_schema, item, depth + 1))
            {
                return false;
            }
        }
    }
    if let Some(object) = value.as_object() {
        if schema["required"].as_array().is_some_and(|required| {
            required
                .iter()
                .any(|key| key.as_str().is_none_or(|key| !object.contains_key(key)))
        }) {
            return false;
        }
        for (key, value) in object {
            if let Some(property) = schema["properties"].get(key) {
                if !validate_value(property, value, depth + 1) {
                    return false;
                }
            } else if schema["additionalProperties"] == false {
                return false;
            } else if schema["additionalProperties"].is_object()
                && !validate_value(&schema["additionalProperties"], value, depth + 1)
            {
                return false;
            }
        }
    }
    true
}
