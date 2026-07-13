//! 包管理器互斥锁:防止同一主机的 apt/dnf 并发冲突
//!
//! apt/dnf 使用文件锁,并发执行会导致 dpkg lock 冲突
//! 本模块提供全局锁,确保同一主机的包管理器操作串行执行

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

/// 包管理器全局锁单例,跨组件共享
#[derive(Clone)]
pub struct PackageManagerLock {
    /// 每个主机一个独立的锁key: host_id
    locks: Arc<RwLock<HashMap<String, Arc<Mutex<()>>>>>,
}

impl PackageManagerLock {
    /// 创建锁实例(单例模式由调用方保证)
    pub fn new() -> Self {
        Self {
            locks: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 获取指定主机的包管理器锁返回的 guard 在 drop 时自动释放
    pub async fn acquire(&self, host_id: &str) -> PackageManagerGuard {
        let lock = {
            let mut map = self.locks.write().await;
            map.entry(host_id.to_string())
                .or_insert_with(|| Arc::new(Mutex::new(())))
                .clone()
        };
        let guard = lock.lock_owned().await;
        PackageManagerGuard { _guard: guard }
    }
}

impl Default for PackageManagerLock {
    fn default() -> Self {
        Self::new()
    }
}

/// 锁的 RAII guarddrop 时自动释放
pub struct PackageManagerGuard {
    _guard: tokio::sync::OwnedMutexGuard<()>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use tokio::time::Duration;

    #[tokio::test]
    async fn test_lock_serializes_same_host() {
        let lock = PackageManagerLock::new();
        let counter = Arc::new(AtomicU32::new(0));

        let mut handles = vec![];
        for _ in 0..3 {
            let lock = lock.clone();
            let counter = counter.clone();
            let h = tokio::spawn(async move {
                let _guard = lock.acquire("host1").await;
                let val = counter.fetch_add(1, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(10)).await;
                val
            });
            handles.push(h);
        }

        let mut results = vec![];
        for h in handles {
            results.push(h.await.unwrap());
        }

        // 串行执行,counter 应该是 0, 1, 2
        assert_eq!(results, vec![0, 1, 2]);
    }

    #[tokio::test]
    async fn test_lock_allows_different_hosts() {
        let lock = PackageManagerLock::new();
        let start = std::time::Instant::now();

        let h1 = {
            let lock = lock.clone();
            tokio::spawn(async move {
                let _guard = lock.acquire("host1").await;
                tokio::time::sleep(Duration::from_millis(50)).await;
            })
        };

        let h2 = {
            let lock = lock.clone();
            tokio::spawn(async move {
                let _guard = lock.acquire("host2").await;
                tokio::time::sleep(Duration::from_millis(50)).await;
            })
        };

        h1.await.unwrap();
        h2.await.unwrap();

        // 并行执行,总时间应接近 50ms 而非 100ms
        assert!(start.elapsed().as_millis() < 80);
    }
}
