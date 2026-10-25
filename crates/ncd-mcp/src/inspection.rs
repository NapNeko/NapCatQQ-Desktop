//! MCP 运维查询：只投影公开状态，不读取凭据、连接主机或修改配置。

use std::sync::Arc;

use ncd_domain::app_framework::AppInstance;
use ncd_domain::deployment_task::{DeploymentTaskKind, DeploymentTaskSnapshot};
use ncd_domain::{BotActorSnapshot, BotConfig, BotId};
use ncd_runtime::{
    AppManager, BotManager, ComponentExecutor, LocalBotConfigRepo, LocalConfigStore, ServerManager,
    ServerProfile, sanitize_log_line,
};
use serde::Deserialize;
use serde_json::{Value, json};

type InspectionBotManager = BotManager<LocalBotConfigRepo<LocalConfigStore>, LocalConfigStore>;

const PAGE_DEFAULT: u32 = 50;
const PAGE_MAX: u32 = 200;
const LOG_DEFAULT: u32 = 50;
const LOG_MAX: u32 = 200;
const LOG_LINE_BYTES: usize = 2048;
const LOG_TOTAL_BYTES: usize = 64 * 1024;
const SUMMARY_TEXT_BYTES: usize = 256;

pub struct McpInspection {
    bots: Arc<InspectionBotManager>,
    apps: Arc<AppManager>,
    servers: Arc<ServerManager>,
    components: Arc<ComponentExecutor>,
}

impl McpInspection {
    pub fn new(
        bots: Arc<InspectionBotManager>,
        apps: Arc<AppManager>,
        servers: Arc<ServerManager>,
        components: Arc<ComponentExecutor>,
    ) -> Self {
        Self {
            bots,
            apps,
            servers,
            components,
        }
    }

    pub async fn dispatch(&self, name: &str, args: &Value) -> Result<Value, String> {
        match name {
            "get_bot_runtime" => {
                let args: BotArgs = parse(args)?;
                let bot_id = checked_bot_id(&args.bot_id)?;
                let config = self.bot_config(&bot_id).await?;
                let snapshot = self.bots.get_snapshot(&bot_id).await.ok();
                Ok(json!({
                    "bot_id": bot_id.as_str(),
                    "config": bot_summary(&config),
                    "runtime": snapshot.as_ref().map(runtime_summary),
                }))
            }
            "tail_bot_logs" => {
                let args: LogArgs = parse(args)?;
                let bot_id = checked_bot_id(&args.bot_id)?;
                let limit = checked_limit(args.limit, LOG_DEFAULT, LOG_MAX)?;
                let config = self.bot_config(&bot_id).await?;
                let snapshot = self
                    .bots
                    .tail_cached_log(&bot_id, limit)
                    .await
                    .map_err(|_| "读取 Bot 本机日志缓存失败".to_owned())?;
                let Some(snapshot) = snapshot else {
                    return Ok(json!({
                        "bot_id": bot_id.as_str(), "available": false,
                        "source": "unavailable", "lines": [], "total_lines": null,
                        "truncated": false,
                        "reason": "没有可直接读取的本机日志缓存；此工具不会连接远端主机或初始化运行时",
                    }));
                };
                let (lines, truncated) = bounded_logs(&snapshot.lines, limit, &config);
                Ok(json!({
                    "bot_id": bot_id.as_str(), "available": true,
                    "source": "local_runtime_cache", "lines": lines,
                    "scope": if config.bot.backend_type == ncd_domain::BackendType::SnowLuma {
                        "shared_daemon"
                    } else { "bot" },
                    "total_lines": snapshot.total_lines,
                    "truncated": truncated || snapshot.total_lines > snapshot.lines.len(),
                }))
            }
            "list_app_instances" => {
                let args = ListArgs::parse(args)?;
                let mut instances = self.apps.list_instances().await;
                instances.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));
                Ok(page(&instances, &args, app_summary))
            }
            "get_app_runtime" => {
                let args: AppArgs = parse(args)?;
                checked_identifier(&args.instance_id, "instance_id")?;
                let instance = self
                    .apps
                    .list_instances()
                    .await
                    .into_iter()
                    .find(|instance| instance.id.as_str() == args.instance_id)
                    .ok_or_else(|| "应用端实例不存在".to_owned())?;
                Ok(app_summary(&instance))
            }
            "list_servers" => {
                let args = ListArgs::parse(args)?;
                let mut servers = self.servers.list_servers_read_only().await;
                servers.sort_by(|a, b| a.id.cmp(&b.id));
                Ok(page(&servers, &args, server_summary))
            }
            "list_component_tasks" => {
                let args = ListArgs::parse(args)?;
                let mut tasks = self.components.deployment_tasks().list().await.tasks;
                tasks.sort_by(|a, b| {
                    b.submitted_at_ms
                        .cmp(&a.submitted_at_ms)
                        .then_with(|| a.task_id.cmp(&b.task_id))
                });
                Ok(page(&tasks, &args, task_summary))
            }
            _ => Err("未知运维查询工具".to_owned()),
        }
    }

    async fn bot_config(&self, bot_id: &BotId) -> Result<BotConfig, String> {
        self.bots
            .get_cached_bot_config(bot_id)
            .await
            .map_err(|_| "读取 Bot 配置概要失败".to_owned())?
            .ok_or_else(|| "Bot 不存在".to_owned())
    }
}

