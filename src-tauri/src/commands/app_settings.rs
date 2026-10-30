//! App 级设置读写命令
//!
//! 薄壳层:非敏感偏好经 ncd_runtime::desktop 读写 app-settings.json,SecretStore
//! 存 GitHub PAT(走 keyring 不明文落盘)设置页一次 get / 一次 set,前端不需要
//! 关心两套存储的差异——DTO 把 PAT 当普通字段,command 层负责拆分落盘

use ncd_domain::{AppSettingsDto, OfflineWebhookChannel};
use ncd_runtime::SecretStoreImpl;
use ncd_runtime::desktop::network::read_proxy_password;
use ncd_runtime::desktop::network_settings::{GITHUB_PAT_SECRET_KEY, save_desktop_settings};
use ncd_traits::SecretStore;
use tauri::{AppHandle, Manager, State};
use tokio_util::sync::CancellationToken;

use crate::AppState;

fn secret_store(state: &AppState) -> SecretStoreImpl {
    SecretStoreImpl::new(state.data_root.join("secrets"))
}

/// 读取 App 设置
///
/// app-settings.json 不存在(旧用户首次进设置页)时返回 AppSettings::default(),
/// 不报错PAT 读 SecretStore,keyring 不可用 / 未设置时回落空串
#[tauri::command]
pub async fn get_app_settings(state: State<'_, AppState>) -> Result<AppSettingsDto, String> {
    let _settings_guard = state.network_update_gate.lock().await;
    let (settings, mut network_error) =
        match ncd_runtime::desktop::load_app_settings_checked(&state.data_root) {
            Ok(settings) => (settings, None),
            Err(error) => (ncd_domain::AppSettings::default(), Some(error)),
        };

    let github_pat = secret_store(&state)
        .get(GITHUB_PAT_SECRET_KEY)
        .ok()
        .flatten()
        .unwrap_or_default();

    let proxy_password = match read_proxy_password(&secret_store(&state)) {
        Ok(password) => password.unwrap_or_default(),
        Err(error) => {
            network_error = Some(error);
            String::new()
        }
    };
    if network_error.is_none() {
        network_error = ncd_runtime::desktop::network::NetworkManager::prepare(
            &settings.network,
            &proxy_password,
        )
        .err();
    }
    Ok(AppSettingsDto {
        settings,
        github_pat,
        proxy_password,
        network_error,
    })
}

