//! 增量 SSE 解析：喂进网络上任意切分的字节块，吐出完整的帧。
//!
//! 只实现调试台用得到的那部分 —— `event:` 与 `data:` 字段、注释行、空行结束一帧；
//! `id:` / `retry:` 不影响我们（断线重连由上层自己做）。行结束符 `\n`、`\r\n`、`\r` 都认。

use super::body::MAX_BODY_BYTES;

/// 一帧 SSE。多行 `data:` 用 `\n` 连起来
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseFrame {
    pub event: Option<String>,
    pub data: String,
}

/// 攒着还没成帧的数据超过上限：对端在一行 / 一帧里塞了离谱的量，这条连接不要了
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("事件流里有一帧超过 {} MiB，已断开", .limit / (1024 * 1024))]
pub struct SseTooLarge {
    pub limit: usize,
}

/// 增量解析器。一个连接用一个，断线重连时换新的（半行残留不该带到下一条连接）
#[derive(Debug)]
pub struct SseParser {
    /// 还没凑成完整行的字节。按字节存而不是按字符串：网络块可能把一个汉字劈成两半，
    /// 行结束符都是 ASCII，所以只对完整的行做 UTF-8 解码就不会劈坏字符
    buf: Vec<u8>,
    /// `buf` 里已经确认没有行结束符的前缀长度，避免大帧被小块反复喂时重复扫描
    scanned: usize,
    frame: FrameBuilder,
    /// 半行加上拼到一半的帧最多攒这么多字节，见 [`SseParser::try_push`]
    limit: usize,
}

impl Default for SseParser {
    fn default() -> Self {
        Self::with_limit(MAX_BODY_BYTES)
    }
}

/// 正在拼的一帧
#[derive(Debug, Default)]
struct FrameBuilder {
    event: Option<String>,
    data: String,
    /// 见过 `data` 字段。`data:` 后面为空也算有 —— 区分「空数据帧」与「只有注释的心跳」
    has_data: bool,
}

impl FrameBuilder {
    fn feed_line(&mut self, line: &[u8], out: &mut Vec<SseFrame>) {
        if line.is_empty() {
            self.dispatch(out);
            return;
        }
        // 注释行（`: heartbeat`）整行忽略
        if line.first() == Some(&b':') {
            return;
        }
        let line = String::from_utf8_lossy(line);
        let (field, value) = match line.split_once(':') {
            // 冒号后的一个空格是格式的一部分，不属于值
            Some((field, value)) => (field, value.strip_prefix(' ').unwrap_or(value)),
            None => (line.as_ref(), ""),
        };
        match field {
            "event" => self.event = Some(value.to_owned()),
            "data" => {
                if self.has_data {
                    self.data.push('\n');
                }
                self.data.push_str(value);
                self.has_data = true;
            }
            // id / retry / 未知字段：用不到
            _ => {}
        }
    }

    fn dispatch(&mut self, out: &mut Vec<SseFrame>) {
        let frame = std::mem::take(self);
        // 没有 data 的一帧（只有 event，或只有注释后的空行）按规范丢弃
        if frame.has_data {
            out.push(SseFrame {
                event: frame.event,
                data: frame.data,
            });
        }
    }
}

impl SseParser {
    pub fn new() -> Self {
        Self::default()
    }

    /// 自定上限（默认 [`MAX_BODY_BYTES`]，和单个响应体一样）
    pub fn with_limit(limit: usize) -> Self {
        Self {
            buf: Vec::new(),
            scanned: 0,
            frame: FrameBuilder::default(),
            limit,
        }
    }

    /// 和 [`SseParser::push`] 一样，但攒着的数据（没成行的半行 + 拼到一半的帧）超过上限时报错。
    /// 长连接上应当用这个：对端一直不换行，`push` 会一直往内存里攒。报错之后解析器的状态
    /// 不再可用，调用方应断开这条连接
    pub fn try_push(&mut self, chunk: &[u8]) -> Result<Vec<SseFrame>, SseTooLarge> {
        let frames = self.push(chunk);
        if self.buf.len().saturating_add(self.frame.data.len()) > self.limit {
            return Err(SseTooLarge { limit: self.limit });
        }
        Ok(frames)
    }

