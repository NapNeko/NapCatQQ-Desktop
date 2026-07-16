//! 启动时指标注入：env + loadNapCat 前缀；探针资产落盘

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use ncd_domain::bot_config::BotConfig;
use ncd_domain::{MetricsNodeMapEntry, NetworkNodeKind};

use super::BotRuntimeMetricsPrefs;
use super::paths::{
    metrics_bot_dir, nodes_map_path_for_bot, probe_script_path, stats_path_for_bot,
};

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

/// 仅刷新节点映射（配置热更新路径）。探针进程会按 mtime 重读该文件。
pub fn write_nodes_map(
    data_root: &Path,
    bot_id: &str,
    config: &BotConfig,
) -> Result<PathBuf, String> {
    let bot_dir = metrics_bot_dir(data_root, bot_id);
    fs::create_dir_all(&bot_dir).map_err(|e| format!("mkdir bot metrics: {e}"))?;
    let nodes_path = nodes_map_path_for_bot(data_root, bot_id);
    let nodes = build_node_map(config);
    let json = serde_json::to_string(&nodes).map_err(|e| e.to_string())?;
    fs::write(&nodes_path, json).map_err(|e| format!("write nodes map: {e}"))?;
    Ok(nodes_path)
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
    let stats_out = stats_path_for_bot(data_root, bot_id);
    let nodes_path = write_nodes_map(data_root, bot_id, config)?;
    Ok(Some(MetricsInjectPlan {
        probe_script,
        stats_out,
        nodes_path,
        interval_ms: prefs.interval_ms,
    }))
}

/// 只写 NCD_METRICS_*（不写 NODE_OPTIONS）。
///
/// NapCat 必须用 loadNapCat.js 里「先写 process.env 再 require」；
/// 若同时设 NODE_OPTIONS=--require 探针，Electron 会在入口脚本之前预加载，
/// 此时 env 尚未写入 → 探针 disabled 并被 module cache 锁死，后续 require 无效。
pub fn apply_metrics_env_vars(env: &mut BTreeMap<String, String>, plan: &MetricsInjectPlan) {
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
}

/// 兼容旧名：等同 apply_metrics_env_vars（不再附带 NODE_OPTIONS）。
pub fn apply_metrics_to_environment(env: &mut BTreeMap<String, String>, plan: &MetricsInjectPlan) {
    apply_metrics_env_vars(env, plan);
}

/// SnowLuma / 无 load 脚本入口时：用 NODE_OPTIONS=--require 预加载探针。
/// 调用方须保证 NCD_METRICS_* 已在同一 environment 中（先于进程启动）。
///
/// Windows 注意两点，否则 node 在 preload 阶段 Function._load 直接崩：
/// 1. 路径含空格必须加引号（data_root 常为 `NapCatQQ Desktop`）
/// 2. 反斜杠会被 NODE_OPTIONS 解析当转义吃掉（`C:\P...` → `C:P...`），
///    必须改成正斜杠 `C:/ProgramData/...`
pub fn merge_node_options_require(env: &mut BTreeMap<String, String>, probe: &Path) {
    let probe_for_node = probe.to_string_lossy().replace('\\', "/");
    let require_arg = format!("--require \"{probe_for_node}\"");
    let key = "NODE_OPTIONS".to_string();
    match env.get(&key) {
        Some(existing) if !existing.is_empty() => {
            if existing.contains(probe_for_node.as_str()) {
                return;
            }
            env.insert(key, format!("{existing} {require_arg}"));
        }
        _ => {
            env.insert(key, require_arg);
        }
    }
}

/// NapCat loadNapCat.js：清 cache 后 require，避免 NODE_OPTIONS 预加载的 disabled 缓存
pub fn build_probe_load_prefix(probe: &Path) -> String {
    // Windows 路径转 file URL 不必要；CJS require 吃绝对路径
    let p = probe.to_string_lossy().replace('\\', "\\\\");
    format!("try {{ delete require.cache['{p}']; }} catch (_) {{}}\nrequire('{p}');\n")
}

