//! ActionCtx:Action 执行上下文(进度上报 + 取消 + 日志注入)
//!
//! 统一进度反馈,杜绝 legacy 的 Qt Signal vs LogLineCallback 双套
//!
//! 上层(ncd-deploy / Tauri Command)通过 ActionCtx 拿到 ProgressEvent 流,
//! 转发到 BroadcastEventBus,前端订阅
//!
//! ProgressEvent / ProgressKind / ProgressLogLevel 已下沉到 ncd-domain,
//! 此处 re-export 保持向后兼容。ActionCtx 留在本 crate（依赖 tokio mpsc）。

use std::sync::Arc;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

// 进度数据类型已下沉到 ncd-domain，re-export 保持向后兼容
pub use ncd_domain::progress::{ProgressEvent, ProgressKind, ProgressLogLevel};

type LoggerFn = Arc<dyn Fn(&str) + Send + Sync>;

/// ActionCtx:Action 执行期间的辅助上下文
///
/// 字段全部 Arc<...> 让 ActionCtx 可以低成本 clone 给子任务
#[derive(Clone)]
pub struct ActionCtx {
    /// 进度事件发送端(满后会阻塞 push 端,保证 UI 不丢消息)
    pub(crate) progress: mpsc::Sender<ProgressEvent>,
    /// 取消信号
    pub(crate) cancel: CancellationToken,
    /// 日志回调(可选,默认走 tracing)
    pub(crate) logger: Option<LoggerFn>,
}

impl ActionCtx {
    /// 创建一个新 ActionCtx,返回 (ctx, progress_rx)
    pub fn new() -> (Self, mpsc::Receiver<ProgressEvent>) {
        Self::with_capacity(64)
    }

    /// 自定义 channel 容量
    pub fn with_capacity(cap: usize) -> (Self, mpsc::Receiver<ProgressEvent>) {
        let (tx, rx) = mpsc::channel(cap);
        let ctx = Self {
            progress: tx,
            cancel: CancellationToken::new(),
            logger: None,
        };
        (ctx, rx)
    }

    /// 派生子 ctx,保持同一进度通道但创建独立取消子节点(parent 取消会传播到 child)
    pub fn child(&self) -> Self {
        Self {
            progress: self.progress.clone(),
            cancel: self.cancel.child_token(),
            logger: self.logger.clone(),
        }
    }

    /// 主动取消(整个 ActionCtx 树)
    pub fn cancel(&self) {
        self.cancel.cancel();
    }

    /// 是否已取消
    pub fn is_cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }

    pub fn cancel_token(&self) -> CancellationToken {
        self.cancel.clone()
    }

    /// 上报进度,channel 满时异步等待,保证 UI 不丢消息
    pub async fn emit(&self, kind: ProgressKind) {
        let _ = self.progress.send(ProgressEvent::new(kind)).await;
    }

    /// 便捷 helper:发 Log 事件
    pub async fn log(&self, level: ProgressLogLevel, message: impl Into<String>) {
        let msg = message.into();
        // 同时写 tracing
        match level {
            ProgressLogLevel::Trace => tracing::trace!(target: "ncd_component", "{}", msg),
            ProgressLogLevel::Debug => tracing::debug!(target: "ncd_component", "{}", msg),
            ProgressLogLevel::Info => tracing::info!(target: "ncd_component", "{}", msg),
            ProgressLogLevel::Warn => tracing::warn!(target: "ncd_component", "{}", msg),
            ProgressLogLevel::Error => tracing::error!(target: "ncd_component", "{}", msg),
        }
        if let Some(cb) = &self.logger {
            cb(&msg);
        }
        self.emit(ProgressKind::Log {
            level,
            message: msg,
        })
        .await;
    }

    /// 便捷 helper:Info 级别日志
    pub async fn info(&self, message: impl Into<String>) {
        self.log(ProgressLogLevel::Info, message).await;
    }

    /// 便捷 helper:Warn 级别日志
    pub async fn warn(&self, message: impl Into<String>) {
        self.log(ProgressLogLevel::Warn, message).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn emit_pushes_event_to_receiver() {
        let (ctx, mut rx) = ActionCtx::new();
        ctx.emit(ProgressKind::Started { total_steps: 3 }).await;
        let evt = rx.recv().await.unwrap();
        assert_eq!(evt.v, 1);
        assert!(matches!(evt.kind, ProgressKind::Started { total_steps: 3 }));
    }

    #[tokio::test]
    async fn child_inherits_cancel() {
        let (parent, _rx) = ActionCtx::new();
        let child = parent.child();
        parent.cancel();
        assert!(child.is_cancelled());
    }

    #[tokio::test]
    async fn child_can_be_cancelled_independently() {
        let (parent, _rx) = ActionCtx::new();
        let child = parent.child();
        child.cancel();
        assert!(child.is_cancelled());
        // 注意:child cancel 不传播到 parent
        assert!(!parent.is_cancelled());
    }

    #[tokio::test]
    async fn log_emits_event() {
        let (ctx, mut rx) = ActionCtx::new();
        ctx.info("hello").await;
        let evt = rx.recv().await.unwrap();
        match evt.kind {
            ProgressKind::Log { level, message } => {
                assert_eq!(level, ProgressLogLevel::Info);
                assert_eq!(message, "hello");
            }
            other => panic!("expected Log, got {other:?}"),
        }
    }

    #[test]
    fn progress_event_serializes_with_version_field() {
        let evt = ProgressEvent::new(ProgressKind::Started { total_steps: 2 });
        let json = serde_json::to_string(&evt).unwrap();
        assert!(json.contains("\"v\":1"));
        assert!(json.contains("\"kind\":\"started\""));
        assert!(json.contains("\"total_steps\":2"));
    }

    #[test]
    fn progress_event_round_trips_with_default_version() {
        // 旧前端解析新事件应能 fallback 到 v=1
        let json = r#"{"timestamp_ms":1234,"kind":"started","total_steps":5}"#;
        let evt: ProgressEvent = serde_json::from_str(json).unwrap();
        assert_eq!(evt.v, 1);
    }
}
