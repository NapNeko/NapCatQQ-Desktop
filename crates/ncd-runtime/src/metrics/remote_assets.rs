//! 单次 SSH 同步固定指标资产，仅原子替换内容变化的文件。

use std::time::Duration;

use ncd_domain::BotConfig;
use ncd_host::{Host, HostCommand, HostPath, Os};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{
    BotRuntimeMetricsPrefs, REMOTE_PROBE_NAME, RemoteMetricsPaths, build_node_map,
    ensure_probe_script, remote_metrics_nodes_posix, remote_metrics_stats_posix,
};

const SYNC_ASSETS: &str = include_str!("remote_assets_batch.py");

#[derive(Serialize, Deserialize)]
struct Asset {
    path: String,
    body: String,
    sha256: String,
}

#[derive(Serialize, Deserialize)]
struct Request {
    v: u32,
    home: String,
    bot_id: String,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Reply {
    v: u32,
    checked: usize,
    changed: usize,
}

pub(super) async fn current_snowluma_assets(
    host: &dyn Host,
    home: &str,
    bot_id: &str,
    config: &BotConfig,
    prefs: &BotRuntimeMetricsPrefs,
    local_data_root: &std::path::Path,
) -> Option<RemoteMetricsPaths> {
    if host.os() != Os::Linux {
        return None;
    }
    let probe = ensure_probe_script(local_data_root).ok()?;
    let probe = std::fs::read(probe).ok()?;
    let probe_path = format!(
        "{}/ncd-watch/metrics/{REMOTE_PROBE_NAME}",
        home.trim_end_matches('/')
    );
    let nodes_path = remote_metrics_nodes_posix(home, bot_id);
    let nodes = serde_json::to_vec(&build_node_map(config)).ok()?;
    let (registration_path, registration) =
        super::super::shared::remote_snowluma_registration_asset(
            host.os(),
            home,
            bot_id,
            config,
            prefs,
        )
        .ok()?;
    let assets = [
        (probe_path.clone(), probe),
        (nodes_path.clone(), nodes),
        (registration_path.as_posix().to_owned(), registration),
    ];
    let root = format!("{}/ncd-watch/metrics/", home.trim_end_matches('/'));
    let request = Request {
        v: 1,
        home: home.to_owned(),
        bot_id: bot_id.to_owned(),
        assets: assets
            .into_iter()
            .map(|(path, body)| {
                Some(Asset {
                    path: path.strip_prefix(&root)?.to_owned(),
                    sha256: hex::encode(Sha256::digest(&body)),
                    body: String::from_utf8(body).ok()?,
                })
            })
            .collect::<Option<Vec<_>>>()?,
    };
    let command = HostCommand::new("python3")
        .args(["-I", "-B", "-c", SYNC_ASSETS])
        .stdin(serde_json::to_vec(&request).ok()?)
        .timeout(Duration::from_secs(10));
    let output = tokio::time::timeout(Duration::from_secs(10), host.run_to_string(command))
        .await
        .ok()?
        .ok()?;
    if !output.success() || output.stdout.len() > 4096 {
        return None;
    }
    let reply: Reply = serde_json::from_str(&output.stdout).ok()?;
    if reply.v != 1 || reply.checked != 3 || reply.changed > 3 {
        return None;
    }
    Some(RemoteMetricsPaths {
        home: home.to_owned(),
        probe_script: HostPath::from_posix(probe_path).render_for(host.os()),
        stats_out: HostPath::from_posix(remote_metrics_stats_posix(home, bot_id))
            .render_for(host.os()),
        nodes_path: HostPath::from_posix(nodes_path).render_for(host.os()),
        interval_ms: prefs.interval_ms,
    })
}

#[cfg(test)]
#[path = "remote_assets_tests.rs"]
mod tests;
