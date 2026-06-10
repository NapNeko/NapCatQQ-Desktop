//! 桌面会话日志：tracing 落盘（对齐 legacy `<data_root>/log/*.log`）。

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::panic;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use ncd_log::facet::{LogSource, LogType};
use ncd_log::format_line;
use ncd_log::log_source_from_target;
use ncd_log::short_module_from_target;
use ncd_runtime::crash_bundle::{write_crash_bundle, CrashBundleInput};
use ncd_runtime::desktop_log;
use tracing::{Event, Subscriber};
use tracing_subscriber::filter::EnvFilter;
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::prelude::*;
use tracing_subscriber::Registry;

use crate::desktop_log_format::{
    format_tracing_event, map_tracing_level, should_capture_tracing,
};

static SESSION: OnceLock<Arc<DesktopLogSession>> = OnceLock::new();
static PANIC_CTX: OnceLock<PanicContext> = OnceLock::new();

struct PanicContext {
    data_root: PathBuf,
    app_version: String,
}

struct DesktopLogSession {
    log_path: PathBuf,
    file: Mutex<std::fs::File>,
}

struct DesktopLogLayer {
    session: Arc<DesktopLogSession>,
}

impl<S: Subscriber> Layer<S> for DesktopLogLayer {
    fn enabled(&self, metadata: &tracing::Metadata<'_>, _ctx: Context<'_, S>) -> bool {
        should_capture_tracing(metadata.target(), metadata.level())
    }

    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        if !should_capture_tracing(event.metadata().target(), event.metadata().level()) {
            return;
        }
        let level = map_tracing_level(event.metadata().level());
        let line = format_tracing_event(event, level);
        if line.is_empty() {
            return;
        }
        write_line_to_session(&self.session, &line);
    }
}

fn write_line_to_session(session: &DesktopLogSession, line: &str) {
    if let Ok(mut file) = session.file.lock() {
        let _ = file.write_all(line.as_bytes());
        let _ = file.flush();
    }
}

/// 直接写一行（无 tracing 订阅时也可用，例如启动横幅、panic、CRIT）。
pub fn write_session_line(level: &str, position: &str, message: &str) {
    let Some(session) = SESSION.get() else {
        return;
    };
    let source = log_source_from_target(position);
    let module = short_module_from_target(position);
    let line = format_line(level, LogType::NoneType, source, &module, message);
    write_line_to_session(session, &line);
}

fn create_session_log_file(data_root: &Path) -> std::io::Result<PathBuf> {
    let dir = desktop_log::desktop_log_dir(data_root);
    fs::create_dir_all(&dir)?;
    let _ = desktop_log::purge_stale_logs(data_root, 7);
    let stamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S");
    let path = dir.join(format!("{stamp}.log"));
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    Ok(path)
}

fn default_env_filter() -> EnvFilter {
    let spec = if cfg!(debug_assertions) {
        "info,ncd_runtime=debug,ncd_host=debug,ncd_component=info,ncd_deploy=info,ncd_network=info,ncd_tauri=info"
    } else {
        "info,ncd_runtime=info,ncd_host=warn,ncd_component=info,ncd_deploy=info,ncd_network=warn,ncd_tauri=info"
    };
    EnvFilter::try_new(spec).unwrap_or_else(|_| EnvFilter::new("info"))
}

fn install_panic_hook(data_root: PathBuf, app_version: String) {
    let _ = PANIC_CTX.set(PanicContext {
        data_root,
        app_version,
    });
    let default = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let payload = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| (*s).to_string())
            .or_else(|| {
                info.payload()
                    .downcast_ref::<String>()
                    .map(|s| s.clone())
            })
            .unwrap_or_else(|| "panic without message".to_string());
        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "unknown".to_string());
        let summary = format!("panic at {location}: {payload}");
        write_session_line("CRIT", "ncd::panic", &summary);
        if let Some(ctx) = PANIC_CTX.get() {
            let log_path = SESSION.get().map(|s| s.log_path.clone());
            let tb = format!("{info}\nlocation={location}\n");
            if let Ok(bundle) = write_crash_bundle(&CrashBundleInput {
                trigger: "rust.panic".to_string(),
                exception_summary: summary.clone(),
                traceback_text: tb,
                log_path,
                data_root: ctx.data_root.clone(),
                app_version: ctx.app_version.clone(),
            }) {
                write_session_line(
                    "INFO",
                    "ncd::crash_bundle",
                    &format!("已生成崩溃诊断包: {}", bundle.display()),
                );
            }
        }
        default(info);
    }));
}

pub fn init_desktop_logging(data_root: &Path, _bus: ncd_runtime::BroadcastEventBus) {
    if SESSION.get().is_some() {
        return;
    }
    let app_version = env!("CARGO_PKG_VERSION").to_string();
    let log_path = match create_session_log_file(data_root) {
        Ok(p) => p,
        Err(err) => {
            eprintln!("[desktop_log] create session file failed: {err}");
            return;
        }
    };
    let file = match OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
    {
        Ok(f) => f,
        Err(err) => {
            eprintln!("[desktop_log] open session file failed: {err}");
            return;
        }
    };
    let session = Arc::new(DesktopLogSession {
        log_path: log_path.clone(),
        file: Mutex::new(file),
    });
    let layer = DesktopLogLayer {
        session: Arc::clone(&session),
    };
    let filter = default_env_filter();
    let _ = Registry::default()
        .with(filter)
        .with(layer)
        .try_init();
    let _ = SESSION.set(session);

    install_panic_hook(data_root.to_path_buf(), app_version);

    write_session_line(
        "INFO",
        "ncd::desktop",
        &format!("Desktop 日志会话已开始，文件: {}", log_path.display()),
    );
    write_session_line(
        "INFO",
        "ncd::desktop",
        "行格式：时间 | 等级 | 来源 模块 | 说明。相同说明 2 秒内只记一条。",
    );
}

pub fn active_log_path() -> Option<PathBuf> {
    SESSION.get().map(|s| s.log_path.clone())
}