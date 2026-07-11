//! NapCat 控制台日志清洗：ANSI / 半成品 CSI / QQ 宿主噪声 / NC 业务噪声
//!
//! 远端 nohup / 本机 pipe 共用。只洗推给 Desktop 的行，不改远端磁盘日志。
//! QQ 壳噪声见 [crate::qq_console_noise]；本模块只保留 NapCat 专属规则
//! （winston orphan CSI、终端二维码方阵）。SnowLuma 规则不得写在这里。
//!
//! 对照：
//! - napcat-core helper/log.ts：winston + colorize
//! - napcat-shell base.ts：qrcode.generate 终端方阵

use crate::deployments::strip_ansi_escapes;
use crate::qq_console_noise::QqConsoleNoiseFilter;

/// 一行清洗结果：空串表示丢弃（空白或全控制字符）
pub fn sanitize_napcat_console_line(input: &str) -> String {
    let stripped = strip_ansi_escapes(input);
    let mut out = String::with_capacity(stripped.len());
    for ch in stripped.chars() {
        let c = ch as u32;
        if c == 0x7f {
            continue;
        }
        if c < 0x20 && ch != '\t' {
            // 丢弃 C0；行级处理不保留 \n/\r
            continue;
        }
        out.push(ch);
    }
    // ESC 丢失后的 colorize 残片：[32minfo[39m] / [31merror[39m]
    let out = repair_orphan_ansi_level_tags(&out);
    out.trim_end().to_string()
}

/// winston colorize 在 ESC 字节丢失后常变成 `[32minfo[39m]` 这类字面量
fn repair_orphan_ansi_level_tags(input: &str) -> String {
    // 仅处理 level 标签常见形态，避免误伤普通文本里的 [数字]
    // [32minfo[39m] → [info]
    // [1;32minfo[39m] → [info]
    // [31merror[0m] / [31merror[39m]
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'[' {
            if let Some((end, level)) = try_parse_orphan_level_tag(&bytes[i..]) {
                out.push('[');
                out.push_str(level);
                out.push(']');
                i += end;
                continue;
            }
        }
        // 按 char 边界推进（i 始终落在合法 UTF-8 边界）
        let Some(ch) = input[i..].chars().next() else {
            break;
        };
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// 匹配 `[` + CSI参数 + `m` + level + `[` + 可选参数 + `m` + `]`
/// 返回 (消耗字节数, level)
fn try_parse_orphan_level_tag(bytes: &[u8]) -> Option<(usize, &'static str)> {
    if bytes.first() != Some(&b'[') {
        return None;
    }
    let mut i = 1;
    // 参数：数字与 ;
    let param_start = i;
    while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b';') {
        i += 1;
    }
    if i == param_start || i >= bytes.len() || bytes[i] != b'm' {
        return None;
    }
    i += 1; // m
    let level_start = i;
    while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
        i += 1;
    }
    if i == level_start {
        return None;
    }
    let level_bytes = &bytes[level_start..i];
    let level = match level_bytes {
        b"trace" | b"TRACE" => "trace",
        b"debug" | b"DEBUG" => "debug",
        b"info" | b"INFO" => "info",
        b"warn" | b"WARN" | b"warning" | b"WARNING" => "warn",
        b"error" | b"ERROR" => "error",
        b"fatal" | b"FATAL" => "fatal",
        b"success" | b"SUCCESS" => "success",
        _ => return None,
    };
    if i >= bytes.len() || bytes[i] != b'[' {
        return None;
    }
    i += 1;
    let reset_start = i;
    while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b';') {
        i += 1;
    }
    if i == reset_start || i >= bytes.len() || bytes[i] != b'm' {
        return None;
    }
    i += 1;
    if i >= bytes.len() || bytes[i] != b']' {
        return None;
    }
    i += 1;
    Some((i, level))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NapcatLogNoiseAction {
    Keep,
    Drop,
}

/// 跨行噪声状态（每 bot 一个实例）
#[derive(Debug, Default, Clone)]
pub struct NapcatLogNoiseFilter {
    in_qr_block: bool,
    qq: QqConsoleNoiseFilter,
}

impl NapcatLogNoiseFilter {
    pub fn new() -> Self {
        Self::default()
    }