fn bot_summary(config: &BotConfig) -> Value {
    let connect = &config.connect;
    json!({
        "name": summary_text(&config.bot.name),
        "backend": config.bot.backend_type,
        "deployment": config.bot.deployment_type,
        "host_id": config.bot.runtime_target.server_id()
            .map(|id| format!("remote:{}", summary_text(id)))
            .unwrap_or_else(|| "local".to_owned()),
        "auto_start": config.advanced.auto_start,
        "offline_auto_restart": config.bot.offline_auto_restart,
        "file_log": config.advanced.file_log,
        "console_log": config.advanced.console_log,
        "enabled_connections": {
            "http_servers": connect.http_servers.iter().filter(|c| c.base.enable).count(),
            "http_sse_servers": connect.http_sse_servers.iter().filter(|c| c.base.enable).count(),
            "http_clients": connect.http_clients.iter().filter(|c| c.base.enable).count(),
            "websocket_servers": connect.websocket_servers.iter().filter(|c| c.base.enable).count(),
            "websocket_clients": connect.websocket_clients.iter().filter(|c| c.base.enable).count(),
        },
    })
}

fn runtime_summary(snapshot: &BotActorSnapshot) -> Value {
    json!({
        "state": snapshot.state,
        "revision": snapshot.revision,
        "pending_restart": snapshot.pending_restart,
        "has_error": snapshot.last_error.is_some(),
    })
}

fn app_summary(instance: &AppInstance) -> Value {
    json!({
        "instance_id": summary_text(instance.id.as_str()),
        "framework_id": summary_text(instance.framework_id.as_str()),
        "display_name": summary_text(&instance.display_name),
        "host_id": summary_text(&instance.host_id),
        "placement": instance.placement,
        "state": instance.state,
        "port": instance.port,
        "installed_version": instance.installed_version.as_deref().map(summary_text),
        "auto_start": instance.auto_start,
        "origin": instance.origin,
        "has_error": instance.last_error.is_some(),
        "link": instance.link.as_ref().map(|link| json!({
            "bot_id": summary_text(link.bot_id.as_str()),
            "mode": link.mode,
            "linked_at_ms": link.linked_at_ms,
        })),
    })
}

fn server_summary(server: &ServerProfile) -> Value {
    json!({
        "server_id": summary_text(&server.id),
        "name": summary_text(&server.name),
        "host": summary_text(&server.host),
        "port": server.port,
        "state": server.state,
        "platform": server.platform.as_deref().map(summary_text),
        "health": server.health.as_ref().map(|health| json!({
            "consecutive_failures": health.consecutive_failures,
            "has_failure": health.last_failure_reason.is_some(),
        })),
    })
}

