//! 事件环形缓冲：给每个 Bot 的事件流编号并保留最近一批，供前端补拉和断线后对账。

use std::collections::VecDeque;
use std::io::{self, Write};
use std::sync::Arc;

use ncd_domain::onebot_debug::{DebugEvent, DebugEventBody};

/// 条数与 JSON payload 预算同时限制缓存；这些字节不代表进程内存。
pub const DEFAULT_RING_CAP: usize = 5000;
/// 环形缓冲保留的 JSON payload 总量。
pub const DEFAULT_RING_PAYLOAD_BYTES: usize = 16 * 1024 * 1024;
/// 超过此 JSON payload 大小的正文只实时发送，补拉用缺失标记。
pub const MAX_EVENT_PAYLOAD_BYTES: usize = 1024 * 1024;

/// 定容的事件缓冲。`seq` 从 1 起单调递增，被挤掉或清空也不回退 ——
/// 前端拿 `seq` 当游标，回退会让它以为事件重复了
#[derive(Debug)]
pub struct EventRing {
    cap: usize,
    next_seq: u64,
    items: VecDeque<StoredEvent>,
    payload_bytes: usize,
    dropped_total: u64,
}

#[derive(Debug)]
struct StoredEvent {
    event: Arc<DebugEvent>,
    payload_bytes: usize,
    loss_counted: bool,
}

