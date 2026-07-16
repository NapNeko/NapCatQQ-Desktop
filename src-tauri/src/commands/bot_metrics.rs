//! Bot 运行时指标 IPC（薄壳）
//!
//! 本机：读 data_root/metrics/<bot>/net-stats.json
//! 远端：经 SSH 读 ~/ncd-watch/metrics/<bot>/net-stats.json（与 watch sync 路径一致）
//! 禁止把远端 bot 的陈旧本机文件当成 live 数据。

use ncd_domain::bot_config::BotConfig;
use ncd_domain::{BotId, BotRuntimeMetrics, MetricsHistoryPoint, ProbeHealth};
use ncd_host::HostPath;
use ncd_runtime::metrics::{
    history_path_for_bot, load_history, load_probe_stats_file, now_ms, parse_probe_stats_json,
    remote_metrics_history_posix, remote_metrics_stats_posix, stats_path_for_bot,
    BotRuntimeMetricsPrefs,
};
use tauri::State;

use crate::commands::components::cached_host_probe;
use crate::AppState;

fn metrics_prefs_from_settings(settings: &ncd_domain::AppSettings) -> BotRuntimeMetricsPrefs {
    let mut prefs = BotRuntimeMetricsPrefs::from_app(settings);
    prefs.normalize();
    prefs
}

fn metrics_from_stats_file(
    bot_id: BotId,
    data_root: &std::path::Path,
    prefs: &BotRuntimeMetricsPrefs,
    now: u64,
) -> BotRuntimeMetrics {
    let stats = stats_path_for_bot(data_root, bot_id.as_str());
    match load_probe_stats_file(&stats) {
        Ok(file) => file.into_metrics(bot_id, prefs.stale_after_ms(), now),
        Err(err) => {
            let mut m = BotRuntimeMetrics::unavailable(bot_id);
            m.probe = ProbeHealth::Error;
            m.probe_error = Some(err);
            m.collected_at_ms = now;
            m
        }
    }
}

fn metrics_from_bytes(
    bot_id: BotId,
    bytes: &[u8],
    prefs: &BotRuntimeMetricsPrefs,
    now: u64,
) -> BotRuntimeMetrics {
    let text = match std::str::from_utf8(bytes) {
        Ok(t) => t,
        Err(e) => {
            let mut m = BotRuntimeMetrics::unavailable(bot_id);
            m.probe = ProbeHealth::Error;
            m.probe_error = Some(format!("utf8: {e}"));
            m.collected_at_ms = now;
            return m;
        }
    };
    match parse_probe_stats_json(text) {
        Ok(file) => file.into_metrics(bot_id, prefs.stale_after_ms(), now),
        Err(err) => {
            let mut m = BotRuntimeMetrics::unavailable(bot_id);
            m.probe = ProbeHealth::Error;
            m.probe_error = Some(err);
            m.collected_at_ms = now;
            m
        }
    }
}

fn remote_unavailable(bot_id: BotId, msg: impl Into<String>, now: u64) -> BotRuntimeMetrics {
    let mut m = BotRuntimeMetrics::unavailable(bot_id);
    m.probe = ProbeHealth::Error;
    m.probe_error = Some(msg.into());
    m.collected_at_ms = now;
    m
}

async fn load_bot_config(state: &AppState, bot_id: &BotId) -> Option<BotConfig> {
    state.bot_manager.get_bot_config(bot_id).await.ok().flatten()
}

async fn resolve_remote_host_and_home(
    state: &AppState,
    server_id: &str,
) -> Result<(std::sync::Arc<dyn ncd_host::Host>, String), String> {
    // 读指标时尽量 ensure_connected，避免「隧道断了但 UI 仍显示未注入」误导
    let host = state
        .server_manager
        .ensure_connected(server_id)
        .await
        .map_err(|e| format!("远端连接失败：{e}"))?;
    let host_id = format!("remote:{server_id}");
    let probe = cached_host_probe(&host_id, host.as_ref(), state).await;
    let home = probe
        .home
        .filter(|h| !h.trim().is_empty())
        .ok_or_else(|| "无法解析远端 $HOME".to_string())?;
    Ok((host, home))
}

