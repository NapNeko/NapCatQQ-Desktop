//! WebView 页面缩放：创建窗口时读当前倍率，设置变更时同步已有窗口。

use std::sync::atomic::{AtomicU64, Ordering};

use ncd_domain::app_config::AppUiPreferences;
use tauri::{AppHandle, Manager, WebviewWindow};

pub struct UiScale(AtomicU64);

impl UiScale {
    pub fn new(preferences: &AppUiPreferences) -> Self {
        Self(AtomicU64::new(preferences.ui_scale_factor().to_bits()))
    }

    fn factor(&self) -> f64 {
        f64::from_bits(self.0.load(Ordering::Acquire))
    }
}

pub fn apply_to_window(window: &WebviewWindow) -> Result<(), String> {
    let scale = window.state::<UiScale>().factor();
    window.set_zoom(scale).map_err(|error| error.to_string())
}

pub fn sync_all(app: &AppHandle, preferences: &AppUiPreferences) {
    let factor = preferences.ui_scale_factor();
    let scale = app.state::<UiScale>();
    if scale.0.swap(factor.to_bits(), Ordering::AcqRel) == factor.to_bits() {
        return;
    }
    for (label, window) in app.webview_windows() {
        if let Err(error) = apply_to_window(&window) {
            tracing::warn!(%label, %error, "界面缩放设置已保存，应用到 WebView 失败");
        }
    }
}
