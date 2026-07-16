//! Bot 运行时指标：内存 + OneBot 网络节点收发快照 / 历史点
//!
//! 探针写出 JSON 与本模块字段 rename 对齐；Desktop 与 ncd-watch 共用解析。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::ids::BotId;

pub fn default_bot_runtime_metrics_interval_ms() -> u64 {
    3000
}

pub const BOT_RUNTIME_METRICS_INTERVAL_MIN_MS: u64 = 1000;
pub const BOT_RUNTIME_METRICS_INTERVAL_MAX_MS: u64 = 30_000;

pub fn clamp_bot_runtime_metrics_interval_ms(raw: u64) -> u64 {
    raw.clamp(
        BOT_RUNTIME_METRICS_INTERVAL_MIN_MS,
        BOT_RUNTIME_METRICS_INTERVAL_MAX_MS,
    )
}

pub fn default_bot_runtime_metrics_retention_days() -> u32 {
    7
}

pub const BOT_RUNTIME_METRICS_RETENTION_MIN_DAYS: u32 = 1;
pub const BOT_RUNTIME_METRICS_RETENTION_MAX_DAYS: u32 = 90;

pub fn clamp_bot_runtime_metrics_retention_days(raw: u32) -> u32 {
    raw.clamp(
        BOT_RUNTIME_METRICS_RETENTION_MIN_DAYS,
        BOT_RUNTIME_METRICS_RETENTION_MAX_DAYS,
    )
}

