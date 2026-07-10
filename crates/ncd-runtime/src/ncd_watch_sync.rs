//! 为远端 server 组装 ncd-watch 的 notify.json,并经 Host 写入
//!
//! 对齐 Desktop 离线通知子集: Webhook + Email + 同机 OneBot + NC 登录探活凭据。

use ncd_deploy::bot_docker_container_name;
use ncd_domain::bot_config::{BackendType, BotConfig, DeploymentType};
use ncd_domain::docker::DockerDeploySpec;
use ncd_domain::kinds::RuntimeTarget;
use ncd_domain::{
    OfflineEmailSettings, OfflineOneBotSettings, OfflineWebhookChannel, OfflineWebhookSettings,
    WebUiPollerSettings,
};
use ncd_host::{Host, HostPath};
use ncd_watch::config::{
    DesktopPresentFile, NotifyBotTarget, NotifyConfig, WATCH_PROTOCOL_V1, WatchOneBotMessenger,
    WatchOneBotSettings, WatchPaths,
};

pub use ncd_watch::config::NotifyConfig as WatchNotifyConfig;

/// 组装 notify 时可选的 per-bot WebUI 凭据(仅 NapCat)与 OneBot
#[derive(Debug, Clone, Default)]
pub struct WatchNotifyExtras {
    /// bot_id(qq 字符串) -> (port, token)
    pub webui_by_bot: std::collections::HashMap<String, (u16, String)>,
    pub webhook_enabled: bool,
    pub email_enabled: bool,
    pub notify_on_recovered: bool,
    pub email: OfflineEmailSettings,
    pub onebot: OfflineOneBotSettings,
}

impl WatchNotifyExtras {
    pub fn from_app(
        poller: &WebUiPollerSettings,
        email: OfflineEmailSettings,
        onebot: OfflineOneBotSettings,
        webui_by_bot: std::collections::HashMap<String, (u16, String)>,
    ) -> Self {
        Self {
            webui_by_bot,
            webhook_enabled: poller.offline_webhook_notice,
            email_enabled: poller.offline_email_notice,
            notify_on_recovered: poller.offline_notify_behavior.notify_on_recovered,
            email,
            onebot,
        }
    }
}

/// 从本机 Bot 列表筛出落在 `server_id` 上的目标
pub fn bots_for_server<'a>(
    server_id: &str,
    bots: impl IntoIterator<Item = &'a BotConfig>,
) -> Vec<&'a BotConfig> {
    bots.into_iter()
        .filter(|c| c.bot.runtime_target.server_id() == Some(server_id))
        .collect()
}

/// Docker NapCat WebUI 宿主机端口: 6099 + (qq % 500)
pub fn napcat_docker_webui_host_port(qq_id: u64) -> u16 {
    DockerDeploySpec::napcat_default()
        .with_host_port_offset(qq_id)
        .host_port_for_container(6099)
        .unwrap_or(6099)
}

/// 从 Bot connect.httpServers 投影 watch 同机可达的 OneBot HTTP 根地址
///
/// host 空/`0.0.0.0`/`127.0.0.1`/`localhost` → `http://127.0.0.1:port`
/// 其它 host 视为非本机可达,跳过(跨机不写进该 server 的 notify)
pub fn watch_onebot_http_from_bot(config: &BotConfig) -> Option<(String, String)> {
    let server = config
        .connect
        .http_servers
        .iter()
        .find(|s| s.base.enable && s.port > 0)?;
    let host = server.host.trim();
    let loopback = host.is_empty()
        || host == "0.0.0.0"
        || host == "127.0.0.1"
        || host.eq_ignore_ascii_case("localhost");
    if !loopback {
        return None;
    }
    Some((
        format!("http://127.0.0.1:{}", server.port),
        server.base.token.clone(),
    ))
}

