//! 事件环形缓冲：给每个 Bot 的事件流编号并保留最近一批，供前端补拉和断线后对账。

use std::collections::VecDeque;

use ncd_domain::onebot_debug::{DebugEvent, DebugEventBody};

/// 默认容量。消息事件自带大块 `raw`，太大会占内存；5000 条够翻好一阵子
pub const DEFAULT_RING_CAP: usize = 5000;

/// 定容的事件缓冲。`seq` 从 1 起单调递增，被挤掉或清空也不回退 ——
/// 前端拿 `seq` 当游标，回退会让它以为事件重复了
#[derive(Debug)]
pub struct EventRing {
    cap: usize,
    next_seq: u64,
    items: VecDeque<DebugEvent>,
    dropped_total: u64,
}

impl EventRing {
    /// 容量至少为 1：容量 0 会让每条事件一进来就被挤掉，调用方拿到的却是个有编号的事件
    pub fn new(cap: usize) -> Self {
        let cap = cap.max(1);
        Self {
            cap,
            next_seq: 1,
            items: VecDeque::with_capacity(cap.min(1024)),
            dropped_total: 0,
        }
    }

    /// 从指定的 `seq` 起编号（小于 1 的按 1）。同一个 Bot 的接收器停了又建时用：
    /// 新缓冲接着旧缓冲的编号往后走，前端按 `seq` 去重时不会把新事件当成看过的旧事件
    pub fn with_start_seq(cap: usize, next_seq: u64) -> Self {
        let mut ring = Self::new(cap);
        ring.next_seq = next_seq.max(1);
        ring
    }

    /// 下一条将要拿到的 `seq`
    pub fn next_seq(&self) -> u64 {
        self.next_seq
    }

    /// 追加一条并返回带了 `seq` 的事件；超出容量时挤掉最老的，累计到 `dropped_total`
    pub fn push(&mut self, at_ms: u64, body: DebugEventBody) -> DebugEvent {
        let event = DebugEvent {
            seq: self.next_seq,
            at_ms,
            body,
        };
        self.next_seq = self.next_seq.saturating_add(1);
        self.items.push_back(event.clone());
        while self.items.len() > self.cap {
            self.items.pop_front();
            self.dropped_total = self.dropped_total.saturating_add(1);
        }
        event
    }

    /// `seq > since_seq` 的事件，从老到新，最多 `limit` 条
    pub fn read_since(&self, since_seq: u64, limit: usize) -> Vec<DebugEvent> {
        // items 按 seq 升序，二分找到第一条大于游标的
        let start = self.items.partition_point(|e| e.seq <= since_seq);
        self.items.iter().skip(start).take(limit).cloned().collect()
    }

