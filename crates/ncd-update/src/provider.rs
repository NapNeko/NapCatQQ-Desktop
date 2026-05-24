//! `UpdateProvider`:更新源抽象。
//!
//! 蓝图 §7.1:实际的 `tauri-plugin-updater` 集成由 `src-tauri` 实装,
//! 本 trait 让 ncd-update 自己也能用 mock provider 测试,不依赖 Tauri runtime。

use async_trait::async_trait;
use std::sync::Mutex;

use crate::channel::UpdateChannel;
use crate::error::UpdateError;
use crate::types::AvailableUpdate;

/// 更新源抽象。
#[async_trait]
pub trait UpdateProvider: Send + Sync {
    /// 检查指定通道是否有更新。`Ok(None)` 表示没有更新。
    async fn check(&self, channel: UpdateChannel) -> Result<Option<AvailableUpdate>, UpdateError>;

    /// 下载 + 验签 + 安装。安装成功后调用方应主动 quit 当前进程
    /// (Tauri plugin 在 Windows 会自动 quit,所以这里可能不返回)。
    async fn download_and_install(&self, update: &AvailableUpdate) -> Result<(), UpdateError>;
}

/// `MockUpdateProvider`:测试用 provider。
pub struct MockUpdateProvider {
    /// 下次 `check` 返回的结果(成功 / 失败可注入)
    next_check: Mutex<Option<Result<Option<AvailableUpdate>, UpdateError>>>,
    /// 下次 `download_and_install` 返回的结果
    next_install: Mutex<Option<Result<(), UpdateError>>>,
    /// 调用计数
    pub check_calls: std::sync::atomic::AtomicU32,
    pub install_calls: std::sync::atomic::AtomicU32,
}

impl MockUpdateProvider {
    pub fn new() -> Self {
        Self {
            next_check: Mutex::new(None),
            next_install: Mutex::new(None),
            check_calls: std::sync::atomic::AtomicU32::new(0),
            install_calls: std::sync::atomic::AtomicU32::new(0),
        }
    }

    pub fn set_next_check(&self, result: Result<Option<AvailableUpdate>, UpdateError>) {
        *self.next_check.lock().unwrap() = Some(result);
    }

    pub fn set_next_install(&self, result: Result<(), UpdateError>) {
        *self.next_install.lock().unwrap() = Some(result);
    }
}

impl Default for MockUpdateProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl UpdateProvider for MockUpdateProvider {
    async fn check(&self, _channel: UpdateChannel) -> Result<Option<AvailableUpdate>, UpdateError> {
        self.check_calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        match self.next_check.lock().unwrap().take() {
            Some(r) => r,
            None => Ok(None),
        }
    }

    async fn download_and_install(&self, _update: &AvailableUpdate) -> Result<(), UpdateError> {
        self.install_calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        match self.next_install.lock().unwrap().take() {
            Some(r) => r,
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::AvailableUpdate;
    use chrono::Utc;
    use ncd_domain::SchemaVersion;
    use semver::Version;
    use std::sync::atomic::Ordering;

    fn fake_update() -> AvailableUpdate {
        AvailableUpdate {
            v: 1,
            version: Version::new(0, 2, 0),
            schema_version: SchemaVersion::V3,
            notes: "x".into(),
            pub_date: Utc::now(),
            download_url: "https://example/u.msi".into(),
            signature: "sig".into(),
        }
    }

    #[tokio::test]
    async fn mock_check_returns_injected_update() {
        let p = MockUpdateProvider::new();
        p.set_next_check(Ok(Some(fake_update())));
        let r = p.check(UpdateChannel::Stable).await.unwrap();
        assert!(r.is_some());
        assert_eq!(p.check_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn mock_check_default_no_update() {
        let p = MockUpdateProvider::new();
        let r = p.check(UpdateChannel::Stable).await.unwrap();
        assert!(r.is_none());
    }

    #[tokio::test]
    async fn mock_install_returns_injected_error() {
        let p = MockUpdateProvider::new();
        p.set_next_install(Err(UpdateError::install_failed("simulated")));
        let r = p.download_and_install(&fake_update()).await;
        assert!(r.is_err());
        assert_eq!(p.install_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn mock_check_can_inject_failure() {
        let p = MockUpdateProvider::new();
        p.set_next_check(Err(UpdateError::check_failed("boom")));
        let r = p.check(UpdateChannel::Stable).await;
        assert!(matches!(r, Err(UpdateError::CheckFailed { .. })));
    }
}
