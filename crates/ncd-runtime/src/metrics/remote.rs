//! 远端主机上的指标探针资产与启动注入
//!
//! 路径与 ncd-watch 对齐：
//!   ~/ncd-watch/metrics/ncd-ob11-stats.cjs
//!   ~/ncd-watch/metrics/<bot>/nodes.json
//!   ~/ncd-watch/metrics/<bot>/net-stats.json
//!
//! NapCat：改写远端 loadNapCat.js（bake NCD_* 到 process.env 再 require 探针）。
//! 不要对 NC 使用 NODE_OPTIONS=--require：会在 load 脚本写 env 前预加载并 cache 成 disabled。
//! SnowLuma：无 load 入口时才合并 NODE_OPTIONS=--require 与 NCD_*。

use std::collections::BTreeMap;

use ncd_domain::bot_config::BotConfig;
use ncd_host::{Host, HostCommand, HostPath};

use super::inject::{
    apply_metrics_to_environment, build_napcat_load_script_with_env, build_node_map,
    ensure_probe_script, MetricsInjectPlan,
};
use super::{
    remote_metrics_nodes_posix, remote_metrics_stats_posix, BotRuntimeMetricsPrefs,
};

const REMOTE_PROBE_NAME: &str = "ncd-ob11-stats.cjs";

#[derive(Debug, Clone)]
pub struct RemoteMetricsPaths {
    pub home: String,
    pub probe_script: String,
    pub stats_out: String,
    pub nodes_path: String,
    pub interval_ms: u64,
}

impl RemoteMetricsPaths {
    pub fn to_inject_plan(&self) -> MetricsInjectPlan {
        MetricsInjectPlan {
            probe_script: std::path::PathBuf::from(&self.probe_script),
            stats_out: std::path::PathBuf::from(&self.stats_out),
            nodes_path: std::path::PathBuf::from(&self.nodes_path),
            interval_ms: self.interval_ms,
        }
    }

    /// 远端 NC 启动 shell 的 metrics env。
    /// 故意不带 NODE_OPTIONS=--require：预加载会在 loadNapCat 写 env 前跑探针并 cache 成 disabled。
    pub fn env_map(&self) -> BTreeMap<String, String> {
        let mut env = BTreeMap::new();
        apply_metrics_to_environment(&mut env, &self.to_inject_plan());
        env.insert(
            "NCD_METRICS_PROBE_PATH".into(),
            self.probe_script.clone(),
        );
        // 双保险：绝不把对本探针的 --require 放进 shell env
        if env
            .get("NODE_OPTIONS")
            .is_some_and(|no| no.contains("ncd-ob11-stats") || no.contains(&self.probe_script))
        {
            env.remove("NODE_OPTIONS");
        }
        env
    }
}

/// 探测远端 $HOME（单次 shell）
pub async fn probe_remote_home(host: &dyn Host) -> Result<String, String> {
    let out = host
        .run_to_string(HostCommand::new("sh").arg("-c").arg("echo $HOME"))
        .await
        .map_err(|e| format!("echo $HOME: {e}"))?;
    if !out.success() {
        return Err(format!(
            "echo $HOME failed: exit={:?} {}",
            out.exit_code,
            out.stderr.trim()
        ));
    }
    let home = out.stdout.lines().next().unwrap_or("").trim().to_string();
    if home.is_empty() {
        return Err("empty $HOME".into());
    }
    Ok(home)
}

/// 上传探针脚本 + 写 nodes.json + 确保 bot metrics 目录
///
/// `local_data_root` 用于从本机 embed 写出探针再 upload（与 ensure_probe_script 同源）。
pub async fn ensure_remote_metrics_assets(
    host: &dyn Host,
    home: &str,
    bot_id: &str,
    config: &BotConfig,
    prefs: &BotRuntimeMetricsPrefs,
    local_data_root: &std::path::Path,
) -> Result<RemoteMetricsPaths, String> {
    if !prefs.enabled {
        return Err("metrics disabled".into());
    }

    let metrics_root = format!("{}/ncd-watch/metrics", home.trim_end_matches('/'));
    let bot_dir = format!("{metrics_root}/{bot_id}");
    host.create_dir_all(&HostPath::from_posix(&bot_dir))
        .await
        .map_err(|e| format!("mkdir remote metrics: {e}"))?;

    // 本机写出当前版本探针，再 upload（避免手写两份脚本）
    let local_probe =
        ensure_probe_script(local_data_root).map_err(|e| format!("local probe: {e}"))?;
    let remote_probe = format!("{metrics_root}/{REMOTE_PROBE_NAME}");
    let remote_probe_path = HostPath::from_posix(&remote_probe);
    host.upload(&local_probe, &remote_probe_path)
        .await
        .map_err(|e| format!("upload probe: {e}"))?;

    let nodes_path = remote_metrics_nodes_posix(home, bot_id);
    let nodes = build_node_map(config);
    let nodes_json = serde_json::to_vec(&nodes).map_err(|e| e.to_string())?;
    host.write_file(&HostPath::from_posix(&nodes_path), &nodes_json)
        .await
        .map_err(|e| format!("write remote nodes: {e}"))?;

    let stats_out = remote_metrics_stats_posix(home, bot_id);

    Ok(RemoteMetricsPaths {
        home: home.to_string(),
        probe_script: remote_probe,
        stats_out,
        nodes_path,
        interval_ms: prefs.interval_ms,
    })
}

