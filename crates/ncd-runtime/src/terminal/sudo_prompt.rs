//! 只在当前输出行仍是 sudo 提示时允许代填；状态不跨输入和终端重开复用。

use std::time::Duration;

use tokio::time::Instant;

const MAX_LINE: usize = 1024;
const MAX_OSC: usize = 1024;
const PROMPT_VALID_FOR: Duration = Duration::from_secs(60);

#[derive(Default)]
enum Escape {
    #[default]
    Text,
    Start,
    Csi,
    Osc,
    OscEnd,
    String,
    StringEnd,
}

#[derive(Default)]
pub(super) struct SudoPrompt {
    line: Vec<u8>,
    escape: Escape,
    osc: Vec<u8>,
    osc_overflow: bool,
    editing_command: bool,
    line_incomplete: bool,
    seen_at: Option<Instant>,
}

impl SudoPrompt {
    pub(super) fn observe(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            match self.escape {
                Escape::Text => match byte {
                    0x1b => self.escape = Escape::Start,
                    b'\r' | b'\n' => self.clear_line(),
                    0x08 => self.invalidate_line(),
                    b'\t' | 0x20..=0xff => {
                        if self.line.len() == MAX_LINE {
                            self.invalidate_line();
                        }
                        if !self.line_incomplete {
                            self.line.push(byte);
                        }
                    }
                    _ => {}
                },
                Escape::Start => {
                    self.escape = match byte {
                        b'[' => Escape::Csi,
                        b']' => {
                            self.osc.clear();
                            self.osc_overflow = false;
                            Escape::Osc
                        }
                        b'P' | b'^' | b'_' | b'X' => {
                            self.invalidate_line();
                            Escape::String
                        }
                        _ => {
                            self.invalidate_line();
                            Escape::Text
                        }
                    };
                }
                Escape::Csi => {
                    if (0x40..=0x7e).contains(&byte) {
                        // 颜色不移动光标；清行、移动光标等不能沿用旧行的提示。
                        if byte != b'm' {
                            self.invalidate_line();
                        }
                        self.escape = Escape::Text;
                    }
                }
                Escape::Osc => match byte {
                    0x07 => self.finish_osc(),
                    0x1b => self.escape = Escape::OscEnd,
                    _ => {
                        if self.osc.len() < MAX_OSC {
                            self.osc.push(byte);
                        } else {
                            self.osc_overflow = true;
                        }
                    }
                },
                Escape::OscEnd => {
                    if byte == b'\\' {
                        self.finish_osc();
                    } else {
                        self.osc_overflow = true;
                        self.escape = Escape::Osc;
                    }
                }
                Escape::String => {
                    if byte == 0x1b {
                        self.escape = Escape::StringEnd;
                    }
                }
                Escape::StringEnd => {
                    self.escape = if byte == b'\\' {
                        Escape::Text
                    } else {
                        Escape::String
                    };
                }
            }
        }
        if !self.editing_command && !self.line_incomplete && is_prompt(&self.line) {
            self.seen_at.get_or_insert_with(Instant::now);
        } else {
            self.seen_at = None;
        }
    }

    pub(super) fn is_current(&mut self) -> bool {
        if self
            .seen_at
            .is_some_and(|seen| seen.elapsed() >= PROMPT_VALID_FOR)
        {
            self.input();
        }
        matches!(self.escape, Escape::Text)
            && self.seen_at
                .is_some_and(|seen| seen.elapsed() < PROMPT_VALID_FOR)
    }

    /// 有任何输入就消费提示；新输出必须重新形成完整提示才能再次授权。
    pub(super) fn input(&mut self) {
        self.invalidate_line();
        self.escape = Escape::Text;
        self.osc.clear();
    }

    fn clear_line(&mut self) {
        self.line.clear();
        self.seen_at = None;
        self.line_incomplete = false;
    }

    fn invalidate_line(&mut self) {
        self.clear_line();
        self.line_incomplete = true;
    }

    fn finish_osc(&mut self) {
        if self.osc_overflow {
            self.editing_command = true;
            self.clear_line();
        } else if let Some(mark) = self.osc.strip_prefix(b"633;") {
            let code = mark.split(|&byte| byte == b';').next().unwrap_or_default();
            match code {
                b"A" | b"B" | b"D" | b"E" => {
                    self.editing_command = true;
                    self.clear_line();
                }
                b"C" => {
                    self.editing_command = false;
                    self.clear_line();
                }
                _ => {}
            }
        }
        self.osc.clear();
        self.escape = Escape::Text;
    }
}

