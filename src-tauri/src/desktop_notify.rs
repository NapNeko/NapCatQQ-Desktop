// 桌面 Toast:OfflineNotifier + DomainEvent 监听(无 WebView 时仍提醒)

use std::sync::{Arc, OnceLock};

use async_trait::async_trait;
use ncd_runtime::{
    BroadcastEventBus, DesktopNotifySettings, DomainEvent, EventBus, EventFilter,
    OfflineNoticeKind, OfflineNotifier,
};
use ncd_runtime::events::NapCatLoginInvalidationReason;
use ncd_runtime::ids::BotId;
use tauri::AppHandle;
#[cfg(not(windows))]
use tauri_plugin_notification::NotificationExt;
use tokio::sync::RwLock;

pub struct TauriOfflineNotifier {
    app: OnceLock<AppHandle>,
}

impl TauriOfflineNotifier {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            app: OnceLock::new(),
        })
    }

    pub fn bind_app(self: &Arc<Self>, app: AppHandle) {
        let _ = self.app.set(app);
    }

    fn aumid(app: &AppHandle) -> String {
        app.config().identifier.clone()
    }

    fn dispatch_toast(app: &AppHandle, headline: &str, body: &str) {
        let headline = headline.to_string();
        let body = body.to_string();
        let aumid = Self::aumid(app);
        #[cfg(windows)]
        {
            let _ = app.run_on_main_thread(move || {
                crate::windows_toast::show_desktop_toast(&aumid, &headline, &body);
            });
        }
        #[cfg(not(windows))]
        {
            let app = app.clone();
            let _ = app.run_on_main_thread(move || {
                if let Err(err) = app
                    .notification()
                    .builder()
                    .title(&headline)
                    .body(&body)
                    .show()
                {
                    tracing::warn!(%err, "desktop notification show failed");
                }
            });
        }
    }

    fn show(&self, headline: &str, body: &str) {
        let Some(app) = self.app.get().cloned() else {
            tracing::debug!("desktop notify skipped: app handle not bound yet");
            return;
        };
        let headline = headline.to_string();
        let body = body.to_string();
        Self::dispatch_toast(&app, &headline, &body);
    }
}

#[async_trait]
impl OfflineNotifier for TauriOfflineNotifier {
    async fn notify(&self, bot_id: &BotId, kind: OfflineNoticeKind) {
        let (headline, body) = match kind {
            OfflineNoticeKind::AutoRestart => (
                "Bot 离线",
                format!("{bot_id} 已离线，已尝试自动重启"),
            ),
            OfflineNoticeKind::Manual => (
                "Bot 离线",
                format!("{bot_id} 已离线，请打开主界面处理"),
            ),
        };
        self.show(headline, &body);
    }
}

fn post_notification(app: &AppHandle, headline: &str, body: &str) {
    TauriOfflineNotifier::dispatch_toast(app, headline, body);
}

pub fn spawn_desktop_notify_listener(
    app: AppHandle,
    event_bus: BroadcastEventBus,
    flags: Arc<RwLock<DesktopNotifySettings>>,
) {
    tauri::async_runtime::spawn(async move {
        let mut sub = event_bus.subscribe(EventFilter::all());
        while let Some(event) = sub.next().await {
            let cfg = flags.read().await;
            match &event {
                DomainEvent::BotProcessExited {
                    bot_id,
                    exit_code,
                    reason,
                } => {
                    if !cfg.notify_on_bot_crashed {
                        continue;
                    }
                    let abnormal = exit_code.map(|c| c != 0).unwrap_or(true);
                    if !abnormal {
                        continue;
                    }
                    let detail = reason
                        .as_deref()
                        .filter(|s| !s.is_empty())
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| {
                            exit_code
                                .map(|c| format!("退出码 {c}"))
                                .unwrap_or_else(|| "进程已结束".to_string())
                        });
                    post_notification(
                        &app,
                        "Bot 进程退出",
                        &format!("{bot_id}：{detail}"),
                    );
                }
                DomainEvent::NapCatLoginInvalidated { bot_id, reason } => {
                    if !cfg.notify_on_login_kicked {
                        continue;
                    }
                    let (headline, body) = match reason {
                        NapCatLoginInvalidationReason::Kicked => (
                            "QQ 被踢下线",
                            format!("Bot {bot_id} 账号被踢，请打开主界面重新登录"),
                        ),
                        NapCatLoginInvalidationReason::LoggedOut => (
                            "登录已失效",
                            format!("Bot {bot_id} 登录失效，请打开主界面处理"),
                        ),
                    };
                    post_notification(&app, headline, &body);
                }
                _ => {}
            }
        }
    });
}