/// 写入 App 设置
///
/// 非敏感偏好原子写入 app-settings.json;PAT 非空写 SecretStore,空串则删除
/// 同时把轮询设置热更新到内存中的 BotManager,让正在运行的 Poller 下次 tick
/// 用新间隔(无需重启)
/// 落盘成功后后台把 Webhook/Email/OneBot 推到各远端 ncd-watch(不挡保存返回)
#[tauri::command]
pub async fn set_app_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    dto: AppSettingsDto,
) -> Result<(), String> {
    state.migrate_gate.ensure_idle()?;
    let _settings_guard = state.network_update_gate.lock().await;
    let (settings, network_changed) =
        save_desktop_settings(&state.data_root, &state.app_settings, &state.network, dto).await?;
    crate::ui_scale::sync_all(&app, &settings.ui_preferences);

    // 已保存的安全策略必须立即生效，不能被其他功能的热应用失败跳过。
    crate::commands::mcp::apply_mcp_settings(
        &state.mcp,
        &state.data_root,
        &state.app_settings,
        &settings.mcp,
    )
    .await;
    state
        .onebot_debug
        .set_enabled(settings.features.api_debug)
        .await;

    #[cfg(windows)]
    {
        let refresh_app = app.clone();
        if let Err(error) = app.run_on_main_thread(move || {
            crate::tray_panel_native::sync_ui_preferences(&refresh_app);
            crate::native_panel::window::refresh_visible_themes();
        }) {
            tracing::warn!(%error, "界面偏好已保存，刷新原生托盘主题失败");
        }
    }

    if network_changed {
        if let Err(err) = ncd_runtime::release::invalidate_release_cache(&state.data_root) {
            // 配置已经落盘，缓存清理失败不应阻断其他设置的热更新。
            tracing::warn!(error = %err, "GitHub 配置已保存，清除版本缓存失败");
        }
    }

    // 开机自启:JSON 为权威源,落盘成功后再收敛 HKCU Run(用户级,无需 UAC)。
    // 若此处失败,启动时 reconcile_launch_on_startup 会再按 JSON 对齐。
    crate::autostart::apply_launch_on_startup(settings.launch_on_startup)
        .map_err(|e| format!("同步开机自启失败: {e}"))?;

    // 热更新内存中的轮询设置,运行中的 Poller 下次 tick 生效
    state
        .bot_manager
        .update_poller_settings(settings.poller.clone())
        .await;
    state
        .bot_manager
        .update_desktop_notify_settings(settings.desktop_notify_flags())
        .await;
    *state.desktop_notify.write().await = settings.desktop_notify_flags();

    // 热更新 SnowLumaDaemon 的 node 路径覆盖
    state.snowluma_daemon.set_node_bin_override(
        settings
            .snowluma_node_override()
            .map(std::path::PathBuf::from),
    );

    // 本机指标采集 prefs 热更新（间隔/开关/保留天数）
    {
        let mut prefs = ncd_runtime::metrics::BotRuntimeMetricsPrefs::from_app(&settings);
        prefs.normalize();
        *state.metrics_collector.prefs().write().await = prefs;
    }
    let bot_manager = state.bot_manager.clone();
    tauri::async_runtime::spawn(async move {
        bot_manager.refresh_snowluma_metrics_registry().await;
    });

    crate::chat_window::apply_chat_feature(&app, settings.features.chat).await?;
    state
        .offline_notifier
        .update_from_app_settings(
            settings.poller.clone(),
            settings.offline_webhook.clone(),
            settings.offline_email.clone(),
            settings.offline_onebot.clone(),
            settings.desktop_notify_flags(),
        )
        .await;

    // 主动探活:设置变化时 cancel 旧 walker,按新 enabled 条件 spawn / restart
    // 先取消旧任务(若有),再根据新开关决定是否启动新 walker
    {
        // 取消旧 walker
        let mut guard = state.health_probe_cancel.lock().await;
        if let Some(token) = guard.take() {
            token.cancel();
        }

        if settings.remote_host_health_probe_enabled {
            let cancel_token = CancellationToken::new();
            let child = cancel_token.child_token();
            *guard = Some(cancel_token);

            let sm = std::sync::Arc::clone(&state.server_manager);
            let settings_arc = std::sync::Arc::clone(&state.app_settings);
            tauri::async_runtime::spawn(async move {
                sm.run_health_probe_loop(settings_arc, child).await;
            });
        }
    }

    // 通知相关已落盘:后台推远端 ncd-watch notify,失败只记日志,不挡保存返回
    tauri::async_runtime::spawn(async move {
        if let Some(st) = app.try_state::<AppState>() {
            crate::commands::ncd_watch::push_notify_after_app_settings_save(st.inner()).await;
        }
    });

    Ok(())
}

/// 发送测试 Webhook
///
/// 优先用前端传入的草稿通道(改完即测,无需先保存);未传时回退到已保存配置。
/// channel_id 为空且未传 channel 时测第一条有效通道
#[tauri::command]
pub async fn test_offline_webhook(
    state: State<'_, AppState>,
    channel_id: Option<String>,
    channel: Option<OfflineWebhookChannel>,
) -> Result<(), String> {
    let settings = state.app_settings.read().await.clone();
    ncd_runtime::send_test_webhook(
        &settings.offline_webhook,
        channel_id.as_deref(),
        channel.as_ref(),
    )
    .await
}

/// 发送测试邮件(使用当前磁盘配置)
#[tauri::command]
pub async fn test_offline_email(state: State<'_, AppState>) -> Result<(), String> {
    let settings = state.app_settings.read().await.clone();
    tokio::task::spawn_blocking(move || ncd_runtime::send_test_email(&settings.offline_email))
        .await
        .map_err(|e| e.to_string())?
}

/// 读取内存中的离线通知投递历史(新→旧)
#[tauri::command]
pub async fn list_offline_delivery_history(
    state: State<'_, AppState>,
) -> Result<Vec<ncd_domain::OfflineDeliveryRecord>, String> {
    Ok(state.offline_notifier.delivery_history().await)
}

