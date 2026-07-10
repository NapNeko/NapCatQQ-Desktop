//! ncd-watch 配置同步与 Desktop 心跳

use ncd_domain::ids::BotId;
use ncd_runtime::ncd_watch_sync::{
    WatchNotifyConfig, WatchNotifyExtras, build_notify_config_with_extras, write_desktop_present,
    write_notify_json,
};
use tauri::{AppHandle, Manager, State};

use crate::AppState;
use crate::commands::components::cached_host_probe;
use crate::commands::host_resolve::resolve_host_with_autoconnect;

async fn collect_webui_map(
    state: &AppState,
    bots: &[ncd_domain::bot_config::BotConfig],
) -> std::collections::HashMap<String, (u16, String)> {
    let mut map = std::collections::HashMap::new();
    for cfg in bots {
        if cfg.bot.backend_type != ncd_domain::bot_config::BackendType::NapCat {
            continue;
        }
        let bot_id = BotId::new(cfg.bot.qq_id.to_string());
        if let Some((port, token)) = state.bot_manager.napcat_webui_for_watch(&bot_id).await {
            map.insert(cfg.bot.qq_id.to_string(), (port, token));
        }
    }
    map
}

async fn build_notify_for_server(
    state: &AppState,
    server_id: &str,
    bots: &[ncd_domain::bot_config::BotConfig],
) -> WatchNotifyConfig {
    let settings = state.app_settings.read().await.clone();
    let webui = collect_webui_map(state, bots).await;
    let extras = WatchNotifyExtras::from_app(
        &settings.poller,
        settings.offline_email,
        settings.offline_onebot,
        webui,
    );
    build_notify_config_with_extras(server_id, bots, &settings.offline_webhook, &extras)
}

/// 将当前 App 通知设置 + 该 server 上的 Bot 列表写入远端 notify.json
#[tauri::command]
pub async fn sync_ncd_watch_notify(
    server_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let host_id = format!("remote:{server_id}");
    let host = resolve_host_with_autoconnect(&host_id, &state).await?;
    let probe = cached_host_probe(&host_id, host.as_ref(), &state).await;
    let home = probe
        .home
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "无法解析远端 $HOME，请先测试连接".to_string())?;

    let bots = state
        .bot_manager
        .list_bot_configs()
        .await
        .map_err(|e| e.to_string())?;
    let notify = build_notify_for_server(state.inner(), &server_id, &bots).await;
    write_notify_json(host.as_ref(), home, &notify).await?;
    write_desktop_present(host.as_ref(), home, Some(env!("CARGO_PKG_VERSION"))).await?;
    Ok(())
}

/// 刷新远端 desktop_present(Desktop 在线心跳)
#[tauri::command]
pub async fn touch_ncd_watch_present(
    server_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let host_id = format!("remote:{server_id}");
    let host = resolve_host_with_autoconnect(&host_id, &state).await?;
    let probe = cached_host_probe(&host_id, host.as_ref(), &state).await;
    let home = probe
        .home
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "无法解析远端 $HOME，请先测试连接".to_string())?;
    write_desktop_present(host.as_ref(), home, Some(env!("CARGO_PKG_VERSION"))).await
}

/// 对所有已连接远端 server 刷 present + 有远端 bot 时同步 notify
pub async fn heartbeat_all_remote_servers(state: &AppState) {
    let servers = state.server_manager.list_servers().await;
    let bots = match state.bot_manager.list_bot_configs().await {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(%e, "ncd-watch heartbeat: list bots failed");
            return;
        }
    };
    let version = env!("CARGO_PKG_VERSION");

    for profile in servers {
        if profile.state != ncd_runtime::ServerState::Connected {
            continue;
        }
        let host_id = format!("remote:{}", profile.id);
        let host = match state.server_manager.ensure_connected(&profile.id).await {
            Ok(h) => h,
            Err(e) => {
                tracing::debug!(server_id = %profile.id, %e, "ncd-watch heartbeat skip");
                continue;
            }
        };
        let probe = cached_host_probe(&host_id, host.as_ref(), state).await;
        let Some(home) = probe.home.as_deref().filter(|s| !s.is_empty()) else {
            continue;
        };
        if let Err(e) = write_desktop_present(host.as_ref(), home, Some(version)).await {
            tracing::debug!(server_id = %profile.id, %e, "write desktop_present failed");
            continue;
        }
        let notify = build_notify_for_server(state, &profile.id, &bots).await;
        if notify.bots.is_empty() {
            continue;
        }
        if let Err(e) = write_notify_json(host.as_ref(), home, &notify).await {
            tracing::debug!(server_id = %profile.id, %e, "write notify.json failed");
        }
    }
}

/// 后台周期心跳(默认 45s,小于 watch 默认 TTL 90s)
pub fn spawn_ncd_watch_heartbeat(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(45));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            let Some(state) = app.try_state::<AppState>() else {
                continue;
            };
            heartbeat_all_remote_servers(state.inner()).await;
        }
    });
}
