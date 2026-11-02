//! 本机指标资产准备：仅负责文件与环境变量，不依赖 BotManager、Actor 或运行会话。

use std::collections::BTreeMap;
use std::path::PathBuf;

use ncd_domain::{BackendType, BotConfig, BotId};
use tracing::{info, warn};

use crate::metrics::{BotRuntimeMetricsPrefs, prepare_snowluma_inject};

/// 文件读取与探针落盘均在阻塞线程完成；调用方负责把环境应用到对应运行时。
pub(super) async fn prepare_local_metrics(
    data_root: PathBuf,
    bot_id: BotId,
    config: BotConfig,
) -> Result<BTreeMap<String, String>, String> {
    tokio::task::spawn_blocking(move || {
        let prefs = BotRuntimeMetricsPrefs::from_data_root(&data_root);
        match config.bot.backend_type {
            BackendType::SnowLuma => {
                prepare_snowluma_inject(&data_root, bot_id.as_str(), &config, &prefs)
            }
            BackendType::NapCat => prepare_napcat_metrics(&data_root, &bot_id, &config, &prefs),
        }
    })
    .await
    .map_err(|error| format!("metrics inject task failed: {error}"))?
}

/// 加载脚本和进程环境使用同一次准备得到的探针与节点映射。
fn prepare_napcat_metrics(
    data_root: &std::path::Path,
    bot_id: &BotId,
    config: &BotConfig,
    prefs: &crate::metrics::BotRuntimeMetricsPrefs,
) -> Result<std::collections::BTreeMap<String, String>, String> {
    use crate::metrics::{apply_metrics_to_environment, prepare_inject};

    let plan = prepare_inject(data_root, bot_id.as_str(), config, prefs)?;
    let mut environment = std::collections::BTreeMap::new();
    if let Some(plan) = &plan {
        apply_metrics_to_environment(&mut environment, plan);
    }
    rewrite_napcat_metrics_loader(data_root, bot_id, plan.as_ref(), &environment);
    if let Some(plan) = plan {
        environment.insert(
            "NCD_METRICS_PROBE_PATH".into(),
            plan.probe_script.to_string_lossy().into_owned(),
        );
    }
    Ok(environment)
}

fn rewrite_napcat_metrics_loader(
    data_root: &std::path::Path,
    bot_id: &BotId,
    plan: Option<&crate::metrics::MetricsInjectPlan>,
    environment: &std::collections::BTreeMap<String, String>,
) {
    let napcat_dir = crate::DataPaths::new(data_root).napcat_install_dir();
    let napcat_mjs = napcat_dir.join("napcat.mjs");
    if !napcat_mjs.is_file() {
        warn!(
            target: "ncd_runtime::bot_manager", bot_id = %bot_id,
            path = %napcat_mjs.display(),
            "metrics: napcat.mjs missing, skip loadNapCat rewrite"
        );
        return;
    }
    let uri = match crate::metrics::napcat_file_uri(&napcat_mjs) {
        Ok(uri) => uri,
        Err(error) => {
            warn!(%error, "NapCat 入口路径无法转换为文件 URL，保留已有加载脚本");
            return;
        }
    };
    let load_path = if cfg!(target_os = "linux") {
        data_root
            .join("components")
            .join(ncd_domain::remote_paths::REL_LOAD_NAPCAT_JS)
    } else {
        napcat_dir.join("loadNapCat.js")
    };
    // WinBoot 注入的 QQ 不一定继承进程 environment，加载脚本也必须包含同一份 env。
    // 调用方在 build_plan 后执行，避免启动计划覆盖探针前缀。
    let script = crate::metrics::build_napcat_load_script_with_env(
        &uri,
        plan.map(|p| p.probe_script.as_path()),
        plan.map(|_| environment),
    );
    if let Err(error) = std::fs::write(&load_path, script) {
        warn!(
            target: "ncd_runtime::bot_manager", bot_id = %bot_id, %error,
            "rewrite loadNapCat.js for metrics failed"
        );
    } else if plan.is_some() {
        info!(
            target: "ncd_runtime::bot_manager", bot_id = %bot_id,
            path = %load_path.display(),
            "metrics: rewrote loadNapCat.js with env + probe require"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::BotRuntimeMetricsPrefs;
    use ncd_test_support::BotConfigBuilder;

    #[test]
    fn loader_and_environment_share_metrics_assets_and_disable_cleanly() {
        let root = tempfile::tempdir().unwrap();
        let napcat_dir = crate::DataPaths::new(root.path()).napcat_install_dir();
        std::fs::create_dir_all(&napcat_dir).unwrap();
        std::fs::write(napcat_dir.join("napcat.mjs"), "export {};").unwrap();
        let loader = if cfg!(target_os = "linux") {
            root.path()
                .join("components")
                .join(ncd_domain::remote_paths::REL_LOAD_NAPCAT_JS)
        } else {
            napcat_dir.join("loadNapCat.js")
        };
        std::fs::create_dir_all(loader.parent().unwrap()).unwrap();
        let config = BotConfigBuilder::new().qq_id(10001).build();
        let bot_id = BotId::new("10001");
        let prefs = BotRuntimeMetricsPrefs {
            enabled: true,
            interval_ms: 2345,
            ..Default::default()
        };
        let environment = prepare_napcat_metrics(root.path(), &bot_id, &config, &prefs).unwrap();
        let script = std::fs::read_to_string(&loader).unwrap();
        assert_eq!(environment["NCD_METRICS_INTERVAL_MS"], "2345");
        assert!(script.contains("process.env['NCD_METRICS_INTERVAL_MS'] = '2345'"));
        for key in ["NCD_METRICS_PROBE_PATH", "NCD_METRICS_NODES_PATH"] {
            assert!(std::path::Path::new(&environment[key]).is_file());
            assert!(script.contains(&ncd_deploy::napcat_loader::js_string(&environment[key])));
        }

        let disabled = prepare_napcat_metrics(
            root.path(),
            &bot_id,
            &config,
            &BotRuntimeMetricsPrefs::default(),
        )
        .unwrap();
        assert!(disabled.is_empty());
        let script = std::fs::read_to_string(&loader).unwrap();
        assert!(!script.contains("NCD_METRICS_"));
        assert!(script.contains("napcat.mjs"));
    }
}
