//! 主循环:探活 → 账号在线(WebUI) → 边沿 → Webhook/Email/同机 OneBot
//!
//! 掉线主信号对齐 Desktop login_poller 的 online/isLogin:
//! - 有 webui port+token:始终 probe 登录态,账号 LoggedIn→LoggedOut 才告警
//! - 进程 pgrep 用于无 WebUI 凭据时的回退边沿,不覆盖已经确认的账号态
//! - 进程 Online 绝不冒充 QQ 已登录

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use ncd_domain::OfflineAlertKind;

use crate::config::{NotifyConfig, WatchConfig, WatchPaths};
use crate::edge::{EdgeAction, EdgeTracker, OfflineEdgeKind};
use crate::email::send_watch_email;
use crate::login_probe::{WebUiLoginCache, has_webui_probe, probe_login_status};
use crate::metrics::{MetricsRunState, WatchMetricsConfig, sample_metrics_once};
use crate::onebot::send_watch_onebot;
use crate::present::desktop_is_present;
use crate::probe::{LoginStatus, ProbeStatus, Prober};
use crate::webhook::{build_offline_alert, send_watch_webhooks};

#[derive(Debug, Clone, Default)]
pub struct RunOnceOutcome {
    pub probed: usize,
    pub fired: usize,
    pub recovered: usize,
    pub debounced: usize,
    pub skipped_desktop_present: bool,
    pub webhook_errors: Vec<String>,
    pub email_errors: Vec<String>,
    pub onebot_errors: Vec<String>,
}

fn alert_kind_for(edge: OfflineEdgeKind) -> OfflineAlertKind {
    match edge {
        OfflineEdgeKind::Process => OfflineAlertKind::ProcessCrashed,
        OfflineEdgeKind::Login => OfflineAlertKind::Manual,
    }
}

/// 单轮探活;供循环与单测复用
pub async fn run_once(
    paths: &WatchPaths,
    watch: &WatchConfig,
    notify: &NotifyConfig,
    prober: &dyn Prober,
    edges: &mut EdgeTracker,
    login_cache: &mut WebUiLoginCache,
) -> RunOnceOutcome {
    let mut out = RunOnceOutcome::default();
    let desktop_online = desktop_is_present(&paths.desktop_present, watch.desktop_present_ttl_secs);
    let allow_notify = watch.notify_while_desktop_present || !desktop_online;
    if desktop_online && !watch.notify_while_desktop_present {
        out.skipped_desktop_present = true;
    }

    let channels = if notify.webhook_enabled {
        notify.enabled_webhooks()
    } else {
        Vec::new()
    };

    // 发送方独立于它自己的监控开关；进程在线不足以证明账号能发消息。
    let mut messenger_online: HashSet<String> = HashSet::new();
    let mut probe_cache: Vec<(crate::config::NotifyBotTarget, crate::probe::ProbeResult)> =
        Vec::new();

    for bot in &notify.bots {
        let monitor = bot.enabled && !crate::relogin_config::owns(notify, &bot.bot_id);
        let messenger = notify
            .onebot
            .messengers
            .iter()
            .find(|m| notify.onebot.enabled && m.bot_id == bot.bot_id);
        if !monitor && messenger.is_none() {
            continue;
        }
        let mut result = prober.probe_bot(bot);
        out.probed += 1;

        // 有 WebUI 凭据就探账号态(不要求进程 Online;进程在也不等于已登录)
        if has_webui_probe(bot) {
            let (login, detail) = probe_login_status(bot, login_cache).await;
            result.login = login;
            if !detail.is_empty() {
                result.detail = format!("{}; {detail}", result.detail);
            }
        }

        // 旧 notify.json 可能没有 WebUI 凭据。此时直接向 OneBot 核实账号与在线状态。
        if result.login == LoginStatus::Unknown && !has_webui_probe(bot) {
            if let Some(messenger) = messenger {
                if crate::onebot::messenger_is_online(messenger).await {
                    result.login = LoginStatus::LoggedIn;
                }
            }
        }

        if messenger.is_some() && result.login == LoginStatus::LoggedIn {
            messenger_online.insert(bot.bot_id.clone());
        }
        // 重登引擎对这些账号持有唯一投递权，包括备用渠道。
        if monitor {
            probe_cache.push((bot.clone(), result));
        }
    }

    for (bot, result) in &probe_cache {
        let prefer_account = has_webui_probe(bot);
        if matches!(result.status, ProbeStatus::Unknown)
            && matches!(result.login, LoginStatus::Unknown)
        {
            tracing::debug!(bot_id = %bot.bot_id, detail = %result.detail, "probe unknown");
            // 无任何有效层时仍更新不了边沿;prefer_account 时 Unknown 登录会只记进程快照
            if !prefer_account {
                continue;
            }
        }

        let actions =
            edges.observe_layers_prefer(&bot.bot_id, result.status, result.login, prefer_account);
        for action in actions {
            match action {
                EdgeAction::None => {}
                EdgeAction::Debounced => {
                    out.debounced += 1;
                    tracing::debug!(bot_id = %bot.bot_id, "offline edge debounced");
                }
                EdgeAction::FireOffline(kind) => {
                    tracing::info!(
                        bot_id = %bot.bot_id,
                        ?kind,
                        prefer_account,
                        detail = %result.detail,
                        allow_notify,
                        "offline edge"
                    );
                    if !allow_notify {
                        continue;
                    }
                    let alert = build_offline_alert(bot, alert_kind_for(kind));
                    deliver_channels(notify, &channels, &messenger_online, &alert, &mut out).await;
                }
                EdgeAction::FireRecovered => {
                    tracing::info!(
                        bot_id = %bot.bot_id,
                        prefer_account,
                        detail = %result.detail,
                        allow_notify,
                        notify_on_recovered = notify.notify_on_recovered,
                        "recovered edge"
                    );
                    if !allow_notify || !notify.notify_on_recovered {
                        continue;
                    }
                    let alert = build_offline_alert(bot, OfflineAlertKind::Recovered);
                    deliver_channels(notify, &channels, &messenger_online, &alert, &mut out).await;
                    out.recovered += 1;
                }
            }
        }
    }

    if let Err(e) = edges.save(&paths.edge_state) {
        tracing::warn!(%e, "save edge state failed");
    }
    out
}