/// 清空内存投递历史
#[tauri::command]
pub async fn clear_offline_delivery_history(state: State<'_, AppState>) -> Result<(), String> {
    state.offline_notifier.clear_delivery_history().await;
    Ok(())
}

/// 列出可作为 OneBot 发送方的 Bot 候选(本机 + 远端)
///
/// 本机: Desktop 在线投递用;eligible = Running 且环回 HTTP。
/// 远端: 仅写入全局 messenger 列表,由 ncd-watch 按同 server 过滤;eligible 恒 false。
#[tauri::command]
pub async fn list_onebot_messenger_candidates(
    state: State<'_, AppState>,
) -> Result<Vec<ncd_domain::OneBotMessengerCandidate>, String> {
    list_onebot_messenger_candidates_inner(&state).await
}

/// 为 Bot 自动补齐/启用环回 HTTP 服务(本机 + 远端)
///
/// 已有可用环回 HTTP 时直接返回;否则启用已有服务或新建 `desktop-offline-http`。
/// 走 BotManager upsert:原生部署热更，Docker 自动同步发布端口并应用容器配置。
/// Docker 监听容器所有网卡，由同机 watch 经宿主机发布端口访问，不跨机打 OneBot。
#[tauri::command]
pub async fn ensure_onebot_messenger_http(
    state: State<'_, AppState>,
    bot_id: String,
) -> Result<ncd_domain::EnsureOneBotMessengerHttpResult, String> {
    use ncd_domain::{
        BotId, EnsureOneBotMessengerHttpResult, HttpServerConfig, MessagePostFormat,
        NetworkBaseFields,
    };
    use std::collections::HashSet;

    let bot_id = BotId::new(bot_id.trim());
    if bot_id.as_str().is_empty() {
        return Err("BotId 不能为空".to_string());
    }

    let mut cfg = state
        .bot_manager
        .get_bot_config(&bot_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Bot 不存在: {bot_id}"))?;

    let host_info = resolve_onebot_host_info(&state, &cfg).await?;
    let is_local = host_info.scope.is_local();
    let is_docker = cfg.bot.deployment_type == ncd_domain::DeploymentType::Docker;

    let snap = state
        .bot_manager
        .get_snapshot(&bot_id)
        .await
        .map_err(|e| e.to_string())?;
    if let Some(port) = onebot_messenger_http_port(bot_id.as_str(), &cfg) {
        let candidate = build_onebot_candidate(bot_id.as_str(), &cfg, &snap.state, &host_info);
        return Ok(EnsureOneBotMessengerHttpResult {
            bot_id: bot_id.to_string(),
            action: "already_ready".to_string(),
            port,
            candidate,
        });
    }

    // 优先启用已有可环回服务,避免重复开端口
    let mut action = "enabled".to_string();
    let mut port = 0u16;
    let mut enabled_existing = false;
    for server in &mut cfg.connect.http_servers {
        if server.port == 0 {
            continue;
        }
        let host = server.host.trim();
        if !matches!(
            host,
            "" | "0.0.0.0" | "::" | "[::]" | "127.0.0.1" | "::1" | "[::1]"
        ) && !host.eq_ignore_ascii_case("localhost")
        {
            continue;
        }
        server.base.enable = true;
        if is_docker {
            server.host = "0.0.0.0".to_string();
        } else if server.host.trim().is_empty() {
            server.host = "127.0.0.1".to_string();
        }
        port = server.port;
        enabled_existing = true;
        break;
    }

    if !enabled_existing {
        let mut used_ports = HashSet::new();
        for server in &cfg.connect.http_servers {
            if server.port > 0 {
                used_ports.insert(server.port);
            }
        }
        used_ports.extend(cfg.connect.websocket_servers.iter().map(|s| s.port));
        used_ports.extend(cfg.connect.http_sse_servers.iter().map(|s| s.port));
        // 同 scope 内其它 Bot 的端口也避开:本机=全部本机;远端=同 server_id
        let this_server = cfg.bot.runtime_target.server_id().map(str::to_string);
        for other in state.bot_manager.list_snapshots().await {
            if other.bot_id == bot_id {
                continue;
            }
            if let Ok(Some(other_cfg)) = state.bot_manager.get_bot_config(&other.bot_id).await {
                let same_host = if is_local {
                    other_cfg.bot.runtime_target.is_local()
                } else {
                    other_cfg.bot.runtime_target.server_id() == this_server.as_deref()
                };
                if !same_host {
                    continue;
                }
                for server in &other_cfg.connect.http_servers {
                    if server.port > 0 {
                        used_ports.insert(server.port);
                    }
                }
            }
        }

        port = if is_local {
            pick_loopback_http_port(&used_ports).ok_or_else(|| {
                "无法分配可用的本机 HTTP 端口,请手动在 Bot 连接配置中添加".to_string()
            })?
        } else {
            // 远端无法在此探测目标机 TCP 占用,只在同机 Bot 配置端口池里挑空位
            pick_configured_http_port(&used_ports).ok_or_else(|| {
                "无法分配可用的环回 HTTP 端口,请手动在 Bot 连接配置中添加".to_string()
            })?
        };
        cfg.connect.http_servers.push(HttpServerConfig {
            base: NetworkBaseFields {
                enable: true,
                name: "desktop-offline-http".to_string(),
                message_post_format: MessagePostFormat::Array,
                token: String::new(),
                debug: false,
            },
            host: if is_docker { "0.0.0.0" } else { "127.0.0.1" }.to_string(),
            port,
            enable_cors: true,
            enable_websocket: false,
            path: "/".to_string(),
        });
        action = "created".to_string();
    }

    state
        .bot_manager
        .upsert_bot_config(cfg.clone())
        .await
        .map_err(|e| e.to_string())?;

    // upsert 后状态可能变化,重新取 snapshot
    let snap = state
        .bot_manager
        .get_snapshot(&bot_id)
        .await
        .map_err(|e| e.to_string())?;
    let candidate = build_onebot_candidate(bot_id.as_str(), &cfg, &snap.state, &host_info);
    // 对 Docker 返回宿主机发布端口，与候选列表和 watch 的访问方式保持一致。
    let port = onebot_messenger_http_port(bot_id.as_str(), &cfg).unwrap_or(port);

    Ok(EnsureOneBotMessengerHttpResult {
        bot_id: bot_id.to_string(),
        action,
        port,
        candidate,
    })
}

struct OneBotHostInfo {
    scope: ncd_domain::OneBotMessengerScope,
    server_id: Option<String>,
    server_label: String,
}

impl OneBotHostInfo {
    fn local() -> Self {
        Self {
            scope: ncd_domain::OneBotMessengerScope::Local,
            server_id: None,
            server_label: "本机".to_string(),
        }
    }

    fn remote(server_id: String, server_label: String) -> Self {
        Self {
            scope: ncd_domain::OneBotMessengerScope::Remote,
            server_id: Some(server_id),
            server_label,
        }
    }
}

fn format_server_label(name: &str, username: &str, host: &str, port: u16) -> String {
    let name = name.trim();
    if !name.is_empty() {
        return name.to_string();
    }
    let port_part = if port != 0 && port != 22 {
        format!(":{port}")
    } else {
        String::new()
    };
    format!("{username}@{host}{port_part}")
}

async fn resolve_onebot_host_info(
    state: &State<'_, AppState>,
    cfg: &ncd_domain::BotConfig,
) -> Result<OneBotHostInfo, String> {
    if cfg.bot.runtime_target.is_local() {
        return Ok(OneBotHostInfo::local());
    }
    let sid = cfg
        .bot
        .runtime_target
        .server_id()
        .ok_or_else(|| "远端 Bot 未绑定服务器档案".to_string())?;
    let servers = state.server_manager.list_servers().await;
    let label = servers
        .iter()
        .find(|p| p.id == sid)
        .map(|p| format_server_label(&p.name, &p.username, &p.host, p.port))
        .unwrap_or_else(|| sid.to_string());
    Ok(OneBotHostInfo::remote(sid.to_string(), label))
}

async fn list_onebot_messenger_candidates_inner(
    state: &State<'_, AppState>,
) -> Result<Vec<ncd_domain::OneBotMessengerCandidate>, String> {
    let snapshots = state.bot_manager.list_snapshots().await;
    let servers = state.server_manager.list_servers().await;
    let server_label_by_id: std::collections::HashMap<String, String> = servers
        .into_iter()
        .map(|p| {
            let label = format_server_label(&p.name, &p.username, &p.host, p.port);
            (p.id, label)
        })
        .collect();

    let mut out = Vec::with_capacity(snapshots.len());

    for snap in snapshots {
        let bot_id = snap.bot_id.clone();
        let cfg = match state.bot_manager.get_bot_config(&bot_id).await {
            Ok(Some(cfg)) => cfg,
            _ => continue,
        };
        let host_info = if cfg.bot.runtime_target.is_local() {
            OneBotHostInfo::local()
        } else if let Some(sid) = cfg.bot.runtime_target.server_id() {
            let label = server_label_by_id
                .get(sid)
                .cloned()
                .unwrap_or_else(|| sid.to_string());
            OneBotHostInfo::remote(sid.to_string(), label)
        } else {
            // 未绑定具体远端档案,跳过,避免误选
            continue;
        };
        out.push(build_onebot_candidate(
            bot_id.as_str(),
            &cfg,
            &snap.state,
            &host_info,
        ));
    }

    out.sort_by(|a, b| {
        // 本机组优先,再按主机标签,组内 eligible / HTTP / 运行中 / 名
        let a_local = a.scope.is_local();
        let b_local = b.scope.is_local();
        b_local
            .cmp(&a_local)
            .then_with(|| a.server_label.cmp(&b.server_label))
            .then_with(|| a.server_id.cmp(&b.server_id))
            .then_with(|| b.eligible.cmp(&a.eligible))
            .then_with(|| b.has_local_http.cmp(&a.has_local_http))
            .then_with(|| {
                let a_running = a.state == "running";
                let b_running = b.state == "running";
                b_running.cmp(&a_running)
            })
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.bot_id.cmp(&b.bot_id))
    });
    Ok(out)
}

