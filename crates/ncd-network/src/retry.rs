//! 有限次数的瞬时网络错误重试(指数退避)
//!
//! 只给短请求(GitHub API,探测)用;大文件流式下载靠 mirror race + idle
//! timeout,不走本模块

use std::future::Future;
use std::time::Duration;

use tokio::time::sleep;

use crate::error::NetworkError;

/// 重试策略:次数含首次尝试(max_attempts=3 表示最多 3 次调用)
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub initial_backoff: Duration,
    pub max_backoff: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_backoff: Duration::from_millis(200),
            max_backoff: Duration::from_secs(2),
        }
    }
}

/// 是否值得再试一次
pub fn is_retryable(err: &NetworkError) -> bool {
    match err {
        NetworkError::Http(_) => true,
        NetworkError::Status(code) => matches!(*code, 429 | 502 | 503 | 504),
        NetworkError::IdleTimeout(_) => true,
        NetworkError::Cancelled
        | NetworkError::ChecksumMismatch { .. }
        | NetworkError::InvalidArgument(_)
        | NetworkError::AllMirrorsFailed(_)
        | NetworkError::Truncated { .. }
        | NetworkError::Io(_) => false,
    }
}

/// 对异步操作执行重试;operation 每次失败应返回可重试的 NetworkError
pub async fn retry_with_backoff<T, F, Fut>(
    policy: &RetryPolicy,
    mut operation: F,
) -> Result<T, NetworkError>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, NetworkError>>,
{
    let mut attempt = 0u32;
    let mut backoff = policy.initial_backoff;
    loop {
        attempt += 1;
        match operation().await {
            Ok(v) => return Ok(v),
            Err(e) if attempt < policy.max_attempts && is_retryable(&e) => {
                sleep(backoff).await;
                backoff = std::cmp::min(backoff.saturating_mul(2), policy.max_backoff);
            }
            Err(e) => return Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    #[tokio::test]
    async fn succeeds_on_third_attempt() {
        let n = AtomicU32::new(0);
        let policy = RetryPolicy {
            max_attempts: 3,
            initial_backoff: Duration::from_millis(1),
            max_backoff: Duration::from_millis(5),
        };
        let out = retry_with_backoff(&policy, || {
            let c = n.fetch_add(1, Ordering::SeqCst);
            async move {
                if c < 2 {
                    Err(NetworkError::Http("conn reset".into()))
                } else {
                    Ok(42u32)
                }
            }
        })
        .await
        .expect("should succeed");
        assert_eq!(out, 42);
        assert_eq!(n.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn does_not_retry_cancelled() {
        let n = AtomicU32::new(0);
        let policy = RetryPolicy::default();
        let err = retry_with_backoff(&policy, || {
            n.fetch_add(1, Ordering::SeqCst);
            async { Err::<(), _>(NetworkError::Cancelled) }
        })
        .await
        .unwrap_err();
        assert!(matches!(err, NetworkError::Cancelled));
        assert_eq!(n.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn status_retryable_codes() {
        assert!(is_retryable(&NetworkError::Status(503)));
        assert!(!is_retryable(&NetworkError::Status(404)));
    }
}
