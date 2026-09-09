//! dotenv 风格文件的保序改写：只改目标键，注释 / 空行 / 其它键原样保留。
//!
//! Karin 的 `.env`、NoneBot2 的 `.env.prod` 都是这种格式。上游各自的解析器都很宽松
//! （`KEY=value`、`KEY="value"`、行尾 `# 注释`），这里写出统一为 `KEY=value`，
//! 值含空格或 `#` 时加双引号。

use std::fmt;

/// 一次写入
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvWrite {
    pub key: String,
    pub value: String,
}

impl EnvWrite {
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
        }
    }
}

/// 一条键值 + 它上一行的 `# 注释`（Karin WebUI 把这行当字段说明展示）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvEntry {
    pub key: String,
    pub value: String,
    /// 去掉 `#` 与首尾空白后的注释文本；上一行不是注释则为空
    pub comment: String,
}

/// 解析后的 .env 文本（按行保留）
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EnvFile {
    lines: Vec<String>,
}

impl EnvFile {
    pub fn parse(text: &str) -> Self {
        Self {
            lines: text.lines().map(str::to_string).collect(),
        }
    }

    /// 取键值（去引号，去行尾注释）；不存在返回 None
    pub fn get(&self, key: &str) -> Option<String> {
        self.lines.iter().rev().find_map(|line| {
            let (k, v) = split_kv(line)?;
            (k == key).then(|| unquote(v))
        })
    }

    /// NoneBot 把环境变量当大小写不敏感；读真实项目时 `port` / `PORT` 都要认
    pub fn get_ci(&self, key: &str) -> Option<String> {
        let want = key.to_ascii_lowercase();
        self.lines.iter().rev().find_map(|line| {
            let (k, v) = split_kv(line)?;
            (k.eq_ignore_ascii_case(&want)).then(|| unquote(v))
        })
    }

    /// 已有同名键（忽略大小写）则改那一处，避免 `PORT` / `port` 双写
    pub fn set_ci(&mut self, key: &str, value: &str) {
        let existing = self.lines.iter().rev().find_map(|line| {
            let (k, _) = split_kv(line)?;
            k.eq_ignore_ascii_case(key).then(|| k.to_string())
        });
        self.set(existing.as_deref().unwrap_or(key), value);
    }

    /// 按文件顺序列出所有键（重复键取最后一处的位置与值），带上一行注释
    pub fn entries(&self) -> Vec<EnvEntry> {
        let mut out: Vec<EnvEntry> = Vec::new();
        for (i, line) in self.lines.iter().enumerate() {
            let Some((k, v)) = split_kv(line) else {
                continue;
            };
            let entry = EnvEntry {
                key: k.to_string(),
                value: unquote(v),
                comment: self.comment_above(i),
            };
            match out.iter_mut().find(|e| e.key == k) {
                Some(existing) => *existing = entry,
                None => out.push(entry),
            }
        }
        out
    }

    /// 上一行若是 `# 注释` 则返回其文本
    fn comment_above(&self, idx: usize) -> String {
        idx.checked_sub(1)
            .and_then(|p| self.lines.get(p))
            .and_then(|l| comment_text(l))
            .unwrap_or_default()
    }

    /// 删除键（所有出现处）；紧贴其上的注释行一并删掉，避免留下孤儿说明
    pub fn remove(&mut self, key: &str) {
        let positions: Vec<usize> = self
            .lines
            .iter()
            .enumerate()
            .filter_map(|(i, line)| split_kv(line).filter(|(k, _)| *k == key).map(|_| i))
            .collect();
        for &i in positions.iter().rev() {
            self.lines.remove(i);
            if let Some(p) = i.checked_sub(1)
                && self.lines.get(p).is_some_and(|l| comment_text(l).is_some())
            {
                self.lines.remove(p);
            }
        }
    }

