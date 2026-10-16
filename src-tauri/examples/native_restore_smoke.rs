//! 隔离验证原生最小化/恢复不闪隐，WebView 状态仍由真实调度器恢复。
#[cfg(windows)]
mod smoke {
    use ncd_tauri::webview_scheduler::{self, WebviewScheduler};
    use std::collections::HashMap;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;
    use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent};

    #[derive(Default)]
    struct ProbeState {
        finished: AtomicBool,
        failure: Mutex<Option<String>>,
        checked: Mutex<HashMap<String, usize>>,
        reminimize: Mutex<Option<String>>,
    }

    impl ProbeState {
        fn fail(&self, message: String) {
            eprintln!("NATIVE_RESTORE_FAILURE {message}");
            if let Ok(mut failure) = self.failure.lock() {
                failure.get_or_insert(message);
            }
        }

        fn check(&self) -> Result<(), String> {
            let failure = self.failure.lock().map_err(|error| error.to_string())?;
            failure
                .as_ref()
                .map_or(Ok(()), |message| Err(message.clone()))
        }

        fn checked(&self, label: &str) -> Result<usize, String> {
            Ok(*self
                .checked
                .lock()
                .map_err(|error| error.to_string())?
                .get(label)
                .unwrap_or(&0))
        }
    }

    fn native_event(window: &tauri::Window, event: &WindowEvent) {
        let state = window.app_handle().state::<ProbeState>();
        let exposed = matches!(event, WindowEvent::Focused(true) | WindowEvent::Resized(_))
            && window.is_visible().ok() == Some(true)
            && window.is_minimized().ok() == Some(false);
        webview_scheduler::handle_window_event(window, event);
        if !exposed {
            return;
        }
        if window.is_visible().ok() != Some(true) {
            state.fail(format!(
                "{}: {event:?} 将已露出的原生窗口隐藏",
                window.label()
            ));
        }
        if let Ok(mut checked) = state.checked.lock() {
            *checked.entry(window.label().into()).or_default() += 1;
        }
        // 在真实恢复事件刚返回时再次最小化，截住仍在排队的 WebView 恢复动作。
        let reminimize = state.reminimize.lock().is_ok_and(|mut pending| {
            if pending.as_deref() == Some(window.label()) {
                *pending = None;
                true
            } else {
                false
            }
        });
        if reminimize && let Err(error) = window.minimize() {
            state.fail(format!("{}: 恢复后再次最小化失败: {error}", window.label()));
        }
    }

    async fn confirm(app: &AppHandle, window: &WebviewWindow, hidden: bool) -> Result<(), String> {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
        let mut last = String::new();
        while tokio::time::Instant::now() < deadline {
            app.state::<ProbeState>().check()?;
            let report = webview_scheduler::memory_report(app).await?;
            let native_ready = window.is_minimized().map_err(|error| error.to_string())? == hidden
                && (hidden || window.is_visible().map_err(|error| error.to_string())?);
            if let Some(level) = report
                .levels
                .iter()
                .find(|level| level.label == window.label())
            {
                last = serde_json::to_string(level).map_err(|error| error.to_string())?;
                if native_ready
                    && level.hidden == hidden
                    && level.dormant == hidden
                    && level.applied_hidden == Some(hidden)
                    && level.applied_dormant == Some(hidden)
                    && level.last_error.is_none()
                {
                    eprintln!("NATIVE_RESTORE_STATE {last}");
                    return Ok(());
                }
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
        Err(format!(
            "{}: hidden={hidden} 未确认，最后状态: {last}",
            window.label()
        ))
    }

    async fn confirm_reminimize(app: &AppHandle, window: &WebviewWindow) -> Result<(), String> {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
        loop {
            app.state::<ProbeState>().check()?;
            if app
                .state::<ProbeState>()
                .reminimize
                .lock()
                .map_err(|error| error.to_string())?
                .is_none()
            {
                break;
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(format!("{}: 未收到恢复事件", window.label()));
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        confirm(app, window, true).await?;
        let deadline = tokio::time::Instant::now() + Duration::from_millis(800);
        while tokio::time::Instant::now() < deadline {
            app.state::<ProbeState>().check()?;
            if !window.is_minimized().map_err(|error| error.to_string())? {
                return Err(format!(
                    "{}: 迟到的恢复动作撤销了再次最小化",
                    window.label()
                ));
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        confirm(app, window, true).await
    }

    async fn experiment(app: AppHandle) -> Result<(), String> {
        let mut windows = Vec::new();
        for label in ["main", ncd_tauri::chat_window::CHAT_WINDOW_LABEL] {
            let mut config = if label == "main" {
                app.config()
                    .app
                    .windows
                    .first()
                    .cloned()
                    .ok_or("缺少主窗配置")?
            } else {
                ncd_tauri::chat_window::chat_window_config(&app)?
            };
            config.label = label.into();
            config.title = format!("Native restore smoke: {label}");
            config.visible = false;
            config.url = WebviewUrl::External(
                "about:blank"
                    .parse()
                    .map_err(|error: url::ParseError| error.to_string())?,
            );
            let window = WebviewWindowBuilder::from_config(&app, &config)
                .and_then(|builder| builder.build())
                .map_err(|error| error.to_string())?;
            webview_scheduler::show_window(&window).await?;
            confirm(&app, &window, false).await?;
            windows.push(window);
        }
        for window in &windows {
            for cycle in 0..3 {
                window.minimize().map_err(|error| error.to_string())?;
                confirm(&app, window, true).await?;
                let checked = app.state::<ProbeState>().checked(window.label())?;
                window.unminimize().map_err(|error| error.to_string())?;
                confirm(&app, window, false).await?;
                if app.state::<ProbeState>().checked(window.label())? <= checked {
                    return Err(format!("{}: 恢复未经过原生事件检查", window.label()));
                }
                eprintln!("NATIVE_RESTORE_CYCLE {} {cycle}", window.label());
            }
            window.minimize().map_err(|error| error.to_string())?;
            confirm(&app, window, true).await?;
            *app.state::<ProbeState>()
                .reminimize
                .lock()
                .map_err(|error| error.to_string())? = Some(window.label().into());
            window.unminimize().map_err(|error| error.to_string())?;
            confirm_reminimize(&app, window).await?;
            eprintln!("NATIVE_RESTORE_REMINIMIZE {}", window.label());
            window.unminimize().map_err(|error| error.to_string())?;
            confirm(&app, window, false).await?;
        }
        app.state::<ProbeState>().check()
    }

    pub fn run() -> Result<(), Box<dyn std::error::Error>> {
        let data = tempfile::Builder::new()
            .prefix("ncd-native-restore-")
            .tempdir()?;
        let mut context = tauri::generate_context!();
        for window in &mut context.config_mut().app.windows {
            window.create = false;
            window.data_directory = Some(data.path().to_path_buf());
        }
        let app = tauri::Builder::default()
            .manage(ProbeState::default())
            .manage(WebviewScheduler::default())
            .on_window_event(native_event)
            .setup(|app| {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    let result = experiment(handle.clone()).await;
                    eprintln!("NATIVE_RESTORE_RESULT {result:?}");
                    handle
                        .state::<ProbeState>()
                        .finished
                        .store(true, Ordering::SeqCst);
                    for window in handle.webview_windows().values() {
                        let _ = window.destroy();
                    }
                    handle.exit(if result.is_ok() { 0 } else { 1 });
                });
                Ok(())
            })
            .build(context)?;
        app.run(|app, event| {
            if let tauri::RunEvent::ExitRequested { api, .. } = event
                && !app.state::<ProbeState>().finished.load(Ordering::SeqCst)
            {
                api.prevent_exit();
            }
        });
        Ok(())
    }
}

#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    smoke::run()
}

#[cfg(not(windows))]
fn main() {
    eprintln!("native_restore_smoke 仅支持 Windows/WebView2");
    std::process::exit(1);
}