/// 历史落盘最小间隔：与采样间隔取 max，默认至少 60s
pub fn history_min_interval_ms(sample_interval_ms: u64) -> u64 {
    sample_interval_ms.max(60_000)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum MetricsSource {
    #[default]
    Unavailable,
    Probe,
    WebUi,
    Hybrid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum ProbeHealth {
    #[default]
    NotInjected,
    Active,
    Stale,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum NetworkNodeKind {
    #[default]
    Unknown,
    HttpServer,
    HttpClient,
    HttpSse,
    WsServer,
    WsClient,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, Default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MemoryMetrics {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub rss_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub heap_used_bytes: Option<u64>,
    /// 主机物理内存总量（主机侧采样，非 WebUI）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub host_total_bytes: Option<u64>,
    /// 主机已用物理内存
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub host_used_bytes: Option<u64>,
    /// 主机 CPU 占用 0–100（主机侧；与进程无关）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub host_cpu_percent: Option<f64>,
    /// 主机系统盘/根分区总量（主机侧）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub host_disk_total_bytes: Option<u64>,
    /// 主机系统盘/根分区已用
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub host_disk_used_bytes: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, Default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct NetworkNodeMetrics {
    pub name: String,
    pub kind: NetworkNodeKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub detail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub events_out: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub actions_in: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub bytes_out: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub bytes_in: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub errors: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub last_activity_at_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, Default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct NodesRollup {
    #[serde(default)]
    #[ts(type = "number")]
    pub events_out_total: u64,
    #[serde(default)]
    #[ts(type = "number")]
    pub actions_in_total: u64,
    #[serde(default)]
    #[ts(type = "number")]
    pub bytes_out_total: u64,
    #[serde(default)]
    #[ts(type = "number")]
    pub bytes_in_total: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub nodes: Option<Vec<NetworkNodeMetrics>>,
}

impl NodesRollup {
    pub fn from_nodes(nodes: &[NetworkNodeMetrics]) -> Self {
        let mut rollup = Self {
            nodes: Some(nodes.to_vec()),
            ..Self::default()
        };
        for n in nodes {
            rollup.events_out_total = rollup
                .events_out_total
                .saturating_add(n.events_out.unwrap_or(0));
            rollup.actions_in_total = rollup
                .actions_in_total
                .saturating_add(n.actions_in.unwrap_or(0));
            rollup.bytes_out_total = rollup
                .bytes_out_total
                .saturating_add(n.bytes_out.unwrap_or(0));
            rollup.bytes_in_total = rollup
                .bytes_in_total
                .saturating_add(n.bytes_in.unwrap_or(0));
        }
        rollup
    }

    pub fn without_nodes(mut self) -> Self {
        self.nodes = None;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct BotRuntimeMetrics {
    /// R14 envelope
    #[serde(default = "metrics_envelope_v")]
    #[ts(type = "number")]
    pub v: u32,
    #[ts(type = "string")]
    pub bot_id: BotId,
    #[serde(default)]
    #[ts(type = "number")]
    pub collected_at_ms: u64,
    #[serde(default)]
    pub source: MetricsSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub memory: Option<MemoryMetrics>,
    #[serde(default)]
    pub nodes: Vec<NetworkNodeMetrics>,
    #[serde(default)]
    pub probe: ProbeHealth,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub probe_error: Option<String>,
}

fn metrics_envelope_v() -> u32 {
    1
}

impl BotRuntimeMetrics {
    pub fn unavailable(bot_id: impl Into<BotId>) -> Self {
        Self {
            v: 1,
            bot_id: bot_id.into(),
            collected_at_ms: 0,
            source: MetricsSource::Unavailable,
            memory: None,
            nodes: Vec::new(),
            probe: ProbeHealth::NotInjected,
            probe_error: None,
        }
    }

    pub fn rollup(&self) -> NodesRollup {
        NodesRollup::from_nodes(&self.nodes).without_nodes()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MetricsHistoryPoint {
    #[serde(default = "metrics_envelope_v")]
    #[ts(type = "number")]
    pub v: u32,
    #[serde(default)]
    #[ts(type = "number")]
    pub at_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub memory: Option<MemoryMetrics>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub nodes_summary: Option<NodesRollup>,
}

impl MetricsHistoryPoint {
    pub fn from_snapshot(snap: &BotRuntimeMetrics) -> Self {
        Self {
            v: 1,
            at_ms: snap.collected_at_ms,
            memory: snap.memory.clone().map(|m| MemoryMetrics {
                rss_bytes: m.rss_bytes,
                heap_used_bytes: None,
                host_total_bytes: m.host_total_bytes,
                host_used_bytes: m.host_used_bytes,
                host_cpu_percent: None,
                host_disk_total_bytes: m.host_disk_total_bytes,
                host_disk_used_bytes: m.host_disk_used_bytes,
            }),
            nodes_summary: Some(snap.rollup()),
        }
    }
}

/// 探针写出的当前快照（磁盘形态，字段 camelCase 与 JS 对齐）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProbeStatsFile {
    #[serde(default)]
    pub collected_at_ms: u64,
    #[serde(default)]
    pub memory: Option<MemoryMetrics>,
    #[serde(default)]
    pub nodes: Vec<NetworkNodeMetrics>,
}

impl ProbeStatsFile {
    pub fn into_metrics(
        self,
        bot_id: BotId,
        stale_after_ms: u64,
        now_ms: u64,
    ) -> BotRuntimeMetrics {
        let age = now_ms.saturating_sub(self.collected_at_ms);
        let probe = if self.collected_at_ms == 0 {
            ProbeHealth::NotInjected
        } else if age > stale_after_ms {
            ProbeHealth::Stale
        } else {
            ProbeHealth::Active
        };
        let has_mem = self.memory.is_some();
        let has_nodes = !self.nodes.is_empty();
        let source = match (has_mem || has_nodes, probe) {
            (false, _) => MetricsSource::Unavailable,
            (true, ProbeHealth::Active | ProbeHealth::Stale) => MetricsSource::Probe,
            _ => MetricsSource::Unavailable,
        };
        BotRuntimeMetrics {
            v: 1,
            bot_id,
            collected_at_ms: self.collected_at_ms,
            source,
            memory: self.memory,
            nodes: self.nodes,
            probe,
            probe_error: None,
        }
    }
}

/// 启动时写入探针的节点映射条目
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricsNodeMapEntry {
    pub name: String,
    pub kind: NetworkNodeKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub listen_port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_url: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_interval_and_retention() {
        assert_eq!(clamp_bot_runtime_metrics_interval_ms(100), 1000);
        assert_eq!(clamp_bot_runtime_metrics_interval_ms(50_000), 30_000);
        assert_eq!(clamp_bot_runtime_metrics_retention_days(0), 1);
        assert_eq!(clamp_bot_runtime_metrics_retention_days(100), 90);
        assert_eq!(history_min_interval_ms(3000), 60_000);
        assert_eq!(history_min_interval_ms(120_000), 120_000);
    }

    #[test]
    fn rollup_sums_nodes() {
        let nodes = vec![
            NetworkNodeMetrics {
                name: "a".into(),
                kind: NetworkNodeKind::WsServer,
                events_out: Some(10),
                actions_in: Some(2),
                bytes_out: Some(100),
                bytes_in: Some(20),
                ..Default::default()
            },
            NetworkNodeMetrics {
                name: "b".into(),
                kind: NetworkNodeKind::HttpClient,
                events_out: Some(5),
                actions_in: Some(1),
                bytes_out: Some(50),
                bytes_in: Some(10),
                ..Default::default()
            },
        ];
        let r = NodesRollup::from_nodes(&nodes).without_nodes();
        assert_eq!(r.events_out_total, 15);
        assert_eq!(r.actions_in_total, 3);
        assert_eq!(r.bytes_out_total, 150);
        assert_eq!(r.bytes_in_total, 30);
        assert!(r.nodes.is_none());
    }

    #[test]
    fn probe_file_stale_detection() {
        let file = ProbeStatsFile {
            collected_at_ms: 1000,
            memory: Some(MemoryMetrics {
                rss_bytes: Some(1024),
                ..Default::default()
            }),
            nodes: vec![],
        };
        let m = file
            .clone()
            .into_metrics(BotId::new("10001"), 9000, 1000 + 10_000);
        assert_eq!(m.probe, ProbeHealth::Stale);
        let m2 = file.into_metrics(BotId::new("10001"), 9000, 1000 + 1000);
        assert_eq!(m2.probe, ProbeHealth::Active);
    }
}
