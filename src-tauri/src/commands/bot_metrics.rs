//! Bot 运行时指标 IPC（薄壳）
//!
//! 本机：MetricsCollector 读 net-stats + system_metrics 合并主机内存/CPU
//! 远端：SSH 读 net-stats + host-stats.json（ncd-watch 写）
//! 禁止把远端 bot 的陈旧本机文件当成 live 数据。

use ncd_domain::bot_config::BotConfig;
use ncd_domain::{BotId, BotRuntimeMetrics, MetricsHistoryPoint, ProbeHealth};
use ncd_host::HostPath;
use ncd_runtime::metrics::{
    BotRuntimeMetricsPrefs, history_path_for_bot, load_history, now_ms, parse_probe_stats_json,
    remote_metrics_history_posix, remote_metrics_host_stats_posix, remote_metrics_stats_posix,
};
use tauri::{AppHandle, Manager, State};

use crate::AppState;
use crate::commands::components::cached_host_probe;
use crate::commands::system_metrics;

fn metrics_prefs_from_settings(settings: &ncd_domain::AppSettings) -> BotRuntimeMetricsPrefs {
    let mut prefs = BotRuntimeMetricsPrefs::from_app(settings);
    prefs.normalize();
    prefs
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

struct HostMerge {
    mem_total: Option<u64>,
    mem_used: Option<u64>,
    cpu: Option<f64>,
    disk_total: Option<u64>,
    disk_used: Option<u64>,
}

/// 把主机侧采样合并进 memory（不覆盖探针进程 RSS/堆）
fn merge_host_into_metrics(mut m: BotRuntimeMetrics, h: HostMerge) -> BotRuntimeMetrics {
    if h.mem_total.is_none()
        && h.mem_used.is_none()
        && h.cpu.is_none()
        && h.disk_total.is_none()
        && h.disk_used.is_none()
    {
        return m;
    }
    let mut mem = m.memory.unwrap_or_default();
    if mem.host_total_bytes.is_none() {
        mem.host_total_bytes = h.mem_total;
    }
    if mem.host_used_bytes.is_none() {
        mem.host_used_bytes = h.mem_used;
    }
    if mem.host_cpu_percent.is_none() {
        mem.host_cpu_percent = h.cpu;
    }
    if mem.host_disk_total_bytes.is_none() {
        mem.host_disk_total_bytes = h.disk_total;
    }
    if mem.host_disk_used_bytes.is_none() {
        mem.host_disk_used_bytes = h.disk_used;
    }
    m.memory = Some(mem);
    m
}

fn merge_local_host(m: BotRuntimeMetrics) -> BotRuntimeMetrics {
    // force=true：列表/轮询路径不 sleep 暖机
    let Ok(s) = system_metrics::sample_host_resources(true) else {
        return m;
    };
    merge_host_into_metrics(
        m,
        HostMerge {
            mem_total: Some(s.total_memory_bytes).filter(|&v| v > 0),
            mem_used: Some(s.used_memory_bytes).filter(|&v| v > 0),
            cpu: Some(s.cpu_percent),
            disk_total: Some(s.disk_total_bytes).filter(|&v| v > 0),
            disk_used: Some(s.disk_used_bytes).filter(|&v| v > 0),
        },
    )
}

fn parse_host_stats_json(text: &str) -> HostMerge {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(text) else {
        return HostMerge {
            mem_total: None,
            mem_used: None,
            cpu: None,
            disk_total: None,
            disk_used: None,
        };
    };
    let mem = v.get("memory");
    let disk = v.get("disk");
    let nz = |x: Option<u64>| x.filter(|&n| n > 0);
    HostMerge {
        mem_total: nz(
            mem.and_then(|m| m.get("totalBytes").or_else(|| m.get("total_bytes")))
                .and_then(|x| x.as_u64()),
        ),
        mem_used: nz(
            mem.and_then(|m| m.get("usedBytes").or_else(|| m.get("used_bytes")))
                .and_then(|x| x.as_u64()),
        ),
        cpu: v
            .get("cpuPercent")
            .or_else(|| v.get("cpu_percent"))
            .and_then(|x| x.as_f64())
            .filter(|p| p.is_finite() && *p >= 0.0),
        disk_total: nz(
            disk.and_then(|d| d.get("totalBytes").or_else(|| d.get("total_bytes")))
                .and_then(|x| x.as_u64()),
        ),
        disk_used: nz(
            disk.and_then(|d| d.get("usedBytes").or_else(|| d.get("used_bytes")))
                .and_then(|x| x.as_u64()),
        ),
    }
}

async fn merge_remote_host(
    host: &dyn ncd_host::Host,
    home: &str,
    m: BotRuntimeMetrics,
) -> BotRuntimeMetrics {
    let path = HostPath::from_posix(remote_metrics_host_stats_posix(home));
    let Ok(bytes) = host.read_file(&path).await else {
        return m;
    };
    let text = String::from_utf8_lossy(&bytes);
    merge_host_into_metrics(m, parse_host_stats_json(&text))
}

fn remote_unavailable(bot_id: BotId, msg: impl Into<String>, now: u64) -> BotRuntimeMetrics {
    let mut m = BotRuntimeMetrics::unavailable(bot_id);
    m.probe = ProbeHealth::Error;
    m.probe_error = Some(msg.into());
    m.collected_at_ms = now;
    m
}

async fn load_bot_config(state: &AppState, bot_id: &BotId) -> Option<BotConfig> {
    state
        .bot_manager
        .get_bot_config(bot_id)
        .await
        .ok()
        .flatten()
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

/// 远端无 net-stats 时尝试读同目录 .err，把探针写失败原因带回 UI
async fn remote_probe_error_hint(
    host: &dyn ncd_host::Host,
    stats_posix: &str,
    read_err: &impl std::fmt::Display,
) -> String {
    let err_path = HostPath::from_posix(format!("{stats_posix}.err"));
    match host.read_file(&err_path).await {
        Ok(bytes) => {
            let text = String::from_utf8_lossy(&bytes);
            let brief = text.lines().next().unwrap_or("").trim();
            if brief.is_empty() {
                format!(
                    "远端无 net-stats（{stats_posix}）：{read_err}。请重启该 Bot（注入会把 env 写进 loadNapCat.js）"
                )
            } else {
                format!("远端探针写失败（{stats_posix}.err）：{brief}")
            }
        }
        Err(_) => format!(
            "远端无 net-stats（{stats_posix}）：{read_err}。请重启该 Bot（注入会把 env 写进 loadNapCat.js）"
        ),
    }
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
        None => {
            let m = state.metrics_collector.refresh_local_bot(&bot_id).await;
            return merge_local_host(m);
        }
    };

    match config.bot.runtime_target.server_id() {
        None => {
            let m = state.metrics_collector.refresh_local_bot(&bot_id).await;
            merge_local_host(m)
        }
        Some(server_id) => {
            let (host, home) = match resolve_remote_host_and_home(state, server_id).await {
                Ok(v) => v,
                Err(e) => return remote_unavailable(bot_id, e, now),
            };
            let remote_posix = remote_metrics_stats_posix(&home, bot_id.as_str());
            let remote_path = HostPath::from_posix(&remote_posix);
            let m = match host.read_file(&remote_path).await {
                Ok(bytes) => metrics_from_bytes(bot_id, &bytes, prefs, now),
                Err(err) => {
                    let hint = remote_probe_error_hint(host.as_ref(), &remote_posix, &err).await;
                    let mut m = BotRuntimeMetrics::unavailable(bot_id);
                    m.probe = ProbeHealth::NotInjected;
                    m.probe_error = Some(hint);
                    m.collected_at_ms = now;
                    m
                }
            };
            merge_remote_host(host.as_ref(), &home, m).await
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

/// 后台周期刷新本机 bot 探针并写 history（即使 UI 未打开列表）
///
/// 间隔取 max(设置 interval, 2s)；关指标时仍 tick 但 refresh 会短路。
/// 远端 bot 跳过：history 由 ncd-watch 写。
pub fn spawn_local_metrics_collector(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(3));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        // 跳过第一次立刻 tick，等 bootstrap 就绪
        interval.tick().await;
        loop {
            interval.tick().await;
            let Some(state) = app.try_state::<AppState>() else {
                continue;
            };
            let prefs = state.metrics_collector.prefs().read().await.clone();
            if !prefs.enabled {
                continue;
            }
            let sleep_ms = prefs.interval_ms.max(2000);
            let snaps = state.bot_manager.list_snapshots().await;
            for s in snaps {
                let config = match state.bot_manager.get_bot_config(&s.bot_id).await {
                    Ok(Some(c)) => c,
                    _ => continue,
                };
                if config.bot.runtime_target.server_id().is_some() {
                    continue;
                }
                let _ = state.metrics_collector.refresh_local_bot(&s.bot_id).await;
            }
            // 下一轮间隔：用用户设置（已 tick 过 3s 基准时再补睡差额）
            if sleep_ms > 3000 {
                tokio::time::sleep(std::time::Duration::from_millis(sleep_ms - 3000)).await;
            }
        }
    });
}