    /// `set` 的带注释版本：注释非空时保证键上一行是 `# 注释`（已有则改写，没有则插入）；
    /// 注释为空则不动已有注释行
    pub fn set_with_comment(&mut self, key: &str, value: &str, comment: &str) {
        self.set(key, value);
        let comment = comment.trim();
        if comment.is_empty() {
            return;
        }
        let Some(idx) = self
            .lines
            .iter()
            .rposition(|line| split_kv(line).is_some_and(|(k, _)| k == key))
        else {
            return;
        };
        let rendered = format!("# {comment}");
        match idx.checked_sub(1) {
            Some(p) if self.lines.get(p).is_some_and(|l| comment_text(l).is_some()) => {
                self.lines[p] = rendered;
            }
            _ => self.lines.insert(idx, rendered),
        }
    }

    /// 存在则原位替换（多处取最后一处，其余删除），不存在则追加到末尾
    pub fn set(&mut self, key: &str, value: &str) {
        let rendered = format!("{key}={}", quote_if_needed(value));
        let positions: Vec<usize> = self
            .lines
            .iter()
            .enumerate()
            .filter_map(|(i, line)| split_kv(line).filter(|(k, _)| *k == key).map(|_| i))
            .collect();
        match positions.split_last() {
            Some((last, earlier)) => {
                self.lines[*last] = rendered;
                for &i in earlier.iter().rev() {
                    self.lines.remove(i);
                }
            }
            None => {
                if self.lines.last().is_some_and(|l| !l.trim().is_empty()) {
                    // 与前面内容留一空行，人读 diff 舒服
                    self.lines.push(String::new());
                }
                self.lines.push(rendered);
            }
        }
    }

    pub fn apply(&mut self, writes: &[EnvWrite]) {
        for w in writes {
            self.set(&w.key, &w.value);
        }
    }

    pub fn render(&self) -> String {
        let mut out = self.lines.join("\n");
        out.push('\n');
        out
    }
}

impl fmt::Display for EnvFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.render())
    }
}

/// `# 注释` 行 → 注释正文；非注释行返回 None
fn comment_text(line: &str) -> Option<String> {
    let trimmed = line.trim();
    trimmed
        .strip_prefix('#')
        .map(|rest| rest.trim().to_string())
}

/// `KEY=value` → (KEY, raw value)；注释 / 空行 / 无 `=` 的行返回 None
fn split_kv(line: &str) -> Option<(&str, &str)> {
    let trimmed = line.trim_start();
    if trimmed.starts_with('#') {
        return None;
    }
    let (k, v) = trimmed.split_once('=')?;
    let k = k.trim().trim_start_matches("export ").trim();
    if k.is_empty() || k.contains(char::is_whitespace) {
        return None;
    }
    Some((k, v.trim()))
}