/// 改写远端 loadNapCat.js：写 process.env + require(探针) + import napcat.mjs
///
/// 必须把 NCD_METRICS_* 写进脚本：Electron/QQ 子进程不一定继承 shell export。
pub async fn rewrite_remote_napcat_load_script(
    host: &dyn Host,
    load_script_path: &str,
    napcat_mjs_posix: &str,
    probe_script_posix: Option<&str>,
    metrics_env: Option<&BTreeMap<String, String>>,
) -> Result<(), String> {
    let uri = if napcat_mjs_posix.starts_with("file:") {
        napcat_mjs_posix.to_string()
    } else {
        format!("file://{napcat_mjs_posix}")
    };
    let probe = probe_script_posix.map(std::path::Path::new);
    let script = build_napcat_load_script_with_env(&uri, probe, metrics_env);
    let path = HostPath::from_posix(load_script_path);
    host.write_file(&path, script.as_bytes())
        .await
        .map_err(|e| format!("write loadNapCat.js: {e}"))?;
    Ok(())
}

/// 把 metrics env 合并进已有 environment（不覆盖无关键）
pub fn merge_metrics_env(
    env: &mut BTreeMap<String, String>,
    paths: &RemoteMetricsPaths,
) {
    let m = paths.env_map();
    for (k, v) in m {
        env.insert(k, v);
    }
}

/// 供 RemoteNativeLaunchTranslator 调用的注入器（实现放在 ncd-runtime，避免 backend 依赖 metrics 细节）
pub struct RuntimeRemoteMetricsInjector {
    local_data_root: std::path::PathBuf,
    prefs: BotRuntimeMetricsPrefs,
}

impl RuntimeRemoteMetricsInjector {
    pub fn new(local_data_root: impl Into<std::path::PathBuf>, prefs: BotRuntimeMetricsPrefs) -> Self {
        Self {
            local_data_root: local_data_root.into(),
            prefs,
        }
    }

    pub fn prefs(&self) -> &BotRuntimeMetricsPrefs {
        &self.prefs
    }
}

#[async_trait::async_trait]
impl ncd_backend_napcat::remote_native_launch::RemoteMetricsInjector
    for RuntimeRemoteMetricsInjector
{
    async fn prepare_napcat(
        &self,
        host: &dyn Host,
        home: &str,
        bot_id: &str,
        config: &BotConfig,
        install_base: &HostPath,
    ) -> Option<BTreeMap<String, String>> {
        if !self.prefs.enabled {
            return None;
        }
        let paths = match ensure_remote_metrics_assets(
            host,
            home,
            bot_id,
            config,
            &self.prefs,
            &self.local_data_root,
        )
        .await
        {
            Ok(p) => p,
            Err(err) => {
                tracing::warn!(
                    target: "ncd_runtime::metrics",
                    bot_id,
                    %err,
                    "remote metrics assets failed (start continues without probe)"
                );
                return None;
            }
        };

        // 与 NapCatComponent 路径一致
        let base = install_base.as_posix().trim_end_matches('/');
        let load_js = format!("{base}/opt/QQ/resources/app/loadNapCat.js");
        let napcat_mjs =
            format!("{base}/opt/QQ/resources/app/app_launcher/napcat/napcat.mjs");

        let env = paths.env_map();
        if let Err(err) = rewrite_remote_napcat_load_script(
            host,
            &load_js,
            &napcat_mjs,
            Some(&paths.probe_script),
            Some(&env),
        )
        .await
        {
            tracing::warn!(
                target: "ncd_runtime::metrics",
                bot_id,
                %err,
                "remote loadNapCat rewrite failed (start continues)"
            );
            // 仍返回 env：shell export 作兜底（无 NODE_OPTIONS）
        } else {
            // 回读校验：确认 bake 真的落盘（权限/路径写错时 write 可能写到别处）
            let verify = host
                .read_file(&HostPath::from_posix(&load_js))
                .await
                .ok()
                .and_then(|b| String::from_utf8(b.to_vec()).ok())
                .unwrap_or_default();
            let has_env = verify.contains("NCD_METRICS_ENABLED");
            let has_require = verify.contains("ncd-ob11-stats");
            if has_env && has_require {
                tracing::info!(
                    target: "ncd_runtime::metrics",
                    bot_id,
                    probe = %paths.probe_script,
                    stats = %paths.stats_out,
                    load_js = %load_js,
                    "remote metrics: probe assets + loadNapCat ready (env baked into load script)"
                );
            } else {
                tracing::warn!(
                    target: "ncd_runtime::metrics",
                    bot_id,
                    load_js = %load_js,
                    has_env,
                    has_require,
                    preview = %verify.chars().take(160).collect::<String>(),
                    "remote loadNapCat rewrite wrote but content missing metrics bake"
                );
            }
        }

        Some(env)
    }
}
