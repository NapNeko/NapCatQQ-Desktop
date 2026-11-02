//! 启动时的指标准备与远端节点映射刷新。

use super::*;

impl<R: BotConfigRepo + 'static, S: ConfigStore + 'static> BotManager<R, S> {
    /// 从 data_root 读 AppSettings 派生指标偏好（缺文件/解析失败 → 默认关）
    pub(super) fn load_metrics_prefs(
        data_root: &std::path::Path,
    ) -> crate::metrics::BotRuntimeMetricsPrefs {
        crate::metrics::BotRuntimeMetricsPrefs::from_data_root(data_root)
    }

    /// 准备一次探针资产，同时用于加载脚本和进程环境；文件 I/O 不占用异步执行器。
    pub(super) async fn apply_runtime_metrics_inject(
        &self,
        bot_id: &BotId,
        config: &BotConfig,
        runtime_config: &mut BotRuntimeConfig,
    ) {
        if !config.bot.runtime_target.is_local()
            || config.bot.deployment_type == DeploymentType::Docker
        {
            return;
        }
        let Some(data_root) = self.store.config_dir().parent().map(|p| p.to_path_buf()) else {
            return;
        };
        let config = config.clone();
        let id = bot_id.clone();
        let daemon = self.snowluma_daemon.clone();
        let prepared = tokio::task::spawn_blocking(move || {
            let prefs = Self::load_metrics_prefs(&data_root);
            if config.bot.backend_type == BackendType::SnowLuma {
                // 共享 SL node 吃主机级注册表，QQ 子进程不吃单账号探针环境。
                if let Some(daemon) = daemon {
                    let env = crate::metrics::prepare_snowluma_inject(
                        &data_root,
                        id.as_str(),
                        &config,
                        &prefs,
                    )?;
                    daemon.set_metrics_child_env(Some(env));
                }
                Ok(std::collections::BTreeMap::new())
            } else {
                prepare_napcat_metrics(&data_root, &id, &config, &prefs)
            }
        })
        .await;
        match prepared {
            Ok(Ok(environment)) => runtime_config.environment.extend(environment),
            Ok(Err(error)) => warn!(
                target: "ncd_runtime::bot_manager", bot_id = %bot_id, %error,
                "metrics inject prepare failed"
            ),
            Err(error) => warn!(
                target: "ncd_runtime::bot_manager", bot_id = %bot_id, %error,
                "metrics inject task failed"
            ),
        }
    }

    /// 远端：把最新 nodes.json 写到 ncd-watch metrics 目录（需 host 已连接）
    pub(super) async fn refresh_remote_metrics_nodes_map(
        &self,
        bot_id: &BotId,
        config: &BotConfig,
    ) {
        use ncd_host::HostPath;

        if config.bot.backend_type == BackendType::SnowLuma
            && config.bot.deployment_type == ncd_domain::DeploymentType::Native
        {
            self.refresh_snowluma_metrics_registration(config).await;
            return;
        }

        let Some(server_id) = config.bot.runtime_target.server_id() else {
            return;
        };
        let Some(resolver) = self.host_resolver.as_ref() else {
            return;
        };
        let host = match resolver.resolve(&config.bot.runtime_target).await {
            Ok(h) => h,
            Err(err) => {
                tracing::debug!(
                    target: "ncd_runtime::bot_manager",
                    bot_id = %bot_id,
                    %err,
                    "metrics remote nodes: host resolve skipped"
                );
                return;
            }
        };
        // 与 watch sync 一致：~/ncd-watch/metrics/<bot>/nodes.json
        let Ok(home) = crate::metrics::probe_remote_home(host.as_ref()).await else {
            return;
        };
        let posix = crate::metrics::remote_metrics_nodes_posix(&home, bot_id.as_str());
        let path = HostPath::from_posix(posix);
        if let Some(parent) = path.parent() {
            let _ = host.create_dir_all(&parent).await;
        }
        let nodes = crate::metrics::build_node_map(config);
        let body = match serde_json::to_vec(&nodes) {
            Ok(b) => b,
            Err(_) => return,
        };
        if let Err(err) = host.write_file(&path, &body).await {
            warn!(
                target: "ncd_runtime::bot_manager",
                bot_id = %bot_id,
                server_id,
                %err,
                "metrics remote nodes map write failed"
            );
        } else {
            info!(
                target: "ncd_runtime::bot_manager",
                bot_id = %bot_id,
                server_id,
                "metrics: refreshed remote nodes.json for hot map"
            );
        }
    }
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