    /// 喂入一块字节，返回这一块之后新完成的帧（可能为空，也可能好几帧）。不检查上限，
    /// 喂的量有数的场合（测试、一次性的整段）用；长连接用 [`SseParser::try_push`]
    pub fn push(&mut self, chunk: &[u8]) -> Vec<SseFrame> {
        self.buf.extend_from_slice(chunk);
        let mut frames = Vec::new();

        let mut line_start = 0;
        let mut pos = self.scanned;
        while let Some(&byte) = self.buf.get(pos) {
            let line_end = pos;
            match byte {
                b'\n' => pos += 1,
                b'\r' => match self.buf.get(pos + 1) {
                    Some(b'\n') => pos += 2,
                    Some(_) => pos += 1,
                    // 块尾的 `\r` 可能是 `\r\n` 的前半，等下一块再定
                    None => break,
                },
                _ => {
                    pos += 1;
                    continue;
                }
            }
            if let Some(line) = self.buf.get(line_start..line_end) {
                self.frame.feed_line(line, &mut frames);
            }
            line_start = pos;
        }

        self.buf.drain(..line_start);
        self.scanned = pos - line_start;
        frames
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn try_push_refuses_a_line_that_never_ends() {
        let mut parser = SseParser::with_limit(16);
        assert!(parser.try_push(b"data: short\n\n").is_ok());
        assert_eq!(parser.try_push(&[b'x'; 17]), Err(SseTooLarge { limit: 16 }));
        // 一帧拼了很多行、每行都不长，也算在内
        let mut parser = SseParser::with_limit(16);
        assert!(parser.try_push(b"data: 0123456\n").is_ok());
        assert!(parser.try_push(b"data: 0123456789\n").is_err());
        assert!(
            SseTooLarge {
                limit: MAX_BODY_BYTES
            }
            .to_string()
            .contains("64 MiB")
        );
    }

    fn frame(event: Option<&str>, data: &str) -> SseFrame {
        SseFrame {
            event: event.map(str::to_owned),
            data: data.to_owned(),
        }
    }

    /// 整段喂进去
    fn parse_whole(input: &[u8]) -> Vec<SseFrame> {
        SseParser::new().push(input)
    }

    /// 一个字节一个字节喂，模拟最碎的网络切分
    fn parse_bytewise(input: &[u8]) -> Vec<SseFrame> {
        let mut parser = SseParser::new();
        input
            .iter()
            .flat_map(|byte| parser.push(std::slice::from_ref(byte)))
            .collect()
    }

    #[test]
    fn single_data_frame() {
        let out = parse_whole(b"data: {\"kind\":\"ready\"}\n\n");
        assert_eq!(out, vec![frame(None, r#"{"kind":"ready"}"#)]);
    }

    #[test]
    fn several_frames_in_one_chunk() {
        let out = parse_whole(b"data: a\n\ndata: b\n\n");
        assert_eq!(out, vec![frame(None, "a"), frame(None, "b")]);
    }

    #[test]
    fn split_at_every_possible_boundary_gives_the_same_frames() {
        let input =
            "event: x\r\ndata: 你好\r\ndata: 第二行\r\n\r\n: heartbeat\n\ndata: {\"a\":1}\n\n"
                .as_bytes();
        let expected = parse_whole(input);
        assert_eq!(
            expected,
            vec![frame(Some("x"), "你好\n第二行"), frame(None, r#"{"a":1}"#)]
        );
        assert_eq!(parse_bytewise(input), expected);
        for cut in 0..=input.len() {
            let mut parser = SseParser::new();
            let (head, tail) = input.split_at(cut);
            let mut out = parser.push(head);
            out.extend(parser.push(tail));
            assert_eq!(out, expected, "在 {cut} 处切开");
        }
    }

    #[test]
    fn crlf_line_endings() {
        let out = parse_bytewise(b"data: one\r\n\r\ndata: two\r\n\r\n");
        assert_eq!(out, vec![frame(None, "one"), frame(None, "two")]);
    }

    #[test]
    fn lone_cr_terminates_a_line() {
        // 末尾的 `\r` 要等下一个字节才知道是不是 `\r\n`，所以后面跟一段半截的帧
        let out = parse_whole(b"data: one\r\rdata: two\r\rdata: half");
        assert_eq!(out, vec![frame(None, "one"), frame(None, "two")]);
    }

    #[test]
    fn crlf_split_between_cr_and_lf_is_one_terminator() {
        let mut parser = SseParser::new();
        assert!(parser.push(b"data: a\r").is_empty());
        assert!(parser.push(b"\n\r").is_empty());
        // 若把 `\r`、`\n` 当两个换行，这里会多出一个空行而提前结帧
        assert_eq!(parser.push(b"\n"), vec![frame(None, "a")]);
    }

    #[test]
    fn multi_line_data_is_joined_with_newline() {
        let out = parse_whole(b"data: line1\ndata: line2\ndata:\ndata: line4\n\n");
        assert_eq!(out, vec![frame(None, "line1\nline2\n\nline4")]);
    }

    #[test]
    fn comment_heartbeat_is_ignored() {
        let out = parse_bytewise(b": heartbeat\n\ndata: x\n\n: heartbeat\n\n");
        assert_eq!(out, vec![frame(None, "x")]);
        assert!(parse_whole(b": heartbeat\n\n").is_empty());
    }

    #[test]
    fn comment_inside_a_frame_does_not_break_it() {
        let out = parse_whole(b"data: a\n: note\ndata: b\n\n");
        assert_eq!(out, vec![frame(None, "a\nb")]);
    }

    #[test]
    fn chinese_char_split_across_chunks() {
        let bytes = "data: 汉字\n\n".as_bytes();
        // "汉" 占 3 字节，从它的中间劈开
        let cut = "data: ".len() + 1;
        let mut parser = SseParser::new();
        assert!(parser.push(&bytes[..cut]).is_empty());
        let out = parser.push(&bytes[cut..]);
        assert_eq!(out, vec![frame(None, "汉字")]);
    }

    #[test]
    fn value_keeps_only_the_first_space_stripped() {
        let out = parse_whole(b"data:  two spaces\n\ndata:nospace\n\n");
        assert_eq!(
            out,
            vec![frame(None, " two spaces"), frame(None, "nospace")]
        );
    }

    #[test]
    fn colon_inside_value_is_kept() {
        let out = parse_whole(b"data: {\"a\":\"b:c\"}\n\n");
        assert_eq!(out, vec![frame(None, r#"{"a":"b:c"}"#)]);
    }

    #[test]
    fn field_without_colon_and_unknown_fields() {
        // 无冒号的行整行当字段名、值为空；`data` 单独一行是一个空数据帧
        let out = parse_whole(b"id: 7\nretry: 1000\nfoo: bar\ndata\n\n");
        assert_eq!(out, vec![frame(None, "")]);
    }

    #[test]
    fn event_only_frame_is_dropped_and_does_not_leak_into_the_next() {
        let out = parse_whole(b"event: lonely\n\ndata: x\n\n");
        assert_eq!(out, vec![frame(None, "x")]);
    }

    #[test]
    fn incomplete_frame_waits_for_more_input() {
        let mut parser = SseParser::new();
        assert!(parser.push(b"data: par").is_empty());
        assert!(parser.push(b"tial\n").is_empty());
        assert_eq!(parser.push(b"\n"), vec![frame(None, "partial")]);
    }

    #[test]
    fn large_frame_fed_in_small_chunks() {
        let body = "y".repeat(200_000);
        let input = format!("data: {body}\n\n");
        let mut parser = SseParser::new();
        let out: Vec<SseFrame> = input
            .as_bytes()
            .chunks(1000)
            .flat_map(|chunk| parser.push(chunk))
            .collect();
        assert_eq!(out, vec![frame(None, &body)]);
    }
}