fn is_prompt(line: &[u8]) -> bool {
    let Ok(line) = std::str::from_utf8(line) else {
        return false;
    };
    let line = line.trim();
    if let Some(body) = line.strip_prefix("[sudo] ") {
        if let Some(body) = body.strip_suffix(':') {
            if let Some(user) = body
                .get(..13)
                .filter(|prefix| prefix.eq_ignore_ascii_case("password for "))
                .and_then(|_| body.get(13..))
            {
                return valid_user(user);
            }
        }
        if let Some(user) = body
            .strip_suffix(" 的密码:")
            .or_else(|| body.strip_suffix(" 的密码："))
        {
            return valid_user(user);
        }
    }
    false
}

fn valid_user(user: &str) -> bool {
    !user.is_empty() && !user.chars().any(|c| c.is_whitespace() || c == ':')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sudo_prompt_handles_fragmented_ansi_and_chinese() {
        let mut state = SudoPrompt::default();
        state.observe(b"\x1b[3");
        state.observe(b"1m[sudo] password for u");
        assert!(!state.is_current());
        state.observe(b":\x1b[0m ");
        assert!(state.is_current());
        state.observe(b"\r\n");
        assert!(!state.is_current());
        let prompt = "[sudo] 测试 的密码：".as_bytes();
        for byte in prompt {
            state.observe(&[*byte]);
        }
        assert!(state.is_current());
    }

    #[test]
    fn sudo_prompt_generic_password_never_inherits_command_authority() {
        let mut state = SudoPrompt::default();
        state.observe(b"Password:");
        assert!(!state.is_current());
        state.observe(b"\x1b]633;E;ssh host\x07\x1b]633;C\x07Password:");
        assert!(!state.is_current());
        state.observe(b"\x1b]633;E;echo sudo\x07\x1b]633;C\x07Password:");
        assert!(!state.is_current());
        state.observe(b"\x1b]633;E;sudo true\\x3b ssh host\x07\x1b]633;C\x07Password:");
        assert!(!state.is_current());
        state.observe(b"\x1b]633;E;sudo ssh host\x1b");
        state.observe(b"\\\x1b]633;C\x07Password:");
        assert!(!state.is_current());
        state.observe(b"\r\n[sudo] password for u:");
        assert!(state.is_current());
        state.input();
        state.observe(b"Password:");
        assert!(!state.is_current());
        state.observe("\r\n密码：".as_bytes());
        assert!(!state.is_current());
        state.observe(b"\x1b]633;D;0\x07");
        assert!(!state.is_current());
        state.observe(b"Password:");
        assert!(!state.is_current());
    }

    #[test]
    fn sudo_prompt_rejects_command_echo_and_osc_command_editing() {
        let mut state = SudoPrompt::default();
        state.observe(b"u@host$ echo [sudo] password for u:");
        assert!(!state.is_current());
        state.observe(b"\x1b]633;A\x07\x1b]633;B\x07[sudo] password for u:");
        assert!(!state.is_current());
        state.observe(b"\x1b]633;E;sudo -i\x07[sudo] password for u:");
        assert!(!state.is_current());
        state.observe(b"\x1b]633;C\x07[sudo] password for u:");
        assert!(state.is_current());
        state.observe(b"\x1b]633;D;0\x07[sudo] password for u:");
        assert!(!state.is_current());
    }

    #[test]
    fn sudo_prompt_requires_a_complete_line_after_input_or_truncation() {
        let mut state = SudoPrompt::default();
        state.input();
        state.observe(b"[sudo] password for u:");
        assert!(!state.is_current());
        state.observe(b"\r\n[sudo] password for u:");
        assert!(state.is_current());
        state.observe(b"\x1b[2K[sudo] password for u:");
        assert!(!state.is_current());

        let mut long_echo = vec![b'x'; MAX_LINE];
        long_echo.extend_from_slice(b"[sudo] password for u:");
        state.observe(b"\r\n");
        state.observe(&long_echo);
        assert!(!state.is_current());
        state.observe(b"\r\n[sudo] password for u:");
        assert!(state.is_current());
    }

    #[tokio::test(start_paused = true)]
    async fn sudo_prompt_expires_without_refreshing_on_colors_or_spaces() {
        let mut state = SudoPrompt::default();
        state.observe(b"[sudo] password for u:");
        assert!(state.is_current());
        tokio::time::advance(PROMPT_VALID_FOR).await;
        state.observe(b"\x1b[0m ");
        assert!(!state.is_current());
        state.input();
        state.observe(b"\r\n[sudo] password for u:");
        assert!(state.is_current());
        state.observe(b"\x1b[2K");
        assert!(!state.is_current());
    }
}