/// 仅收录与 `server_id` 同机、且配置了可用 HTTP 的 messenger
pub fn build_watch_onebot_for_server(
    server_id: &str,
    all_bots: &[BotConfig],
    settings: &OfflineOneBotSettings,
) -> WatchOneBotSettings {
    let mut onebot = settings.clone();
    onebot.normalize();
    if !onebot.enabled {
        return WatchOneBotSettings::default();
    }
    let mut messengers = Vec::new();
    for mid in onebot.effective_messenger_ids() {
        let Some(cfg) = all_bots.iter().find(|c| c.bot.qq_id.to_string() == mid) else {
            continue;
        };
        if cfg.bot.runtime_target.server_id() != Some(server_id) {
            continue;
        }
        let Some((base_url, access_token)) = watch_onebot_http_from_bot(cfg) else {
            continue;
        };
        messengers.push(WatchOneBotMessenger {
            bot_id: mid,
            base_url,
            access_token,
        });
    }
    let targets = onebot.effective_target_ids();
    WatchOneBotSettings {
        enabled: onebot.enabled && !messengers.is_empty() && !targets.is_empty(),
        messengers,
        target_type: onebot.target_type,
        target_ids: targets,
        message_template: onebot.message_template,
    }
}

/// 把 BotConfig 投影成 watch 探活目标。
pub fn bot_to_notify_target(config: &BotConfig) -> NotifyBotTarget {
    bot_to_notify_target_with_webui(config, None)
}

pub fn bot_to_notify_target_with_webui(
    config: &BotConfig,
    webui: Option<(u16, String)>,
) -> NotifyBotTarget {
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
            let process_match = if qq > 0 {
                Some(format!("-q {qq}$"))
            } else {
                None
            };
            (None, None, process_match)
        }
    };

    let (webui_port, webui_token) = match (config.bot.backend_type, webui) {
        (BackendType::NapCat, Some((port, token))) if port > 0 && !token.trim().is_empty() => {
            (Some(port), Some(token))
        }
        (BackendType::NapCat, None)
            if matches!(config.bot.deployment_type, DeploymentType::Docker) =>
        {
            (Some(napcat_docker_webui_host_port(qq)), None)
        }
        _ => (None, None),
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
        webui_port,
        webui_token,
        enabled: true,
    }
}

pub fn build_notify_config(
    server_id: &str,
    bots: &[BotConfig],
    webhook: &OfflineWebhookSettings,
) -> NotifyConfig {
    build_notify_config_with_extras(server_id, bots, webhook, &WatchNotifyExtras::default())
}

pub fn build_notify_config_with_extras(
    server_id: &str,
    bots: &[BotConfig],
    webhook: &OfflineWebhookSettings,
    extras: &WatchNotifyExtras,
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
        .map(|c| {
            let webui = extras.webui_by_bot.get(&c.bot.qq_id.to_string()).cloned();
            bot_to_notify_target_with_webui(c, webui)
        })
        .collect();
    // 旧调用未填 extras 时:有通道则 webhook 视为开启
    let webhook_enabled = extras.webhook_enabled || !webhooks.is_empty();
    let onebot = build_watch_onebot_for_server(server_id, bots, &extras.onebot);
    NotifyConfig {
        protocol: WATCH_PROTOCOL_V1,
        server_id: Some(server_id.to_string()),
        bots: targets,
        webhooks,
        webhook_enabled,
        email_enabled: extras.email_enabled,
        notify_on_recovered: extras.notify_on_recovered,
        email: extras.email.clone(),
        onebot,
    }
}

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

pub fn local_paths_under(root: impl AsRef<std::path::Path>) -> WatchPaths {
    WatchPaths::from_root(root.as_ref())
}

