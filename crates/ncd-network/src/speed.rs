//! 滑动窗口瞬时速度采样。
//!
//! 调用方 record(now, downloaded_bytes) 喂样本，current_bps() 给最近 window 时间内的
//! 平均速度。样本不足或全在同一时刻视为不可估计，返回 None。
//!
//! 不持有锁，调用方自行用 Mutex / 单线程驱动。download.rs 的下载循环
//! 是单线程消费 stream，不需要锁；race.rs 内部聚合时也保证 SpeedSampler
//! 由单个 task 持有。

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// 瞬时速度，单位字节/秒。
pub type Bps = u64;

/// 滑动窗口速度计。
#[derive(Debug, Clone)]
pub struct SpeedSampler {
    window: Duration,
    samples: VecDeque<(Instant, u64)>,
}

impl SpeedSampler {
    /// 默认 3 秒窗口。短了波动太大，长了对网速变化反应慢。
    pub fn new() -> Self {
        Self::with_window(Duration::from_secs(3))
    }

    pub fn with_window(window: Duration) -> Self {
        Self {
            window,
            samples: VecDeque::with_capacity(64),
        }
    }
}

impl SpeedSampler {
    /// 记录一个样本。downloaded 是从下载开始累计的字节数（单调递增）。
    pub fn record(&mut self, now: Instant, downloaded: u64) {
        self.samples.push_back((now, downloaded));
        // 丢弃落出窗口的老样本，但至少保留 1 个供 current_bps 比较。
        while self.samples.len() > 1 {
            let Some(&(oldest, _)) = self.samples.front() else {
                break;
            };
            if now.saturating_duration_since(oldest) <= self.window {
                break;
            }
            self.samples.pop_front();
        }
    }

    /// 当前瞬时速度。样本少于 2 或时间跨度 < 100ms 时返回 None
    /// （太短的窗口算出来不稳定，UI 上闪烁严重）。
    pub fn current_bps(&self) -> Option<Bps> {
        if self.samples.len() < 2 {
            return None;
        }
        let (t0, b0) = *self.samples.front()?;
        let (t1, b1) = *self.samples.back()?;
        let dt = t1.saturating_duration_since(t0);
        if dt < Duration::from_millis(100) {
            return None;
        }
        if b1 <= b0 {
            return Some(0);
        }
        let bytes = b1 - b0;
        // f64 -> u64 的 saturating_cast 在 Rust 标准库里没有 stable API；
        // 这里手动饱和，避免速度估出 NaN / 负数 / 溢出。
        let bps = (bytes as f64) / dt.as_secs_f64();
        if !bps.is_finite() || bps < 0.0 {
            return Some(0);
        }
        Some(bps.min(u64::MAX as f64) as Bps)
    }
}

impl Default for SpeedSampler {
    fn default() -> Self {
        Self::new()
    }
}
