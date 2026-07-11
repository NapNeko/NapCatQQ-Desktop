//! SnowLuma 日志噪声过滤（与 NapCat 分离）
//!
//! L1：sanitize CSI。QQ 宿主噪声：QqConsoleNoiseFilter。
//! L2 SL 专属：SQLite experimental warning 等。
//! 历史：磁盘只留 current + 一代 .prev；UI 再裁最后一次 starting 会话，并按 UIN 收窄。

use ncd_deploy::QqConsoleNoiseFilter;

use super::log_sanitize::sanitize_log_line;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnowLumaLogNoiseAction {
    Keep,
    Drop,
}

#[derive(Debug, Default, Clone)]
pub struct SnowLumaLogNoiseFilter {
    qq: QqConsoleNoiseFilter,
}

impl SnowLumaLogNoiseFilter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn classify(&mut self, line: &str) -> SnowLumaLogNoiseAction {
        if line.is_empty() {
            return SnowLumaLogNoiseAction::Drop;
        }
        if self.qq.is_noise(line) {
            return SnowLumaLogNoiseAction::Drop;
        }
        if is_snowluma_only_noise(line) {
            return SnowLumaLogNoiseAction::Drop;
        }
        SnowLumaLogNoiseAction::Keep
    }

    pub fn process_line(&mut self, raw: &str) -> Option<String> {
        let cleaned = sanitize_log_line(raw);
        self.process_sanitized_line(cleaned)
    }

    pub fn process_sanitized_line(&mut self, cleaned: String) -> Option<String> {
        let trimmed = cleaned.trim_end();
        if trimmed.is_empty() {
            return None;
        }
        if self.classify(trimmed) == SnowLumaLogNoiseAction::Drop {
            return None;
        }
        // 无尾空白时复用原 String，避免二次分配
        if trimmed.len() == cleaned.len() {
            Some(cleaned)
        } else {
            Some(trimmed.to_string())
        }
    }
}

pub fn filter_snowluma_console_lines<I, S>(lines: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut filter = SnowLumaLogNoiseFilter::new();
    lines
        .into_iter()
        .filter_map(|line| filter.process_line(line.as_ref()))
        .collect()
}

/// Bot 日志页历史：先滤噪声，再把 daemon 裁到当前会话并按 UIN 收窄，最后接上 bot 文件行。
pub fn prepare_snowluma_bot_history_lines(
    bot_raw_lines: impl IntoIterator<Item = impl AsRef<str>>,
    daemon_raw_lines: impl IntoIterator<Item = impl AsRef<str>>,
    qq_id: &str,
) -> Vec<String> {
    let mut filter = SnowLumaLogNoiseFilter::new();
    let bot: Vec<String> = bot_raw_lines
        .into_iter()
        .filter_map(|l| filter.process_line(l.as_ref()))
        .collect();

    let mut filter = SnowLumaLogNoiseFilter::new();
    let daemon: Vec<String> = daemon_raw_lines
        .into_iter()
        .filter_map(|l| filter.process_line(l.as_ref()))
        .collect();
    let daemon = scope_daemon_log_to_current_session(daemon);
    let daemon = filter_daemon_lines_for_bot(daemon, qq_id);

    let mut out = daemon;
    out.extend(bot);
    out
}

/// 只保留最后一次 `SnowLuma starting` 起的行（含该行）。
pub fn scope_daemon_log_to_current_session(mut lines: Vec<String>) -> Vec<String> {
    if let Some(i) = lines
        .iter()
        .rposition(|line| line.contains("SnowLuma starting"))
    {
        lines.drain(0..i);
    }
    lines
}

/// 丢掉明确属于其它 UIN 的行；全局行（App/WebUI/Hook 无 UIN）保留。
pub fn filter_daemon_lines_for_bot(mut lines: Vec<String>, qq_id: &str) -> Vec<String> {
    if qq_id.is_empty() {
        return lines;
    }
    lines.retain(|line| daemon_line_belongs_to_bot(line, qq_id));
    lines
}

fn daemon_line_belongs_to_bot(line: &str, qq_id: &str) -> bool {
    // 行内显式 UIN=其它
    if let Some(uin) = extract_uin_token(line) {
        return uin == qq_id;
    }
    // [572381217] 标签
    if let Some(tagged) = extract_bracket_uin(line) {
        return tagged == qq_id;
    }
    true
}

