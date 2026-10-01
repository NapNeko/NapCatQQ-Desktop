//! MCP 工具面：`tools/list` 的定义（含 inputSchema）和 `tools/call` 的分发。
//! 每个工具直接对应 `DebugManager` 的一组现成方法，失败文案沿用调试台的中文口径。

use std::sync::Arc;

use ncd_domain::onebot_debug::{
    DebugActionSafety, DebugCallOrigin, DebugCallRequest, DebugChannelId, DebugCollections,
    DebugEventBatch, DebugHistoryQuery, DebugWorkspace,
};
use ncd_domain::{BackendType, McpServerStatus};
use ncd_runtime::onebot_debug::{DebugManager, error_text};
use ncd_runtime::DebugEventSink;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::gate::{GateKeeper, ToolSafety};

/// 历史查询的默认页大小，别让 agent 一次把整份历史搬走
const HISTORY_DEFAULT_LIMIT: u32 = 50;
/// 单批事件的上限：接收器环比这小得多时按环的来；这里只是防 agent 填一个天文数字
const EVENTS_LIMIT_CAP: u32 = 2000;

pub(crate) struct ToolsCtx {
    pub debug: Arc<DebugManager>,
    pub gate: Arc<GateKeeper>,
    /// `server_status` 的报告来源（McpServer 的弱引用挂在闭包里）
    pub status: Arc<dyn Fn() -> McpServerStatus + Send + Sync>,
}

/// MCP 订阅的出口：事件靠接收器缓冲 + `read_events` 轮询，sink 不存东西，
/// 返回 true 只是保活——接收器没了这个「观众」，30 分钟空闲就会被清扫停掉
struct PollingSink;

impl DebugEventSink for PollingSink {
    fn send(&self, _batch: &DebugEventBatch) -> bool {
        true
    }
}

pub(crate) struct ToolDef {
    pub name: &'static str,
    pub description: String,
    pub input_schema: Value,
}

pub(crate) fn tool_defs() -> &'static [ToolDef] {
    static DEFS: std::sync::OnceLock<Vec<ToolDef>> = std::sync::OnceLock::new();
    DEFS.get_or_init(build_defs)
}

pub(crate) fn tool_exists(name: &str) -> bool {
    tool_defs().iter().any(|def| def.name == name)
}

/// `服务端拒绝 / 参数不对 / 执行失败` 的统一出口：`message` 给人看，`structured` 随结果带走
pub(crate) struct ToolError {
    pub message: String,
    pub structured: Option<Value>,
}

impl ToolError {
    fn plain(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            structured: None,
        }
    }
}

/// 工具自带的静态分级 + `call_action` 按目录折算的动态分级
pub(crate) async fn effective_safety(ctx: &ToolsCtx, name: &str, args: &Value) -> ToolSafety {
    if name != "call_action" {
        return crate::gate::static_safety(name);
    }
    match action_safety(ctx, args).await {
        Some(DebugActionSafety::ReadOnly) => ToolSafety::ReadOnly,
        Some(DebugActionSafety::Dangerous) => ToolSafety::Dangerous,
        // 目录里查不到的动作按有副作用处理（上游多半也会拒绝它）
        _ => ToolSafety::SideEffect,
    }
}

/// `call_action` 调的动作在目录里的分级。Bot 不存在 / 动作未知都给 None
async fn action_safety(ctx: &ToolsCtx, args: &Value) -> Option<DebugActionSafety> {
    let bot_id = args.get("bot_id").and_then(Value::as_str)?;
    let action = args.get("action").and_then(Value::as_str)?;
    let backend = ctx
        .debug
        .list_targets()
        .await
        .into_iter()
        .find(|t| t.bot_id == bot_id)
        .map(|t| t.backend)?;
    ctx.debug
        .describe(Some(bot_id), backend, action)
        .await
        .map(|spec| spec.safety)
}

