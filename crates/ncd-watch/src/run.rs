//! 主循环:探活 → 登录态 → 边沿 → Webhook/Email/同机 OneBot

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use ncd_domain::OfflineAlertKind;

use crate::config::{NotifyConfig, WatchConfig, WatchPaths};
use crate::edge::{EdgeAction, EdgeTracker, OfflineEdgeKind};
use crate::email::send_watch_email;
use crate::login_probe::probe_login_status;
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

    // 先扫一轮进程态,供 OneBot 选「本轮仍在线」的 messenger
    let mut process_online: HashSet<String> = HashSet::new();
    let mut probe_cache: Vec<(crate::config::NotifyBotTarget, crate::probe::ProbeResult)> =
        Vec::new();

    for bot in notify.enabled_bots() {
        let mut result = prober.probe_bot(bot);
        out.probed += 1;

        if matches!(result.status, ProbeStatus::Online) {
            process_online.insert(bot.bot_id.clone());
        }

        if matches!(result.status, ProbeStatus::Online)
            && bot.backend.eq_ignore_ascii_case("napcat")
            && bot.webui_port.is_some()
            && bot.webui_token.as_ref().is_some_and(|t| !t.trim().is_empty())
        {
            let (login, detail) = probe_login_status(bot).await;
            result.login = login;
            if !detail.is_empty() {
                result.detail = format!("{}; {detail}", result.detail);
            }
        }

        probe_cache.push((bot.clone(), result));
    }

    for (bot, result) in &probe_cache {
        if matches!(result.status, ProbeStatus::Unknown)
            && matches!(result.login, LoginStatus::Unknown)
        {
            tracing::debug!(bot_id = %bot.bot_id, detail = %result.detail, "probe unknown");
            continue;
        }

        let actions = edges.observe_layers(&bot.bot_id, result.status, result.login);
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
                        detail = %result.detail,
                        allow_notify,
                        "offline edge"
                    );
                    if !allow_notify {
                        continue;
                    }
                    let alert = build_offline_alert(bot, alert_kind_for(kind));
                    deliver_channels(notify, &channels, &process_online, &alert, &mut out).await;
                }
                EdgeAction::FireRecovered => {
                    tracing::info!(
                        bot_id = %bot.bot_id,
                        detail = %result.detail,
                        allow_notify,
                        notify_on_recovered = notify.notify_on_recovered,
                        "recovered edge"
                    );
                    if !allow_notify || !notify.notify_on_recovered {
                        continue;
                    }
                    let alert = build_offline_alert(bot, OfflineAlertKind::Recovered);
                    deliver_channels(notify, &channels, &process_online, &alert, &mut out).await;
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
    let mut any = false;
    if notify.webhook_enabled {
        if channels.is_empty() {
            tracing::debug!(bot_id = %alert.bot_id, "webhook enabled but no channels");
        } else {
            match send_watch_webhooks(channels, alert).await {
                Ok(()) => {
                    any = true;
                }
                Err(e) => {
                    out.webhook_errors
                        .push(format!("{}: {e}", alert.bot_id.as_str()));
                    tracing::warn!(bot_id = %alert.bot_id, %e, "webhook failed");
                }
            }
        }
    }
    if notify.email_enabled {
        let email = notify.email.clone();
        let alert_c = alert.clone();
        match tokio::task::spawn_blocking(move || send_watch_email(&email, &alert_c)).await {
            Ok(Ok(())) => any = true,
            Ok(Err(e)) => {
                out.email_errors
                    .push(format!("{}: {e}", alert.bot_id.as_str()));
                tracing::warn!(bot_id = %alert.bot_id, %e, "email failed");
            }
            Err(e) => {
                out.email_errors
                    .push(format!("{}: join {e}", alert.bot_id.as_str()));
                tracing::warn!(bot_id = %alert.bot_id, %e, "email task join failed");
            }
        }
    }
    if notify.onebot.enabled {
        match send_watch_onebot(
            &notify.onebot,
            alert.bot_id.as_str(),
            Some(process_online),
            alert,
        )
        .await
        {
            Ok(()) => any = true,
            Err(e) => {
                out.onebot_errors
                    .push(format!("{}: {e}", alert.bot_id.as_str()));
                tracing::warn!(bot_id = %alert.bot_id, %e, "onebot failed");
            }
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
    let mut edges = EdgeTracker::load(
        &paths.edge_state,
        WatchConfig::load_or_default(&paths.watch_json)
            .unwrap_or_default()
            .debounce_secs,
    );

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

        let out = run_once(&paths, &watch, &notify, prober.as_ref(), &mut edges).await;
        tracing::debug!(
            probed = out.probed,
            fired = out.fired,
            recovered = out.recovered,
            debounced = out.debounced,
            skipped_desktop = out.skipped_desktop_present,
            "run_once summary"
        );

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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::NotifyBotTarget;
    use crate::probe::{LoginStatus, MapProber, ProbeStatus};
    use std::collections::HashMap;

    fn sample_bot() -> NotifyBotTarget {
        NotifyBotTarget {
            bot_id: "42".into(),
            qq_id: 42,
            bot_name: "t".into(),
            backend: "napcat".into(),
            deployment: "native".into(),
            container_name: None,
            pid_file: None,
            process_match: Some("x".into()),
            webui_port: None,
            webui_token: None,
            enabled: true,
        }
    }

    #[tokio::test]
    async fn fires_only_when_desktop_absent() {
        let dir = tempfile::tempdir().unwrap();
        let paths = WatchPaths::from_root(dir.path());
        std::fs::create_dir_all(&paths.state_dir).unwrap();
        std::fs::create_dir_all(&paths.config_dir).unwrap();

        let watch = WatchConfig {
            notify_while_desktop_present: false,
            desktop_present_ttl_secs: 90,
            ..WatchConfig::default()
        };
        let notify = NotifyConfig {
            bots: vec![sample_bot()],
            webhooks: vec![],
            webhook_enabled: true,
            ..NotifyConfig::default()
        };
        let mut map = HashMap::new();
        map.insert("42".into(), (ProbeStatus::Online, LoginStatus::Unknown));
        let prober = MapProber { map };
        let mut edges = EdgeTracker::new(0);

        let _ = run_once(&paths, &watch, &notify, &prober, &mut edges).await;

        let mut prober2 = MapProber::default();
        prober2
            .map
            .insert("42".into(), (ProbeStatus::Offline, LoginStatus::Unknown));
        let out = run_once(&paths, &watch, &notify, &prober2, &mut edges).await;
        assert_eq!(out.probed, 1);
        assert_eq!(out.fired, 0);
        assert!(!out.skipped_desktop_present);

        let present = crate::config::DesktopPresentFile::now();
        std::fs::write(
            &paths.desktop_present,
            serde_json::to_string(&present).unwrap(),
        )
        .unwrap();
        let mut edges2 = EdgeTracker::new(0);
        let mut p_on = MapProber::default();
        p_on.map
            .insert("42".into(), (ProbeStatus::Online, LoginStatus::Unknown));
        let _ = run_once(&paths, &watch, &notify, &p_on, &mut edges2).await;
        let mut p_off = MapProber::default();
        p_off
            .map
            .insert("42".into(), (ProbeStatus::Offline, LoginStatus::Unknown));
        let out2 = run_once(&paths, &watch, &notify, &p_off, &mut edges2).await;
        assert!(out2.skipped_desktop_present);
        assert_eq!(out2.fired, 0);
    }
}
