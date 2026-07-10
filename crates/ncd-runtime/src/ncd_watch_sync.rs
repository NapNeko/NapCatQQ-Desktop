//! 为远端 server 组装 ncd-watch 的 notify.json,并经 Host 写入

use ncd_domain::bot_config::{BackendType, BotConfig, DeploymentType};
use ncd_domain::kinds::RuntimeTarget;
use ncd_domain::{OfflineWebhookChannel, OfflineWebhookSettings};
use ncd_host::{Host, HostPath};
use ncd_watch::config::{
    DesktopPresentFile, NotifyBotTarget, NotifyConfig, WATCH_PROTOCOL_V1, WatchPaths,
};

/// 从本机 Bot 列表筛出落在 `server_id` 上的目标
pub fn bots_for_server<'a>(
    server_id: &str,
    bots: impl IntoIterator<Item = &'a BotConfig>,
) -> Vec<&'a BotConfig> {
    bots.into_iter()
        .filter(|c| c.bot.runtime_target.server_id() == Some(server_id))
        .collect()
}

pub fn bot_to_notify_target(config: &BotConfig) -> NotifyBotTarget {
    let qq = config.bot.qq_id;
    let backend = match config.bot.backend_type {
        BackendType::SnowLuma => "snowluma",
        BackendType::NapCat => "napcat",
    };
    let deployment = match config.bot.deployment_type {
        DeploymentType::Docker => "docker",
        DeploymentType::Native => "native",
    };
    NotifyBotTarget {
        bot_id: qq.to_string(),
        qq_id: qq,
        bot_name: config.bot.name.clone(),
        backend: backend.into(),
        deployment: deployment.into(),
        container_name: None,
        pid_file: None,
        // Native 无 pid 文件时用进程名弱匹配;Docker 走 container
        process_match: if matches!(config.bot.deployment_type, DeploymentType::Native) {
            Some("QQ".into())
        } else {
            None
        },
        enabled: true,
    }
}

pub fn build_notify_config(
    server_id: &str,
    bots: &[BotConfig],
    webhook: &OfflineWebhookSettings,
) -> NotifyConfig {
    let mut settings = webhook.clone();
    settings.normalize();
    let webhooks: Vec<OfflineWebhookChannel> = settings
        .effective_channels()
        .into_iter()
        .filter(|c| c.enabled && !c.url.trim().is_empty())
        .collect();
    let targets = bots_for_server(server_id, bots)
        .into_iter()
        .map(bot_to_notify_target)
        .collect();
    NotifyConfig {
        protocol: WATCH_PROTOCOL_V1,
        server_id: Some(server_id.to_string()),
        bots: targets,
        webhooks,
    }
}

/// 远端安装根(与 Component 一致):$HOME/ncd-watch
pub fn remote_watch_root(home: &str) -> HostPath {
    HostPath::from_posix(format!("{}/ncd-watch", home.trim_end_matches('/')))
}

pub async fn write_notify_json(
    host: &dyn Host,
    home: &str,
    notify: &NotifyConfig,
) -> Result<(), String> {
    let root = remote_watch_root(home);
    let config_dir = root.join("config");
    host.create_dir_all(&config_dir)
        .await
        .map_err(|e| e.to_string())?;
    let path = config_dir.join("notify.json");
    let body = serde_json::to_vec_pretty(notify).map_err(|e| e.to_string())?;
    host.write_file(&path, &body)
        .await
        .map_err(|e| e.to_string())?;
    // 尽量 chmod 600
    let _ = host
        .run_to_string(
            ncd_host::HostCommand::new("chmod")
                .arg("600")
                .arg(path.as_posix()),
        )
        .await;
    Ok(())
}

pub async fn write_desktop_present(
    host: &dyn Host,
    home: &str,
    desktop_version: Option<&str>,
) -> Result<(), String> {
    let root = remote_watch_root(home);
    let state_dir = root.join("state");
    host.create_dir_all(&state_dir)
        .await
        .map_err(|e| e.to_string())?;
    let path = state_dir.join("desktop_present");
    let body = DesktopPresentFile {
        updated_at_unix: chrono::Utc::now().timestamp(),
        desktop_version: desktop_version.map(|s| s.to_string()),
    };
    let bytes = serde_json::to_vec(&body).map_err(|e| e.to_string())?;
    host.write_file(&path, &bytes)
        .await
        .map_err(|e| e.to_string())
}

/// 本机路径布局(测试/文档)
pub fn local_paths_under(root: impl AsRef<std::path::Path>) -> WatchPaths {
    WatchPaths::from_root(root.as_ref())
}

/// 仅当 runtime_target 指向某 server 时返回 id
pub fn server_id_of(config: &BotConfig) -> Option<&str> {
    match &config.bot.runtime_target {
        RuntimeTarget::Server(id) => Some(id.as_str()),
        RuntimeTarget::Local => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ncd_domain::bot_config::{AdvancedConfig, BotBasicConfig, ConnectConfig};

    fn sample_remote(server: &str, qq: u64) -> BotConfig {
        BotConfig {
            bot: BotBasicConfig {
                name: format!("b{qq}"),
                qq_id: qq,
                music_sign_url: String::new(),
                auto_restart_schedule: Default::default(),
                offline_auto_restart: false,
                runtime_target: RuntimeTarget::server(server),
                backend_type: BackendType::NapCat,
                deployment_type: DeploymentType::Docker,
                snowluma_start_mode: None,
            },
            connect: ConnectConfig {
                http_servers: vec![],
                http_sse_servers: vec![],
                http_clients: vec![],
                websocket_servers: vec![],
                websocket_clients: vec![],
                plugins: vec![],
            },
            advanced: AdvancedConfig::default(),
            status_command: None,
        }
    }

    #[test]
    fn filters_server_bots() {
        let bots = vec![
            sample_remote("s1", 1),
            sample_remote("s2", 2),
            {
                let mut l = sample_remote("s1", 3);
                l.bot.runtime_target = RuntimeTarget::Local;
                l.bot.qq_id = 3;
                l
            },
        ];
        let got = bots_for_server("s1", &bots);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].bot.qq_id, 1);
    }

    #[test]
    fn build_notify_includes_webhook_channels() {
        let bots = vec![sample_remote("s1", 9)];
        let mut wh = OfflineWebhookSettings::default();
        wh.url = "https://example.invalid/h".into();
        let n = build_notify_config("s1", &bots, &wh);
        assert_eq!(n.bots.len(), 1);
        assert_eq!(n.bots[0].resolved_container_name(), "ncbot-9");
        assert!(!n.webhooks.is_empty());
    }
}