fn extract_uin_token(line: &str) -> Option<&str> {
    // UIN=572381217
    let key = "UIN=";
    let idx = line.find(key)?;
    let rest = &line[idx + key.len()..];
    let end = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    let uin = &rest[..end];
    if uin.is_empty() { None } else { Some(uin) }
}

fn extract_bracket_uin(line: &str) -> Option<&str> {
    //  " [572381217]  " 或 "[2707600964]"
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'[' {
            let start = i + 1;
            let mut j = start;
            while j < bytes.len() && bytes[j].is_ascii_digit() {
                j += 1;
            }
            if j > start && j < bytes.len() && bytes[j] == b']' {
                // 至少 5 位，避免误伤 [App] 等
                if j - start >= 5 {
                    return Some(std::str::from_utf8(&bytes[start..j]).ok()?);
                }
            }
        }
        i += 1;
    }
    None
}

fn is_snowluma_only_noise(line: &str) -> bool {
    line.contains("ExperimentalWarning: SQLite is an experimental feature")
        || line.contains("Use `node --trace-warnings")
        || line.contains("X connection error received")
        || line.contains("ui/gfx/x/connection.cc")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_qq_host_noise_via_shared_rule() {
        let mut f = SnowLumaLogNoiseFilter::new();
        assert!(
            f.process_line("version_config_filename :/x/config.json")
                .is_none()
        );
        assert!(
            f.process_line("Failed to connect to the bus: /tmp/dbus-x")
                .is_none()
        );
        assert!(f.process_line("linux-bugly: init bugly ...").is_none());
    }

    #[test]
    fn keeps_snowluma_business() {
        let mut f = SnowLumaLogNoiseFilter::new();
        assert!(
            f.process_line("22:21:24 INFO               [App] SnowLuma starting")
                .is_some()
        );
        assert!(
            f.process_line("[INFO] onebot instance online uin=123")
                .is_some()
        );
    }

    #[test]
    fn drops_sqlite_experimental_noise() {
        let mut f = SnowLumaLogNoiseFilter::new();
        assert!(f
            .process_line(
                "(node:80916) ExperimentalWarning: SQLite is an experimental feature and might change at any time"
            )
            .is_none());
    }

    #[test]
    fn scopes_to_last_starting_session() {
        let lines = vec![
            "22:21:24 INFO [App] SnowLuma starting".into(),
            "22:22:04 INFO old session".into(),
            "23:44:54 INFO [App] SnowLuma starting".into(),
            "23:45:05 INFO current only".into(),
        ];
        let scoped = scope_daemon_log_to_current_session(lines);
        assert_eq!(scoped.len(), 2);
        assert!(scoped[0].contains("23:44:54") || scoped[0].contains("SnowLuma starting"));
        assert!(scoped[1].contains("current only"));
    }

    #[test]
    fn filters_other_uin_rows() {
        let lines = vec![
            "09:50:46 INFO [OneBot] session started: UIN=2707600964".into(),
            "09:50:46 OK    [2707600964] [OneBot.HTTP] listening".into(),
            "20:07:17 INFO [OneBot] session started: UIN=572381217".into(),
            "20:07:17 OK    [572381217]  [OneBot.HTTP] listening".into(),
            "20:06:56 INFO [WebUI] listening http://0.0.0.0:5099".into(),
        ];
        let kept = filter_daemon_lines_for_bot(lines, "572381217");
        assert_eq!(kept.len(), 3);
        assert!(kept.iter().all(|l| !l.contains("2707600964")));
    }

    #[test]
    fn prepare_history_does_not_dump_ancient_sessions() {
        let daemon = vec![
            "22:21:24 INFO [App] SnowLuma starting",
            "22:22:04 INFO old",
            "20:10:57 INFO [App] SnowLuma starting",
            "20:10:57 INFO [WebUI] listening",
            "20:07:17 INFO [OneBot] session started: UIN=572381217",
        ];
        let bot = vec!["noise DroppedFrame(1): host_id=1", "bot local line"];
        let out = prepare_snowluma_bot_history_lines(bot, daemon, "572381217");
        assert!(
            out.iter()
                .all(|l| !l.contains("22:21:24") && !l.contains("old"))
        );
        assert!(
            out.iter()
                .any(|l| l.contains("20:10:57") || l.contains("SnowLuma starting"))
        );
        assert!(out.iter().any(|l| l.contains("bot local line")));
        assert!(out.iter().all(|l| !l.contains("DroppedFrame")));
    }
}
