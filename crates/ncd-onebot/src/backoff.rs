//! 断线重连的退避：1、2、4、8、16 秒，之后一直 30 秒。
//!
//! 不加随机抖动：对端是本机（或隧道映射的本机端口）的单个 Bot，没有惊群问题，
//! 固定序列反而让界面上「N 秒后重试」可预期、测试可断言。

use std::time::Duration;

/// 依次的等待秒数，用完后停在最后一档
const STEPS_SECS: [u64; 6] = [1, 2, 4, 8, 16, 30];

/// 重连退避计数器。连上之后调用 [`Backoff::reset`] 归零
#[derive(Debug, Clone, Default)]
pub struct Backoff {
    attempt: u32,
}

impl Backoff {
    pub fn new() -> Self {
        Self::default()
    }

    /// 取下一次重试前该等多久，并记一次尝试
    pub fn next_delay(&mut self) -> Duration {
        let index = usize::try_from(self.attempt)
            .unwrap_or(usize::MAX)
            .min(STEPS_SECS.len() - 1);
        self.attempt = self.attempt.saturating_add(1);
        Duration::from_secs(STEPS_SECS.get(index).copied().unwrap_or(30))
    }

    /// 已经取过几次延迟，也就是第几次重试
    pub fn attempt(&self) -> u32 {
        self.attempt
    }

    pub fn reset(&mut self) {
        self.attempt = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secs(backoff: &mut Backoff, n: usize) -> Vec<u64> {
        (0..n).map(|_| backoff.next_delay().as_secs()).collect()
    }

    #[test]
    fn sequence_grows_then_caps_at_thirty() {
        let mut backoff = Backoff::new();
        assert_eq!(secs(&mut backoff, 9), [1, 2, 4, 8, 16, 30, 30, 30, 30]);
    }

    #[test]
    fn attempt_counts_delays_taken() {
        let mut backoff = Backoff::new();
        assert_eq!(backoff.attempt(), 0);
        backoff.next_delay();
        backoff.next_delay();
        assert_eq!(backoff.attempt(), 2);
    }

    #[test]
    fn reset_restarts_the_sequence() {
        let mut backoff = Backoff::new();
        secs(&mut backoff, 7);
        backoff.reset();
        assert_eq!(backoff.attempt(), 0);
        assert_eq!(secs(&mut backoff, 3), [1, 2, 4]);
    }

    #[test]
    fn attempt_counter_saturates() {
        let mut backoff = Backoff { attempt: u32::MAX };
        assert_eq!(backoff.next_delay(), Duration::from_secs(30));
        assert_eq!(backoff.attempt(), u32::MAX);
    }
}