fn task_summary(task: &DeploymentTaskSnapshot) -> Value {
    // resources / progress_events / message / error 中可能带路径、命令和口令。
    let kind = match &task.kind {
        DeploymentTaskKind::ComponentAction {
            component_id,
            action,
        } => json!({
            "kind": "component_action", "component_id": summary_text(component_id),
            "action": summary_text(action),
        }),
        DeploymentTaskKind::SystemPackage { package_group } => json!({
            "kind": "system_package", "package_group": summary_text(package_group),
        }),
        DeploymentTaskKind::DockerInstall => json!({"kind": "docker_install"}),
        DeploymentTaskKind::DockerImagePull { flavor } => json!({
            "kind": "docker_image_pull", "flavor": flavor,
        }),
        DeploymentTaskKind::AppPlugin {
            instance_id,
            action,
            resource,
            ..
        } => json!({
            "kind": "app_plugin", "instance_id": summary_text(instance_id),
            "action": action, "resource": resource,
        }),
    };
    json!({
        "task_id": summary_text(&task.task_id),
        "kind": kind,
        "host_id": summary_text(&task.host_id),
        "status": task.status,
        "submitted_at_ms": task.submitted_at_ms,
        "started_at_ms": task.started_at_ms,
        "ended_at_ms": task.ended_at_ms,
        "cancellable": task.cancellable,
        "has_error": task.error.is_some(),
    })
}

fn page<T>(items: &[T], args: &ListArgs, project: impl Fn(&T) -> Value) -> Value {
    let offset = args.offset as usize;
    let limit = args.limit.unwrap_or(PAGE_DEFAULT) as usize;
    let selected: Vec<Value> = items.iter().skip(offset).take(limit).map(project).collect();
    let next_offset = offset.saturating_add(selected.len());
    json!({
        "items": selected, "total": items.len(), "offset": offset, "limit": limit,
        "next_offset": (next_offset < items.len()).then_some(next_offset),
    })
}

fn summary_text(text: &str) -> String {
    truncate_utf8(&sanitize_log_line(text), SUMMARY_TEXT_BYTES)
        .0
        .to_owned()
}

fn truncate_utf8(text: &str, limit: usize) -> (&str, bool) {
    let mut end = limit.min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    (&text[..end], end < text.len())
}

fn bounded_logs(input: &[String], limit: usize, config: &BotConfig) -> (Vec<String>, bool) {
    let mut tokens: Vec<&str> = config
        .connect
        .http_servers
        .iter()
        .map(|c| c.base.token.as_str())
        .chain(
            config
                .connect
                .http_sse_servers
                .iter()
                .map(|c| c.base.token.as_str()),
        )
        .chain(
            config
                .connect
                .http_clients
                .iter()
                .map(|c| c.base.token.as_str()),
        )
        .chain(
            config
                .connect
                .websocket_servers
                .iter()
                .map(|c| c.base.token.as_str()),
        )
        .chain(
            config
                .connect
                .websocket_clients
                .iter()
                .map(|c| c.base.token.as_str()),
        )
        .filter(|token| !token.is_empty())
        .collect();
    tokens.sort_unstable_by_key(|token| std::cmp::Reverse(token.len()));
    tokens.dedup();
    let mut lines = Vec::new();
    let mut bytes_left = LOG_TOTAL_BYTES;
    let mut truncated = input.len() > limit;
    for input_line in input.iter().rev().take(limit) {
        let mut line = sanitize_log_line(input_line);
        for token in &tokens {
            line = line.replace(token, "[REDACTED]");
        }
        if bytes_left == 0 {
            truncated = true;
            break;
        }
        let (line, shortened) = truncate_utf8(&line, LOG_LINE_BYTES.min(bytes_left));
        truncated |= shortened;
        bytes_left -= line.len();
        lines.push(line.to_owned());
    }
    lines.reverse();
    (lines, truncated)
}

fn parse<T: serde::de::DeserializeOwned>(args: &Value) -> Result<T, String> {
    serde_json::from_value(args.clone())
        .map_err(|_| "参数不对：字段或类型不符合工具要求".to_owned())
}

fn checked_identifier(value: &str, field: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > 128 || value.chars().any(char::is_control) {
        return Err(format!("参数不对：{field} 必须是 1 至 128 字节的标识符"));
    }
    Ok(())
}

