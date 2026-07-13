//! 登录/在线状态转移与事件发射

use super::types::{LoginState, PollerConfig, PollerDeps};
use crate::napcat::offline_notifier::OfflineNoticeKind;
use crate::napcat::webui_client::{CheckLoginStatusData, GetQQLoginInfoData};
use ncd_domain::domain_event::DomainEvent;
use ncd_domain::ids::BotId;
use ncd_domain::napcat_events::NapCatLoginInvalidationReason;
use ncd_traits::events::EventBus;

pub(crate) fn online_from_login_status(data: &CheckLoginStatusData) -> Option<bool> {
    if data.is_login {
        return Some(true);
    }
    if data.is_offline == Some(true) {
        return Some(false);
    }
    None
}

pub(crate) fn apply_login_status(
    bot_id: &BotId,
    data: CheckLoginStatusData,
    deps: &PollerDeps,
    state: &mut LoginState,
) {
    let prev_login = state.is_logged_in;
    state.is_logged_in = data.is_login;

    if data.is_login {
        state.login_invalidated_while_online = false;
        state.suppress_qrcode_until_online = false;
        deps.event_bus
            .publish(DomainEvent::napcat_login_qrcode_removed(bot_id.clone()));
        return;
    }

    if prev_login && state.online {
        state.login_invalidated_while_online = true;
        deps.event_bus
            .publish(DomainEvent::napcat_login_invalidated(
                bot_id.clone(),
                NapCatLoginInvalidationReason::Kicked,
            ));
    }

    if !data.qrcode_url.is_empty()
        && !state.login_invalidated_while_online
        && !state.suppress_qrcode_until_online
    {
        deps.event_bus.publish(DomainEvent::napcat_login_qrcode(
            bot_id.clone(),
            data.qrcode_url,
        ));
    }
}

pub(crate) async fn apply_online_status(
    bot_id: &BotId,
    data: GetQQLoginInfoData,
    cfg: &PollerConfig,
    deps: &PollerDeps,
    state: &mut LoginState,
) {
    let Some(online) = data.online else {
        return;
    };
    apply_observed_online_status(bot_id, online, cfg, deps, state).await;
}

pub(crate) async fn apply_observed_online_status(
    bot_id: &BotId,
    online: bool,
    cfg: &PollerConfig,
    deps: &PollerDeps,
    state: &mut LoginState,
) {
    let prev_online = state.online;
    let kicked = state.login_invalidated_while_online;
    state.online = online;

    deps.event_bus
        .publish(DomainEvent::napcat_login_online(bot_id.clone(), online));

    if online {
        if !prev_online && state.offline_notice_sent {
            deps.notifier
                .notify(bot_id, OfflineNoticeKind::Recovered)
                .await;
        }
        state.offline_notice_sent = false;
        state.login_invalidated_while_online = false;
        state.suppress_qrcode_until_online = false;
        return;
    }

    if !prev_online {
        return;
    }

    if kicked {
        state.login_invalidated_while_online = false;
        state.suppress_qrcode_until_online = true;
        deps.event_bus
            .publish(DomainEvent::napcat_login_qrcode_removed(bot_id.clone()));
    }

    if !state.is_logged_in && !kicked {
        return;
    }

    if cfg.offline_auto_restart {
        if !state.offline_notice_sent && cfg.offline_notice_enabled {
            deps.notifier
                .notify(bot_id, OfflineNoticeKind::AutoRestart)
                .await;
            state.offline_notice_sent = true;
        }
        deps.restart_handle.restart_bot(bot_id).await;
        return;
    }

    if !state.offline_notice_sent && cfg.offline_notice_enabled {
        deps.notifier
            .notify(bot_id, OfflineNoticeKind::Manual)
            .await;
        state.offline_notice_sent = true;
    }
}
