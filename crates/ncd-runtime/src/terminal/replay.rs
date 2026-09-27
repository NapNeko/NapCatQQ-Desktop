//! 每个会话留一段最近的输出。前端重新接上时（轻量模式重建网页、刷新页面）先回放这段，
//! 再接实时输出，看起来和没断过一样

use std::collections::VecDeque;

/// 每个会话最多留这么多字节
pub(crate) const REPLAY_CAPACITY: usize = 1024 * 1024;

/// 截掉开头后往后找换行最多找这么远；找不到就从截断处开始
const LINE_SEEK_LIMIT: usize = 4096;

pub(crate) struct ReplayBuffer {
    buf: VecDeque<u8>,
    cap: usize,
}

impl ReplayBuffer {
    pub(crate) fn new(cap: usize) -> Self {
        Self {
            buf: VecDeque::with_capacity(cap.min(64 * 1024)),
            cap: cap.max(1),
        }
    }

    pub(crate) fn push(&mut self, bytes: &[u8]) {
        let truncated = if bytes.len() >= self.cap {
            self.buf.clear();
            self.buf.extend(&bytes[bytes.len() - self.cap..]);
            true
        } else {
            self.buf.extend(bytes);
            let over = self.buf.len() > self.cap;
            if over {
                let excess = self.buf.len() - self.cap;
                self.buf.drain(..excess);
            }
            over
        };
        if !truncated {
            return;
        }
        // 从下一行开头算起，回放不至于从半截转义序列或半个汉字开始
        if let Some(pos) = self
            .buf
            .iter()
            .take(LINE_SEEK_LIMIT)
            .position(|b| *b == b'\n')
        {
            self.buf.drain(..=pos);
        }
    }

    pub(crate) fn snapshot(&self) -> Vec<u8> {
        let (head, tail) = self.buf.as_slices();
        let mut out = Vec::with_capacity(self.buf.len());
        out.extend_from_slice(head);
        out.extend_from_slice(tail);
        out
    }

    pub(crate) fn clear(&mut self) {
        self.buf.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_everything_under_capacity() {
        let mut buf = ReplayBuffer::new(64);
        buf.push(b"hello ");
        buf.push(b"world");
        assert_eq!(buf.snapshot(), b"hello world");
    }

    #[test]
    fn trims_to_the_next_line_when_full() {
        let mut buf = ReplayBuffer::new(16);
        buf.push(b"line-one\nline-two\nthree");
        // 超出的从头截掉，再对齐到下一行开头
        assert_eq!(buf.snapshot(), b"line-two\nthree");
        buf.push(b"\nfour\n");
        assert_eq!(buf.snapshot(), b"three\nfour\n");
    }

    #[test]
    fn huge_chunk_keeps_its_tail() {
        let mut buf = ReplayBuffer::new(8);
        buf.push(b"0123456789abcdef");
        assert_eq!(buf.snapshot(), b"89abcdef");
    }

    #[test]
    fn clear_empties() {
        let mut buf = ReplayBuffer::new(8);
        buf.push(b"abc");
        buf.clear();
        assert!(buf.snapshot().is_empty());
    }
}
