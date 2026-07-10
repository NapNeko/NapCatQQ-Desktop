//! App 级设置读写命令
//!
//! 薄壳层:组合 ConfigStore(非敏感偏好,落 app-settings.json)+ SecretStore
//! (GitHub PAT,走 keyring 不明文落盘)设置页一次 get / 一次 set,前端不需要
//! 关心两套存储的差异——DTO 把 PAT 当普通字段,command 层负责拆分落盘
//!
//! 路径权威性:app-settings.json 落在 LocalConfigStore::config_dir(),即
//! <data_root>/config/,与 bot.json / servers.json 同级

use ncd_domain::{AppSettings, AppSettingsDto};
use ncd_runtime::{LocalConfigStore, SecretStoreImpl};
use ncd_traits::{ConfigStore, SecretStore};
use tauri::State;
use tokio_util::sync::CancellationToken;

use crate::AppState;

/// GitHub PAT 在 SecretStore 里的 key与 SSH 凭证的 ssh:{id} 命名风格一致
const GITHUB_PAT_SECRET_KEY: &str = "app:github_pat";

/// app-settings.json 在 config_dir 下的文件名
const APP_SETTINGS_FILE: &str = "app-settings.json";

fn config_store(state: &AppState) -> LocalConfigStore {
    LocalConfigStore::new(&state.data_root)
}

fn secret_store(state: &AppState) -> SecretStoreImpl {
    SecretStoreImpl::new(state.data_root.join("secrets"))
}

/// 读取 App 设置
///
/// app-settings.json 不存在(旧用户首次进设置页)时返回 AppSettings::default(),
/// 不报错PAT 读 SecretStore,keyring 不可用 / 未设置时回落空串
#[tauri::command]
pub async fn get_app_settings(state: State<'_, AppState>) -> Result<AppSettingsDto, String> {
    let store = config_store(&state);
    let path = store.config_dir().join(APP_SETTINGS_FILE);

    let settings = load_app_settings_from(&store, &path);

    let github_pat = secret_store(&state)
        .get(GITHUB_PAT_SECRET_KEY)
        .ok()
        .flatten()
        .unwrap_or_default();

    Ok(AppSettingsDto {
        settings,
        github_pat,
    })
}

/// 写入 App 设置
///
/// 非敏感偏好原子写入 app-settings.json;PAT 非空写 SecretStore,空串则删除
/// 同时把轮询设置热更新到内存中的 BotManager,让正在运行的 Poller 下次 tick
/// 用新间隔(无需重启)
#[tauri::command]
pub async fn set_app_settings(
    state: State<'_, AppState>,
    dto: AppSettingsDto,
) -> Result<(), String> {
    let store = config_store(&state);
    let path = store.config_dir().join(APP_SETTINGS_FILE);

    let mut settings = dto.settings;
    settings.normalize_performance_monitor();
    settings.normalize_task_queue_cleanup();
    settings.normalize_lightweight_prefs();
    settings.normalize_remote_host_health_probe();
    settings.offline_webhook.normalize();
    settings.offline_onebot.normalize();
    settings.poller.offline_notify_behavior.normalize();
    let payload =
        serde_json::to_value(&settings).map_err(|e| format!("序列化 app 设置失败: {e}"))?;
    store
        .write_json_atomic(&path, &payload)
        .map_err(|e| format!("写入 app-settings.json 失败: {e}"))?;

    // PAT:非空写 keyring,空串清除删除失败(本就没有)忽略
    let secrets = secret_store(&state);
    let pat = dto.github_pat.trim();
    if pat.is_empty() {
        let _ = secrets.delete(GITHUB_PAT_SECRET_KEY);
    } else {
        secrets
            .put(GITHUB_PAT_SECRET_KEY, pat)
            .map_err(|e| format!("保存 GitHub PAT 失败: {e}"))?;
    }

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
    *state.app_settings.write().await = settings.clone();

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

    Ok(())
}

/// 启动期把磁盘上的 closeAction 同步给前端(localStorage 偏好)
#[tauri::command]
pub fn sync_close_action_preference(
    _app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let store = config_store(&state);
    let path = store.config_dir().join(APP_SETTINGS_FILE);
    let settings = load_app_settings_from(&store, &path);
    serde_json::to_value(&settings.close_action)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .ok_or_else(|| "serialize close_action failed".to_string())
}

