//! 将 tracing 事件格式化为桌面会话行（供 Tauri layer 使用）。

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use ncd_log::facet::{LogSource, LogType};
use ncd_log::format_line;
use ncd_log::{log_source_from_target, short_module_from_target};
use tracing::field::{Field, Visit};
use tracing::{Event, Level};

const DEDUPE_WINDOW: Duration = Duration::from_secs(2);

static DEDUPE: Mutex<Option<DedupeState>> = Mutex::new(None);

struct DedupeState {
    last: HashMap<String, Instant>,
}

fn dedupe_should_skip(key: &str) -> bool {
    let now = Instant::now();
    let Ok(mut guard) = DEDUPE.lock() else {
        return false;
    };
    let state = guard.get_or_insert(DedupeState {
        last: HashMap::new(),
    });
    if let Some(t) = state.last.get(key) {
        if now.duration_since(*t) < DEDUPE_WINDOW {
            return true;
        }
    }
    state.last.insert(key.to_string(), now);
    false
}

pub struct TracingFieldVisitor {
    pub message: String,
    pub bot_id: Option<String>,
    pub server_id: Option<String>,
    pub qq_id: Option<u64>,
    pub host: Option<String>,
    pub url: Option<String>,
    pub err: Option<String>,
}

impl TracingFieldVisitor {
    pub fn new() -> Self {
        Self {
            message: String::new(),
            bot_id: None,
            server_id: None,
            qq_id: None,
            host: None,
            url: None,
            err: None,
        }
    }
}

impl Visit for TracingFieldVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        let name = field.name();
        let text = format!("{value:?}");
        let unquoted = |s: &str| -> String {
            if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
                s[1..s.len() - 1].to_string()
            } else {
                s.to_string()
            }
        };
        match name {
            "message" => self.message = unquoted(&text),
            "bot_id" => self.bot_id = Some(unquoted(&text)),
            "server_id" => self.server_id = Some(unquoted(&text)),
            "host" => self.host = Some(unquoted(&text)),
            "url" => self.url = Some(unquoted(&text)),
            "error" | "err" => self.err = Some(text),
            _ => {}
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        match field.name() {
            "message" => self.message = value.to_string(),
            "bot_id" => self.bot_id = Some(value.to_string()),
            "server_id" => self.server_id = Some(value.to_string()),
            "host" => self.host = Some(value.to_string()),
            "url" => self.url = Some(value.to_string()),
            "error" | "err" => self.err = Some(value.to_string()),
            _ => {}
        }
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        if field.name() == "qq_id" {
            self.qq_id = Some(value);
        }
    }
}

pub fn enrich_message(visitor: &TracingFieldVisitor) -> String {
    let mut parts = Vec::new();
    if let Some(ref id) = visitor.bot_id {
        parts.push(format!("bot={id}"));
    }
    if let Some(qq) = visitor.qq_id {
        parts.push(format!("qq={qq}"));
    }
    if let Some(ref id) = visitor.server_id {
        parts.push(format!("server={id}"));
    }
    if let Some(ref h) = visitor.host {
        parts.push(format!("host={h}"));
    }
    if let Some(ref u) = visitor.url {
        let short = if u.len() > 80 {
            format!("{}…", &u[..80])
        } else {
            u.clone()
        };
        parts.push(format!("url={short}"));
    }
    if let Some(ref e) = visitor.err {
        parts.push(format!("err={e}"));
    }
    let base = if visitor.message.is_empty() {
        "(no message)".to_string()
    } else {
        visitor.message.clone()
    };
    if parts.is_empty() {
        base
    } else {
        format!("{base} ({})", parts.join(", "))
    }
}

pub fn format_tracing_event(event: &Event<'_>, level: &str) -> String {
    let mut visitor = TracingFieldVisitor::new();
    event.record(&mut visitor);
    let target = event.metadata().target();
    let module = short_module_from_target(target);
    let source = log_source_from_target(target);
    let message = enrich_message(&visitor);
    let dedupe_key = format!("{level}|{target}|{message}");
    if dedupe_should_skip(&dedupe_key) {
        return String::new();
    }
    format_line(level, LogType::NoneType, source, &module, &message)
}

pub fn map_tracing_level(level: &Level) -> &'static str {
    match *level {
        Level::ERROR => "EROR",
        Level::WARN => "WARN",
        Level::INFO => "INFO",
        Level::DEBUG => "DBUG",
        Level::TRACE => "TRCE",
    }
}

pub fn should_capture_tracing(target: &str, level: &Level) -> bool {
    if target.contains("event_emit") {
        return false;
    }
    matches!(*level, Level::ERROR | Level::WARN | Level::INFO)
        || (cfg!(debug_assertions) && matches!(*level, Level::DEBUG | Level::TRACE))
}