    /// 最新的 `limit` 条，从老到新
    pub fn tail(&self, limit: usize) -> Vec<DebugEvent> {
        let skip = self.items.len().saturating_sub(limit);
        self.items.iter().skip(skip).cloned().collect()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// 缓冲里最老一条的 `seq`；空的时候是下一条将要拿到的 `seq`
    pub fn first_seq(&self) -> u64 {
        self.items.front().map_or(self.next_seq, |e| e.seq)
    }

    /// 最近一次分配出去的 `seq`（一条都没推过为 0）。清空缓冲后仍保持，方便前端续用游标
    pub fn last_seq(&self) -> u64 {
        self.next_seq - 1
    }

    /// 因容量满被挤掉的累计条数（手动 `clear` 不计入）
    pub fn dropped_total(&self) -> u64 {
        self.dropped_total
    }

    /// 清空缓冲。`seq` 继续往后走，`dropped_total` 保留
    pub fn clear(&mut self) {
        self.items.clear();
    }
}

impl Default for EventRing {
    fn default() -> Self {
        Self::new(DEFAULT_RING_CAP)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn body(n: u64) -> DebugEventBody {
        DebugEventBody::Ob11 {
            payload: json!({"n": n}),
        }
    }

    fn seqs(events: &[DebugEvent]) -> Vec<u64> {
        events.iter().map(|e| e.seq).collect()
    }

    fn filled(cap: usize, count: u64) -> EventRing {
        let mut ring = EventRing::new(cap);
        for n in 1..=count {
            ring.push(1000 + n, body(n));
        }
        ring
    }

    #[test]
    fn seq_starts_at_one_and_is_monotonic() {
        let mut ring = EventRing::new(10);
        assert_eq!(ring.first_seq(), 1);
        assert_eq!(ring.last_seq(), 0);
        let a = ring.push(10, body(1));
        let b = ring.push(20, body(2));
        assert_eq!((a.seq, b.seq), (1, 2));
        assert_eq!((a.at_ms, b.at_ms), (10, 20));
        assert_eq!(a.body, body(1));
        assert_eq!(ring.len(), 2);
        assert_eq!((ring.first_seq(), ring.last_seq()), (1, 2));
    }

    #[test]
    fn evicts_oldest_beyond_cap_and_counts_drops() {
        let ring = filled(3, 5);
        assert_eq!(ring.len(), 3);
        assert_eq!(seqs(&ring.tail(10)), [3, 4, 5]);
        assert_eq!((ring.first_seq(), ring.last_seq()), (3, 5));
        assert_eq!(ring.dropped_total(), 2);
    }

    #[test]
    fn no_drops_until_full() {
        let ring = filled(3, 3);
        assert_eq!(ring.dropped_total(), 0);
        assert_eq!(ring.len(), 3);
    }

    #[test]
    fn read_since_returns_newer_events_ascending() {
        let ring = filled(10, 6);
        assert_eq!(seqs(&ring.read_since(0, 100)), [1, 2, 3, 4, 5, 6]);
        assert_eq!(seqs(&ring.read_since(3, 100)), [4, 5, 6]);
        assert!(ring.read_since(6, 100).is_empty());
        assert!(ring.read_since(99, 100).is_empty());
    }

    #[test]
    fn read_since_honours_limit_from_the_oldest_side() {
        let ring = filled(10, 6);
        // 补拉要从游标处连续往后读，limit 截的是新的那头
        assert_eq!(seqs(&ring.read_since(1, 2)), [2, 3]);
        assert!(ring.read_since(0, 0).is_empty());
    }

    #[test]
    fn read_since_with_an_evicted_cursor_starts_at_the_oldest_kept() {
        let ring = filled(3, 10);
        assert_eq!(seqs(&ring.read_since(2, 100)), [8, 9, 10]);
    }

    #[test]
    fn tail_returns_latest_in_ascending_order() {
        let ring = filled(10, 6);
        assert_eq!(seqs(&ring.tail(2)), [5, 6]);
        assert_eq!(seqs(&ring.tail(100)), [1, 2, 3, 4, 5, 6]);
        assert!(ring.tail(0).is_empty());
    }

    #[test]
    fn clear_empties_but_keeps_seq_moving() {
        let mut ring = filled(3, 5);
        ring.clear();
        assert!(ring.is_empty());
        assert_eq!(ring.len(), 0);
        assert_eq!(ring.dropped_total(), 2);
        assert_eq!(ring.last_seq(), 5);
        assert_eq!(ring.first_seq(), 6);
        let next = ring.push(1, body(6));
        assert_eq!(next.seq, 6);
        assert_eq!(seqs(&ring.tail(10)), [6]);
    }

    #[test]
    fn zero_cap_is_bumped_to_one() {
        let mut ring = EventRing::new(0);
        ring.push(1, body(1));
        ring.push(2, body(2));
        assert_eq!(seqs(&ring.tail(10)), [2]);
        assert_eq!(ring.dropped_total(), 1);
    }

    #[test]
    fn with_start_seq_continues_numbering() {
        let mut ring = EventRing::with_start_seq(3, 42);
        assert!(ring.is_empty());
        assert_eq!(ring.first_seq(), 42);
        assert_eq!(ring.last_seq(), 41);
        assert_eq!(ring.next_seq(), 42);
        assert_eq!(ring.push(1, body(1)).seq, 42);
        assert_eq!(ring.push(2, body(2)).seq, 43);
        assert_eq!(seqs(&ring.read_since(41, 10)), [42, 43]);
        assert_eq!(ring.next_seq(), 44);
        // 0 不是合法的起点：last_seq 会下溢
        let ring = EventRing::with_start_seq(3, 0);
        assert_eq!((ring.first_seq(), ring.last_seq()), (1, 0));
    }

    #[test]
    fn default_uses_the_documented_capacity() {
        let ring = EventRing::default();
        assert_eq!(ring.cap, DEFAULT_RING_CAP);
    }
}