fn unquote(raw: &str) -> String {
    let raw = raw.trim();
    if raw.len() >= 2 {
        let bytes = raw.as_bytes();
        if (bytes[0] == b'"' && bytes[raw.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[raw.len() - 1] == b'\'')
        {
            return raw[1..raw.len() - 1].to_string();
        }
    }
    // 未加引号：去掉行尾注释
    match raw.find(" #") {
        Some(idx) => raw[..idx].trim().to_string(),
        None => raw.to_string(),
    }
}

fn quote_if_needed(value: &str) -> String {
    if value.is_empty() {
        return String::new();
    }
    if value.contains(char::is_whitespace) || value.contains('#') || value.contains('"') {
        format!("\"{}\"", value.replace('"', "\\\""))
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KARIN_ENV: &str = "# 是否启用HTTP\nHTTP_ENABLE=true\n# HTTP监听端口\nHTTP_PORT=7777\n# HTTP监听地址\nHTTP_HOST=0.0.0.0\n# HTTP鉴权秘钥 仅用于karin自身Api\nHTTP_AUTH_KEY=abc123\n# ws_server鉴权秘钥\nWS_SERVER_AUTH_KEY=\n\n\nRUNTIME=node\nLOG_FNC_COLOR=\"#E1D919\"\n";

    #[test]
    fn get_ci_and_set_ci_keep_original_key_case() {
        let mut env = EnvFile::parse("port=13120\nDriver=~httpx\n");
        assert_eq!(env.get_ci("PORT").as_deref(), Some("13120"));
        env.set_ci("PORT", "8080");
        env.set_ci("DRIVER", "~fastapi+~httpx");
        assert_eq!(env.render(), "port=8080\nDriver=~fastapi+~httpx\n");
    }

    #[test]
    fn get_reads_quoted_and_empty_values() {
        let env = EnvFile::parse(KARIN_ENV);
        assert_eq!(env.get("HTTP_PORT").as_deref(), Some("7777"));
        assert_eq!(env.get("WS_SERVER_AUTH_KEY").as_deref(), Some(""));
        assert_eq!(env.get("LOG_FNC_COLOR").as_deref(), Some("#E1D919"));
        assert_eq!(env.get("MISSING"), None);
    }

    #[test]
    fn set_replaces_in_place_and_keeps_comments() {
        let mut env = EnvFile::parse(KARIN_ENV);
        env.set("HTTP_PORT", "7801");
        env.set("WS_SERVER_AUTH_KEY", "tok-1");
        let out = env.render();
        assert!(out.contains("# HTTP监听端口\nHTTP_PORT=7801\n"));
        assert!(out.contains("# ws_server鉴权秘钥\nWS_SERVER_AUTH_KEY=tok-1\n"));
        assert!(out.contains("LOG_FNC_COLOR=\"#E1D919\""));
        // 行数不变（原位替换）
        assert_eq!(out.lines().count(), KARIN_ENV.lines().count());
    }

    #[test]
    fn set_appends_missing_key_with_blank_separator() {
        let mut env = EnvFile::parse("A=1\n");
        env.set("B", "two words");
        assert_eq!(env.render(), "A=1\n\nB=\"two words\"\n");
    }

    #[test]
    fn duplicate_keys_collapse_to_last_occurrence() {
        let mut env = EnvFile::parse("K=1\nX=0\nK=2\n");
        env.set("K", "9");
        assert_eq!(env.render(), "X=0\nK=9\n");
        assert_eq!(env.get("K").as_deref(), Some("9"));
    }

    #[test]
    fn round_trip_is_stable() {
        let env = EnvFile::parse(KARIN_ENV);
        assert_eq!(env.render(), KARIN_ENV);
    }

    #[test]
    fn entries_carry_preceding_comment_in_file_order() {
        let env = EnvFile::parse(KARIN_ENV);
        let entries = env.entries();
        let keys: Vec<&str> = entries.iter().map(|e| e.key.as_str()).collect();
        assert_eq!(
            keys,
            [
                "HTTP_ENABLE",
                "HTTP_PORT",
                "HTTP_HOST",
                "HTTP_AUTH_KEY",
                "WS_SERVER_AUTH_KEY",
                "RUNTIME",
                "LOG_FNC_COLOR"
            ]
        );
        assert_eq!(entries[1].comment, "HTTP监听端口");
        assert_eq!(entries[1].value, "7777");
        // RUNTIME 上一行是空行 → 无注释
        assert_eq!(entries[5].comment, "");
        assert_eq!(entries[6].value, "#E1D919");
    }

    #[test]
    fn remove_drops_key_and_its_comment_line() {
        let mut env = EnvFile::parse("# a\nA=1\nB=2\n# c\nC=3\n");
        env.remove("A");
        env.remove("C");
        assert_eq!(env.render(), "B=2\n");
        env.remove("MISSING");
        assert_eq!(env.render(), "B=2\n");
    }

    #[test]
    fn set_with_comment_inserts_or_rewrites_comment_line() {
        let mut env = EnvFile::parse("# old\nA=1\nB=2\n");
        env.set_with_comment("A", "9", "new");
        env.set_with_comment("B", "3", "");
        env.set_with_comment("C", "x", "fresh");
        assert_eq!(env.render(), "# new\nA=9\nB=3\n\n# fresh\nC=x\n");
    }
}
