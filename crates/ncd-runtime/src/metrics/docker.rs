//! Docker bot 实例指标：宿主机资产 + compose overlay + NC load 覆盖文件
//!
//! 路径约定见 plan §4.4：
//! - 宿主机 `~/ncd-watch/metrics` bind → 容器 `/ncd-metrics`
//! - NC：额外 bind 覆盖镜像内 `/opt/QQ/resources/app/loadNapCat.js`
//! - SL：compose 注入 NODE_OPTIONS=--require（禁止对 NC 使用）

use std::collections::BTreeMap;
use std::path::Path;

use ncd_deploy::{DOCKER_NAPCAT_MJS_URI, DockerMetricsOverlay};
use ncd_domain::bot_config::BotConfig;
use ncd_domain::BotFlavor;
use ncd_host::{Host, HostPath};

use super::inject::{
    apply_metrics_env_vars, build_napcat_load_script_with_env, MetricsInjectPlan,
};
use super::{BotRuntimeMetricsPrefs, ensure_remote_metrics_assets};

/// 准备 Docker metrics：上传探针/nodes，写 NC load 覆盖，返回 compose overlay
pub async fn prepare_docker_metrics_overlay(
    host: &dyn Host,
    home: &str,
    bot_id: &str,
    config: &BotConfig,
    prefs: &BotRuntimeMetricsPrefs,
    local_data_root: &Path,
    project_dir: &str,
    flavor: BotFlavor,
) -> Result<Option<DockerMetricsOverlay>, String> {
    if !prefs.enabled {
        return Ok(None);
    }

    ensure_remote_metrics_assets(host, home, bot_id, config, prefs, local_data_root).await?;

    let host_metrics_root = format!("{}/ncd-watch/metrics", home.trim_end_matches('/'));

    let overlay = match flavor {
        BotFlavor::NapCat => {
            let load_host =
                write_docker_napcat_load_script(host, project_dir, bot_id, prefs.interval_ms)
                    .await?;
            DockerMetricsOverlay::for_napcat(
                &host_metrics_root,
                &load_host,
                bot_id,
                prefs.interval_ms,
            )
        }
        BotFlavor::SnowLuma => {
            DockerMetricsOverlay::for_snowluma(&host_metrics_root, bot_id, prefs.interval_ms)
        }
    };

    let _ = host
        .run_to_string(
            ncd_host::HostCommand::new("sh").arg("-c").arg(format!(
                "chown -R 1000:1000 {host_metrics_root} 2>/dev/null || true"
            )),
        )
        .await;

    Ok(Some(overlay))
}

async fn write_docker_napcat_load_script(
    host: &dyn Host,
    project_dir: &str,
    bot_id: &str,
    interval_ms: u64,
) -> Result<String, String> {
    let dir = format!("{}/ncd-metrics-load", project_dir.trim_end_matches('/'));
    host.create_dir_all(&HostPath::from_posix(&dir))
        .await
        .map_err(|e| format!("mkdir docker load dir: {e}"))?;

    let (out, nodes, probe) = DockerMetricsOverlay::container_paths(bot_id);
    let mut env = BTreeMap::new();
    let plan = MetricsInjectPlan {
        probe_script: std::path::PathBuf::from(&probe),
        stats_out: std::path::PathBuf::from(&out),
        nodes_path: std::path::PathBuf::from(&nodes),
        interval_ms,
    };
    apply_metrics_env_vars(&mut env, &plan);

    let script = build_napcat_load_script_with_env(
        DOCKER_NAPCAT_MJS_URI,
        Some(std::path::Path::new(&probe)),
        Some(&env),
    );
    let host_path = format!("{dir}/loadNapCat.js");
    host.write_file(&HostPath::from_posix(&host_path), script.as_bytes())
        .await
        .map_err(|e| format!("write docker loadNapCat.js: {e}"))?;

    let _ = host
        .run_to_string(
            ncd_host::HostCommand::new("chmod")
                .arg("644")
                .arg(&host_path),
        )
        .await;

    Ok(host_path)
}
