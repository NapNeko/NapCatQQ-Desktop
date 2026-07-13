//! ncd-watch 配置同步与 Desktop 心跳

use ncd_domain::ids::BotId;
use ncd_runtime::ncd_watch_sync::{
    WatchNotifyConfig, WatchNotifyExtras, build_notify_config_with_extras, clear_desktop_present,
    write_desktop_present, write_notify_json_merged,
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
    write_notify_json_merged(host.as_ref(), home, &notify).await?;
    // 实例指标：同步 metrics.json 供 Desktop 退出后 ncd-watch 续采
    {
        use ncd_runtime::ncd_watch_sync::{
            bots_for_server, build_watch_metrics_config, write_metrics_json,
        };
        let settings = state.app_settings.read().await.clone();
        let remote_bots = bots_for_server(&server_id, bots.iter());
        let metrics = build_watch_metrics_config(
            home,
            settings.bot_runtime_metrics_enabled,
            settings.bot_runtime_metrics_interval_ms,
            settings.bot_runtime_metrics_retention_days,
            &remote_bots,
        );
        if let Err(e) = write_metrics_json(host.as_ref(), home, &metrics).await {
            tracing::warn!(%server_id, %e, "write metrics.json failed");
        }
    }
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

/// 远端 notify 同步策略:心跳 vs 保存后推送的差异只在筛选/日志级别
#[derive(Debug, Clone, Copy)]
enum NotifyPushMode {
    /// 仅已 Connected;无 bot 跳过写 notify;失败 debug
    Heartbeat,
    /// 该机有 Bot 才推;ensure_connected;失败 warn;汇总 info
    AfterSettingsSave,
}

/// 单机写 present + notify。成功 Ok(());跳过/失败 Err(原因字面量,已打日志)
async fn push_notify_for_server(
    state: &AppState,
    profile: &ncd_runtime::ServerProfile,
    bots: &[ncd_domain::bot_config::BotConfig],
    version: &str,
    mode: NotifyPushMode,
) -> Result<(), ()> {
    let log_tag = match mode {
        NotifyPushMode::Heartbeat => "heartbeat",
        NotifyPushMode::AfterSettingsSave => "save-sync",
    };

    match mode {
        NotifyPushMode::Heartbeat => {
            if profile.state != ncd_runtime::ServerState::Connected {
                return Err(());
            }
        }
        NotifyPushMode::AfterSettingsSave => {
            let has_bots = bots
                .iter()
                .any(|cfg| cfg.bot.runtime_target.server_id() == Some(profile.id.as_str()));
            if !has_bots {
                return Err(());
            }
        }
    }

    let host_id = format!("remote:{}", profile.id);
    let host = match state.server_manager.ensure_connected(&profile.id).await {
        Ok(h) => h,
        Err(e) => {
            match mode {
                NotifyPushMode::Heartbeat => {
                    tracing::debug!(server_id = %profile.id, %e, %log_tag, "ncd-watch skip");
                }
                NotifyPushMode::AfterSettingsSave => {
                    tracing::warn!(
                        server_id = %profile.id,
                        %e,
                        %log_tag,
                        "ncd-watch connect failed"
                    );
                }
            }
            return Err(());
        }
    };
    let probe = cached_host_probe(&host_id, host.as_ref(), state).await;
    let Some(home) = probe.home.as_deref().filter(|s| !s.is_empty()) else {
        if matches!(mode, NotifyPushMode::AfterSettingsSave) {
            tracing::warn!(server_id = %profile.id, %log_tag, "ncd-watch no $HOME");
        }
        return Err(());
    };
    if let Err(e) = write_desktop_present(host.as_ref(), home, Some(version)).await {
        match mode {
            NotifyPushMode::Heartbeat => {
                tracing::debug!(server_id = %profile.id, %e, "write desktop_present failed");
            }
            NotifyPushMode::AfterSettingsSave => {
                tracing::warn!(
                    server_id = %profile.id,
                    %e,
                    %log_tag,
                    "ncd-watch present failed"
                );
            }
        }
        return Err(());
    }
    let notify = build_notify_for_server(state, &profile.id, bots).await;
    if matches!(mode, NotifyPushMode::Heartbeat) && notify.bots.is_empty() {
        return Ok(());
    }
    if let Err(e) = write_notify_json_merged(host.as_ref(), home, &notify).await {
        match mode {
            NotifyPushMode::Heartbeat => {
                tracing::debug!(server_id = %profile.id, %e, "write notify.json failed");
            }
            NotifyPushMode::AfterSettingsSave => {
                tracing::warn!(
                    server_id = %profile.id,
                    %e,
                    %log_tag,
                    "ncd-watch notify failed"
                );
            }
        }
        return Err(());
    }
    Ok(())
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
    for profile in &servers {
        let _ =
            push_notify_for_server(state, profile, &bots, version, NotifyPushMode::Heartbeat).await;
    }
}

/// 保存 App 设置后立刻推 notify:对「该机有 Bot」的远端 try ensure_connected + 写盘。
/// 失败只记日志,不回传给设置保存(本地设置已落盘优先)。
pub async fn push_notify_after_app_settings_save(state: &AppState) {
    let servers = state.server_manager.list_servers().await;
    let bots = match state.bot_manager.list_bot_configs().await {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(%e, "ncd-watch save-sync: list bots failed");
            return;
        }
    };
    let version = env!("CARGO_PKG_VERSION");
    let mut ok = 0u32;
    let mut fail = 0u32;
    for profile in &servers {
        match push_notify_for_server(
            state,
            profile,
            &bots,
            version,
            NotifyPushMode::AfterSettingsSave,
        )
        .await
        {
            Ok(()) => {
                // 无 bot 的机返回 Err(());有 bot 且成功才 Ok
                // AfterSettingsSave 对无 bot 直接 Err,此处 Ok 即成功推送
                ok += 1;
            }
            Err(()) => {
                // 无 bot 跳过也算 fail 计数不准:仅统计「本该推却失败」
                let should = bots
                    .iter()
                    .any(|cfg| cfg.bot.runtime_target.server_id() == Some(profile.id.as_str()));
                if should {
                    fail += 1;
                }
            }
        }
    }
    if ok > 0 || fail > 0 {
        tracing::info!(
            ok,
            fail,
            "ncd-watch save-sync finished (notify after app-settings)"
        );
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

/// Desktop 退出前:对远端删掉 desktop_present,watch 立即接管告警。
/// 失败只记日志,不挡退出。
pub async fn clear_present_on_all_remote_servers(state: &AppState) {
    let servers = state.server_manager.list_servers().await;
    for profile in servers {
        let host = match state.server_manager.ensure_connected(&profile.id).await {
            Ok(h) => h,
            Err(e) => {
                tracing::debug!(server_id = %profile.id, %e, "clear present: connect skip");
                continue;
            }
        };
        let host_id = format!("remote:{}", profile.id);
        let probe = cached_host_probe(&host_id, host.as_ref(), state).await;
        let Some(home) = probe.home.as_deref().filter(|s| !s.is_empty()) else {
            continue;
        };
        if let Err(e) = clear_desktop_present(host.as_ref(), home).await {
            tracing::debug!(server_id = %profile.id, %e, "clear desktop_present failed");
        } else {
            tracing::info!(server_id = %profile.id, "cleared desktop_present on exit");
        }
    }
}
