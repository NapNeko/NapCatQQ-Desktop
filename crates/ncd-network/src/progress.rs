//! 下载进度回调抽象。
//!
//! 不直接 import ncd-component::ActionCtx，避免本 crate 反向依赖上层。
//! 调用方实现一个 [`DownloadProgressSink`] 适配器把 [`ProgressUpdate`]
//! 翻译成自己的 ProgressKind / 事件即可。
//!
//! 节流由本 crate 内部完成（每 250ms 或每 1MB 推一次），sink 实现不需要
//! 自己再节流。

use async_trait::async_trait;

use crate::speed::Bps;

/// 下载阶段。UI 据此决定话术与图标。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadStage {
    /// mirror race 期间，多个 connection 并发探测。
    Racing,
    /// 单镜像稳定下载中（或切片下载中的某一片）。
    Streaming,
    /// 当前镜像 stall（idle timeout），切到下一个。
    SwitchingMirror,
    /// 续传：已有 .part 文件，从断点继续。
    Resuming,
}

/// 一次进度更新。
///
/// 字段语义：
/// - `downloaded` / `total`：当前文件已经写入磁盘的字节 / 总大小（None 表示
///   服务端未给 Content-Length，前端只能显示已下载）
/// - `speed_bps`：滑动窗口算出的瞬时速度，None 表示样本不足
/// - `mirror_url`：当前胜出的镜像 URL；UI 调试 / 日志用
#[derive(Debug, Clone)]
pub struct ProgressUpdate {
    pub stage: DownloadStage,
    pub downloaded: u64,
    pub total: Option<u64>,
    pub speed_bps: Option<Bps>,
    pub mirror_url: Option<String>,
    pub message: String,
}

/// 下载进度接收端。
///
/// `tick` 由 download / race / chunked 在节流之后调用，传入最新一帧
/// `ProgressUpdate`。实现里通常做的事：转 ncd-component::ProgressKind →
/// emit 到 ActionCtx → 走事件总线推到前端。
///
/// `&self` 不是 `&mut self`，因为 sink 需要在多个任务（race / chunked）
/// 共享并发调用。实现自己用 Mutex / atomic 守护内部状态。
#[async_trait]
pub trait DownloadProgressSink: Send + Sync {
    async fn tick(&self, update: ProgressUpdate);
}

/// 永远不上报的 sink，给单测和 caller 不关心进度的场景用。
pub struct NoopProgressSink;

#[async_trait]
impl DownloadProgressSink for NoopProgressSink {
    async fn tick(&self, _update: ProgressUpdate) {}
}
