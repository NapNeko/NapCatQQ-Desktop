//! SnowLuma 子进程 stdout 行清洗工具。
//! 由 spec `snowluma-backend-runtime` 落地。
//! NapCat / SnowLuma 守护进程在不同平台上会输出 ANSI 颜色码、光标移动等
//! 转义序列以及零散的控制字符（BEL/BS/VT 等），如果直接送进事件总线、
//! UI 日志面板或文本搜索都会产生 tofu 字符 / 错乱高亮 / 不可读分隔。
//! 与 legacy Python `SnowLumaDaemon._sanitize_log_text` 行为对齐：
//! - 剥除 ANSI CSI 序列：`\x1b\[[0-9;]*[a-zA-Z]`（参数仅数字 + `;`，终止字节是字母）。
//! - 丢弃所有 `< 0x20` 不可打印控制字符，仅保留 `\t` (0x09) / `\n` (0x0A) / `\r` (0x0D)。
//! - 丢弃 `\x7f` (DEL)。
//! - 保留 UTF-8 多字节序列（任何 ≥ 0x80 的字节原样透出）。
//! 实现采用纯字节级状态机扫描，不引入 `regex` crate 依赖
//! 在最坏情况下与输入长度线性。
//! 注意：本函数只清洗 CSI 形式的 ANSI 序列（task 描述的 `\x1b\[...`）
//! 与 `runtime_backend::strip_ansi_escapes` 那种处理 OSC/DCS/SOS/PM/APC
//! 的完整状态机刻意拆开 —— SnowLuma daemon 输出仅使用 CSI 序列，足够。

/// 清洗 SnowLuma 子进程一行 stdout，剥除 ANSI CSI 序列与非打印控制字符。
/// # Examples
/// ```
/// use ncd_core::snowluma::log_sanitize::sanitize_log_line;
/// assert_eq!(sanitize_log_line("\x1b[31mred\x1b[0m"), "red");
/// assert_eq!(sanitize_log_line("plain"), "plain");
/// ```
pub fn sanitize_log_line(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];

        // 1) ANSI CSI: ESC '[' [0-9;]* final-letter
        if b == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
            // 跳过 ESC + '['
            i += 2;
            // 吞掉参数字节：数字 + 分号
            while i < bytes.len() {
                let p = bytes[i];
                if p.is_ascii_digit() || p == b';' {
                    i += 1;
                } else {
                    break;
                }
            }
            // 吞掉最终字母（终止字节）
            if i < bytes.len() && bytes[i].is_ascii_alphabetic() {
                i += 1;
            }
            // 不匹配的孤立 ESC '[' 也算消化掉，与 legacy 正则贪婪匹配语义一致。
            continue;
        }

        // 2) 孤立 ESC：丢弃单字节
        if b == 0x1b {
            i += 1;
            continue;
        }

        // 3) DEL (0x7f)：丢弃
        if b == 0x7f {
            i += 1;
            continue;
        }

        // 4) C0 控制字符：除 \t \n \r 外全部丢弃
        if b < 0x20 && b != b'\t' && b != b'\n' && b != b'\r' {
            i += 1;
            continue;
        }

        // 5) 普通 ASCII / UTF-8 多字节 (>= 0x80) 原样保留
        out.push(b);
        i += 1;
    }

    // out 中保留的字节序列：
    // - 普通 ASCII 字符 (含 \t \n \r)
    // - 完整保留的 UTF-8 多字节序列（仅在 >=0x80 范围且原始输入是合法 UTF-8）
    // 因此构造回 String 不会失败；为安全起见仍走 lossy 兜底。
    match String::from_utf8(out) {
        Ok(s) => s,
        Err(err) => String::from_utf8_lossy(err.as_bytes()).into_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::sanitize_log_line;

    #[test]
    fn pure_ascii_passes_through_unchanged() {
        let input = "hello world 1234 !@#";
        assert_eq!(sanitize_log_line(input), input);
    }

    #[test]
    fn ansi_color_codes_are_stripped() {
        let input = "\x1b[31mred\x1b[0m";
        assert_eq!(sanitize_log_line(input), "red");
    }

    #[test]
    fn cursor_movement_sequences_are_stripped() {
        // \x1b[2J 清屏 + \x1b[1;1H 光标定位
        let input = "\x1b[2J\x1b[1;1Hhello";
        assert_eq!(sanitize_log_line(input), "hello");
    }

    #[test]
    fn empty_input_returns_empty() {
        assert_eq!(sanitize_log_line(""), "");
    }

    #[test]
    fn only_control_chars_yields_empty_string() {
        // BEL + BS + US (0x1f) — 全部应被丢弃
        let input = "\x07\x08\x1f";
        assert_eq!(sanitize_log_line(input), "");
    }

    #[test]
    fn whitespace_control_chars_are_preserved() {
        let input = "line1\nline2\tcol\rend";
        assert_eq!(sanitize_log_line(input), "line1\nline2\tcol\rend");
    }

    #[test]
    fn del_character_is_dropped() {
        let input = "ab\x7fcd";
        assert_eq!(sanitize_log_line(input), "abcd");
    }

    #[test]
    fn utf8_multibyte_is_preserved() {
        let input = "前缀\x1b[33m中文\x1b[0m后缀";
        assert_eq!(sanitize_log_line(input), "前缀中文后缀");
    }

    #[test]
    fn lone_escape_is_dropped() {
        // 单独的 ESC 不构成 CSI，应被丢弃，后续文本保留
        let input = "before\x1bafter";
        assert_eq!(sanitize_log_line(input), "beforeafter");
    }

    #[test]
    fn multiple_csi_in_one_line() {
        let input = "\x1b[1;32m[INFO]\x1b[0m starting \x1b[31merror\x1b[0m";
        assert_eq!(sanitize_log_line(input), "[INFO] starting error");
    }
}