async fn deliver_channels(
    notify: &NotifyConfig,
    channels: &[&ncd_domain::OfflineWebhookChannel],
    process_online: &HashSet<String>,
    alert: &ncd_domain::OfflineAlert,
    out: &mut RunOnceOutcome,
) {
    // webhook / email / onebot 并行:SMTP 慢不能拖死 webhook
    let webhook_fut = async {
        if !notify.webhook_enabled {
            return Ok::<Option<()>, String>(None);
        }
        if channels.is_empty() {
            tracing::debug!(bot_id = %alert.bot_id, "webhook enabled but no channels");
            return Ok(None);
        }
        send_watch_webhooks(channels, alert).await.map(Some)
    };
    let email_fut = async {
        if !notify.email_enabled {
            return Ok::<Option<()>, String>(None);
        }
        let email = notify.email.clone();
        let alert_c = alert.clone();
        match tokio::task::spawn_blocking(move || send_watch_email(&email, &alert_c)).await {
            Ok(Ok(())) => Ok(Some(())),
            Ok(Err(e)) => Err(e),
            Err(e) => Err(format!("join {e}")),
        }
    };
    let onebot_fut = async {
        if !notify.onebot.enabled {
            return Ok::<Option<()>, String>(None);
        }
        send_watch_onebot(
            &notify.onebot,
            alert.bot_id.as_str(),
            Some(process_online),
            alert,
        )
        .await
        .map(Some)
    };

    let (wh, em, ob) = tokio::join!(webhook_fut, email_fut, onebot_fut);
    let mut any = false;
    match wh {
        Ok(Some(())) => {
            any = true;
            tracing::info!(bot_id = %alert.bot_id, "webhook delivered");
        }
        Ok(None) => {}
        Err(e) => {
            out.webhook_errors
                .push(format!("{}: {e}", alert.bot_id.as_str()));
            tracing::warn!(bot_id = %alert.bot_id, %e, "webhook failed");
        }
    }
    match em {
        Ok(Some(())) => {
            any = true;
            tracing::info!(bot_id = %alert.bot_id, "email delivered");
        }
        Ok(None) => {}
        Err(e) => {
            out.email_errors
                .push(format!("{}: {e}", alert.bot_id.as_str()));
            tracing::warn!(bot_id = %alert.bot_id, %e, "email failed");
        }
    }
    match ob {
        Ok(Some(())) => any = true,
        Ok(None) => {}
        Err(e) => {
            out.onebot_errors
                .push(format!("{}: {e}", alert.bot_id.as_str()));
            tracing::warn!(bot_id = %alert.bot_id, %e, "onebot failed");
        }
    }
    if any && alert.is_offline_edge() {
        out.fired += 1;
    }
    if !notify.webhook_enabled && !notify.email_enabled && !notify.onebot.enabled {
        tracing::warn!(
            bot_id = %alert.bot_id,
            "edge fired but webhook/email/onebot all disabled"
        );
    }
}