fn build_onebot_candidate(
    bot_id: &str,
    cfg: &ncd_domain::BotConfig,
    state: &ncd_domain::BotActorState,
    host: &OneBotHostInfo,
) -> ncd_domain::OneBotMessengerCandidate {
    use ncd_domain::BotActorState;

    let is_running = *state == BotActorState::Running;
    let is_local = host.scope.is_local();
    let port = onebot_messenger_http_port(bot_id, cfg);
    let has_local_http = port.is_some();
    let http_port = port.unwrap_or(0);
    let state_label = match state {
        BotActorState::Stopped => "stopped",
        BotActorState::Starting => "starting",
        BotActorState::Running => "running",
        BotActorState::Stopping => "stopping",
        BotActorState::Crashed => "crashed",
        BotActorState::Repairing => "repairing",
    };
    let backend_type = match cfg.bot.backend_type {
        ncd_domain::BackendType::NapCat => "napcat",
        ncd_domain::BackendType::SnowLuma => "snowluma",
    };

    ncd_domain::OneBotMessengerCandidate {
        bot_id: bot_id.to_string(),
        name: cfg.bot.name.clone(),
        state: state_label.to_string(),
        backend_type: backend_type.to_string(),
        has_local_http,
        has_relogin_ws: ncd_runtime::ncd_watch_sync::watch_event_endpoint(cfg).is_some(),
        http_port,
        // Desktop 只对本机 Running+HTTP 当场发;远端 eligible 恒 false(watch 用 has_local_http)
        eligible: is_local && is_running && has_local_http,
        // 本机/远端缺环回 HTTP 都可一键写配置(远端走 upsert + 热推)
        can_enable_http: !has_local_http,
        scope: host.scope,
        server_id: host.server_id.clone(),
        server_label: host.server_label.clone(),
    }
}

