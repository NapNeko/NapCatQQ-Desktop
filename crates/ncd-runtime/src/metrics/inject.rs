//! 启动时指标注入：env + loadNapCat 前缀；探针资产落盘

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use ncd_domain::bot_config::BotConfig;
use ncd_domain::{MetricsNodeMapEntry, NetworkNodeKind};

use super::paths::{
    metrics_bot_dir, nodes_map_path_for_bot, probe_script_path, stats_path_for_bot,
};
use super::BotRuntimeMetricsPrefs;

// 与 src-tauri/resources/metrics/ncd-ob11-stats.cjs 同源；改脚本须两边一致。
const EMBEDDED_PROBE_CJS: &str =
    include_str!("../../../../src-tauri/resources/metrics/ncd-ob11-stats.cjs");

#[derive(Debug, Clone)]
pub struct MetricsInjectPlan {
    pub probe_script: PathBuf,
    pub stats_out: PathBuf,
    pub nodes_path: PathBuf,
    pub interval_ms: u64,
}

pub fn ensure_probe_script(data_root: &Path) -> Result<PathBuf, String> {
    let path = probe_script_path(data_root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("mkdir metrics: {e}"))?;
    }
    // 始终覆盖为当前 Desktop 版本探针，避免旧脚本残留
    fs::write(&path, EMBEDDED_PROBE_CJS).map_err(|e| format!("write probe script: {e}"))?;
    Ok(path)
}

pub fn build_node_map(config: &BotConfig) -> Vec<MetricsNodeMapEntry> {
    let mut out = Vec::new();
    for s in &config.connect.http_servers {
        out.push(MetricsNodeMapEntry {
            name: s.base.name.clone(),
            kind: NetworkNodeKind::HttpServer,
            listen_port: Some(s.port),
            target_url: None,
        });
    }
    for s in &config.connect.http_sse_servers {
        out.push(MetricsNodeMapEntry {
            name: s.base.name.clone(),
            kind: NetworkNodeKind::HttpSse,
            listen_port: Some(s.port),
            target_url: None,
        });
    }
    for s in &config.connect.http_clients {
        out.push(MetricsNodeMapEntry {
            name: s.base.name.clone(),
            kind: NetworkNodeKind::HttpClient,
            listen_port: None,
            target_url: Some(s.url.clone()),
        });
    }
    for s in &config.connect.websocket_servers {
        out.push(MetricsNodeMapEntry {
            name: s.base.name.clone(),
            kind: NetworkNodeKind::WsServer,
            listen_port: Some(s.port),
            target_url: None,
        });
    }
    for s in &config.connect.websocket_clients {
        out.push(MetricsNodeMapEntry {
            name: s.base.name.clone(),
            kind: NetworkNodeKind::WsClient,
            listen_port: None,
            target_url: Some(s.url.clone()),
        });
    }
    out
}

pub fn prepare_inject(
    data_root: &Path,
    bot_id: &str,
    config: &BotConfig,
    prefs: &BotRuntimeMetricsPrefs,
) -> Result<Option<MetricsInjectPlan>, String> {
    if !prefs.enabled {
        return Ok(None);
    }
    let probe_script = ensure_probe_script(data_root)?;
    let bot_dir = metrics_bot_dir(data_root, bot_id);
    fs::create_dir_all(&bot_dir).map_err(|e| format!("mkdir bot metrics: {e}"))?;
    let stats_out = stats_path_for_bot(data_root, bot_id);
    let nodes_path = nodes_map_path_for_bot(data_root, bot_id);
    let nodes = build_node_map(config);
    let json = serde_json::to_string(&nodes).map_err(|e| e.to_string())?;
    fs::write(&nodes_path, json).map_err(|e| format!("write nodes map: {e}"))?;
    Ok(Some(MetricsInjectPlan {
        probe_script,
        stats_out,
        nodes_path,
        interval_ms: prefs.interval_ms,
    }))
}

