//! 主循环：auth/status ticker、credential 刷新、status poll

use std::time::Duration;

use tokio::sync::mpsc;
use tokio::time::{Instant, Interval, MissedTickBehavior, interval};
use tokio_util::sync::CancellationToken;

use super::backoff::{AuthFailureKind, hint, issue_for, retry_delay};
use super::transitions::{
    apply_login_status, apply_online_status, mark_probe_unavailable, online_from_login_status,
};
use super::types::{LoginState, PollerCommand, PollerConfig, PollerDeps};
use crate::napcat::webui_client::NapCatWebUiError;
use ncd_domain::domain_event::DomainEvent;
use ncd_domain::ids::BotId;
use ncd_traits::events::EventBus;

pub(crate) async fn run_poller(
    bot_id: BotId,
    port: u16,
    token: String,
    cfg: PollerConfig,
    deps: PollerDeps,
    cancel: CancellationToken,
) {
    let mut state = LoginState::new();

    let mut auth_ticker = interval(cfg.auth_refresh_period);
    auth_ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let _ = auth_ticker.tick().await;

    let mut status_period = cfg.unlogged_interval;
    let mut status_ticker = interval(status_period);
    status_ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

    let (cmd_tx, mut cmd_rx) = mpsc::channel::<PollerCommand>(8);

    do_auth_refresh(&bot_id, port, &token, &cfg, &deps, &mut state).await;
    do_status_poll(&bot_id, port, &cfg, &deps, &mut state, &cmd_tx).await;
    adjust_status_interval(&mut status_ticker, &mut status_period, &state, &cfg);

    loop {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                deps.event_bus
                    .publish(DomainEvent::napcat_login_qrcode_removed(bot_id.clone()));
                break;
            }
            _ = auth_ticker.tick() => {
                do_auth_refresh(&bot_id, port, &token, &cfg, &deps, &mut state).await;
            }
            _ = status_ticker.tick() => {
                do_status_poll(&bot_id, port, &cfg, &deps, &mut state, &cmd_tx).await;
                adjust_status_interval(&mut status_ticker, &mut status_period, &state, &cfg);
            }
            Some(cmd) = cmd_rx.recv() => {
                match cmd {
                    PollerCommand::RequestAuthRefresh => {
                        if state.auth_refresh_allowed(cfg.auth_refresh_throttle) {
                            state.auth = None;
                            do_auth_refresh(&bot_id, port, &token, &cfg, &deps, &mut state)
                                .await;
                        }
                    }
                    PollerCommand::UpdateInterval(_d) => {}
                }
            }
        }
    }
}

pub(crate) fn adjust_status_interval(
    status_ticker: &mut Interval,
    current_period: &mut Duration,
    state: &LoginState,
    cfg: &PollerConfig,
) {
    let target = if state.is_logged_in {
        if state.consecutive_offline_observations == 0 && state.consecutive_probe_failures == 0 {
            cfg.login_check_interval
        } else {
            cfg.unlogged_interval
        }
    } else {
        cfg.unlogged_interval
    };
    if target != *current_period {
        *current_period = target;
        let mut new_ticker = interval(target);
        new_ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
        *status_ticker = new_ticker;
    }
}

pub(crate) async fn do_auth_refresh(
    bot_id: &BotId,
    port: u16,
    token: &str,
    _cfg: &PollerConfig,
    deps: &PollerDeps,
    state: &mut LoginState,
) {
    state.last_auth_refresh_attempt_at = Some(Instant::now());
    match deps.http.fetch_credential(port, token).await {
        Ok(credential) => {
            state.auth = Some(credential);
            if state.auth_failure_kind.is_some() {
                tracing::info!(
                    bot_id = %bot_id,
                    failures = state.auth_failure_streak,
                    "NapCat WebUI 登录凭证已恢复"
                );
            }
            state.auth_failure_kind = None;
            state.auth_failure_streak = 0;
            state.auth_retry_delay = Duration::ZERO;
            if state.auth_issue_published.take().is_some() {
                deps.event_bus
                    .publish(DomainEvent::napcat_webui_auth_changed(bot_id.clone(), None));
            }
        }
        // 失败不清 state.auth:旧凭据可能还有效,是否作废由 caller 决定
        Err(err) => record_auth_failure(bot_id, deps, state, &err),
    }
}

