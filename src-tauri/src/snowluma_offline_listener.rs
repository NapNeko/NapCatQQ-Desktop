// 订阅 SnowLuma 登录态边沿: LoggedIn→Disconnected 离线, Disconnected→LoggedIn 恢复

use std::collections::HashMap;
use std::sync::Arc;

use ncd_domain::ids::BotId;
use ncd_domain::{OfflineAlert, OfflineAlertKind, OfflineAlertSource, SnowLumaLoginState};
use ncd_runtime::{
    BroadcastEventBus, CompositeOfflineNotifier, DomainEvent, EventBus, EventFilter,
};

pub fn spawn_snowluma_offline_listener(
    event_bus: BroadcastEventBus,
    notifier: Arc<CompositeOfflineNotifier>,
    bot_manager: Arc<crate::AppBotManager>,
) {
    tauri::async_runtime::spawn(async move {
        let mut last: HashMap<String, SnowLumaLoginState> = HashMap::new();
        let mut sub = event_bus.subscribe(EventFilter::kind(
            ncd_runtime::DomainEventKind::SnowLumaLoginStateChanged,
        ));
        while let Some(event) = sub.next().await {
            let DomainEvent::SnowLumaLoginStateChanged { bot_id, state, .. } = event else {
                continue;
            };
            let key = bot_id.as_str().to_string();
            let prev = last.insert(key.clone(), state);
            let was_logged_in = prev == Some(SnowLumaLoginState::LoggedIn);
            let went_offline = was_logged_in && state == SnowLumaLoginState::Disconnected;
            let recovered = prev == Some(SnowLumaLoginState::Disconnected)
                && state == SnowLumaLoginState::LoggedIn;
            if !went_offline && !recovered {
                continue;
            }

            // Bot 高级 offlineNotice 门控(recovered 同门)
            let allow = match bot_manager.get_bot_config(&bot_id).await {
                Ok(Some(cfg)) => cfg.advanced.offline_notice,
                _ => false,
            };
            if !allow {
                continue;
            }

            let (qq_id, bot_name, auto_restart) = match bot_manager.get_bot_config(&bot_id).await {
                Ok(Some(cfg)) => (
                    cfg.bot.qq_id,
                    cfg.bot.name.clone(),
                    cfg.bot.offline_auto_restart,
                ),
                _ => (bot_id.as_str().parse().unwrap_or(0), String::new(), false),
            };
            let kind = if recovered {
                OfflineAlertKind::Recovered
            } else if auto_restart {
                OfflineAlertKind::AutoRestart
            } else {
                OfflineAlertKind::Manual
            };
            let alert = OfflineAlert {
                bot_id: BotId::new(key),
                qq_id,
                bot_name,
                kind,
                source: OfflineAlertSource::SnowLuma,
                at: chrono::Local::now().format("%Y/%m/%d %H:%M:%S").to_string(),
            };
            notifier.deliver(alert).await;
        }
    });
}
