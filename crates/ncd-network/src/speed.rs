//! 滑动窗口瞬时速度采样
//!
//! 调用方 record(now, downloaded_bytes) 喂样本,current_bps() 给最近 window 时间内的
//! 平均速度. 样本不足或全在同一时刻视为不可估计,返回 None
//!
//! 不持有锁,调用方自行用 Mutex / 单线程驱动;download.rs 的下载循环
//! 是单线程消费 stream,不需要锁;race.rs 内部聚合时也保证 SpeedSampler
//! 由单个 task 持有

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// 瞬时速度,单位字节/秒
pub type Bps = u64;

/// 滑动窗口速度计
#[derive(Debug, Clone)]
pub struct SpeedSampler {
    window: Duration,
    samples: VecDeque<(Instant, u64)>,
}

impl SpeedSampler {
    /// 默认 3 秒窗口短了波动太大,长了对网速变化反应慢
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
    /// 记录一个样本,downloaded 是从下载开始累计的字节数(单调递增)
    pub fn record(&mut self, now: Instant, downloaded: u64) {
        self.samples.push_back((now, downloaded));
        // 保留窗口边界前的一个样本；长时间无字节后也能估出实际低速。
        while self.samples.len() > 2 {
            let Some(&(next, _)) = self.samples.get(1) else {
                break;
            };
            if now.saturating_duration_since(next) <= self.window {
                break;
            }
            self.samples.pop_front();
        }
    }

    /// 当前瞬时速度. 样本少于 2 或时间跨度 < 100ms 时返回 None
    /// (太短的窗口算出来不稳定,UI 上闪烁严重)
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
        // f64 -> u64 的 saturating_cast 在 Rust 标准库里没有 stable API;
        // 这里手动饱和,避免速度估出 NaN / 负数 / 溢出
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

/// 持续低速检测；只在还有其它候选源时启用，单源下载默认不设速度下限。
#[derive(Debug, Clone)]
pub struct LowSpeedConfig {
    pub min_bps: u64,
    /// 连接建立后的预热时间，避免把 TLS / CDN 启动算成低速。
    pub grace_period: Duration,
    /// 必须观察完整窗口，短暂抖动不会触发换源。
    pub window: Duration,
}

impl Default for LowSpeedConfig {
    fn default() -> Self {
        Self {
            min_bps: 128 * 1024,
            grace_period: Duration::from_secs(4),
            window: Duration::from_secs(6),
        }
    }
}

pub(crate) struct LowSpeedMonitor {
    cfg: LowSpeedConfig,
    started: Instant,
    samples: VecDeque<(Instant, u64)>,
}

impl LowSpeedMonitor {
    pub(crate) fn new(cfg: LowSpeedConfig, now: Instant, _downloaded: u64) -> Self {
        Self {
            cfg,
            started: now,
            samples: VecDeque::new(),
        }
    }

    pub(crate) fn check(
        &mut self,
        now: Instant,
        downloaded: u64,
        total: Option<u64>,
    ) -> Option<u64> {
        if self.cfg.min_bps == 0
            || self.cfg.window.is_zero()
            || now.saturating_duration_since(self.started) < self.cfg.grace_period
        {
            return None;
        }
        self.samples.push_back((now, downloaded));
        while self.samples.len() > 2 {
            let (time, _) = self.samples[1];
            if now.saturating_duration_since(time) < self.cfg.window {
                break;
            }
            self.samples.pop_front();
        }
        let &(start, bytes) = self.samples.front()?;
        let elapsed = now.saturating_duration_since(start);
        if elapsed < self.cfg.window {
            return None;
        }
        let bps = (downloaded.saturating_sub(bytes) as f64 / elapsed.as_secs_f64()) as u64;
        // 按实测速度估计剩余时间；不能让固定的小文件阈值豁免滴流下载。
        let nearly_done = total.is_some_and(|total| {
            total.saturating_sub(downloaded) <= (bps as f64 * self.cfg.window.as_secs_f64()) as u64
        });
        (bps < self.cfg.min_bps && !nearly_done).then_some(bps)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sparse_samples_include_wait_time() {
        let start = Instant::now();
        let mut sampler = SpeedSampler::new();
        sampler.record(start, 0);
        sampler.record(start + Duration::from_secs(10), 1000);
        assert_eq!(sampler.current_bps(), Some(100));
    }

    #[test]
    fn low_speed_requires_warmup_and_full_window() {
        let start = Instant::now();
        let cfg = LowSpeedConfig::default();
        let mut monitor = LowSpeedMonitor::new(cfg, start, 0);
        for seconds in 0..10 {
            assert_eq!(
                monitor.check(start + Duration::from_secs(seconds), seconds * 1024, None),
                None
            );
        }
        assert_eq!(
            monitor.check(start + Duration::from_secs(10), 10 * 1024, None),
            Some(1024)
        );
        assert_eq!(
            monitor.check(start + Duration::from_secs(11), 11 * 1024, Some(12 * 1024)),
            None
        );
    }
}
