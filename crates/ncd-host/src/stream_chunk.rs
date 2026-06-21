//! 流式命令输出按「逻辑行」切分。
//!
//! docker pull 在非 TTY 下常用 \r 在同一物理行上刷新进度，只有 \n 切行会
//! 长时间收不到更新。本模块在 \n 与 \r 处都产出完整片段供回调消费。

/// 将一段字节追加到缓冲，对每个完整逻辑行调用 on_line。
///
/// 规则：遇到 \n 或 \r 时，若缓冲非空则产出一条（去掉行尾 \r/\n），
/// 然后清空缓冲。\r 单独出现时也会 flush（覆盖式进度行的常见形态）。
pub fn feed_stream_chunk(buf: &mut Vec<u8>, data: &[u8], mut on_line: impl FnMut(String)) {
    buf.extend_from_slice(data);
    loop {
        let pos = buf.iter().position(|&b| b == b'\n' || b == b'\r');
        let Some(pos) = pos else {
            break;
        };
        let delim = buf[pos];
        let raw: Vec<u8> = buf.drain(..=pos).collect();
        let mut s = String::from_utf8_lossy(&raw).into_owned();
        while s.ends_with('\n') || s.ends_with('\r') {
            s.pop();
        }
        if !s.is_empty() {
            on_line(s);
        }
        // 连续 \r\n：若 drain 后下一个仍是分隔符，继续循环
        if delim == b'\r' && buf.first() == Some(&b'\n') {
            buf.drain(..1);
        }
    }
}

/// 命令结束前 flush 缓冲里未以分隔符结尾的残行。
pub fn flush_stream_remainder(buf: &mut Vec<u8>, mut on_line: impl FnMut(String)) {
    if buf.is_empty() {
        return;
    }
    let s = String::from_utf8_lossy(buf).into_owned();
    buf.clear();
    let t = s.trim_end_matches('\r').trim_end_matches('\n').trim();
    if !t.is_empty() {
        on_line(t.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newline_splits() {
        let mut buf = Vec::new();
        let mut lines = Vec::new();
        feed_stream_chunk(&mut buf, b"a: line1\nb: line2\n", |s| lines.push(s));
        flush_stream_remainder(&mut buf, |s| lines.push(s));
        assert_eq!(lines, vec!["a: line1", "b: line2"]);
    }

    #[test]
    fn carriage_return_emits_progress_updates() {
        let mut buf = Vec::new();
        let mut lines = Vec::new();
        feed_stream_chunk(
            &mut buf,
            b"deadbeef: Downloading [=>   ] 1MB/10MB\r",
            |s| lines.push(s),
        );
        feed_stream_chunk(
            &mut buf,
            b"deadbeef: Downloading [====>] 5MB/10MB\r",
            |s| lines.push(s),
        );
        feed_stream_chunk(&mut buf, b"deadbeef: Download complete\n", |s| lines.push(s));
        flush_stream_remainder(&mut buf, |s| lines.push(s));
        assert_eq!(lines.len(), 3);
        assert!(lines[0].contains("1MB"));
        assert!(lines[1].contains("5MB"));
        assert_eq!(lines[2], "deadbeef: Download complete");
    }
}