//! `ActionCtx`:Action 执行上下文(进度上报 + 取消 + 日志注入)。
//!
//! 统一进度反馈,杜绝 legacy 的 Qt Signal vs LogLineCallback 双套。
//!
//! 上层(`ncd-deploy` / Tauri Command)通过 `ActionCtx` 拿到 ProgressEvent 流,
//! 转发到 `BroadcastEventBus`,前端订阅。

use std::sync::Arc;
use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// 进度事件类型。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProgressKind {
    /// 整个 Action 开始,带步骤总数
    Started { total_steps: u32 },
    /// 进入第 N 步
    StepBegin { step: u32, message: String },
    /// 第 N 步进度(percent 0-100)
    StepProgress {
        step: u32,
        percent: u8,
        message: String,
    },
    /// 第 N 步结束
    StepEnd { step: u32, ok: bool },
    /// 整个 Action 结束
    Finished { ok: bool },
    /// 普通日志(不算进度,只是过程信息)
    Log {
        level: LogLevel,
        message: String,
    },
}

/// 日志级别(对齐 tracing 风格)。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

/// 进度事件(envelope)。跨边界事件必须带版本号,便于增量演进。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressEvent {
    /// 协议版本(默认 1,bump 时同步前端)
    #[serde(default = "default_version")]
    pub v: u32,
    /// 事件时间戳(unix millis)
    pub timestamp_ms: u64,
    /// 事件内容
    #[serde(flatten)]
    pub kind: ProgressKind,
}

fn default_version() -> u32 {
    1
}

impl ProgressEvent {
    /// 创建一个新事件,自动填时间戳与 v=1。
    pub fn new(kind: ProgressKind) -> Self {
        Self {
            v: 1,
            timestamp_ms: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
            kind,
        }
    }
}

/// `ActionCtx`:Action 执行期间的辅助上下文。
///
/// 字段全部 `Arc<...>` 让 ActionCtx 可以低成本 clone 给子任务。
#[derive(Clone)]
pub struct ActionCtx {
    /// 进度事件发送端(满后会阻塞 push 端,保证 UI 不丢消息)
    pub(crate) progress: mpsc::Sender<ProgressEvent>,
    /// 取消信号
    pub(crate) cancel: CancellationToken,
    /// 日志回调(可选,默认走 tracing)
    pub(crate) logger: Option<Arc<dyn Fn(&str) + Send + Sync>>,
}

impl ActionCtx {
    /// 创建一个新 ActionCtx,返回 (ctx, progress_rx)。
    pub fn new() -> (Self, mpsc::Receiver<ProgressEvent>) {
        Self::with_capacity(64)
    }

    /// 自定义 channel 容量。
    pub fn with_capacity(cap: usize) -> (Self, mpsc::Receiver<ProgressEvent>) {
        let (tx, rx) = mpsc::channel(cap);
        let ctx = Self {
            progress: tx,
            cancel: CancellationToken::new(),
            logger: None,
        };
        (ctx, rx)
    }

    /// 派生子 ctx,保持同一进度通道但创建独立取消子节点(parent 取消会传播到 child)。
    pub fn child(&self) -> Self {
        Self {
            progress: self.progress.clone(),
            cancel: self.cancel.child_token(),
            logger: self.logger.clone(),
        }
    }

    /// 主动取消(整个 ActionCtx 树)。
    pub fn cancel(&self) {
        self.cancel.cancel();
    }

    /// 是否已取消。
    pub fn is_cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }

    pub fn cancel_token(&self) -> CancellationToken {
        self.cancel.clone()
    }

    /// 上报进度。channel 满会异步等待,保证 UI 不丢消息。
    pub async fn emit(&self, kind: ProgressKind) {
        let _ = self.progress.send(ProgressEvent::new(kind)).await;
    }

    /// 便捷 helper:发 Log 事件。
    pub async fn log(&self, level: LogLevel, message: impl Into<String>) {
        let msg = message.into();
        // 同时写 tracing
        match level {
            LogLevel::Trace => tracing::trace!(target: "ncd_component", "{}", msg),
            LogLevel::Debug => tracing::debug!(target: "ncd_component", "{}", msg),
            LogLevel::Info => tracing::info!(target: "ncd_component", "{}", msg),
            LogLevel::Warn => tracing::warn!(target: "ncd_component", "{}", msg),
            LogLevel::Error => tracing::error!(target: "ncd_component", "{}", msg),
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

    /// 便捷 helper:Info 级别日志。
    pub async fn info(&self, message: impl Into<String>) {
        self.log(LogLevel::Info, message).await;
    }

    /// 便捷 helper:Warn 级别日志。
    pub async fn warn(&self, message: impl Into<String>) {
        self.log(LogLevel::Warn, message).await;
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
                assert_eq!(level, LogLevel::Info);
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
