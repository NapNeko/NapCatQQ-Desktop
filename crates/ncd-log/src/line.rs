//! 六段行：时间 | [LEVEL] | type | source | position | message

use chrono::Local;

use crate::facet::{LogSource, LogType};

/// Legacy 等级标签（与 ncd-runtime::desktop_log::filter_preview_text 一致）。
pub fn level_tag(level: &str) -> String {
    format!("[{level}]")
}

/// 用当前本地时间格式化一行（含末尾 \n）。
pub fn format_line(
    level: &str,
    log_type: LogType,
    log_source: LogSource,
    position: &str,
    message: &str,
) -> String {
    let time = Local::now().format("%y-%m-%d %H:%M:%S");
    format_line_with_time(
        &time.to_string(),
        level,
        log_type,
        log_source,
        position,
        message,
    )
}

/// 指定时间字符串（用于测试或重放）。
pub fn format_line_with_time(
    time_text: &str,
    level: &str,
    log_type: LogType,
    log_source: LogSource,
    position: &str,
    message: &str,
) -> String {
    format!(
        "{time_text} | {} | {} | {} | [{position}] | {message}\n",
        level_tag(level),
        log_type.segment(),
        log_source.segment(),
    )
}

/// UI preview：时间 | 等级 | [来源] 模块 | 消息（比旧版三段多一列模块，便于读懂）。
pub fn preview_line(line: &str) -> String {
    let newline = if line.ends_with('\n') { "\n" } else { "" };
    let raw = line.trim_end_matches('\n');
    let parts: Vec<&str> = raw.splitn(6, " | ").collect();
    if parts.len() != 6 {
        return line.to_string();
    }
    let time_text = parts[0];
    let level_text = parts[1];
    let source_text = parts[3].trim();
    let position = parts[4].trim_start_matches('[').trim_end_matches(']');
    let message_text = parts[5];
    format!(
        "{time_text} | {level_text} | {source_text} {position} | {message_text}{newline}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facet::{LogSource, LogType};

    #[test]
    fn six_field_round_trip_preview() {
        let line = format_line_with_time(
            "26-03-22 13:42:22",
            "WARN",
            LogType::NoneType,
            LogSource::Ui,
            "default > <qt>:0",
            "warn line",
        );
        assert_eq!(
            line,
            "26-03-22 13:42:22 | [WARN] | [ NONE_TYPE ] | [  UI  ] | [default > <qt>:0] | warn line\n"
        );
        assert_eq!(
            preview_line(&line),
            "26-03-22 13:42:22 | [WARN] | [  UI  ] default > <qt>:0 | warn line\n"
        );
    }
}