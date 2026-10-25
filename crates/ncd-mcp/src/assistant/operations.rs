//! Bot 启停只接受桌面逐次批准，调用方没有批准选项或可回放凭据。

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ncd_domain::assistant::{
    AssistantPermissionOption, AssistantPermissionRequest, AssistantPermissionSource,
};
use serde_json::{Value, json};

use super::{SessionCtx, random_id};

const APPROVAL_TIMEOUT: Duration = Duration::from_secs(60);
const OPERATION_TIMEOUT: Duration = Duration::from_secs(45);

pub(super) const DEFINITIONS: &[(&str, &str)] = &[
    (
        "start_bot",
        "请求桌面用户批准后启动当前 Bot。没有用户本次批准则不执行。",
    ),
    (
        "stop_bot",
        "请求桌面用户批准后停止当前 Bot，期间机器人服务会中断。",
    ),
    (
        "restart_bot",
        "请求桌面用户批准后重启当前 Bot，期间机器人服务会中断。",
    ),
];

pub(super) fn is_operation(name: &str) -> bool {
    DEFINITIONS.iter().any(|(operation, _)| *operation == name)
}

pub(super) async fn execute(ctx: &SessionCtx, name: &str, args: &Value) -> Result<Value, String> {
    if !is_operation(name)
        || args.as_object().is_none_or(|args| {
            args.len() != 1
                || args.get("bot_id").and_then(Value::as_str) != Some(ctx.bot_id.as_str())
        })
    {
        return Err("Bot 操作只能使用当前会话的固定目标，不能附加配置或命令".to_owned());
    }
    let _operation = tokio::select! {
        biased;
        _ = ctx.server.cancel.cancelled() => return Err("诊断会话已结束，未提交 Bot 操作".to_owned()),
        lock = ctx.operation_lock.lock() => lock,
    };
    if ctx.server.cancel.is_cancelled() {
        return Err("诊断会话已结束，未提交 Bot 操作".to_owned());
    }
    let target = tokio::select! {
        biased;
        _ = ctx.server.cancel.cancelled() => return Err("诊断会话已结束，未提交 Bot 操作".to_owned()),
        target = ctx.inspection.assistant_operation_target(&ctx.bot_id) => target?,
    };
    let verb = match name {
        "start_bot" => "启动",
        "stop_bot" => "停止",
        "restart_bot" => "重启",
        _ => return Err("不支持的 Bot 操作".to_owned()),
    };
    let details = ctx.clean_value(json!({
        "session_id": ctx.session_id,
        "operation": name,
        "arguments": {"bot_id": ctx.bot_id},
        "target": target,
        "effect": if name == "start_bot" {
            "启动该 Bot，使用桌面端保存的配置。"
        } else { "中断该 Bot 的运行；重启还会使用桌面端保存的配置再次启动。" },
    }));
    let request = AssistantPermissionRequest {
        id: format!("ncd-operation-{}", random_id(16)?),
        source: AssistantPermissionSource::NcdOperation,
        title: format!("{verb} Bot {}", ctx.bot_id),
        details: details.to_string(),
        options: vec![
            AssistantPermissionOption {
                id: "allow_once".to_owned(),
                label: format!("本次{verb}"),
                kind: "allow_once".to_owned(),
            },
            AssistantPermissionOption {
                id: "reject_once".to_owned(),
                label: "拒绝".to_owned(),
                kind: "reject_once".to_owned(),
            },
        ],
        created_at_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as f64,
    };
    let decision = tokio::select! {
        biased;
        _ = ctx.server.cancel.cancelled() => return Err("诊断会话已结束，Bot 操作未获批准".to_owned()),
        result = tokio::time::timeout(APPROVAL_TIMEOUT, ctx.interaction.request_permission(request)) => {
            result.map_err(|_| "桌面批准已超时，Bot 操作未执行".to_owned())??
        }
    };
    if decision.as_deref() != Some("allow_once") {
        return Err("桌面用户未批准本次 Bot 操作".to_owned());
    }
    if ctx.server.cancel.is_cancelled() {
        return Err("诊断会话已结束，Bot 操作未提交".to_owned());
    }
    // 接受对话框不代表当前 Desktop 协议与数据迁移门禁仍然有效。
    (ctx.operation_gate)()?;
    let operation = async {
        let current = ctx
            .inspection
            .assistant_operation_target(&ctx.bot_id)
            .await?;
        if current != target {
            return Err("Bot 目标或配置已变化，请重新请求本次操作".to_owned());
        }
        if ctx.server.cancel.is_cancelled() {
            return Err("诊断会话已结束，Bot 操作未提交".to_owned());
        }
        (ctx.operation_gate)()?;
        ctx.inspection.assistant_operate(&ctx.bot_id, name).await
    };
    tokio::select! {
        biased;
        _ = ctx.server.cancel.cancelled() => Err(
            "诊断会话已取消。Bot 操作可能已提交，不能保证撤销；请核实状态，勿自动重试。".to_owned()
        ),
        result = tokio::time::timeout(OPERATION_TIMEOUT, operation) => result
            .map_err(|_| "Bot 操作超时，可能已提交；请核实状态，勿自动重试。".to_owned())?,
    }
}
