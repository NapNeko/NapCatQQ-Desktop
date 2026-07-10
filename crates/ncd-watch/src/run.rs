//! 主循环:探活 → 边沿 → (可选) Webhook

use std::sync::Arc;
use std::time::Duration;

use ncd_domain::OfflineAlertKind;

use crate::config::{NotifyConfig, WatchConfig, WatchPaths};
use crate::edge::{EdgeAction, EdgeTracker};
use crate::present::desktop_is_present;
use crate::probe::{ProbeStatus, Prober};
use crate::webhook::{build_offline_alert, send_watch_webhooks};

#[derive(Debug, Clone, Default)]
pub struct RunOnceOutcome {
    pub probed: usize,
    pub fired: usize,
    pub debounced: usize,
    pub skipped_desktop_present: bool,
    pub webhook_errors: Vec<String>,
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
    let allow_webhook = watch.notify_while_desktop_present || !desktop_online;
    if desktop_online && !watch.notify_while_desktop_present {
        out.skipped_desktop_present = true;
    }

    let channels = notify.enabled_webhooks();

    for bot in notify.enabled_bots() {
        let result = prober.probe_bot(bot);
        out.probed += 1;
        if matches!(result.status, ProbeStatus::Unknown) {
            tracing::debug!(bot_id = %bot.bot_id, detail = %result.detail, "probe unknown");
            continue;
        }
        let action = edges.observe(&bot.bot_id, result.status);
        match action {
            EdgeAction::None => {}
            EdgeAction::Debounced => {
                out.debounced += 1;
                tracing::debug!(bot_id = %bot.bot_id, "offline edge debounced");
            }
            EdgeAction::FireOffline => {
                tracing::info!(
                    bot_id = %bot.bot_id,
                    detail = %result.detail,
                    allow_webhook,
                    "offline edge"
                );
                if !allow_webhook {
                    continue;
                }
                if channels.is_empty() {
                    tracing::warn!(bot_id = %bot.bot_id, "offline edge but no webhook configured");
                    continue;
                }
                let alert = build_offline_alert(bot, OfflineAlertKind::Manual);
                match send_watch_webhooks(&channels, &alert).await {
                    Ok(()) => out.fired += 1,
                    Err(e) => {
                        out.webhook_errors.push(format!("{}: {e}", bot.bot_id));
                        tracing::warn!(bot_id = %bot.bot_id, %e, "webhook failed");
                    }
                }
            }
        }
    }

    if let Err(e) = edges.save(&paths.edge_state) {
        tracing::warn!(%e, "save edge state failed");
    }
    out
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

        // 热更新 debounce:仅当配置变化时重建 tracker 会丢 last_fire;这里只改字段
        // EdgeTracker 无 setter,debounce 在 new/load 时固定;改 debounce 需重启进程
        let _ = run_once(&paths, &watch, &notify, prober.as_ref(), &mut edges).await;

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
    use crate::probe::{MapProber, ProbeStatus};
    use crate::config::NotifyBotTarget;
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
            ..NotifyConfig::default()
        };
        let mut map = HashMap::new();
        map.insert("42".into(), ProbeStatus::Online);
        let prober = MapProber { map };
        let mut edges = EdgeTracker::new(0);

        // warm online
        let _ = run_once(&paths, &watch, &notify, &prober, &mut edges).await;

        // go offline, no webhook channels → no fire count but edge observed
        let mut prober2 = MapProber::default();
        prober2.map.insert("42".into(), ProbeStatus::Offline);
        let out = run_once(&paths, &watch, &notify, &prober2, &mut edges).await;
        assert_eq!(out.probed, 1);
        // no channels → fired stays 0
        assert_eq!(out.fired, 0);
        assert!(!out.skipped_desktop_present);

        // with desktop present, skip flag set
        let present = crate::config::DesktopPresentFile::now();
        std::fs::write(
            &paths.desktop_present,
            serde_json::to_string(&present).unwrap(),
        )
        .unwrap();
        let mut edges2 = EdgeTracker::new(0);
        let mut p_on = MapProber::default();
        p_on.map.insert("42".into(), ProbeStatus::Online);
        let _ = run_once(&paths, &watch, &notify, &p_on, &mut edges2).await;
        let mut p_off = MapProber::default();
        p_off.map.insert("42".into(), ProbeStatus::Offline);
        let out2 = run_once(&paths, &watch, &notify, &p_off, &mut edges2).await;
        assert!(out2.skipped_desktop_present);
        assert_eq!(out2.fired, 0);
    }
}
