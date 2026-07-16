//! Bot 运行时指标：路径、探针注入、历史落盘、采集

mod docker;
mod history;
mod inject;
mod paths;
mod probe_parse;
mod remote;

pub use docker::prepare_docker_metrics_overlay;
pub use history::{append_history_point, load_history, prune_history_file};
pub use inject::{
    MetricsInjectPlan, apply_metrics_env_vars, apply_metrics_to_environment,
    build_metrics_env_prefix, build_napcat_load_script, build_napcat_load_script_with_env,
    build_node_map, build_probe_load_prefix, ensure_probe_script, merge_node_options_require,
    prepare_inject, write_nodes_map,
};
pub use paths::{
    history_path_for_bot, metrics_bot_dir, metrics_root, nodes_map_path_for_bot, probe_script_path,
    stats_path_for_bot,
};
pub use probe_parse::load_probe_stats_file;
pub use remote::{
    RemoteMetricsPaths, RuntimeRemoteMetricsInjector, ensure_remote_metrics_assets,
    merge_metrics_env, probe_remote_home, rewrite_remote_napcat_load_script,
};

/// 远端 ncd-watch 侧 metrics 路径（与 watch/sync::build_watch_metrics_config 一致）
pub fn remote_metrics_stats_posix(home: &str, bot_id: &str) -> String {
    let root = home.trim_end_matches('/');
    format!("{root}/ncd-watch/metrics/{bot_id}/net-stats.json")
}

pub fn remote_metrics_history_posix(home: &str, bot_id: &str) -> String {
    let root = home.trim_end_matches('/');
    format!("{root}/ncd-watch/metrics/{bot_id}/history.jsonl")
}

pub fn remote_metrics_nodes_posix(home: &str, bot_id: &str) -> String {
    let root = home.trim_end_matches('/');
    format!("{root}/ncd-watch/metrics/{bot_id}/nodes.json")
}

/// 远端整机资源快照（ncd-watch 写；非 per-bot）
pub fn remote_metrics_host_stats_posix(home: &str) -> String {
    let root = home.trim_end_matches('/');
    format!("{root}/ncd-watch/metrics/host-stats.json")
}

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use ncd_domain::{
    BotId, BotRuntimeMetrics, MetricsHistoryPoint, MetricsSource, ProbeHealth, ProbeStatsFile,
    history_min_interval_ms,
};
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct BotRuntimeMetricsPrefs {
    pub enabled: bool,
    pub interval_ms: u64,
    pub retention_days: u32,
}

impl Default for BotRuntimeMetricsPrefs {
    fn default() -> Self {
        Self {
            enabled: false,
            interval_ms: ncd_domain::default_bot_runtime_metrics_interval_ms(),
            retention_days: ncd_domain::default_bot_runtime_metrics_retention_days(),
        }
    }
}

impl BotRuntimeMetricsPrefs {
    pub fn from_app(settings: &ncd_domain::AppSettings) -> Self {
        Self {
            enabled: settings.bot_runtime_metrics_enabled,
            interval_ms: settings.bot_runtime_metrics_interval_ms,
            retention_days: settings.bot_runtime_metrics_retention_days,
        }
    }

    pub fn normalize(&mut self) {
        self.interval_ms = ncd_domain::clamp_bot_runtime_metrics_interval_ms(self.interval_ms);
        self.retention_days =
            ncd_domain::clamp_bot_runtime_metrics_retention_days(self.retention_days);
    }

    pub fn history_min_interval_ms(&self) -> u64 {
        history_min_interval_ms(self.interval_ms)
    }

    pub fn stale_after_ms(&self) -> u64 {
        self.interval_ms.saturating_mul(3).max(3000)
    }
}

#[derive(Debug, Default)]
struct CollectorState {
    cache: HashMap<String, BotRuntimeMetrics>,
    last_history_at: HashMap<String, u64>,
}