/// 在 require 探针前写入 process.env（Electron/QQ 不一定继承 shell export）
pub fn build_metrics_env_prefix(env: &BTreeMap<String, String>) -> String {
    let mut s = String::new();
    // 只写 NCD_METRICS_*，避免污染无关变量
    for (k, v) in env {
        if !k.starts_with("NCD_METRICS_") {
            continue;
        }
        let key = js_single_quote(k);
        let val = js_single_quote(v);
        s.push_str(&format!("process.env['{key}'] = '{val}';\n"));
    }
    s
}

fn js_single_quote(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\'', "\\'")
}

pub fn build_napcat_load_script(napcat_mjs_uri: &str, probe: Option<&Path>) -> String {
    build_napcat_load_script_with_env(napcat_mjs_uri, probe, None)
}

/// 带 metrics env 的 load 脚本：env 写进 process.env 再 require，不依赖父进程环境
pub fn build_napcat_load_script_with_env(
    napcat_mjs_uri: &str,
    probe: Option<&Path>,
    metrics_env: Option<&BTreeMap<String, String>>,
) -> String {
    let mut s = String::new();
    if let Some(env) = metrics_env {
        s.push_str(&build_metrics_env_prefix(env));
    }
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
    use ncd_domain::MessagePostFormat;
    use ncd_domain::bot_config::{
        AdvancedConfig, AutoRestartSchedule, BackendType, BotBasicConfig, ConnectConfig,
        DeploymentType, HttpServerConfig, NetworkBaseFields,
    };
    use ncd_domain::kinds::RuntimeTarget;

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

    #[test]
    fn load_script_bakes_metrics_env_before_require() {
        let mut env = BTreeMap::new();
        env.insert("NCD_METRICS_ENABLED".into(), "1".into());
        env.insert(
            "NCD_METRICS_OUT".into(),
            "/home/u/ncd-watch/metrics/1/net-stats.json".into(),
        );
        env.insert("NODE_OPTIONS".into(), "--require /x".into()); // 不应写入 load 脚本
        let script = build_napcat_load_script_with_env(
            "file:///x/napcat.mjs",
            Some(Path::new("/home/u/ncd-watch/metrics/ncd-ob11-stats.cjs")),
            Some(&env),
        );
        assert!(script.contains("process.env['NCD_METRICS_ENABLED'] = '1'"));
        assert!(script.contains("NCD_METRICS_OUT"));
        assert!(script.contains("require('/home/u/ncd-watch/metrics/ncd-ob11-stats.cjs')"));
        assert!(script.contains("delete require.cache"));
        assert!(!script.contains("NODE_OPTIONS"));
        // env 必须在 require 之前
        let env_pos = script.find("process.env").unwrap();
        let req_pos = script.find("require('").unwrap();
        assert!(env_pos < req_pos);
    }

    #[test]
    fn apply_metrics_env_does_not_set_node_options() {
        let plan = MetricsInjectPlan {
            probe_script: PathBuf::from("/tmp/ncd-ob11-stats.cjs"),
            stats_out: PathBuf::from("/tmp/net-stats.json"),
            nodes_path: PathBuf::from("/tmp/nodes.json"),
            interval_ms: 3000,
        };
        let mut env = BTreeMap::new();
        apply_metrics_to_environment(&mut env, &plan);
        assert_eq!(
            env.get("NCD_METRICS_ENABLED").map(String::as_str),
            Some("1")
        );
        assert!(!env.contains_key("NODE_OPTIONS"));
        merge_node_options_require(&mut env, &plan.probe_script);
        assert!(
            env.get("NODE_OPTIONS")
                .is_some_and(|v| v.contains("ncd-ob11-stats"))
        );
    }

    #[test]
    fn merge_node_options_quotes_paths_with_spaces() {
        let probe = PathBuf::from(r"C:\ProgramData\NapCatQQ Desktop\metrics\ncd-ob11-stats.cjs");
        let mut env = BTreeMap::new();
        merge_node_options_require(&mut env, &probe);
        let opts = env.get("NODE_OPTIONS").expect("NODE_OPTIONS set");
        // 正斜杠 + 引号：避免反斜杠被 NODE_OPTIONS 当转义、空格被拆 argv
        assert_eq!(
            opts,
            r#"--require "C:/ProgramData/NapCatQQ Desktop/metrics/ncd-ob11-stats.cjs""#
        );
    }
}