async fn fetch_one_metrics(
    state: &AppState,
    bot_id: BotId,
    prefs: &BotRuntimeMetricsPrefs,
    now: u64,
) -> BotRuntimeMetrics {
    if !prefs.enabled {
        return BotRuntimeMetrics::unavailable(bot_id);
    }

    let config = match load_bot_config(state, &bot_id).await {
        Some(c) => c,
        None => return metrics_from_stats_file(bot_id, &state.data_root, prefs, now),
    };

    match config.bot.runtime_target.server_id() {
        None => metrics_from_stats_file(bot_id, &state.data_root, prefs, now),
        Some(server_id) => {
            let (host, home) = match resolve_remote_host_and_home(state, server_id).await {
                Ok(v) => v,
                Err(e) => return remote_unavailable(bot_id, e, now),
            };
            let remote_path =
                HostPath::from_posix(remote_metrics_stats_posix(&home, bot_id.as_str()));
            match host.read_file(&remote_path).await {
                Ok(bytes) => metrics_from_bytes(bot_id, &bytes, prefs, now),
                Err(err) => {
                    // 无文件：未注入 / 探针未写出，不要回落本机陈旧文件
                    let mut m = BotRuntimeMetrics::unavailable(bot_id);
                    m.probe = ProbeHealth::NotInjected;
                    m.probe_error = Some(format!(
                        "远端无 net-stats（{path}）：{err}。请重启该 Bot（注入会把 env 写进 loadNapCat.js）",
                        path = remote_path.as_posix()
                    ));
                    m.collected_at_ms = now;
                    m
                }
            }
        }
    }
}

#[tauri::command]
pub async fn get_bot_runtime_metrics(
    state: State<'_, AppState>,
    bot_id: String,
) -> Result<BotRuntimeMetrics, String> {
    let id = BotId::new(bot_id);
    let settings = state.app_settings.read().await;
    let prefs = metrics_prefs_from_settings(&settings);
    drop(settings);
    Ok(fetch_one_metrics(&state, id, &prefs, now_ms()).await)
}

#[tauri::command]
pub async fn get_bot_runtime_metrics_history(
    state: State<'_, AppState>,
    bot_id: String,
    from_ms: Option<u64>,
    to_ms: Option<u64>,
) -> Result<Vec<MetricsHistoryPoint>, String> {
    let id = BotId::new(bot_id);
    let config = load_bot_config(&state, &id).await;

    let Some(server_id) = config
        .as_ref()
        .and_then(|c| c.bot.runtime_target.server_id().map(|s| s.to_string()))
    else {
        let path = history_path_for_bot(&state.data_root, id.as_str());
        return load_history(&path, from_ms, to_ms);
    };

    let (host, home) = resolve_remote_host_and_home(&state, &server_id).await?;
    let remote_path = HostPath::from_posix(remote_metrics_history_posix(&home, id.as_str()));
    let bytes = match host.read_file(&remote_path).await {
        Ok(b) => b,
        Err(_) => return Ok(Vec::new()),
    };
    let text = String::from_utf8_lossy(&bytes);
    let mut points = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(p) = serde_json::from_str::<MetricsHistoryPoint>(line) else {
            continue;
        };
        if from_ms.is_some_and(|from| p.at_ms < from) {
            continue;
        }
        if to_ms.is_some_and(|to| p.at_ms > to) {
            continue;
        }
        points.push(p);
    }
    Ok(points)
}

#[tauri::command]
pub async fn list_bot_runtime_metrics(
    state: State<'_, AppState>,
) -> Result<Vec<BotRuntimeMetrics>, String> {
    let settings = state.app_settings.read().await;
    let prefs = metrics_prefs_from_settings(&settings);
    drop(settings);
    if !prefs.enabled {
        return Ok(Vec::new());
    }
    let snaps = state.bot_manager.list_snapshots().await;
    let now = now_ms();
    let mut out = Vec::with_capacity(snaps.len());
    for s in snaps {
        out.push(fetch_one_metrics(&state, s.bot_id, &prefs, now).await);
    }
    Ok(out)
}
