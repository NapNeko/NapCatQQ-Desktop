//! 离线通知 trait 与默认 noop 实现。
//!
//! 本模块提供：
//! - `OfflineNoticeKind`：仅 `AutoRestart` / `Manual` 两个变体，分别对应
//!   `bot.offline_auto_restart=true` 与 `=false` 两条离线分支。
//! - `OfflineNotifier`：异步 trait，暴露 `notify(bot_id, kind)`。
//! - `NoopOfflineNotifier`：默认实现，仅 `tracing::info!` 记录一条事件，
//!   不发起任何外部网络 I/O。

use async_trait::async_trait;

use crate::ids::BotId;

/// 离线通知的语义分类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfflineNoticeKind {
    /// `bot.offline_auto_restart=true` 路径：通知用户「检测到离线，已自动重启」。
    AutoRestart,
    /// `bot.offline_auto_restart=false` 路径：通知用户「检测到离线，请手动处理」。
    Manual,
}

/// 离线通知抽象。
///
/// 由 `NapCatLoginPoller` 在离线分支调用；具体通道（webhook / email / 桌面通知）
/// 由实现方决定。
#[async_trait]
pub trait OfflineNotifier: Send + Sync {
    /// 通知 `bot_id` 发生离线事件。
    ///
    /// 实现 SHOULD 在内部消化所有错误，通知失败不应阻断 Poller 的命令路径。
    async fn notify(&self, bot_id: &BotId, kind: OfflineNoticeKind);
}

/// 默认 noop 实现。仅 `tracing::info!` 记录一条 log，不发起任何 I/O。
///
/// 用作 `BotManager::new` 的默认 wiring；接入真实通道时直接替换
/// `Arc<dyn OfflineNotifier>` 即可，不需要改动 Poller。
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopOfflineNotifier;

#[async_trait]
impl OfflineNotifier for NoopOfflineNotifier {
    async fn notify(&self, bot_id: &BotId, kind: OfflineNoticeKind) {
        tracing::info!(%bot_id, ?kind, "offline notice (noop, awaiting integration)");
    }
}

#[cfg(test)]
mod tests {
    //! 验证 `NoopOfflineNotifier::notify` 调用后无副作用、不 panic，
    //! 并通过 `tracing-subscriber` 抓 log 行验证 `kind` 字段。
    //!
    //! 测试拓扑：
    //! - `VecWriter` 把 fmt 层输出捕获到 `Arc<Mutex<Vec<u8>>>`，避免依赖
    //!   `tracing-test`。
    //! - 使用 `tracing::subscriber::set_default` 在当前线程作用域内启用
    //!   subscriber；`#[tokio::test]` 默认 current_thread runtime，跨 await
    //!   仍在同一线程，subscriber 不会丢失。

    #![allow(clippy::unwrap_in_result, clippy::expect_used)]
    use super::*;
    use crate::ids::BotId;

    use std::io;
    use std::sync::{Arc, Mutex};

    use tracing_subscriber::fmt::MakeWriter;

    /// 把 tracing fmt 层输出收集到内存 buffer 的 writer。
    #[derive(Clone, Default)]
    struct VecWriter(Arc<Mutex<Vec<u8>>>);

    impl VecWriter {
        fn captured(&self) -> String {
            let bytes = self.0.lock().expect("VecWriter mutex poisoned");
            String::from_utf8(bytes.clone()).expect("captured log output is not valid UTF-8")
        }
    }

    impl io::Write for VecWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0
                .lock()
                .expect("VecWriter mutex poisoned")
                .extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl<'a> MakeWriter<'a> for VecWriter {
        type Writer = VecWriter;

        fn make_writer(&'a self) -> Self::Writer {
            self.clone()
        }
    }

    /// `NoopOfflineNotifier::notify` 仅打日志，无副作用。
    ///
    /// 通过自定义 `MakeWriter` 抓取 fmt 层输出，断言两次调用分别记录了
    /// `AutoRestart` 与 `Manual` 两个 kind 字段，且 `bot_id` 字段也出现在日志中。
    #[tokio::test]
    async fn noop_notifier_notify_logs_kind_and_bot_id() {
        let buf = VecWriter::default();
        let subscriber = tracing_subscriber::fmt()
            .with_writer(buf.clone())
            .with_max_level(tracing::Level::INFO)
            .with_ansi(false)
            .with_target(false)
            .finish();

        // current_thread runtime + set_default ⇒ subscriber 跨 await 仍生效。
        let _guard = tracing::subscriber::set_default(subscriber);

        let bot_id = BotId::new("test-bot-001");
        let notifier = NoopOfflineNotifier;

        notifier
            .notify(&bot_id, OfflineNoticeKind::AutoRestart)
            .await;
        notifier.notify(&bot_id, OfflineNoticeKind::Manual).await;

        // 释放 guard 后再读 buffer，确保所有日志已经 flush。
        drop(_guard);

        let output = buf.captured();
        assert!(
            output.contains("AutoRestart"),
            "log should record AutoRestart kind, got: {output}"
        );
        assert!(
            output.contains("Manual"),
            "log should record Manual kind, got: {output}"
        );
        assert!(
            output.contains("test-bot-001"),
            "log should record bot_id, got: {output}"
        );
    }

    /// 在没有任何 subscriber 注册时调用 `notify` 也必须不 panic、并且
    /// 不会抛错（tracing 的 noop dispatcher 兜底）。
    #[tokio::test]
    async fn noop_notifier_notify_completes_without_subscriber() {
        let bot_id = BotId::new("test-bot-002");
        let notifier = NoopOfflineNotifier;

        notifier
            .notify(&bot_id, OfflineNoticeKind::AutoRestart)
            .await;
        notifier.notify(&bot_id, OfflineNoticeKind::Manual).await;
    }

    /// `OfflineNoticeKind` 仅 `AutoRestart` / `Manual` 两个变体，
    /// 且实现 `Copy + Eq`（用于 `LoginState` 内部 flag 比较）。
    #[test]
    fn offline_notice_kind_supports_copy_and_eq() {
        let a = OfflineNoticeKind::AutoRestart;
        let b = a; // Copy
        assert_eq!(a, b);
        assert_ne!(OfflineNoticeKind::AutoRestart, OfflineNoticeKind::Manual);
    }
}
