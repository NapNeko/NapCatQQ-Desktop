//! SnowLuma 共享 node 的账号注册表。环境只标识主机，账号与设置通过独立文件热更新。

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use ncd_domain::bot_config::BotConfig;
use ncd_host::{Host, HostPath};
use serde::{Deserialize, Serialize};

use super::{
    BotRuntimeMetricsPrefs, build_node_map, ensure_probe_script, merge_node_options_require,
    nodes_map_path_for_bot, snowluma_registration_path, snowluma_registry_dir, stats_path_for_bot,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnowLumaMetricsRegistration {
    pub bot_id: String,
    pub account_id: String,
    pub nodes_path: String,
    pub stats_out: String,
    pub enabled: bool,
    pub interval_ms: u64,
}

pub fn snowluma_metrics_env(probe: &Path, registry: &Path) -> BTreeMap<String, String> {
    let mut env = BTreeMap::from([
        // 关闭统计时也预加载，后续只需热改账号注册，不用重启其它 Bot 的共享 node。
        ("NCD_METRICS_ENABLED".into(), "1".into()),
        (
            "NCD_METRICS_REGISTRY_DIR".into(),
            registry.to_string_lossy().replace('\\', "/"),
        ),
        (
            "NCD_METRICS_PROBE_PATH".into(),
            probe.to_string_lossy().replace('\\', "/"),
        ),
    ]);
    merge_node_options_require(&mut env, probe);
    env
}

pub fn prepare_snowluma_inject(
    data_root: &Path,
    bot_id: &str,
    config: &BotConfig,
    prefs: &BotRuntimeMetricsPrefs,
) -> Result<BTreeMap<String, String>, String> {
    let probe = ensure_probe_script(data_root)?;
    write_local_snowluma_registration(data_root, bot_id, config, prefs)?;
    Ok(snowluma_metrics_env(
        &probe,
        &snowluma_registry_dir(data_root),
    ))
}

pub fn write_local_snowluma_registration(
    data_root: &Path,
    bot_id: &str,
    config: &BotConfig,
    prefs: &BotRuntimeMetricsPrefs,
) -> Result<(), String> {
    let nodes_path = nodes_map_path_for_bot(data_root, bot_id);
    let nodes = serde_json::to_vec(&build_node_map(config)).map_err(|e| e.to_string())?;
    write_local_atomic(&nodes_path, &nodes)?;
    let registration = SnowLumaMetricsRegistration {
        bot_id: bot_id.to_string(),
        account_id: config.bot.qq_id.to_string(),
        nodes_path: nodes_path.to_string_lossy().replace('\\', "/"),
        stats_out: stats_path_for_bot(data_root, bot_id)
            .to_string_lossy()
            .replace('\\', "/"),
        enabled: prefs.enabled,
        interval_ms: prefs.interval_ms,
    };
    let body = serde_json::to_vec(&registration).map_err(|e| e.to_string())?;
    write_local_atomic(&snowluma_registration_path(data_root, bot_id), &body)
}

pub fn remote_snowluma_registry_posix(home: &str) -> String {
    format!(
        "{}/ncd-watch/metrics/snowluma-shared",
        home.trim_end_matches('/')
    )
}

pub async fn write_remote_snowluma_registration(
    host: &dyn Host,
    home: &str,
    bot_id: &str,
    config: &BotConfig,
    prefs: &BotRuntimeMetricsPrefs,
) -> Result<(), String> {
    let nodes_path = super::remote_metrics_nodes_posix(home, bot_id);
    let nodes = serde_json::to_vec(&build_node_map(config)).map_err(|e| e.to_string())?;
    write_remote_atomic(host, &HostPath::from_posix(&nodes_path), &nodes).await?;
    let registration = SnowLumaMetricsRegistration {
        bot_id: bot_id.to_string(),
        account_id: config.bot.qq_id.to_string(),
        nodes_path: HostPath::from_posix(nodes_path).render_for(host.os()),
        stats_out: HostPath::from_posix(super::remote_metrics_stats_posix(home, bot_id))
            .render_for(host.os()),
        enabled: prefs.enabled,
        interval_ms: prefs.interval_ms,
    };
    let path =
        HostPath::from_posix(remote_snowluma_registry_posix(home)).join(format!("{bot_id}.json"));
    let body = serde_json::to_vec(&registration).map_err(|e| e.to_string())?;
    write_remote_atomic(host, &path, &body).await
}

pub(super) fn write_local_atomic(path: &Path, body: &[u8]) -> Result<(), String> {
    if fs::read(path).is_ok_and(|current| current == body) {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("mkdir metrics: {e}"))?;
    }
    let temporary = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    fs::write(&temporary, body).map_err(|e| format!("write metrics: {e}"))?;
    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(format!("replace metrics: {error}"));
    }
    Ok(())
}

