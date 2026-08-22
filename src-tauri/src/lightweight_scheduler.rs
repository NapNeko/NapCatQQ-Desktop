// 关窗后延迟/立即进入轻量模式的计时与取消

use std::sync::Arc;

use ncd_domain::AppSettings;
use tokio::sync::{Mutex, RwLock};
use tokio_util::sync::CancellationToken;

use tauri::Manager;

use crate::lightweight;

pub struct LightweightScheduler {
    settings: Arc<RwLock<AppSettings>>,
    cancel: Mutex<Option<CancellationToken>>,
}

impl LightweightScheduler {
    pub fn new(settings: Arc<RwLock<AppSettings>>) -> Self {
        Self {
            settings,
            cancel: Mutex::new(None),
        }
    }

    pub async fn cancel_pending(&self) {
        let mut guard = self.cancel.lock().await;
        if let Some(token) = guard.take() {
            token.cancel();
        }
    }

    /// 主窗口已隐藏到托盘后调用(close_action=tray 且非 exit)
    pub async fn on_main_window_hidden(&self, app: tauri::AppHandle) {
        if has_active_component_tasks(&app).await {
            tracing::debug!("skip auto lightweight: component tasks running");
            return;
        }
        let cfg = self.settings.read().await.clone();
        if cfg.close_action != ncd_domain::CloseAction::Tray {
            return;
        }
        match cfg.after_close_ui_behavior {
            ncd_domain::AfterCloseUiBehavior::Hide => return,
            ncd_domain::AfterCloseUiBehavior::ImmediateLightweight => {
                if has_active_component_tasks(&app).await {
                    return;
                }
                let _ = lightweight::enter_lightweight_mode(&app);
            }
            ncd_domain::AfterCloseUiBehavior::DelayedLightweight => {
                let mut delay = cfg.enter_lightweight_delay_secs;
                if delay == 0 {
                    if has_active_component_tasks(&app).await {
                        return;
                    }
                    let _ = lightweight::enter_lightweight_mode(&app);
                    return;
                }
                delay = ncd_domain::clamp_lightweight_delay_secs(delay);
                self.cancel_pending().await;
                let token = CancellationToken::new();
                let child = token.child_token();
                {
                    let mut guard = self.cancel.lock().await;
                    *guard = Some(token);
                }
                let app2 = app.clone();
                tauri::async_runtime::spawn(async move {
                    tokio::select! {
                        () = tokio::time::sleep(std::time::Duration::from_secs(delay as u64)) => {
                            if has_active_component_tasks(&app2).await {
                                return;
                            }
                            if lightweight::is_lightweight_mode() {
                                return;
                            }
                            if let Some(w) = app2.get_webview_window(lightweight::MAIN_WINDOW_LABEL) {
                                if w.is_visible().unwrap_or(true) {
                                    return;
                                }
                            }
                            let _ = lightweight::enter_lightweight_mode(&app2);
                        }
                        () = child.cancelled() => {}
                    }
                });
            }
            _ => {}
        }
    }
}

async fn has_active_component_tasks(app: &tauri::AppHandle) -> bool {
    let state = app.state::<crate::AppState>();
    !state.active_tasks.lock().await.is_empty()
}