pub fn apply_metrics_to_environment(
    env: &mut BTreeMap<String, String>,
    plan: &MetricsInjectPlan,
) {
    env.insert("NCD_METRICS_ENABLED".into(), "1".into());
    env.insert(
        "NCD_METRICS_OUT".into(),
        plan.stats_out.to_string_lossy().into_owned(),
    );
    env.insert(
        "NCD_METRICS_INTERVAL_MS".into(),
        plan.interval_ms.to_string(),
    );
    env.insert(
        "NCD_METRICS_NODES_PATH".into(),
        plan.nodes_path.to_string_lossy().into_owned(),
    );
    merge_node_options_require(env, &plan.probe_script);
}

pub fn merge_node_options_require(env: &mut BTreeMap<String, String>, probe: &Path) {
    let require_arg = format!("--require {}", probe.display());
    let key = "NODE_OPTIONS".to_string();
    match env.get(&key) {
        Some(existing) if !existing.is_empty() => {
            if existing.contains(&probe.to_string_lossy().to_string()) {
                return;
            }
            env.insert(key, format!("{existing} {require_arg}"));
        }
        _ => {
            env.insert(key, require_arg);
        }
    }
}

/// NapCat loadNapCat.js：指标开时在 import 前 require 探针
pub fn build_probe_load_prefix(probe: &Path) -> String {
    // Windows 路径转 file URL 不必要；CJS require 吃绝对路径
    let p = probe.to_string_lossy().replace('\\', "\\\\");
    format!("require('{p}');\n")
}

pub fn build_napcat_load_script(napcat_mjs_uri: &str, probe: Option<&Path>) -> String {
    let mut s = String::new();
    if let Some(p) = probe {
        s.push_str(&build_probe_load_prefix(p));
    }
    s.push_str(&format!(
        "(async () => {{await import('{napcat_mjs_uri}')}})()"
    ));
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use ncd_domain::bot_config::{
        AdvancedConfig, AutoRestartSchedule, BackendType, BotBasicConfig, ConnectConfig,
        DeploymentType, HttpServerConfig, NetworkBaseFields,
    };
    use ncd_domain::kinds::RuntimeTarget;
    use ncd_domain::MessagePostFormat;

    fn sample_config() -> BotConfig {
        BotConfig {
            bot: BotBasicConfig {
                name: "t".into(),
                qq_id: 10001,
                music_sign_url: String::new(),
                auto_restart_schedule: AutoRestartSchedule::default(),
                offline_auto_restart: false,
                runtime_target: RuntimeTarget::Local,
                backend_type: BackendType::NapCat,
                deployment_type: DeploymentType::Native,
                snowluma_start_mode: None,
            },
            connect: ConnectConfig {
                http_servers: vec![HttpServerConfig {
                    base: NetworkBaseFields {
                        enable: true,
                        name: "http-1".into(),
                        message_post_format: MessagePostFormat::Array,
                        token: String::new(),
                        debug: false,
                    },
                    host: "127.0.0.1".into(),
                    port: 3000,
                    enable_cors: false,
                    enable_websocket: false,
                    path: "/".into(),
                }],
                ..Default::default()
            },
            advanced: AdvancedConfig::default(),
            status_command: None,
        }
    }

    #[test]
    fn node_map_from_config() {
        let m = build_node_map(&sample_config());
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].name, "http-1");
        assert_eq!(m[0].listen_port, Some(3000));
    }

    #[test]
    fn load_script_with_and_without_probe() {
        let plain = build_napcat_load_script("file:///x/napcat.mjs", None);
        assert!(plain.starts_with("(async"));
        assert!(!plain.contains("require("));
        let with = build_napcat_load_script(
            "file:///x/napcat.mjs",
            Some(Path::new(r"C:\data\metrics\ncd-ob11-stats.cjs")),
        );
        assert!(with.contains("require("));
        assert!(with.contains("napcat.mjs"));
    }
}