/// 常驻循环;收到取消后返回
pub async fn run_loop(
    paths: WatchPaths,
    prober: Arc<dyn Prober>,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    let relogin_task = tokio::spawn(crate::relogin::run(paths.clone(), shutdown.clone()));
    let mut edges = EdgeTracker::load(
        &paths.edge_state,
        WatchConfig::load_or_default(&paths.watch_json)
            .unwrap_or_default()
            .debounce_secs,
    );
    let mut metrics_state = MetricsRunState::default();
    // 跨轮持有,凭据复用与登录退避靠它
    let mut login_cache = WebUiLoginCache::new();

    loop {
        let watch = WatchConfig::load_or_default(&paths.watch_json)
            .unwrap_or_else(|e| {
                tracing::warn!(%e, "watch.json load failed, using defaults");
                WatchConfig::default()
            })
            .clamp();
        let notify = NotifyConfig::load_or_default(&paths.notify_json).unwrap_or_else(|e| {
            tracing::warn!(%e, "notify.json load failed, using empty");
            NotifyConfig::default()
        });

        let out = run_once(
            &paths,
            &watch,
            &notify,
            prober.as_ref(),
            &mut edges,
            &mut login_cache,
        )
        .await;
        tracing::debug!(
            probed = out.probed,
            fired = out.fired,
            recovered = out.recovered,
            debounced = out.debounced,
            skipped_desktop = out.skipped_desktop_present,
            "run_once summary"
        );

        // 指标续采：与告警解耦；Desktop 退出后仍写 history + host-stats
        let metrics_cfg = WatchMetricsConfig::load_or_default(&paths.metrics_json()).clamp();
        if metrics_cfg.enabled {
            sample_metrics_once(&metrics_cfg, &mut metrics_state);
            crate::metrics::sample_host_stats(
                &paths.host_stats_json(),
                &mut metrics_state,
                u64::from(metrics_cfg.sample_interval_ms),
            );
        }

        let wait = Duration::from_secs(u64::from(watch.probe_interval_secs.max(1)));
        tokio::select! {
            _ = tokio::time::sleep(wait) => {}
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    tracing::info!("shutdown requested");
                    break;
                }
            }
        }
    }
    relogin_task.abort();
    let _ = relogin_task.await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::probe::{ProbeKind, ProbeResult};
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    struct BotProber {
        messenger_login: LoginStatus,
        messenger_status: ProbeStatus,
    }

    impl Prober for BotProber {
        fn probe_bot(&self, bot: &crate::config::NotifyBotTarget) -> ProbeResult {
            ProbeResult {
                bot_id: bot.bot_id.clone(),
                kind: ProbeKind::ProcessMatch,
                status: if bot.bot_id == "10001" {
                    ProbeStatus::Offline
                } else {
                    self.messenger_status
                },
                login: if bot.bot_id == "10001" {
                    LoginStatus::Unknown
                } else {
                    self.messenger_login
                },
                detail: String::new(),
            }
        }
    }

    fn notification_config(url: &str) -> NotifyConfig {
        let mut config = NotifyConfig::example();
        config.webhook_enabled = false;
        config.email_enabled = false;
        config.bots[0].webui_port = None;
        config.bots[0].webui_token = None;
        let mut messenger = config.bots[0].clone();
        messenger.bot_id = "20002".into();
        messenger.qq_id = 20002;
        config.bots.push(messenger);
        config.onebot.enabled = true;
        config.onebot.target_ids = vec![99];
        config.onebot.messengers = vec![
            serde_json::from_value(serde_json::json!({
                "botId":"20002", "baseUrl":url, "accessToken":"", "path":"/"
            }))
            .unwrap(),
        ];
        config
    }

    async fn offline_edge(config: &NotifyConfig, messenger_login: LoginStatus) -> RunOnceOutcome {
        offline_edge_with_status(config, messenger_login, ProbeStatus::Online).await
    }

    async fn offline_edge_with_status(
        config: &NotifyConfig,
        messenger_login: LoginStatus,
        messenger_status: ProbeStatus,
    ) -> RunOnceOutcome {
        let dir = tempfile::tempdir().unwrap();
        let paths = WatchPaths::from_root(dir.path());
        let mut edges = EdgeTracker::new(0);
        edges.observe("10001", ProbeStatus::Online);
        run_once(
            &paths,
            &WatchConfig::default(),
            config,
            &BotProber {
                messenger_login,
                messenger_status,
            },
            &mut edges,
            &mut WebUiLoginCache::new(),
        )
        .await
    }

    #[tokio::test]
    async fn messenger_can_send_with_own_notice_disabled_or_owned_by_relogin() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/send_private_msg"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "status":"ok", "retcode":0, "data":{"message_id":1}
            })))
            .expect(2)
            .mount(&server)
            .await;
        let mut config = notification_config(&server.uri());
        config.bots[1].enabled = false;
        let disabled = offline_edge(&config, LoginStatus::LoggedIn).await;
        assert_eq!(disabled.fired, 1);
        assert!(disabled.onebot_errors.is_empty());
        config.bots[1].enabled = true;
        config.onebot.relogin.settings.enabled = true;
        config.onebot.relogin.settings.administrator_ids = vec![99];
        config.onebot.relogin.settings.bot_ids = vec!["20002".into()];
        config.onebot.relogin.event_endpoints = vec![crate::relogin_config::WatchEventEndpoint {
            bot_id: "20002".into(),
            port: 3001,
            path: "/".into(),
            ..Default::default()
        }];
        let owned = offline_edge(&config, LoginStatus::LoggedIn).await;
        assert_eq!(owned.fired, 1);
        assert!(owned.onebot_errors.is_empty());
    }

    #[tokio::test]
    async fn legacy_messenger_without_webui_uses_verified_onebot_account() {
        let server = MockServer::start().await;
        for (action, data) in [
            ("get_login_info", serde_json::json!({"user_id":20002})),
            ("get_status", serde_json::json!({"online":true})),
            ("send_private_msg", serde_json::json!({"message_id":1})),
        ] {
            Mock::given(method("POST"))
                .and(path(format!("/{action}")))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "status":"ok", "retcode":0, "data":data
                })))
                .expect(2)
                .mount(&server)
                .await;
        }
        let mut config = notification_config(&server.uri());
        config.bots[1].enabled = false;
        for process_status in [ProbeStatus::Online, ProbeStatus::Unknown] {
            let result =
                offline_edge_with_status(&config, LoginStatus::Unknown, process_status).await;
            assert_eq!(result.fired, 1);
            assert!(result.onebot_errors.is_empty());
        }
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn authenticated_messenger_can_send_when_process_probe_is_unknown_or_offline() {
        let server = MockServer::start().await;
        for (method_name, action, body) in [
            ("POST", "/api/login", serde_json::json!({"token":"session"})),
            (
                "GET",
                "/api/processes",
                serde_json::json!({"list":[{
                    "pid":std::process::id(),"path":"/qq","uin":"20002",
                    "status":"online","loggedIn":true,"connected":true
                }]}),
            ),
            (
                "POST",
                "/send_private_msg",
                serde_json::json!({
                    "status":"ok", "retcode":0, "data":{"message_id":1}
                }),
            ),
        ] {
            Mock::given(method(method_name))
                .and(path(action))
                .respond_with(ResponseTemplate::new(200).set_body_json(body))
                .expect(2)
                .mount(&server)
                .await;
        }
        let mut config = notification_config(&server.uri());
        config.bots[1].enabled = false;
        config.bots[1].backend = "snowluma".into();
        config.bots[1].deployment = "native".into();
        config.bots[1].webui_port = Some(server.address().port());
        config.bots[1].webui_token = Some("secret".into());
        for process_status in [ProbeStatus::Unknown, ProbeStatus::Offline] {
            let result =
                offline_edge_with_status(&config, LoginStatus::Unknown, process_status).await;
            assert_eq!(result.fired, 1);
            assert!(result.onebot_errors.is_empty());
        }
    }

    #[tokio::test]
    async fn process_alive_but_account_logged_out_is_not_a_messenger() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/send_private_msg"))
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(&server)
            .await;
        let config = notification_config(&server.uri());
        let result = offline_edge(&config, LoginStatus::LoggedOut).await;
        assert_eq!(result.fired, 0);
        assert!(
            result
                .onebot_errors
                .iter()
                .any(|error| error.contains("无可用"))
        );
    }
}