/// 候选只描述已配置的访问方式；watch 投递前仍以 inspect 的实际端口为准。
fn onebot_messenger_http_port(bot_id: &str, cfg: &ncd_domain::BotConfig) -> Option<u16> {
    if cfg.bot.deployment_type == ncd_domain::DeploymentType::Docker {
        let messenger = ncd_runtime::ncd_watch_sync::watch_onebot_messenger_from_bot(cfg)?;
        return parse_port_from_base_url(&messenger.base_url);
    }
    let servers: Vec<ncd_runtime::LocalHttpServerCandidate> = cfg
        .connect
        .http_servers
        .iter()
        .map(|s| ncd_runtime::LocalHttpServerCandidate {
            enable: s.base.enable,
            host: s.host.clone(),
            port: s.port,
            token: s.base.token.clone(),
        })
        .collect();
    let messenger =
        ncd_runtime::resolve_local_onebot_messenger(bot_id, "__candidate__", true, &servers)
            .ok()?;
    parse_port_from_base_url(&messenger.base_url)
}

fn parse_port_from_base_url(base_url: &str) -> Option<u16> {
    let url = base_url.trim().trim_end_matches('/');
    let after_scheme = url.split("://").nth(1)?;
    let host_port = after_scheme.split('/').next()?;
    let port = host_port.rsplit(':').next()?;
    port.parse().ok()
}