/// 发送测试 Webhook(使用当前内存/磁盘配置)
/// channel_id 为空时测第一条有 URL 的通道
#[tauri::command]
pub async fn test_offline_webhook(
    state: State<'_, AppState>,
    channel_id: Option<String>,
) -> Result<(), String> {
    let settings = state.app_settings.read().await.clone();
    ncd_runtime::send_test_webhook(&settings.offline_webhook, channel_id.as_deref()).await
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

/// 列出可作为 OneBot 发送方的本机 Bot 候选
///
/// 前端用它做多选,而不是手填 QQ。eligible = Running 且具备环回 HTTP 服务。
#[tauri::command]
pub async fn list_onebot_messenger_candidates(
    state: State<'_, AppState>,
) -> Result<Vec<ncd_domain::OneBotMessengerCandidate>, String> {
    list_onebot_messenger_candidates_inner(&state).await
}

/// 为指定本机 Bot 自动补齐/启用环回 HTTP 服务
///
/// 已有可用 HTTP 时直接返回;否则启用已有服务或新建 `desktop-offline-http`。
/// 走 BotManager upsert,Running 中的 NC/SL 会热更新派生配置。
#[tauri::command]
pub async fn ensure_onebot_messenger_http(
    state: State<'_, AppState>,
    bot_id: String,
) -> Result<ncd_domain::EnsureOneBotMessengerHttpResult, String> {
    use ncd_domain::{
        BotId, EnsureOneBotMessengerHttpResult, HttpServerConfig, MessagePostFormat,
        NetworkBaseFields,
    };
    use ncd_runtime::resolve_local_onebot_messenger;
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

    if !cfg.bot.runtime_target.is_local() {
        return Err("仅支持为本机 Bot 自动配置 OneBot HTTP".to_string());
    }

    let snap = state
        .bot_manager
        .get_snapshot(&bot_id)
        .await
        .map_err(|e| e.to_string())?;

    let servers_before: Vec<ncd_runtime::LocalHttpServerCandidate> = cfg
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
    if let Ok(ready) =
        resolve_local_onebot_messenger(bot_id.as_str(), "__candidate__", true, &servers_before)
    {
        let port = parse_port_from_base_url(&ready.base_url).unwrap_or(0);
        let candidate = build_onebot_candidate(&bot_id.to_string(), &cfg, &snap.state);
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
        let host = if server.host.trim().is_empty() || server.host == "0.0.0.0" {
            "127.0.0.1"
        } else {
            server.host.trim()
        };
        if host != "127.0.0.1" && host != "localhost" {
            continue;
        }
        server.base.enable = true;
        if server.host.trim().is_empty() {
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
        // 也避开其它本机 Bot 已占用的 HTTP 端口,减少冲突
        for other in state.bot_manager.list_snapshots().await {
            if other.bot_id == bot_id {
                continue;
            }
            if let Ok(Some(other_cfg)) = state.bot_manager.get_bot_config(&other.bot_id).await {
                if !other_cfg.bot.runtime_target.is_local() {
                    continue;
                }
                for server in &other_cfg.connect.http_servers {
                    if server.port > 0 {
                        used_ports.insert(server.port);
                    }
                }
            }
        }

        port = pick_loopback_http_port(&used_ports).ok_or_else(|| {
            "无法分配可用的本机 HTTP 端口,请手动在 Bot 连接配置中添加".to_string()
        })?;
        cfg.connect.http_servers.push(HttpServerConfig {
            base: NetworkBaseFields {
                enable: true,
                name: "desktop-offline-http".to_string(),
                message_post_format: MessagePostFormat::Array,
                token: String::new(),
                debug: false,
            },
            host: "127.0.0.1".to_string(),
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
    let candidate = build_onebot_candidate(&bot_id.to_string(), &cfg, &snap.state);

    Ok(EnsureOneBotMessengerHttpResult {
        bot_id: bot_id.to_string(),
        action,
        port,
        candidate,
    })
}

async fn list_onebot_messenger_candidates_inner(
    state: &State<'_, AppState>,
) -> Result<Vec<ncd_domain::OneBotMessengerCandidate>, String> {
    let snapshots = state.bot_manager.list_snapshots().await;
    let mut out = Vec::with_capacity(snapshots.len());

    for snap in snapshots {
        let bot_id = snap.bot_id.clone();
        let cfg = match state.bot_manager.get_bot_config(&bot_id).await {
            Ok(Some(cfg)) => cfg,
            _ => continue,
        };
        if !cfg.bot.runtime_target.is_local() {
            continue;
        }
        out.push(build_onebot_candidate(
            &bot_id.to_string(),
            &cfg,
            &snap.state,
        ));
    }

    out.sort_by(|a, b| {
        b.eligible
            .cmp(&a.eligible)
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
) -> ncd_domain::OneBotMessengerCandidate {
    use ncd_domain::BotActorState;
    use ncd_runtime::resolve_local_onebot_messenger;

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

    let is_running = *state == BotActorState::Running;
    let resolved = resolve_local_onebot_messenger(bot_id, "__candidate__", true, &servers).ok();
    let has_local_http = resolved.is_some();
    let http_port = resolved
        .as_ref()
        .and_then(|m| parse_port_from_base_url(&m.base_url))
        .unwrap_or(0);
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
        http_port,
        eligible: is_running && has_local_http,
        can_enable_http: !has_local_http,
    }
}

fn parse_port_from_base_url(base_url: &str) -> Option<u16> {
    let url = base_url.trim().trim_end_matches('/');
    let after_scheme = url.split("://").nth(1)?;
    let host_port = after_scheme.split('/').next()?;
    let port = host_port.rsplit(':').next()?;
    port.parse().ok()
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

/// 从磁盘加载 AppSettings;文件缺失或解析失败一律回落 Default,不抛错
/// 供 command 与启动期共用(启动期通过 read_app_settings 包装)
fn load_app_settings_from(store: &LocalConfigStore, path: &std::path::Path) -> AppSettings {
    match store.read_json(path) {
        Ok(value) => serde_json::from_value(value).unwrap_or_default(),
        Err(_) => AppSettings::default(),
    }
}

/// 启动期读取 AppSettings:给 lib.rs 在构造 BotManager 前加载磁盘值用
/// 与 get_app_settings 共用同一份回落语义
pub fn read_app_settings(data_root: &std::path::Path) -> AppSettings {
    let store = LocalConfigStore::new(data_root);
    let path = store.config_dir().join(APP_SETTINGS_FILE);
    load_app_settings_from(&store, &path)
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