impl EventRing {
    /// 容量至少为 1：容量 0 会让每条事件一进来就被挤掉，调用方拿到的却是个有编号的事件
    pub fn new(cap: usize) -> Self {
        let cap = cap.max(1);
        Self {
            cap,
            next_seq: 1,
            items: VecDeque::with_capacity(cap.min(1024)),
            payload_bytes: 0,
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
        let (event, _) = self.push_shared(at_ms, body);
        Arc::unwrap_or_clone(event)
    }

    /// 接收器的实时队列与缓存共享正文，只有走出 IPC 边界时才需要拥有一份独立事件。
    pub fn push_shared(&mut self, at_ms: u64, body: DebugEventBody) -> (Arc<DebugEvent>, usize) {
        let event = Arc::new(DebugEvent {
            seq: self.next_seq,
            at_ms,
            body,
        });
        self.next_seq = self.next_seq.saturating_add(1);
        let bytes = payload_bytes(&event);
        let loss_counted = bytes > MAX_EVENT_PAYLOAD_BYTES;
        let stored = if loss_counted {
            // 保留同一 seq 的显式缺失标记，补拉仍连续；实时订阅者拿到原始完整正文。
            self.dropped_total = self.dropped_total.saturating_add(1);
            Arc::new(DebugEvent {
                seq: event.seq,
                at_ms,
                body: DebugEventBody::Dropped { count: 1 },
            })
        } else {
            Arc::clone(&event)
        };
        let stored_bytes = if loss_counted {
            payload_bytes(&stored)
        } else {
            bytes
        };
        self.items.push_back(StoredEvent {
            event: stored,
            payload_bytes: stored_bytes,
            loss_counted,
        });
        self.payload_bytes = self.payload_bytes.saturating_add(stored_bytes);
        while self.items.len() > self.cap || self.payload_bytes > DEFAULT_RING_PAYLOAD_BYTES {
            if let Some(old) = self.items.pop_front() {
                self.payload_bytes = self.payload_bytes.saturating_sub(old.payload_bytes);
                if !old.loss_counted {
                    self.dropped_total = self.dropped_total.saturating_add(1);
                }
            }
        }
        (event, bytes)
    }

    /// `seq > since_seq` 的事件，从老到新，最多 `limit` 条
    pub fn read_since(&self, since_seq: u64, limit: usize) -> Vec<DebugEvent> {
        self.iter_since(since_seq)
            .take(limit)
            .map(|(event, _)| event.clone())
            .collect()
    }

    /// 补发按批次克隆，避免先复制整个缓冲；第二项是该条 JSON payload 字节数。
    pub fn iter_since(&self, since_seq: u64) -> impl Iterator<Item = (&DebugEvent, usize)> {
        // items 按 seq 升序，二分找到第一条大于游标的
        let start = self.items.partition_point(|e| e.event.seq <= since_seq);
        self.items
            .iter()
            .skip(start)
            .map(|item| (item.event.as_ref(), item.payload_bytes))
    }

    /// 最新的 `limit` 条，从老到新
    pub fn tail(&self, limit: usize) -> Vec<DebugEvent> {
        let skip = self.items.len().saturating_sub(limit);
        self.items
            .iter()
            .skip(skip)
            .map(|item| item.event.as_ref().clone())
            .collect()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// 缓冲里最老一条的 `seq`；空的时候是下一条将要拿到的 `seq`
    pub fn first_seq(&self) -> u64 {
        self.items.front().map_or(self.next_seq, |e| e.event.seq)
    }

    /// 最近一次分配出去的 `seq`（一条都没推过为 0）。清空缓冲后仍保持，方便前端续用游标
    pub fn last_seq(&self) -> u64 {
        self.next_seq - 1
    }

    /// 被预算省略或挤掉的累计事件数（已计数的缺失标记淘汰、手动 `clear` 不再计入）。
    pub fn dropped_total(&self) -> u64 {
        self.dropped_total
    }

    /// 清空缓冲。`seq` 继续往后走，`dropped_total` 保留
    pub fn clear(&mut self) {
        self.items = VecDeque::new();
        self.payload_bytes = 0;
    }
}

fn payload_bytes(event: &DebugEvent) -> usize {
    struct Counter(usize);

    impl Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0 = self.0.saturating_add(bytes.len());
            if self.0 > MAX_EVENT_PAYLOAD_BYTES {
                return Err(io::Error::other("event exceeds cache payload budget"));
            }
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    // 只计数、不构造 JSON String；超过单条预算后停止遍历。
    let mut counter = Counter(0);
    if serde_json::to_writer(&mut counter, event).is_err() {
        MAX_EVENT_PAYLOAD_BYTES + 1
    } else {
        counter.0
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
        assert_eq!((ring.payload_bytes, ring.items.capacity()), (0, 0));
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

    #[test]
    fn shared_push_retains_one_body_and_counts_json_bytes_without_a_copy() {
        let mut ring = EventRing::new(3);
        let (event, bytes) = ring.push_shared(
            10,
            DebugEventBody::Ob11 {
                payload: json!({"text": "中文\n\"quoted\""}),
            },
        );
        assert!(Arc::ptr_eq(&event, &ring.items[0].event));
        assert_eq!(bytes, serde_json::to_vec(event.as_ref()).unwrap().len());
        assert_eq!(ring.payload_bytes, bytes);
    }

    #[test]
    fn payload_budget_evicts_old_events_before_the_count_limit() {
        let mut ring = EventRing::new(DEFAULT_RING_CAP);
        let count = 40_u64;
        for n in 1..=count {
            ring.push_shared(
                n,
                DebugEventBody::Ob11 {
                    payload: json!({"text": "x".repeat(512 * 1024)}),
                },
            );
        }
        assert!(ring.payload_bytes <= DEFAULT_RING_PAYLOAD_BYTES);
        assert!(ring.len() < count as usize);
        assert_eq!(ring.dropped_total(), count - ring.len() as u64);
        assert_eq!(
            seqs(&ring.read_since(0, DEFAULT_RING_CAP)),
            (ring.first_seq()..=count).collect::<Vec<_>>()
        );
        assert_eq!(ring.last_seq(), count);
    }

    #[test]
    fn oversized_body_is_returned_whole_and_cached_as_one_loss_only() {
        let mut ring = EventRing::new(2);
        let large = DebugEventBody::Ob11 {
            payload: json!({"text": "x".repeat(MAX_EVENT_PAYLOAD_BYTES + 1)}),
        };
        let (event, bytes) = ring.push_shared(10, large.clone());
        assert!(bytes > MAX_EVENT_PAYLOAD_BYTES);
        assert_eq!(event.body, large);
        let backlog = ring.read_since(0, 10);
        assert_eq!(backlog[0].seq, event.seq);
        assert_eq!(backlog[0].body, DebugEventBody::Dropped { count: 1 });
        assert_eq!(ring.dropped_total(), 1);

        ring.push(20, body(2));
        ring.push(30, body(3));
        assert_eq!(seqs(&ring.tail(10)), [2, 3]);
        assert_eq!(ring.dropped_total(), 1, "已省略正文的占位淘汰不重复计数");
        assert_eq!((ring.first_seq(), ring.last_seq()), (2, 3));
    }
}