/// 远端新建端口:只避开已占用配置号,不做本机 bind 探测
fn pick_configured_http_port(used: &std::collections::HashSet<u16>) -> Option<u16> {
    (3010u16..=3999).find(|port| !used.contains(port))
}

fn pick_loopback_http_port(used: &std::collections::HashSet<u16>) -> Option<u16> {
    use std::net::TcpListener;

    // 优先 3010+ 段,避开常见 3000 业务端口;失败再让系统分配
    for port in 3010u16..=3999 {
        if used.contains(&port) {
            continue;
        }
        if TcpListener::bind(("127.0.0.1", port)).is_ok() {
            return Some(port);
        }
    }
    let listener = TcpListener::bind(("127.0.0.1", 0)).ok()?;
    listener.local_addr().ok().map(|addr| addr.port())
}

/// 读取已保存的 GitHub PAT(SecretStore),未设置 / keyring 不可用时回 None
/// 给 release fetcher 拉 GitHub API 时带认证头用,复用同一 secret key
pub fn read_github_pat(data_root: &std::path::Path) -> Option<String> {
    SecretStoreImpl::new(data_root.join("secrets"))
        .get(GITHUB_PAT_SECRET_KEY)
        .ok()
        .flatten()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

#[cfg(test)]
mod onebot_http_candidate_tests {
    use ncd_domain::{
        BotActorState, BotDockerConfig, DeploymentType, HttpServerConfig, MessagePostFormat,
        NetworkBaseFields, PortMapping,
    };
    use ncd_test_support::BotConfigBuilder;

    use super::*;

    fn bot_with_http(deployment: DeploymentType, host: &str) -> ncd_domain::BotConfig {
        let mut cfg = BotConfigBuilder::new().deployment_type(deployment).build();
        cfg.connect.http_servers.push(HttpServerConfig {
            base: NetworkBaseFields {
                enable: true,
                name: "notify".into(),
                message_post_format: MessagePostFormat::Array,
                token: String::new(),
                debug: false,
            },
            host: host.into(),
            port: 3010,
            path: "/".into(),
            enable_cors: false,
            enable_websocket: false,
        });
        cfg
    }

    #[test]
    fn docker_candidate_uses_custom_host_mapping_for_nondefault_listener() {
        let mut cfg = bot_with_http(DeploymentType::Docker, "0.0.0.0");
        cfg.docker = Some(BotDockerConfig {
            port_mappings: vec![PortMapping::new(18080, 3010)],
            ..BotDockerConfig::default()
        });
        let host = OneBotHostInfo::remote("server".into(), "Linux".into());
        let candidate = build_onebot_candidate("10001", &cfg, &BotActorState::Running, &host);
        assert!(candidate.has_local_http);
        assert_eq!(candidate.http_port, 18080);
        assert!(!candidate.eligible, "远端通知由同机 watch 投递");
        assert!(!candidate.can_enable_http);
    }

    #[test]
    fn native_candidate_keeps_listener_port_and_disabled_http_is_not_ready() {
        let mut cfg = bot_with_http(DeploymentType::Native, "127.0.0.1");
        assert_eq!(onebot_messenger_http_port("10001", &cfg), Some(3010));
        cfg.connect.http_servers[0].base.enable = false;
        assert_eq!(onebot_messenger_http_port("10001", &cfg), None);
    }

    #[test]
    fn docker_listener_on_specific_interface_is_not_a_watch_candidate() {
        let cfg = bot_with_http(DeploymentType::Docker, "192.0.2.10");
        assert_eq!(onebot_messenger_http_port("10001", &cfg), None);
    }
}