pub(crate) async fn dispatch(ctx: &ToolsCtx, name: &str, args: Value) -> Result<Value, ToolError> {
    match name {
        "list_targets" => value(&ctx.debug.list_targets().await),
        "list_channels" => {
            let a: BotArgs = parse(&args)?;
            ctx.debug
                .list_channels(&a.bot_id)
                .await
                .map_err(|e| ToolError::plain(error_text(&e)))
                .and_then(|v| value(&v))
        }
        "test_channel" => {
            let a: BotChannelArgs = parse(&args)?;
            ctx.debug
                .test_channel(&a.bot_id, a.channel.unwrap_or(DebugChannelId::Auto))
                .await
                .map_err(|e| ToolError::plain(error_text(&e)))
                .and_then(|v| value(&v))
        }
        "list_actions" => {
            let a: CatalogArgs = parse(&args)?;
            value(&ctx.debug.catalog(a.bot_id.as_deref(), a.backend).await)
        }
        "describe_action" => {
            let a: DescribeArgs = parse(&args)?;
            value(
                &ctx.debug
                    .describe(a.bot_id.as_deref(), a.backend, &a.action)
                    .await,
            )
        }
        "call_action" => {
            let a: CallActionArgs = parse(&args)?;
            let request = DebugCallRequest {
                request_id: format!("mcp-{}", hex::encode(rand::random::<[u8; 8]>())),
                bot_id: a.bot_id,
                channel: a.channel.unwrap_or(DebugChannelId::Auto),
                action: a.action,
                params: a.params,
                timeout_ms: a.timeout_ms,
                origin: DebugCallOrigin::Mcp,
            };
            value(&ctx.debug.call(request).await)
        }
        "cancel_call" => {
            let a: RequestIdArgs = parse(&args)?;
            ctx.debug.cancel(&a.request_id);
            Ok(json!({"cancelled": a.request_id}))
        }
        "subscribe_events" => {
            let a: SubscribeArgs = parse(&args)?;
            ctx.debug
                .subscribe(
                    &a.bot_id,
                    a.source.unwrap_or(DebugChannelId::Auto),
                    Arc::new(PollingSink),
                )
                .await
                .map_err(|e| ToolError::plain(error_text(&e)))
                .and_then(|v| value(&v))
        }
        "unsubscribe_events" => {
            let a: SubscriptionArgs = parse(&args)?;
            ctx.debug.unsubscribe(&a.subscription_id).await;
            Ok(json!({"unsubscribed": a.subscription_id}))
        }
        "read_events" => {
            let a: ReadEventsArgs = parse(&args)?;
            value(
                &ctx.debug
                    .read_events(&a.bot_id, a.since_seq, a.limit.unwrap_or(200).min(EVENTS_LIMIT_CAP))
                    .await,
            )
        }
        "list_receivers" => value(&ctx.debug.receivers().await),
        "stop_receiver" => {
            let a: BotArgs = parse(&args)?;
            ctx.debug.stop_receiver(&a.bot_id).await;
            Ok(json!({"stopped": a.bot_id}))
        }
        "get_workspace" => value(&ctx.debug.workspace().await),
        "save_workspace" => {
            let workspace: DebugWorkspace = parse(&args)?;
            ctx.debug
                .save_workspace(workspace)
                .await
                .map_err(ToolError::plain)
                .map(|()| json!({"saved": true}))
        }
        "get_collections" => value(&ctx.debug.collections().await),
        "save_collections" => {
            let collections: DebugCollections = parse(&args)?;
            ctx.debug
                .save_collections(collections)
                .await
                .map_err(ToolError::plain)
                .map(|()| json!({"saved": true}))
        }
        "query_history" => {
            let mut a: HistoryArgs = parse(&args)?;
            if a.limit == 0 {
                a.limit = HISTORY_DEFAULT_LIMIT;
            }
            value(&ctx.debug.history(a.into_query()).await)
        }
        "get_history_entry" => {
            let a: HistoryEntryArgs = parse(&args)?;
            value(&ctx.debug.history_entry(&a.id).await)
        }
        "clear_history" => ctx
            .debug
            .clear_history()
            .await
            .map_err(ToolError::plain)
            .map(|()| json!({"cleared": true})),
        "set_enabled" => {
            let a: EnabledArgs = parse(&args)?;
            ctx.debug.set_enabled(a.enabled).await;
            Ok(json!({"enabled": a.enabled}))
        }
        "close_all" => {
            ctx.debug.close_all().await;
            Ok(json!({"closed": true}))
        }
        "server_status" => value(&(ctx.status)()),
        _ => Err(ToolError::plain(format!("未知工具：{name}"))),
    }
}