fn record_auth_failure(
    bot_id: &BotId,
    deps: &PollerDeps,
    state: &mut LoginState,
    err: &NapCatWebUiError,
) {
    let kind = AuthFailureKind::from_error(err);
    let kind_changed = state.auth_failure_kind != Some(kind);
    let streak = if kind_changed {
        1
    } else {
        state.auth_failure_streak.saturating_add(1)
    };
    state.auth_failure_kind = Some(kind);
    state.auth_failure_streak = streak;
    state.auth_retry_delay = retry_delay(kind, streak);
    let retry_in_secs = state.auth_retry_delay.as_secs();

    // 同类失败只在第一次 warn,其余降 debug,避免每 5s 刷一条
    if kind_changed {
        tracing::warn!(
            bot_id = %bot_id,
            error = %err,
            reason = hint(kind),
            retry_in_secs,
            "NapCat WebUI 刷新登录凭证失败"
        );
    } else {
        tracing::debug!(
            bot_id = %bot_id,
            error = %err,
            failures = streak,
            retry_in_secs,
            "NapCat WebUI 刷新登录凭证仍失败"
        );
    }

    // 暂时性失败没到阈值时保留上一次已发的问题,不来回闪
    let issue = issue_for(kind, streak);
    if issue.is_some() && issue != state.auth_issue_published {
        state.auth_issue_published = issue;
        deps.event_bus
            .publish(DomainEvent::napcat_webui_auth_changed(
                bot_id.clone(),
                issue,
            ));
    }
}

fn note_status_error(
    bot_id: &BotId,
    state: &mut LoginState,
    what: &'static str,
    err: &NapCatWebUiError,
) {
    if state.status_error_warned {
        tracing::debug!(%bot_id, error = %err, "{what}");
    } else {
        state.status_error_warned = true;
        tracing::warn!(%bot_id, error = %err, "{what}");
    }
}

pub(crate) async fn do_status_poll(
    bot_id: &BotId,
    port: u16,
    cfg: &PollerConfig,
    deps: &PollerDeps,
    state: &mut LoginState,
    cmd_tx: &mpsc::Sender<PollerCommand>,
) {
    let Some(auth) = state.auth.clone() else {
        let _ = cmd_tx.try_send(PollerCommand::RequestAuthRefresh);
        mark_probe_unavailable(bot_id, deps, state);
        return;
    };

    let login_fut = deps.http.check_login_status(port, &auth);
    let online_fut = deps.http.check_online_status(port, &auth);
    let (login_res, online_res) = tokio::join!(login_fut, online_fut);

    let mut observed_online = None;
    let mut query_failed = false;
    match login_res {
        Ok(data) => {
            observed_online = online_from_login_status(&data);
            apply_login_status(bot_id, data, deps, state);
        }
        Err(NapCatWebUiError::Unauthorized(_)) => {
            let _ = cmd_tx.try_send(PollerCommand::RequestAuthRefresh);
            mark_probe_unavailable(bot_id, deps, state);
            return;
        }
        Err(err) => {
            query_failed = true;
            note_status_error(
                bot_id,
                state,
                "NapCat 登录态查询失败（check_login_status）",
                &err,
            );
        }
    }

    match online_res {
        Ok(data) => {
            if observed_online.is_none() {
                observed_online = data.online;
            }
        }
        Err(NapCatWebUiError::Unauthorized(_)) => {
            let _ = cmd_tx.try_send(PollerCommand::RequestAuthRefresh);
        }
        Err(err) => {
            query_failed = true;
            note_status_error(
                bot_id,
                state,
                "NapCat 在线状态查询失败（check_online_status）",
                &err,
            );
        }
    }
    if !query_failed {
        state.status_error_warned = false;
    }

    if let Some(online) = observed_online {
        apply_online_status(
            bot_id,
            crate::napcat::webui_client::GetQQLoginInfoData {
                online: Some(online),
            },
            cfg,
            deps,
            state,
        )
        .await;
    } else {
        mark_probe_unavailable(bot_id, deps, state);
    }
}
