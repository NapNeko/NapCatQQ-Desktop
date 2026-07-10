//! 为远端 server 组装 ncd-watch 的 notify.json,并经 Host 写入

use ncd_deploy::bot_docker_container_name;
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

/// 把 BotConfig 投影成 watch 探活目标。
///
/// Docker: 写入与 deploy 一致的容器名(ncbot-/slbot-),不靠 watch 侧猜前缀。
/// Native: 无稳定跨 HOME 的 pid 路径可写时,用 `-q <qq>$` 做 pgrep -f(对齐远端启停);
/// 不再用裸 `QQ`,避免同机多 Bot 全员命中。
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

    let (container_name, pid_file, process_match) = match config.bot.deployment_type {
        DeploymentType::Docker => (
            Some(bot_docker_container_name(config.bot.backend_type, qq)),
            None,
            None,
        ),
        DeploymentType::Native => {
            // SL 有 $HOME/snowluma-remote/.../pid_bot_<qq>,但 sync 时未必有 HOME;
            // 进程命令行带 -q <qq>,与 remote_qq_running_pid / NC xvfb 启动参数一致。
            // `$` 降低 `-q 1` 误匹配 `-q 12` 的概率(pgrep -f 按正则)。
            let process_match = if qq > 0 {
                Some(format!("-q {qq}$"))
            } else {
                None
            };
            (None, None, process_match)
        }
    };

    NotifyBotTarget {
        bot_id: qq.to_string(),
        qq_id: qq,
        bot_name: config.bot.name.clone(),
        backend: backend.into(),
        deployment: deployment.into(),
        container_name,
        pid_file,
        process_match,
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
        let bots = vec![sample_remote("s1", 1), sample_remote("s2", 2), {
            let mut l = sample_remote("s1", 3);
            l.bot.runtime_target = RuntimeTarget::Local;
            l.bot.qq_id = 3;
            l
        }];
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
        assert_eq!(n.bots[0].container_name.as_deref(), Some("ncbot-9"));
        assert_eq!(n.bots[0].resolved_container_name(), "ncbot-9");
        assert!(!n.webhooks.is_empty());
    }

    #[test]
    fn docker_targets_use_flavor_container_names() {
        let nc = sample_remote("s1", 10001);
        let mut sl = sample_remote("s1", 10002);
        sl.bot.backend_type = BackendType::SnowLuma;
        sl.bot.name = "sl".into();

        let t_nc = bot_to_notify_target(&nc);
        let t_sl = bot_to_notify_target(&sl);
        assert_eq!(t_nc.container_name.as_deref(), Some("ncbot-10001"));
        assert_eq!(t_sl.container_name.as_deref(), Some("slbot-10002"));
        assert!(t_nc.process_match.is_none());
        assert!(t_sl.process_match.is_none());
    }

    #[test]
    fn native_targets_use_per_qq_process_match() {
        let mut a = sample_remote("s1", 11);
        a.bot.deployment_type = DeploymentType::Native;
        let mut b = sample_remote("s1", 22);
        b.bot.deployment_type = DeploymentType::Native;
        b.bot.backend_type = BackendType::SnowLuma;

        let ta = bot_to_notify_target(&a);
        let tb = bot_to_notify_target(&b);
        assert_eq!(ta.process_match.as_deref(), Some("-q 11$"));
        assert_eq!(tb.process_match.as_deref(), Some("-q 22$"));
        assert_ne!(ta.process_match, tb.process_match);
        assert!(ta.container_name.is_none());
        assert!(tb.pid_file.is_none());
    }
}
