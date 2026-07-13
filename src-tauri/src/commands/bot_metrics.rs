//! Bot 运行时指标 IPC（薄壳）

use ncd_domain::{BotId, BotRuntimeMetrics, MetricsHistoryPoint, ProbeHealth};
use ncd_runtime::metrics::{
    history_path_for_bot, load_history, load_probe_stats_file, now_ms, stats_path_for_bot,
    BotRuntimeMetricsPrefs,
};
use tauri::State;

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

#[tauri::command]
pub async fn get_bot_runtime_metrics(
    state: State<'_, AppState>,
    bot_id: String,
) -> Result<BotRuntimeMetrics, String> {
    let id = BotId::new(bot_id);
    let settings = state.app_settings.read().await;
    let prefs = metrics_prefs_from_settings(&settings);
    if !prefs.enabled {
        return Ok(BotRuntimeMetrics::unavailable(id));
    }
    Ok(metrics_from_stats_file(
        id,
        &state.data_root,
        &prefs,
        now_ms(),
    ))
}

#[tauri::command]
pub async fn get_bot_runtime_metrics_history(
    state: State<'_, AppState>,
    bot_id: String,
    from_ms: Option<u64>,
    to_ms: Option<u64>,
) -> Result<Vec<MetricsHistoryPoint>, String> {
    let id = BotId::new(bot_id);
    let path = history_path_for_bot(&state.data_root, id.as_str());
    load_history(&path, from_ms, to_ms)
}

#[tauri::command]
pub async fn list_bot_runtime_metrics(
    state: State<'_, AppState>,
) -> Result<Vec<BotRuntimeMetrics>, String> {
    let settings = state.app_settings.read().await;
    let prefs = metrics_prefs_from_settings(&settings);
    if !prefs.enabled {
        return Ok(Vec::new());
    }
    let snaps = state.bot_manager.list_snapshots().await;
    let now = now_ms();
    let mut out = Vec::with_capacity(snaps.len());
    for s in snaps {
        out.push(metrics_from_stats_file(
            s.bot_id,
            &state.data_root,
            &prefs,
            now,
        ));
    }
    Ok(out)
}
