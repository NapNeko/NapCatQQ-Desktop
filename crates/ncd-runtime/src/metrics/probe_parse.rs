use std::fs;
use std::path::Path;

use ncd_domain::{
    MemoryMetrics, NetworkNodeKind, NetworkNodeMetrics, ProbeStatsFile,
};
use serde_json::Value;

pub fn load_probe_stats_file(path: &Path) -> Result<ProbeStatsFile, String> {
    if !path.is_file() {
        return Ok(ProbeStatsFile::default());
    }
    let text = fs::read_to_string(path).map_err(|e| format!("read stats: {e}"))?;
    parse_probe_stats_json(&text)
}

pub fn parse_probe_stats_json(text: &str) -> Result<ProbeStatsFile, String> {
    let v: Value = serde_json::from_str(text).map_err(|e| format!("parse stats json: {e}"))?;
    let collected_at_ms = v
        .get("collected_at_ms")
        .or_else(|| v.get("collectedAtMs"))
        .and_then(|x| x.as_u64())
        .unwrap_or(0);

    let memory = v.get("memory").and_then(|m| {
        if m.is_null() {
            return None;
        }
        Some(MemoryMetrics {
            rss_bytes: m
                .get("rss_bytes")
                .or_else(|| m.get("rssBytes"))
                .and_then(|x| x.as_u64()),
            heap_used_bytes: m
                .get("heap_used_bytes")
                .or_else(|| m.get("heapUsedBytes"))
                .and_then(|x| x.as_u64()),
            host_total_bytes: m
                .get("host_total_bytes")
                .or_else(|| m.get("hostTotalBytes"))
                .and_then(|x| x.as_u64()),
            host_used_bytes: m
                .get("host_used_bytes")
                .or_else(|| m.get("hostUsedBytes"))
                .and_then(|x| x.as_u64()),
        })
    });

    let mut nodes = Vec::new();
    if let Some(arr) = v.get("nodes").and_then(|x| x.as_array()) {
        for n in arr {
            let name = n
                .get("name")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            if name.is_empty() {
                continue;
            }
            let kind = parse_kind(
                n.get("kind")
                    .and_then(|x| x.as_str())
                    .unwrap_or("unknown"),
            );
            nodes.push(NetworkNodeMetrics {
                name,
                kind,
                status: n
                    .get("status")
                    .and_then(|x| x.as_str())
                    .map(|s| s.to_string()),
                detail: n
                    .get("detail")
                    .and_then(|x| x.as_str())
                    .map(|s| s.to_string()),
                events_out: n
                    .get("events_out")
                    .or_else(|| n.get("eventsOut"))
                    .and_then(|x| x.as_u64()),
                actions_in: n
                    .get("actions_in")
                    .or_else(|| n.get("actionsIn"))
                    .and_then(|x| x.as_u64()),
                bytes_out: n
                    .get("bytes_out")
                    .or_else(|| n.get("bytesOut"))
                    .and_then(|x| x.as_u64()),
                bytes_in: n
                    .get("bytes_in")
                    .or_else(|| n.get("bytesIn"))
                    .and_then(|x| x.as_u64()),
                errors: n.get("errors").and_then(|x| x.as_u64()),
                last_activity_at_ms: n
                    .get("last_activity_at_ms")
                    .or_else(|| n.get("lastActivityAtMs"))
                    .and_then(|x| x.as_u64()),
            });
        }
    }

    Ok(ProbeStatsFile {
        collected_at_ms,
        memory,
        nodes,
    })
}

fn parse_kind(s: &str) -> NetworkNodeKind {
    match s {
        "httpServer" | "HttpServer" | "http_server" => NetworkNodeKind::HttpServer,
        "httpClient" | "HttpClient" | "http_client" => NetworkNodeKind::HttpClient,
        "httpSse" | "HttpSse" | "http_sse" => NetworkNodeKind::HttpSse,
        "wsServer" | "WsServer" | "ws_server" => NetworkNodeKind::WsServer,
        "wsClient" | "WsClient" | "ws_client" => NetworkNodeKind::WsClient,
        _ => NetworkNodeKind::Unknown,
    }
}
