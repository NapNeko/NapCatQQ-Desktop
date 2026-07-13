//! 主循环:探活 → 账号在线(WebUI) → 边沿 → Webhook/Email/同机 OneBot
//!
//! 掉线主信号对齐 Desktop login_poller 的 online/isLogin:
//! - 有 webui port+token:始终 probe 登录态,账号 LoggedIn→LoggedOut 才告警
//! - 进程 pgrep 仅作 OneBot messenger 选择,以及无 WebUI 凭据时的回退边沿
//! - 进程 Online 绝不冒充 QQ 已登录

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use ncd_domain::OfflineAlertKind;

use crate::config::{NotifyConfig, WatchConfig, WatchPaths};
use crate::edge::{EdgeAction, EdgeTracker, OfflineEdgeKind};
use crate::email::send_watch_email;
use crate::login_probe::{has_webui_probe, probe_login_status};
use crate::metrics::{sample_metrics_once, MetricsRunState, WatchMetricsConfig};
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

    // process_online: OneBot 选仍在线 messenger;不是掉线主信号
    let mut process_online: HashSet<String> = HashSet::new();
    let mut probe_cache: Vec<(crate::config::NotifyBotTarget, crate::probe::ProbeResult)> =
        Vec::new();

    for bot in notify.enabled_bots() {
        let mut result = prober.probe_bot(bot);
        out.probed += 1;

        if matches!(result.status, ProbeStatus::Online) {
            process_online.insert(bot.bot_id.clone());
        }

        // 有 WebUI 凭据就探账号态(不要求进程 Online;进程在也不等于已登录)
        if has_webui_probe(bot) {
            let (login, detail) = probe_login_status(bot).await;
            result.login = login;
            if !detail.is_empty() {
                result.detail = format!("{}; {detail}", result.detail);
            }
        }

        probe_cache.push((bot.clone(), result));
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
                    deliver_channels(notify, &channels, &process_online, &alert, &mut out).await;
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
    let mut edges = EdgeTracker::load(
        &paths.edge_state,
        WatchConfig::load_or_default(&paths.watch_json)
            .unwrap_or_default()
            .debounce_secs,
    );
    let mut metrics_state = MetricsRunState::default();

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

        // 指标续采：与告警解耦；Desktop 退出后仍写 history
        let metrics_cfg = WatchMetricsConfig::load_or_default(&paths.metrics_json()).clamp();
        if metrics_cfg.enabled {
            sample_metrics_once(&metrics_cfg, &mut metrics_state);
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
}