/// 结果统一走 Value（工具结果就是 JSON）。序列化失败只剩「类型本身不可序列化」，不该发生
fn value<T: serde::Serialize>(v: &T) -> Result<Value, ToolError> {
    serde_json::to_value(v).map_err(|e| ToolError::plain(format!("结果序列化失败：{e}")))
}

fn parse<T: serde::de::DeserializeOwned>(args: &Value) -> Result<T, ToolError> {
    serde_json::from_value(args.clone())
        .map_err(|e| ToolError::plain(format!("参数不对：{e}")))
}

fn empty_object() -> Value {
    json!({})
}

#[derive(Deserialize)]
struct BotArgs {
    bot_id: String,
}

#[derive(Deserialize)]
struct BotChannelArgs {
    bot_id: String,
    #[serde(default)]
    channel: Option<DebugChannelId>,
}

#[derive(Deserialize)]
struct CatalogArgs {
    #[serde(default)]
    bot_id: Option<String>,
    backend: BackendType,
}

#[derive(Deserialize)]
struct DescribeArgs {
    #[serde(default)]
    bot_id: Option<String>,
    backend: BackendType,
    action: String,
}

#[derive(Deserialize)]
struct CallActionArgs {
    bot_id: String,
    action: String,
    #[serde(default)]
    channel: Option<DebugChannelId>,
    #[serde(default = "empty_object")]
    params: Value,
    #[serde(default)]
    timeout_ms: Option<u32>,
    /// 权限闸用；dispatch 用不到，留着只为让它通过反序列化
    #[serde(default)]
    #[allow(dead_code)]
    confirm_token: Option<String>,
}

#[derive(Deserialize)]
struct RequestIdArgs {
    request_id: String,
}

#[derive(Deserialize)]
struct SubscribeArgs {
    bot_id: String,
    #[serde(default)]
    source: Option<DebugChannelId>,
}

#[derive(Deserialize)]
struct SubscriptionArgs {
    subscription_id: String,
}

#[derive(Deserialize)]
struct ReadEventsArgs {
    bot_id: String,
    #[serde(default)]
    since_seq: u64,
    #[serde(default)]
    limit: Option<u32>,
}

#[derive(Deserialize)]
struct HistoryArgs {
    #[serde(default)]
    action: Option<String>,
    #[serde(default)]
    bot_id: Option<String>,
    #[serde(default)]
    ok: Option<bool>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    limit: u32,
    #[serde(default)]
    offset: u32,
}

impl HistoryArgs {
    fn into_query(self) -> DebugHistoryQuery {
        DebugHistoryQuery {
            action: self.action,
            bot_id: self.bot_id,
            ok: self.ok,
            text: self.text,
            limit: self.limit,
            offset: self.offset,
        }
    }
}

#[derive(Deserialize)]
struct HistoryEntryArgs {
    id: String,
}

#[derive(Deserialize)]
struct EnabledArgs {
    enabled: bool,
}

const CONFIRM_HINT: &str = "（有副作用：首次不带 confirm_token 调用只发确认令牌，带牌重放同一调用才执行）";