/// 本机指标采集缓存（远端历史由 ncd-watch 写）
#[derive(Clone)]
pub struct MetricsCollector {
    data_root: PathBuf,
    prefs: Arc<RwLock<BotRuntimeMetricsPrefs>>,
    state: Arc<RwLock<CollectorState>>,
}

impl MetricsCollector {
    pub fn new(data_root: impl Into<PathBuf>, prefs: Arc<RwLock<BotRuntimeMetricsPrefs>>) -> Self {
        Self {
            data_root: data_root.into(),
            prefs,
            state: Arc::new(RwLock::new(CollectorState::default())),
        }
    }

    pub fn prefs(&self) -> Arc<RwLock<BotRuntimeMetricsPrefs>> {
        Arc::clone(&self.prefs)
    }

    pub fn data_root(&self) -> &Path {
        &self.data_root
    }

    pub async fn get_cached(&self, bot_id: &BotId) -> BotRuntimeMetrics {
        let state = self.state.read().await;
        state
            .cache
            .get(bot_id.as_str())
            .cloned()
            .unwrap_or_else(|| BotRuntimeMetrics::unavailable(bot_id.clone()))
    }

    pub async fn list_cached(&self) -> Vec<BotRuntimeMetrics> {
        let state = self.state.read().await;
        state.cache.values().cloned().collect()
    }

    /// 读本机探针文件并更新缓存；可选写历史
    pub async fn refresh_local_bot(&self, bot_id: &BotId) -> BotRuntimeMetrics {
        let prefs = self.prefs.read().await.clone();
        if !prefs.enabled {
            let m = BotRuntimeMetrics::unavailable(bot_id.clone());
            let mut state = self.state.write().await;
            state.cache.insert(bot_id.as_str().to_string(), m.clone());
            return m;
        }

        let stats_path = stats_path_for_bot(&self.data_root, bot_id.as_str());
        let now = now_ms();
        let mut metrics = match load_probe_stats_file(&stats_path) {
            Ok(file) => file.into_metrics(bot_id.clone(), prefs.stale_after_ms(), now),
            Err(err) => {
                let mut m = BotRuntimeMetrics::unavailable(bot_id.clone());
                m.probe = ProbeHealth::Error;
                m.probe_error = Some(err);
                m.collected_at_ms = now;
                m
            }
        };

        // 无文件时 NotInjected
        if !stats_path.is_file() && metrics.probe != ProbeHealth::Error {
            metrics.probe = ProbeHealth::NotInjected;
            metrics.source = MetricsSource::Unavailable;
        }

        {
            let mut state = self.state.write().await;
            let key = bot_id.as_str().to_string();
            let last = state.last_history_at.get(&key).copied().unwrap_or(0);
            let min_iv = prefs.history_min_interval_ms();
            if prefs.enabled
                && metrics.probe == ProbeHealth::Active
                && now.saturating_sub(last) >= min_iv
            {
                let hist = history_path_for_bot(&self.data_root, bot_id.as_str());
                let point = MetricsHistoryPoint::from_snapshot(&metrics);
                if append_history_point(&hist, &point).is_ok() {
                    state.last_history_at.insert(key.clone(), now);
                    let _ = prune_history_file(&hist, prefs.retention_days, now);
                }
            }
            state.cache.insert(key, metrics.clone());
        }

        metrics
    }

    pub async fn history(
        &self,
        bot_id: &BotId,
        from_ms: Option<u64>,
        to_ms: Option<u64>,
    ) -> Vec<MetricsHistoryPoint> {
        let path = history_path_for_bot(&self.data_root, bot_id.as_str());
        load_history(&path, from_ms, to_ms).unwrap_or_default()
    }
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 从探针 JSON 宽松解析（兼容 camelCase / snake_case 混用）
pub fn parse_probe_stats_json(text: &str) -> Result<ProbeStatsFile, String> {
    probe_parse::parse_probe_stats_json(text)
}
