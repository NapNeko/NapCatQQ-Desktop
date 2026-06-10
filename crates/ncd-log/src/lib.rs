//! 桌面会话日志行格式，对齐 legacy `Log.to_string()` 六段结构。
//!
//! 写入由 `src-tauri` 的 tracing layer 完成；本 crate 只定义序列化与 preview 辅助。

pub mod facet;
pub mod line;
pub mod target;

pub use facet::{LogSource, LogType};
pub use line::{format_line, format_line_with_time, preview_line};
pub use target::{log_source_from_target, short_module_from_target};