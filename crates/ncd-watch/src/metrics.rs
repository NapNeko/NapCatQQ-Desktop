//! 远端脱管时续采 Bot 探针快照并写入 history.jsonl
//!
//! 不启停 Bot；失败不影响探活/告警主路径。

use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::config::WatchPaths;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct WatchMetricsConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_sample_interval")]
    pub sample_interval_ms: u32,
    #[serde(default = "default_retention_days")]
    pub retention_days: u32,
    #[serde(default = "default_history_min_interval")]
    pub history_min_interval_ms: u32,
    #[serde(default)]
    pub bots: Vec<WatchMetricsBot>,
}

fn default_sample_interval() -> u32 {
    3000
}

fn default_retention_days() -> u32 {
    7
}

fn default_history_min_interval() -> u32 {
    60_000
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct WatchMetricsBot {
    pub bot_id: String,
    #[serde(default)]
    pub qq_id: u64,
    pub stats_path: String,
    pub history_path: String,
}

impl WatchMetricsConfig {
    pub fn load_or_default(path: &Path) -> Self {
        if !path.is_file() {
            return Self::default();
        }
        match fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn clamp(mut self) -> Self {
        self.sample_interval_ms = self.sample_interval_ms.clamp(1000, 30_000);
        self.retention_days = self.retention_days.clamp(1, 90);
        if self.history_min_interval_ms < self.sample_interval_ms {
            self.history_min_interval_ms = self.sample_interval_ms.max(60_000);
        }
        self
    }
}

impl WatchPaths {
    pub fn metrics_json(&self) -> PathBuf {
        self.config_dir.join("metrics.json")
    }
}

#[derive(Debug, Default)]
pub struct MetricsRunState {
    pub last_history_at: std::collections::HashMap<String, u64>,
}

/// 读探针文件 → 若间隔足够则 append history → GC
pub fn sample_metrics_once(cfg: &WatchMetricsConfig, state: &mut MetricsRunState) {
    if !cfg.enabled {
        return;
    }
    let now = now_ms();
    let min_iv = u64::from(cfg.history_min_interval_ms.max(1));
    for bot in &cfg.bots {
        let stats = PathBuf::from(&bot.stats_path);
        if !stats.is_file() {
            continue;
        }
        let last = state.last_history_at.get(&bot.bot_id).copied().unwrap_or(0);
        if now.saturating_sub(last) < min_iv {
            continue;
        }
        let Ok(text) = fs::read_to_string(&stats) else {
            continue;
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        let point = history_point_from_probe_value(&v, now);
        let hist = PathBuf::from(&bot.history_path);
        if append_jsonl(&hist, &point).is_ok() {
            state.last_history_at.insert(bot.bot_id.clone(), now);
            let _ = prune_jsonl(&hist, cfg.retention_days, now);
        }
    }
}

fn history_point_from_probe_value(v: &serde_json::Value, now: u64) -> serde_json::Value {
    // 探针 collected_at 异常偏旧时用采样时刻，避免随后 GC 立刻删掉
    let at = v
        .get("collected_at_ms")
        .or_else(|| v.get("collectedAtMs"))
        .and_then(|x| x.as_u64())
        .filter(|&t| t > 0 && now.saturating_sub(t) < 86_400_000)
        .unwrap_or(now);
    let mut events = 0u64;
    let mut actions = 0u64;
    let mut bout = 0u64;
    let mut bin = 0u64;
    if let Some(nodes) = v.get("nodes").and_then(|x| x.as_array()) {
        for n in nodes {
            events += n
                .get("events_out")
                .or_else(|| n.get("eventsOut"))
                .and_then(|x| x.as_u64())
                .unwrap_or(0);
            actions += n
                .get("actions_in")
                .or_else(|| n.get("actionsIn"))
                .and_then(|x| x.as_u64())
                .unwrap_or(0);
            bout += n
                .get("bytes_out")
                .or_else(|| n.get("bytesOut"))
                .and_then(|x| x.as_u64())
                .unwrap_or(0);
            bin += n
                .get("bytes_in")
                .or_else(|| n.get("bytesIn"))
                .and_then(|x| x.as_u64())
                .unwrap_or(0);
        }
    }
    let rss = v
        .get("memory")
        .and_then(|m| m.get("rss_bytes").or_else(|| m.get("rssBytes")))
        .and_then(|x| x.as_u64());
    serde_json::json!({
        "v": 1,
        "at_ms": at,
        "memory": rss.map(|r| serde_json::json!({ "rss_bytes": r })),
        "nodes_summary": {
            "events_out_total": events,
            "actions_in_total": actions,
            "bytes_out_total": bout,
            "bytes_in_total": bin
        }
    })
}

fn append_jsonl(path: &Path, value: &serde_json::Value) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut f = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    let line = serde_json::to_string(value).map_err(|e| e.to_string())?;
    writeln!(f, "{line}").map_err(|e| e.to_string())
}

fn prune_jsonl(path: &Path, retention_days: u32, now_ms: u64) -> Result<(), String> {
    if !path.is_file() {
        return Ok(());
    }
    let cutoff = now_ms.saturating_sub(u64::from(retention_days.max(1)) * 86_400_000);
    let f = File::open(path).map_err(|e| e.to_string())?;
    let mut kept = Vec::new();
    for line in BufReader::new(f).lines() {
        let line = line.map_err(|e| e.to_string())?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let at = v.get("at_ms").and_then(|x| x.as_u64()).unwrap_or(0);
        if at >= cutoff {
            kept.push(line.to_string());
        }
    }
    const HARD_CAP: usize = 50_000;
    if kept.len() > HARD_CAP {
        kept = kept.split_off(kept.len() - HARD_CAP);
    }
    let tmp = path.with_extension("jsonl.tmp");
    {
        let mut out = File::create(&tmp).map_err(|e| e.to_string())?;
        for line in &kept {
            writeln!(out, "{line}").map_err(|e| e.to_string())?;
        }
    }
    fs::rename(&tmp, path).map_err(|e| e.to_string())
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn sample_appends_history() {
        let dir = std::env::temp_dir().join(format!(
            "ncd-watch-metrics-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let stats = dir.join("net-stats.json");
        let hist = dir.join("history.jsonl");
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        fs::write(
            &stats,
            format!(
                r#"{{"collectedAtMs":{now},"memory":{{"rssBytes":100}},"nodes":[{{"name":"a","kind":"wsServer","eventsOut":3,"actionsIn":1,"bytesOut":10,"bytesIn":2}}]}}"#
            ),
        )
        .unwrap();
        let cfg = WatchMetricsConfig {
            enabled: true,
            sample_interval_ms: 3000,
            retention_days: 7,
            history_min_interval_ms: 1,
            bots: vec![WatchMetricsBot {
                bot_id: "10001".into(),
                qq_id: 10001,
                stats_path: stats.to_string_lossy().into(),
                history_path: hist.to_string_lossy().into(),
            }],
        };
        let mut state = MetricsRunState::default();
        sample_metrics_once(&cfg, &mut state);
        let text = fs::read_to_string(&hist).unwrap();
        assert!(
            text.contains("events_out_total") || text.contains("\"events_out_total\""),
            "history={text}"
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