fn channel_schema() -> Value {
    json!({
        "oneOf": [
            { "type": "object", "properties": { "kind": { "const": "auto" } }, "required": ["kind"], "description": "由桌面端按可用性自动挑" },
            { "type": "object", "properties": { "kind": { "const": "internal" } }, "required": ["kind"], "description": "上游 WebUI 自带的调试接口" },
            { "type": "object", "properties": { "kind": { "const": "http" }, "name": { "type": "string" } }, "required": ["kind", "name"], "description": "用户配置的 HTTP 服务端" },
            { "type": "object", "properties": { "kind": { "const": "ws" }, "name": { "type": "string" } }, "required": ["kind", "name"], "description": "用户配置的 WebSocket 服务端" }
        ]
    })
}

fn object_schema(properties: Value, required: &[&str]) -> Value {
    json!({
        "type": "object",
        "properties": properties,
        "required": required,
    })
}

fn build_defs() -> Vec<ToolDef> {
    let bot = || json!({"bot_id": {"type": "string", "description": "`list_targets` 返回的 bot_id"}});
    let confirm = || {
        json!({"confirm_token": {"type": "string", "description": "确认令牌：有副作用的调用首次会拿到一张，带它重放同一调用才执行"}})
    };
    let backend_prop = || {
        json!({"backend": {"type": "string", "enum": ["napcat", "snowluma"], "description": "后端目录。Bot 在跑时用它的；不给 bot_id 则查随包快照"}})
    };
    vec![
        ToolDef {
            name: "list_targets",
            description: "列出桌面端管着的 Bot：运行状态、登录态、后端、所在主机（本机 / 远端 / Docker）".to_owned(),
            input_schema: object_schema(json!({}), &[]),
        },
        ToolDef {
            name: "list_channels",
            description: "列出一个 Bot 的调用 / 收事件通道及可用状态；「自动」当前会落到哪条也在里面".to_owned(),
            input_schema: object_schema(bot(), &["bot_id"]),
        },
        ToolDef {
            name: "test_channel",
            description: format!("测一次通道连通并返回最新状态{CONFIRM_HINT}"),
            input_schema: object_schema(
                json!({"bot_id": bot()["bot_id"], "channel": channel_schema(), "confirm_token": confirm()["confirm_token"]}),
                &["bot_id"],
            ),
        },
        ToolDef {
            name: "list_actions",
            description: "列出某后端的 OneBot 动作目录（名称、一句话说明、副作用分级）；Bot 在跑时取现取目录，否则取随包快照".to_owned(),
            input_schema: object_schema(
                json!({"bot_id": bot()["bot_id"], "backend": backend_prop()["backend"]}),
                &["backend"],
            ),
        },
        ToolDef {
            name: "describe_action",
            description: "一个动作的完整说明：副作用分级（safety）、参数 JSON Schema（params_schema）、文档与示例。调 call_action 前先看它".to_owned(),
            input_schema: object_schema(
                json!({
                    "bot_id": bot()["bot_id"],
                    "backend": backend_prop()["backend"],
                    "action": {"type": "string", "description": "动作名，如 send_group_msg"},
                }),
                &["backend", "action"],
            ),
        },
        ToolDef {
            name: "call_action",
            description: format!("经调试台发一个 OneBot 动作：按动作的副作用分级自动放行 / 要确认 / 拒绝{CONFIRM_HINT}"),
            input_schema: object_schema(
                json!({
                    "bot_id": bot()["bot_id"],
                    "action": {"type": "string"},
                    "channel": channel_schema(),
                    "params": {"type": "object", "description": "动作参数，形状见 describe_action 的 params_schema"},
                    "timeout_ms": {"type": "integer", "description": "超时（毫秒），默认 60000"},
                    "confirm_token": confirm()["confirm_token"],
                }),
                &["bot_id", "action"],
            ),
        },
        ToolDef {
            name: "cancel_call",
            description: "取消一次在途的 call_action（参数是它返回里的 request_id；上游可能已经执行了，这里只是不再等）".to_owned(),
            input_schema: object_schema(
                json!({"request_id": {"type": "string"}}),
                &["request_id"],
            ),
        },
        ToolDef {
            name: "subscribe_events",
            description: format!("开始接收一个 Bot 的事件流（返回 subscription_id 和接收器状态）。事件经 read_events 按游标轮询，不是推送{CONFIRM_HINT}"),
            input_schema: object_schema(
                json!({"bot_id": bot()["bot_id"], "source": channel_schema(), "confirm_token": confirm()["confirm_token"]}),
                &["bot_id"],
            ),
        },
        ToolDef {
            name: "unsubscribe_events",
            description: "退订事件流（subscribe_events 返回的 subscription_id）。接收器没人看满 30 分钟会被自动停掉".to_owned(),
            input_schema: object_schema(
                json!({"subscription_id": {"type": "string"}}),
                &["subscription_id"],
            ),
        },
        ToolDef {
            name: "read_events",
            description: "按游标读事件缓冲：返回 seq > since_seq 的事件（从老到新）。首批用 since_seq = 0".to_owned(),
            input_schema: object_schema(
                json!({
                    "bot_id": bot()["bot_id"],
                    "since_seq": {"type": "integer"},
                    "limit": {"type": "integer", "description": "最多几条，默认 200"},
                }),
                &["bot_id"],
            ),
        },
        ToolDef {
            name: "list_receivers",
            description: "列出所有在跑的事件接收器：来源、连接状态、缓冲量、断流丢下数".to_owned(),
            input_schema: object_schema(json!({}), &[]),
        },
        ToolDef {
            name: "stop_receiver",
            description: format!("手动停掉一个 Bot 的事件接收器（缓冲随之丢掉，编号不回退）{CONFIRM_HINT}"),
            input_schema: object_schema(
                json!({"bot_id": bot()["bot_id"], "confirm_token": confirm()["confirm_token"]}),
                &["bot_id"],
            ),
        },
        ToolDef {
            name: "get_workspace",
            description: "读调试台工作区：打开的编辑标签、选中的 Bot、通道选择、布局".to_owned(),
            input_schema: object_schema(json!({}), &[]),
        },
        ToolDef {
            name: "save_workspace",
            description: format!("整份覆盖调试台工作区（结构与 get_workspace 返回的相同；会顶掉界面上的编辑状态）{CONFIRM_HINT}"),
            input_schema: json!({"type": "object"}),
        },
        ToolDef {
            name: "get_collections",
            description: "读收藏夹：文件夹和保存的请求".to_owned(),
            input_schema: object_schema(json!({}), &[]),
        },
        ToolDef {
            name: "save_collections",
            description: format!("整份覆盖收藏夹（结构与 get_collections 返回的相同）{CONFIRM_HINT}"),
            input_schema: json!({"type": "object"}),
        },
        ToolDef {
            name: "query_history",
            description: "查调用历史（按动作 / Bot / 成败 / 文本过滤，分页）".to_owned(),
            input_schema: object_schema(
                json!({
                    "action": {"type": "string"},
                    "bot_id": {"type": "string"},
                    "ok": {"type": "boolean"},
                    "text": {"type": "string"},
                    "limit": {"type": "integer"},
                    "offset": {"type": "integer"},
                }),
                &[],
            ),
        },
        ToolDef {
            name: "get_history_entry",
            description: "取一条历史的完整内容（参数和回包）".to_owned(),
            input_schema: object_schema(json!({"id": {"type": "string"}}), &["id"]),
        },
        ToolDef {
            name: "clear_history",
            description: "清空全部调用历史（危险级，默认拒绝执行）".to_owned(),
            input_schema: object_schema(
                json!({"confirm_token": confirm()["confirm_token"]}),
                &[],
            ),
        },
        ToolDef {
            name: "set_enabled",
            description: format!("开关整个 OneBot 调试台（关掉会停掉所有接收器、会话和在途调用，界面和 MCP 都不能再调）{CONFIRM_HINT}"),
            input_schema: object_schema(
                json!({"enabled": {"type": "boolean"}, "confirm_token": confirm()["confirm_token"]}),
                &["enabled"],
            ),
        },
        ToolDef {
            name: "close_all",
            description: format!("停掉所有接收器和会话、取消在途调用（调试台保持开，能重新再来）{CONFIRM_HINT}"),
            input_schema: object_schema(
                json!({"confirm_token": confirm()["confirm_token"]}),
                &[],
            ),
        },
        ToolDef {
            name: "server_status",
            description: "MCP 服务自身状态：开着没、在听哪个端口、调试台功能开关的状态".to_owned(),
            input_schema: object_schema(json!({}), &[]),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests_support::test_debug_manager;

    fn test_ctx(root: &std::path::Path) -> ToolsCtx {
        ToolsCtx {
            debug: test_debug_manager(root.to_path_buf()),
            gate: Arc::new(GateKeeper::new()),
            status: Arc::new(|| McpServerStatus {
                enabled: false,
                listening: false,
                port: None,
                last_error: None,
            }),
        }
    }

    #[test]
    fn tool_list_is_complete_and_every_tool_has_schema() {
        let names: Vec<&str> = tool_defs().iter().map(|d| d.name).collect();
        let expected = [
            "list_targets",
            "list_channels",
            "test_channel",
            "list_actions",
            "describe_action",
            "call_action",
            "cancel_call",
            "subscribe_events",
            "unsubscribe_events",
            "read_events",
            "list_receivers",
            "stop_receiver",
            "get_workspace",
            "save_workspace",
            "get_collections",
            "save_collections",
            "query_history",
            "get_history_entry",
            "clear_history",
            "set_enabled",
            "close_all",
            "server_status",
        ];
        assert_eq!(names, expected);
        for def in tool_defs() {
            assert_eq!(def.input_schema["type"], "object", "{}", def.name);
            assert!(!def.description.is_empty(), "{}", def.name);
        }
    }

    #[tokio::test]
    async fn list_targets_and_unknown_bot_channel() {
        let root = tempfile::tempdir().unwrap();
        let ctx = test_ctx(root.path());
        let targets = dispatch(&ctx, "list_targets", json!({})).await.unwrap();
        assert_eq!(targets, json!([]));

        let error = dispatch(&ctx, "list_channels", json!({"bot_id": "404"}))
            .await
            .map(|_| ())
            .unwrap_err();
        assert!(error.message.contains("Bot"), "{}", error.message);

        let error = dispatch(&ctx, "list_channels", json!({}))
            .await
            .map(|_| ())
            .unwrap_err();
        assert!(error.message.contains("参数不对"), "{}", error.message);
    }

    #[tokio::test]
    async fn call_action_reaches_debug_manager() {
        let root = tempfile::tempdir().unwrap();
        let ctx = test_ctx(root.path());
        // Bot 不存在：错误随 DebugCallResponse.result 走（数据），不是工具层面的失败
        let value = dispatch(
            &ctx,
            "call_action",
            json!({"bot_id": "404", "action": "get_status"}),
        )
        .await
        .unwrap();
        assert_eq!(
            value["request_id"]
                .as_str()
                .map(|s| s.starts_with("mcp-")),
            Some(true)
        );
        assert_eq!(value["result"]["kind"], "err");
        assert_eq!(value["result"]["error"]["kind"], "bot_not_found");
    }

    #[tokio::test]
    async fn action_safety_unknown_bot_or_action_defaults_to_side_effect() {
        let root = tempfile::tempdir().unwrap();
        let ctx = test_ctx(root.path());
        let safety = effective_safety(&ctx, "call_action", &json!({"bot_id": "404", "action": "get_status"})).await;
        assert_eq!(safety, ToolSafety::SideEffect);
        // 静态表与动态分级互不干扰
        let safety = effective_safety(&ctx, "clear_history", &json!({})).await;
        assert_eq!(safety, ToolSafety::Dangerous);
    }

    #[tokio::test]
    async fn server_status_uses_status_source() {
        let root = tempfile::tempdir().unwrap();
        let ctx = test_ctx(root.path());
        let value = dispatch(&ctx, "server_status", json!({})).await.unwrap();
        assert_eq!(value["enabled"], false);
        assert_eq!(value["listening"], false);
    }
}