pub(super) async fn write_remote_atomic(
    host: &dyn Host,
    path: &HostPath,
    body: &[u8],
) -> Result<(), String> {
    if host
        .read_file(path)
        .await
        .is_ok_and(|current| current.as_ref() == body)
    {
        return Ok(());
    }
    let parent = path
        .parent()
        .ok_or_else(|| "metrics path has no parent".to_string())?;
    host.create_dir_all(&parent)
        .await
        .map_err(|e| format!("mkdir metrics: {e}"))?;
    let temporary = parent.join(format!(".{}.tmp", uuid::Uuid::new_v4()));
    host.write_file(&temporary, body)
        .await
        .map_err(|e| format!("write metrics: {e}"))?;
    if let Err(error) = host.rename(&temporary, path).await {
        let _ = host.remove_file(&temporary).await;
        return Err(format!("replace metrics: {error}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ncd_test_support::BotConfigBuilder;

    #[test]
    fn shared_snowluma_env_is_independent_of_bot_and_settings() {
        let root = tempfile::tempdir().unwrap();
        let a = BotConfigBuilder::new().qq_id(10001).build();
        let b = BotConfigBuilder::new().qq_id(10002).build();
        let disabled = BotRuntimeMetricsPrefs::default();
        let enabled = BotRuntimeMetricsPrefs {
            enabled: true,
            interval_ms: 1000,
            ..disabled.clone()
        };
        let first = prepare_snowluma_inject(root.path(), "10001", &a, &disabled).unwrap();
        let second = prepare_snowluma_inject(root.path(), "10002", &b, &enabled).unwrap();
        assert_eq!(first, second);
        assert!(!first.contains_key("NCD_METRICS_OUT"));
        assert!(!first.contains_key("NCD_METRICS_NODES_PATH"));
        let read = |id| {
            serde_json::from_slice::<SnowLumaMetricsRegistration>(
                &fs::read(snowluma_registration_path(root.path(), id)).unwrap(),
            )
            .unwrap()
        };
        assert!(!read("10001").enabled);
        assert_eq!(read("10002").account_id, "10002");
        assert!(read("10002").enabled);
        assert_eq!(read("10002").interval_ms, 1000);
        assert_ne!(read("10001").stats_out, read("10002").stats_out);
    }

    #[test]
    fn changing_one_registration_preserves_other_accounts() {
        let root = tempfile::tempdir().unwrap();
        let a = BotConfigBuilder::new().qq_id(10001).build();
        let b = BotConfigBuilder::new().qq_id(10002).build();
        let prefs = BotRuntimeMetricsPrefs {
            enabled: true,
            ..Default::default()
        };
        write_local_snowluma_registration(root.path(), "10001", &a, &prefs).unwrap();
        write_local_snowluma_registration(root.path(), "10002", &b, &prefs).unwrap();
        let b_before = fs::read(snowluma_registration_path(root.path(), "10002")).unwrap();
        write_local_snowluma_registration(
            root.path(),
            "10001",
            &a,
            &BotRuntimeMetricsPrefs::default(),
        )
        .unwrap();
        assert_eq!(
            fs::read(snowluma_registration_path(root.path(), "10002")).unwrap(),
            b_before
        );
        assert_eq!(
            fs::read_dir(snowluma_registry_dir(root.path()))
                .unwrap()
                .count(),
            2
        );
    }

    #[test]
    fn concurrent_bot_registrations_keep_both_accounts_and_complete_json() {
        let root = tempfile::tempdir().unwrap();
        let barrier = std::sync::Barrier::new(2);
        std::thread::scope(|scope| {
            for id in [10001, 10002] {
                let barrier = &barrier;
                let data_root = root.path();
                scope.spawn(move || {
                    let config = BotConfigBuilder::new().qq_id(id).build();
                    barrier.wait();
                    for interval_ms in [1000, 3000, 30_000] {
                        let prefs = BotRuntimeMetricsPrefs {
                            enabled: true,
                            interval_ms,
                            ..Default::default()
                        };
                        prepare_snowluma_inject(data_root, &id.to_string(), &config, &prefs)
                            .unwrap();
                    }
                });
            }
        });
        for id in ["10001", "10002"] {
            let registration: SnowLumaMetricsRegistration = serde_json::from_slice(
                &fs::read(snowluma_registration_path(root.path(), id)).unwrap(),
            )
            .unwrap();
            assert_eq!(registration.bot_id, id);
            assert_eq!(registration.interval_ms, 30_000);
            assert_eq!(fs::read_to_string(registration.nodes_path).unwrap(), "[]");
        }
        assert_eq!(
            fs::read_dir(snowluma_registry_dir(root.path()))
                .unwrap()
                .count(),
            2
        );
    }
}