    /// 对**已 sanitize** 的行做噪声分类；会更新内部 QR 状态。
    /// 增量路径若已 decode/sanitize，应调 [`Self::process_sanitized_line`]，避免二次扫描。
    pub fn classify(&mut self, line: &str) -> NapcatLogNoiseAction {
        if line.is_empty() {
            return NapcatLogNoiseAction::Drop;
        }

        if self.in_qr_block {
            if is_qr_terminal_line(line) || line.chars().all(|c| c.is_whitespace()) {
                return NapcatLogNoiseAction::Drop;
            }
            if line.contains("二维码解码URL") || line.contains("二维码已保存") {
                self.in_qr_block = false;
                return NapcatLogNoiseAction::Keep;
            }
            // 方阵中断：退出状态后按普通规则再判一次
            self.in_qr_block = false;
            return self.classify_normal(line);
        }

        if line.contains("请扫描下面的二维码") {
            self.in_qr_block = true;
            return NapcatLogNoiseAction::Keep;
        }

        self.classify_normal(line)
    }

    fn classify_normal(&mut self, line: &str) -> NapcatLogNoiseAction {
        // 历史 tail 可能截断在方阵中间，无 prompt 也要丢纯 block 行
        if is_qr_terminal_line(line) {
            return NapcatLogNoiseAction::Drop;
        }
        if is_fixed_noise_line(line) {
            return NapcatLogNoiseAction::Drop;
        }
        // QQ 宿主：有状态（JSON/堆栈续行）
        if self.qq.is_noise(line) {
            return NapcatLogNoiseAction::Drop;
        }
        NapcatLogNoiseAction::Keep
    }

    /// 原始行：sanitize + classify（远端文件 / Docker logs 原文入口）
    pub fn process_line(&mut self, raw: &str) -> Option<String> {
        let cleaned = sanitize_napcat_console_line(raw);
        self.process_sanitized_line(cleaned)
    }

    /// 已 sanitize 的行：只做 L2（本机 decode_log_line 之后）
    pub fn process_sanitized_line(&mut self, cleaned: String) -> Option<String> {
        let trimmed = cleaned.trim_end();
        if trimmed.is_empty() {
            return None;
        }
        if self.classify(trimmed) == NapcatLogNoiseAction::Drop {
            return None;
        }
        if trimmed.len() == cleaned.len() {
            Some(cleaned)
        } else {
            Some(trimmed.to_string())
        }
    }
}

/// 批量清洗：历史 tail / 整文件读路径用；QR 状态机跨行有效
///
/// 应对完整（或足够长的尾部）片段顺序调用，不要对每行独立 new filter。
pub fn filter_napcat_console_lines<I, S>(lines: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut filter = NapcatLogNoiseFilter::new();
    lines
        .into_iter()
        .filter_map(|line| filter.process_line(line.as_ref()))
        .collect()
}

fn is_qr_terminal_line(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }
    // qrcode small 输出几乎全是 block 字符
    let mut total = 0usize;
    let mut block = 0usize;
    for ch in trimmed.chars() {
        total += 1;
        if is_qr_block_char(ch) {
            block += 1;
        }
    }
    total >= 8 && block * 10 >= total * 7
}

fn is_qr_block_char(ch: char) -> bool {
    matches!(
        ch,
        '█' | '▄'
            | '▀'
            | '▌'
            | '▐'
            | '░'
            | '▒'
            | '▓'
            | '■'
            | '□'
            | '▪'
            | '▫'
            | '●'
            | '○'
            | ' '
            | '　'
    )
}

fn is_fixed_noise_line(line: &str) -> bool {
    // NapCat 专属固定噪声；QQ 宿主噪声走 QqConsoleNoiseFilter（有状态）
    is_napcat_only_noise(line)
}

