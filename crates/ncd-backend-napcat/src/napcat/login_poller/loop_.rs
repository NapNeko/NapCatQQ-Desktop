//! 主循环：auth/status ticker、credential 刷新、status poll

use std::time::{Duration, Instant};

use tokio::sync::mpsc;
use tokio::time::{Interval, MissedTickBehavior, interval};
use tokio_util::sync::CancellationToken;

use crate::napcat::webui_client::NapCatWebUiError;
use super::transitions::{
    apply_login_status, apply_observed_online_status, apply_online_status, online_from_login_status,
};
use super::types::{LoginState, PollerCommand, PollerConfig, PollerDeps};
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
                        if state.auth_refresh_throttle_elapsed(cfg.auth_refresh_throttle) {
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
        cfg.login_check_interval
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
        }
        Err(err) => {
            tracing::warn!(
                bot_id = %bot_id,
                error = ?err,
                "NapCat WebUI 刷新登录凭证失败"
            );
        }
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
        return;
    };

    let login_fut = deps.http.check_login_status(port, &auth);
    let online_fut = deps.http.check_online_status(port, &auth);
    let (login_res, online_res) = tokio::join!(login_fut, online_fut);

    let mut login_observed_online = None;
    match login_res {
        Ok(data) => {
            login_observed_online = online_from_login_status(&data);
            apply_login_status(bot_id, data, deps, state);
        }
        Err(NapCatWebUiError::Unauthorized(_)) => {
            let _ = cmd_tx.try_send(PollerCommand::RequestAuthRefresh);
            return;
        }
        Err(err) => {
            tracing::warn!(?err, %bot_id, "NapCat 登录态查询失败（check_login_status）");
        }
    }

    match online_res {
        Ok(mut data) => {
            if let Some(online) = login_observed_online {
                data.online = Some(online);
            }
            apply_online_status(bot_id, data, cfg, deps, state).await;
        }
        Err(NapCatWebUiError::Unauthorized(_)) => {
            let _ = cmd_tx.try_send(PollerCommand::RequestAuthRefresh);
            if let Some(online) = login_observed_online {
                apply_observed_online_status(bot_id, online, cfg, deps, state).await;
            }
        }
        Err(err) => {
            tracing::warn!(?err, %bot_id, "NapCat 在线状态查询失败（check_online_status）");
            if let Some(online) = login_observed_online {
                apply_observed_online_status(bot_id, online, cfg, deps, state).await;
            }
        }
    }
}