fn checked_bot_id(value: &str) -> Result<BotId, String> {
    if value.is_empty()
        || value.len() > 20
        || !value.bytes().all(|b| b.is_ascii_digit())
        || value.parse::<u64>().ok().is_none_or(|id| id == 0)
    {
        return Err("参数不对：bot_id 必须是有效的非零 QQ 号字符串".to_owned());
    }
    Ok(BotId::new(value))
}

fn checked_limit(limit: Option<u32>, default: u32, max: u32) -> Result<usize, String> {
    let limit = limit.unwrap_or(default);
    if limit == 0 || limit > max {
        return Err(format!("参数不对：limit 必须介于 1 和 {max} 之间"));
    }
    Ok(limit as usize)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BotArgs {
    bot_id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AppArgs {
    instance_id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LogArgs {
    bot_id: String,
    #[serde(default)]
    limit: Option<u32>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ListArgs {
    #[serde(default)]
    limit: Option<u32>,
    #[serde(default)]
    offset: u32,
}

impl ListArgs {
    fn parse(value: &Value) -> Result<Self, String> {
        let args: Self = parse(value)?;
        checked_limit(args.limit, PAGE_DEFAULT, PAGE_MAX)?;
        Ok(args)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bot_config() -> BotConfig {
        serde_json::from_value(json!({
            "bot": {"name": "test", "QQID": 10001, "musicSignUrl": "https://secret.invalid/private"},
            "connect": {"httpServers": [{"name": "http", "host": "127.0.0.1", "port": 3000, "token": "connection-secret"}]},
            "advanced": {},
        })).unwrap()
    }

    #[test]
    fn bot_projection_never_exports_connections_or_private_configuration() {
        let value = bot_summary(&bot_config());
        assert_eq!(value["enabled_connections"]["http_servers"], 1);
        let text = value.to_string();
        for private in [
            "connection-secret",
            "secret.invalid",
            "127.0.0.1",
            "musicSignUrl",
            "token",
        ] {
            assert!(!text.contains(private), "leaked {private}: {text}");
        }
    }

    #[test]
    fn server_projection_excludes_credentials_and_error_details() {
        let server: ServerProfile = serde_json::from_value(json!({
            "id": "s1", "name": "server", "host": "example.com", "username": "private-user",
            "privateKeyPath": "C:/secret/id_rsa", "webuiUrl": "http://example.com/?token=private-token",
            "health": {"lastFailureReason": "password=private-password"},
        })).unwrap();
        let value = server_summary(&server);
        assert_eq!(value["server_id"], "s1");
        assert!(!value.to_string().contains("private"));
        assert!(!value.to_string().contains("id_rsa"));
    }

    #[test]
    fn app_projection_excludes_install_path_and_unstructured_error() {
        let instance: AppInstance = serde_json::from_value(json!({
            "id": "a1", "framework_id": "karin", "display_name": "app",
            "placement": "local_native", "host_id": "local",
            "install_dir": "C:/private/install", "port": 9000, "state": "stopped",
            "last_error": "password=private-password",
            "link": {"bot_id": "10001", "mode": "reverse_ws", "connection_name": "private-name", "linked_at_ms": 100},
        })).unwrap();
        let value = app_summary(&instance);
        assert_eq!(value["link"]["bot_id"], "10001");
        assert_eq!(value["has_error"], true);
        assert!(!value.to_string().contains("private"));
    }

    #[test]
    fn task_projection_excludes_log_commands_resources_and_errors() {
        let task: DeploymentTaskSnapshot = serde_json::from_value(json!({
            "taskId": "t1", "kind": {"kind": "docker_install"}, "status": "failed",
            "hostId": "local", "title": "private command", "submittedAtMs": 100,
            "message": "private-message", "error": "private-error", "dedupeKey": "private-key",
            "resources": [{"kind": "install_target", "host_id": "local", "target": "C:/private/install"}],
        })).unwrap();
        let value = task_summary(&task);
        assert_eq!(value["status"], "failed");
        assert_eq!(value["has_error"], true);
        assert!(!value.to_string().contains("private"));
    }

    #[test]
    fn logs_remove_known_tokens_and_preserve_utf8_under_budgets() {
        let input =
            vec![format!("\u{1b}[31mconnection-secret\u{1b}[0m {}", "中".repeat(3000)); 200];
        let (lines, truncated) = bounded_logs(&input, 200, &bot_config());
        assert!(truncated);
        assert!(lines.iter().all(|line| line.len() <= LOG_LINE_BYTES));
        assert!(lines.iter().map(String::len).sum::<usize>() <= LOG_TOTAL_BYTES);
        assert!(
            lines
                .iter()
                .all(|line| !line.contains("connection-secret") && !line.contains('\u{1b}'))
        );
    }

    #[test]
    fn queries_reject_unknown_fields_invalid_ids_and_unbounded_limits() {
        assert!(ListArgs::parse(&json!({"limit": 0})).is_err());
        assert!(ListArgs::parse(&json!({"limit": 201})).is_err());
        assert!(ListArgs::parse(&json!({"path": "C:/private"})).is_err());
        assert!(parse::<BotArgs>(&json!({"bot_id": "1", "config": {}})).is_err());
        for id in ["", "0", "../1", "18446744073709551616"] {
            assert!(checked_bot_id(id).is_err());
        }
        assert!(checked_bot_id("10001").is_ok());
    }

    #[test]
    fn pages_are_bounded_and_supply_a_continuation_offset() {
        let args = ListArgs::parse(&json!({"limit": 2, "offset": 1})).unwrap();
        let value = page(&[1, 2, 3, 4], &args, |item| json!(item));
        assert_eq!(value["items"], json!([2, 3]));
        assert_eq!(value["next_offset"], 3);
    }
}

impl McpInspection {
    pub(crate) async fn assistant_context(
        &self,
        bot_id: &str,
    ) -> Result<(String, ncd_domain::BackendType, Value), String> {
        let bot_id = checked_bot_id(bot_id)?;
        let config = self.bot_config(&bot_id).await?;
        let snapshot = self.bots.get_snapshot(&bot_id).await.ok();
        let mut linked = self
            .apps
            .list_instances()
            .await
            .into_iter()
            .filter(|app| app.link.as_ref().is_some_and(|link| link.bot_id == bot_id))
            .collect::<Vec<_>>();
        linked.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));
        let linked_count = linked.len();
        let linked: Vec<_> = linked
            .iter()
            .take(32)
            .map(|app| {
                json!({
                    "instance_id": summary_text(app.id.as_str()),
                    "framework_id": summary_text(app.framework_id.as_str()),
                    "name": summary_text(&app.display_name),
                    "state": app.state,
                    "has_error": app.last_error.is_some(),
                })
            })
            .collect();
        let summary = json!({
            "bot_id": bot_id.as_str(),
            "bot": bot_summary(&config),
            "runtime": snapshot.as_ref().map(runtime_summary),
            "linked_apps": linked,
            "linked_apps_truncated": linked_count > 32,
            "logs": "只读取当前 Bot 的本机缓存，不主动连接远端主机。",
        });
        Ok((
            summary_text(&config.bot.name),
            config.bot.backend_type,
            summary,
        ))
    }

    pub(crate) async fn assistant_operation_target(&self, bot_id: &str) -> Result<Value, String> {
        let bot_id = checked_bot_id(bot_id)?;
        let config = self.bot_config(&bot_id).await?;
        Ok(json!({"bot_id": bot_id.as_str(), "config": bot_summary(&config)}))
    }

    pub(crate) async fn assistant_operate(
        &self,
        bot_id: &str,
        operation: &str,
    ) -> Result<Value, String> {
        let bot_id = checked_bot_id(bot_id)?;
        let snapshot = match operation {
            "start_bot" => self.bots.start_bot(&bot_id).await,
            "stop_bot" => self.bots.stop_bot(&bot_id).await,
            "restart_bot" => self.bots.restart_bot(&bot_id).await,
            _ => return Err("不支持的 Bot 操作".to_owned()),
        }
        .map_err(|error| sanitize_log_line(&error.to_string()))?;
        Ok(
            json!({"bot_id": bot_id.as_str(), "operation": operation, "runtime": runtime_summary(&snapshot)}),
        )
    }
}