fn is_napcat_only_noise(line: &str) -> bool {
    // 预留：仅 NapCat 壳/注入器特有、且不应出现在 SL 的行
    // 目前 QQ 壳噪声已覆盖用户样本；保持函数便于后续扩展
    let _ = line;
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_napcat_webui_line;

    #[test]
    fn strip_true_ansi_color() {
        let input = "07-11 17:06:19 [\x1b[32minfo\x1b[39m] hello";
        let out = sanitize_napcat_console_line(input);
        assert_eq!(out, "07-11 17:06:19 [info] hello");
    }

    #[test]
    fn repair_orphan_ansi_level_tags() {
        let input = "07-11 17:06:19 [32minfo[39m] [NapCat] ready";
        let out = sanitize_napcat_console_line(input);
        assert_eq!(out, "07-11 17:06:19 [info] [NapCat] ready");
    }

    #[test]
    fn repair_orphan_error_tag() {
        let input = "07-11 17:56:42 [31merror[39m] QIAO | 发生错误";
        let out = sanitize_napcat_console_line(input);
        assert!(out.contains("[error]"));
        assert!(!out.contains("31m"));
    }

    #[test]
    fn drops_qq_startup_and_gpu_and_js_loaded() {
        let mut f = NapcatLogNoiseFilter::new();
        assert!(
            f.process_line("version_config_filename :/home/ubuntu/.config/QQ/versions/config.json")
                .is_none()
        );
        assert!(f.process_line("not mini app.").is_none());
        assert!(f
            .process_line(
                "[ERROR:components/viz/service/main/viz_main_impl.cc:184] Exiting GPU process due to errors during initialization"
            )
            .is_none());
        assert!(f.process_line("141 js loaded").is_none());
        assert!(
            f.process_line("[preload] succeeded. /path/major.node")
                .is_none()
        );
    }

    #[test]
    fn drops_bugly_pipeline() {
        let mut f = NapcatLogNoiseFilter::new();
        assert!(f.process_line("linux-bugly: init bugly ...").is_none());
        assert!(f.process_line("InitBuglyManager").is_none());
        assert!(
            f.process_line("[BuglyManager.cpp][InitBuglyManager][212]InitBuglyManager path: /x")
                .is_none()
        );
        assert!(
            f.process_line(
                "[NativeCrashHandler.cpp][uploadCrashEvent][331]get null crashDetailBean, return!"
            )
            .is_none()
        );
    }

    #[test]
    fn qr_block_dropped_url_kept() {
        let mut f = NapcatLogNoiseFilter::new();
        let prompt = f
            .process_line("07-11 17:06:20 [warn] 请扫描下面的二维码，然后在手Q上授权登录：")
            .expect("prompt keep");
        assert!(prompt.contains("请扫描下面的二维码"));

        assert!(
            f.process_line("▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄")
                .is_none()
        );
        assert!(
            f.process_line("█ ▄▄▄▄▄ █▄▄▄ ▀▄██ ▄▀ ██▄█▀█ ▄▄▄▄▄ █")
                .is_none()
        );
        assert!(f.process_line("").is_none());

        let url = f
            .process_line("二维码解码URL: https://txz.qq.com/p?k=abc")
            .expect("url keep");
        assert!(url.contains("二维码解码URL"));

        let saved = f
            .process_line("07-11 17:06:20 [warn] 二维码已保存到 /tmp/qrcode.png")
            .expect("saved keep");
        assert!(saved.contains("二维码已保存到"));
    }

    #[test]
    fn keeps_business_lines() {
        let mut f = NapcatLogNoiseFilter::new();
        let samples = [
            "07-11 17:06:19 [info] [NapCat] [WebUi] WebUi User Panel Url: http://127.0.0.1:6100/webui?token=9efa38f13c55",
            "07-11 17:06:20 [error] 快速登录错误： 你的用户身份已失效",
            "07-11 17:06:28 [info] QIAO | 接收 <- 群聊 [天理云(498950511)] [图]",
            "07-11 17:56:24 [info] QIAO | 账号状态变更为离线",
            "07-11 17:56:42 [error] QIAO | 发生错误 Error: Timeout: NTEvent",
            "07-11 17:06:28 [info] [OneBot] [HTTP Server Adapter] Start On 127.0.0.1:3011",
        ];
        for s in samples {
            let out = f
                .process_line(s)
                .unwrap_or_else(|| panic!("should keep: {s}"));
            assert!(!out.is_empty(), "{s}");
        }
    }

    #[test]
    fn webui_parse_after_sanitize() {
        let line = "07-11 17:06:19 [32minfo[39m] [NapCat] [WebUi] WebUi User Panel Url: http://127.0.0.1:6100/webui?token=9efa38f13c55";
        let cleaned = sanitize_napcat_console_line(line);
        let (port, token) = parse_napcat_webui_line(&cleaned).expect("parse");
        assert_eq!(port, 6100);
        assert_eq!(token, "9efa38f13c55");
    }

    #[test]
    fn process_line_pipeline_on_mixed_chunk() {
        let mut f = NapcatLogNoiseFilter::new();
        let chunk = r#"not mini app.
version_config_filename :/home/ubuntu/.config/QQ/versions/config.json
07-11 17:06:19 [32minfo[39m] [NapCat] [Core] NapCat.Core Version: 4.18.9
07-11 17:06:19 [32minfo[39m] [NapCat] [WebUi] WebUi User Panel Url: http://127.0.0.1:6100/webui?token=abc123
linux-bugly: init bugly ...
141 js loaded
07-11 17:06:28 [32minfo[39m] QIAO | 接收 <- 群聊 [test] hi
"#;
        let mut kept = Vec::new();
        for line in chunk.lines() {
            if let Some(c) = f.process_line(line) {
                kept.push(c);
            }
        }
        assert_eq!(kept.len(), 3);
        assert!(kept[0].contains("NapCat.Core Version"));
        assert!(kept[1].contains("WebUi User Panel Url"));
        assert!(kept[2].contains("接收 <-"));
        let (port, token) = parse_napcat_webui_line(&kept[1]).unwrap();
        assert_eq!((port, token.as_str()), (6100, "abc123"));
    }

    #[test]
    fn keeps_unrelated_setparam_business_line() {
        let mut f = NapcatLogNoiseFilter::new();
        assert!(
            f.process_line("07-11 19:00:00 [info] plugin setParam ok")
                .is_some(),
            "weak bugly words must not drop business logs"
        );
    }

    #[test]
    fn filter_batch_history_matches_user_remote_noise() {
        // 开页 tail 用整段 filter；模拟用户远端 nohup 片段
        let raw = r#"not mini app.
version_config_filename :/home/ubuntu/.config/QQ/versions/config.json
app_package_filename :/home/ubuntu/Napcat/opt/QQ/resources/app/package.json
config_build_id :45758
[preload] succeeded. /home/ubuntu/Napcat/opt/QQ/resources/app/major.node
resourcesPath: /home/ubuntu/Napcat/opt/QQ/resources
[ERROR:components/viz/service/main/viz_main_impl.cc:184] Exiting GPU process due to errors during initialization
NapCat Shell App Loading...
07-11 19:16:49 [32minfo[39m] [NapCat] [WebUi] WebUi User Panel Url: http://127.0.0.1:6099/webui?token=9efa38f13c55
07-11 19:16:54 [33mwarn[39m] 请扫描下面的二维码，然后在手Q上授权登录：
▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄
█ ▄▄▄▄▄ ██▄▄ ▀ ▀█ ▄██  ▀█▀█ ▄▄▄▄▄ █
二维码解码URL: https://txz.qq.com/p?k=abc
07-11 19:16:54 [33mwarn[39m] 二维码已保存到 /tmp/qrcode.png
linux-bugly: init bugly ...
InitBuglyManager
141 js loaded
07-11 19:17:09 [32minfo[39m] [OneBot] [HTTP Server Adapter] Start On 127.0.0.1:3011
"#;
        let kept = filter_napcat_console_lines(raw.lines());
        assert!(
            kept.iter().all(|l| {
                !l.contains("version_config")
                    && !l.contains("not mini app")
                    && !l.contains("Bugly")
                    && !l.contains("js loaded")
                    && !l.contains("Exiting GPU")
                    && !l.contains('█')
                    && !l.contains("32minfo")
            }),
            "noise leaked: {kept:?}"
        );
        assert!(kept.iter().any(|l| l.contains("WebUi User Panel Url")));
        assert!(kept.iter().any(|l| l.contains("请扫描下面的二维码")));
        assert!(kept.iter().any(|l| l.contains("二维码解码URL")));
        assert!(kept.iter().any(|l| l.contains("二维码已保存到")));
        assert!(kept.iter().any(|l| l.contains("OneBot")));
        assert!(kept.iter().any(|l| l.contains("NapCat Shell App Loading")));
    }
}
