//! 桌面会话日志：tracing 落盘（对齐 legacy `<data_root>/log/*.log`）。
//!
//! 不从 tracing 层 publish 到事件总线（会与 IPC 转发形成正反馈）。
//! 设置页通过轮询 `tail_desktop_log` 读文件。

use chrono::Local;
use ncd_runtime::desktop_log;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use tracing::field::{Field, Visit};
use tracing::{Event, Level, Subscriber};
use tracing_subscriber::filter::EnvFilter;
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::prelude::*;
use tracing_subscriber::Registry;

static SESSION: OnceLock<Arc<DesktopLogSession>> = OnceLock::new();

struct DesktopLogSession {
    log_path: PathBuf,
    file: Mutex<std::fs::File>,
}

struct FieldVisitor {
    message: String,
}

impl Visit for FieldVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{value:?}");
            if self.message.starts_with('"') && self.message.ends_with('"') && self.message.len() >= 2 {
                self.message = self.message[1..self.message.len() - 1].to_string();
            }
        }
    }
}

struct DesktopLogLayer {
    session: Arc<DesktopLogSession>,
}

impl<S: Subscriber> Layer<S> for DesktopLogLayer {
    fn enabled(&self, metadata: &tracing::Metadata<'_>, _ctx: Context<'_, S>) -> bool {
        should_capture_tracing(metadata)
    }

    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        if !should_capture_tracing(event.metadata()) {
            return;
        }
        let mut visitor = FieldVisitor {
            message: String::new(),
        };
        event.record(&mut visitor);
        if visitor.message.is_empty() {
            visitor.message = "(no message)".to_string();
        }
        let level = map_tracing_level(event.metadata().level());
        let target = event.metadata().target();
        let time = Local::now().format("%y-%m-%d %H:%M:%S");
        let line = format!(
            "{time} | [{level}] | [ NONE_TYPE ] | [ CORE ] | [{target}] | {}\n",
            visitor.message
        );
        write_line_to_session(&self.session, &line);
    }
}

/// 禁止记录 IPC 事件转发诊断（曾与 publish 形成正反馈）；其余 INFO+ 写入会话文件。
fn should_capture_tracing(metadata: &tracing::Metadata<'_>) -> bool {
    let target = metadata.target();
    if target.contains("event_emit") {
        return false;
    }
    matches!(
        *metadata.level(),
        Level::ERROR | Level::WARN | Level::INFO
    ) || (cfg!(debug_assertions)
        && matches!(*metadata.level(), Level::DEBUG | Level::TRACE))
}

fn map_tracing_level(level: &Level) -> &'static str {
    match *level {
        Level::ERROR => "EROR",
        Level::WARN => "WARN",
        Level::INFO => "INFO",
        Level::DEBUG => "DBUG",
        Level::TRACE => "TRCE",
    }
}

fn write_line_to_session(session: &DesktopLogSession, line: &str) {
    if let Ok(mut file) = session.file.lock() {
        let _ = file.write_all(line.as_bytes());
        let _ = file.flush();
    }
}

/// 直接写一行（无 tracing 订阅时也可用，例如启动横幅）。
pub fn write_session_line(level: &str, target: &str, message: &str) {
    let Some(session) = SESSION.get() else {
        return;
    };
    let time = Local::now().format("%y-%m-%d %H:%M:%S");
    let line = format!(
        "{time} | [{level}] | [ NONE_TYPE ] | [ CORE ] | [{target}] | {message}\n"
    );
    write_line_to_session(session, &line);
}

fn create_session_log_file(data_root: &std::path::Path) -> std::io::Result<PathBuf> {
    let dir = desktop_log::desktop_log_dir(data_root);
    fs::create_dir_all(&dir)?;
    let _ = desktop_log::purge_stale_logs(data_root, 7);
    let stamp = Local::now().format("%Y-%m-%d_%H-%M-%S");
    let path = dir.join(format!("{stamp}.log"));
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)?;
    Ok(path)
}

fn default_env_filter() -> EnvFilter {
    let spec = if cfg!(debug_assertions) {
        "info,ncd_runtime=debug,ncd_host=debug,ncd_component=info,ncd_deploy=info,ncd_tauri=info"
    } else {
        "info,ncd_runtime=info,ncd_host=warn,ncd_component=info,ncd_deploy=info,ncd_tauri=info"
    };
    EnvFilter::try_new(spec).unwrap_or_else(|_| EnvFilter::new("info"))
}

/// 启动期调用一次：注册全局 tracing subscriber，并缓存会话供 tail 命令使用。
pub fn init_desktop_logging(data_root: &std::path::Path, _bus: ncd_runtime::BroadcastEventBus) {
    if SESSION.get().is_some() {
        return;
    }
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

    write_session_line(
        "INFO",
        "ncd::desktop",
        &format!(
            "Desktop 日志会话已开始，文件: {}",
            log_path.display()
        ),
    );
    write_session_line(
        "INFO",
        "ncd::desktop",
        "提示: 旧版 Python 全链路 Logger 尚未 1:1 迁完；本文件记录 tracing(INFO+) 与显式 write_session_line。",
    );
}

pub fn active_log_path() -> Option<PathBuf> {
    SESSION.get().map(|s| s.log_path.clone())
}