pub fn server_id_of(config: &BotConfig) -> Option<&str> {
    match &config.bot.runtime_target {
        RuntimeTarget::Server(id) => Some(id.as_str()),
        RuntimeTarget::Local => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ncd_domain::bot_config::{
        AdvancedConfig, BotBasicConfig, ConnectConfig, HttpServerConfig, MessagePostFormat,
        NetworkBaseFields,
    };

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

    fn with_http(mut cfg: BotConfig, port: u16) -> BotConfig {
        cfg.connect.http_servers.push(HttpServerConfig {
            base: NetworkBaseFields {
                enable: true,
                name: "http".into(),
                message_post_format: MessagePostFormat::Array,
                token: "t".into(),
                debug: false,
            },
            host: "0.0.0.0".into(),
            port,
            enable_cors: false,
            enable_websocket: false,
            path: "/".into(),
        });
        cfg
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
        assert!(n.webhook_enabled);
    }

    #[test]
    fn docker_targets_use_flavor_container_names() {
        let nc = sample_remote("s1", 10001);
        let mut sl = sample_remote("s1", 10002);
        sl.bot.backend_type = BackendType::SnowLuma;
        let t_nc = bot_to_notify_target(&nc);
        let t_sl = bot_to_notify_target(&sl);
        assert_eq!(t_nc.container_name.as_deref(), Some("ncbot-10001"));
        assert_eq!(t_sl.container_name.as_deref(), Some("slbot-10002"));
        assert_eq!(t_nc.webui_port, Some(napcat_docker_webui_host_port(10001)));
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
    }

    #[test]
    fn extras_attach_webui_and_email() {
        let bots = vec![sample_remote("s1", 10001)];
        let mut map = std::collections::HashMap::new();
        map.insert("10001".into(), (6100, "tok".into()));
        let mut email = OfflineEmailSettings::default();
        email.smtp_server = "smtp.example".into();
        let extras = WatchNotifyExtras {
            webui_by_bot: map,
            webhook_enabled: true,
            email_enabled: true,
            notify_on_recovered: true,
            email,
            onebot: OfflineOneBotSettings::default(),
        };
        let n = build_notify_config_with_extras(
            "s1",
            &bots,
            &OfflineWebhookSettings::default(),
            &extras,
        );
        assert_eq!(n.bots[0].webui_port, Some(6100));
        assert_eq!(n.bots[0].webui_token.as_deref(), Some("tok"));
        assert!(n.email_enabled);
        assert!(n.notify_on_recovered);
        assert!(!n.onebot.enabled);
    }

    #[test]
    fn same_host_onebot_messengers_only() {
        let m_s1 = with_http(sample_remote("s1", 10002), 3002);
        let m_s2 = with_http(sample_remote("s2", 10003), 3003);
        let mut local = with_http(sample_remote("s1", 10004), 3004);
        local.bot.runtime_target = RuntimeTarget::Local;
        let bots = vec![sample_remote("s1", 10001), m_s1, m_s2, local];
        let mut onebot = OfflineOneBotSettings::default();
        onebot.enabled = true;
        onebot.messenger_bot_ids = vec!["10002".into(), "10003".into(), "10004".into()];
        onebot.target_ids = vec![999];
        let extras = WatchNotifyExtras {
            onebot,
            ..WatchNotifyExtras::default()
        };
        let n = build_notify_config_with_extras(
            "s1",
            &bots,
            &OfflineWebhookSettings::default(),
            &extras,
        );
        assert!(n.onebot.enabled);
        assert_eq!(n.onebot.messengers.len(), 1);
        assert_eq!(n.onebot.messengers[0].bot_id, "10002");
        assert_eq!(n.onebot.messengers[0].base_url, "http://127.0.0.1:3002");
        assert_eq!(n.onebot.target_ids, vec![999]);
    }

    #[test]
    fn skips_non_loopback_http_host() {
        let mut m = sample_remote("s1", 10002);
        m.connect.http_servers.push(HttpServerConfig {
            base: NetworkBaseFields {
                enable: true,
                name: "http".into(),
                message_post_format: MessagePostFormat::Array,
                token: String::new(),
                debug: false,
            },
            host: "10.0.0.5".into(),
            port: 3000,
            enable_cors: false,
            enable_websocket: false,
            path: "/".into(),
        });
        assert!(watch_onebot_http_from_bot(&m).is_none());
    }
}
