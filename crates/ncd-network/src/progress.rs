//! 下载进度回调抽象;不 import ncd-component 避免本 crate 反向依赖上层,调用方
//! 实现 DownloadProgressSink 适配器翻译成自己的 ProgressKind;事件节流由本 crate
//! 完成(250ms 或 1MB 一次),sink 不用再节流

use async_trait::async_trait;

use crate::speed::Bps;

/// 下载阶段,UI 据此决定话术与图标
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadStage {
    /// mirror race 期间,多个 connection 并发探测
    Racing,
    /// 单镜像稳定下载中(或切片下载中的某一片)
    Streaming,
    /// 当前镜像 stall(idle timeout),切到下一个
    SwitchingMirror,
    /// 续传:已有 .part 文件,从断点继续
    Resuming,
}

/// 一次进度更新:total None 表示服务端未给 Content-Length,speed_bps None 表示样本不足
#[derive(Debug, Clone)]
pub struct ProgressUpdate {
    pub stage: DownloadStage,
    pub downloaded: u64,
    pub total: Option<u64>,
    pub speed_bps: Option<Bps>,
    pub mirror_url: Option<String>,
    pub message: String,
}

/// 下载进度接收端,tick 由 download/race/chunked 节流后调用;&self 非 &mut self,
/// 因为 sink 要在多任务共享并发调用,实现自己用 Mutex/atomic 守内部状态
#[async_trait]
pub trait DownloadProgressSink: Send + Sync {
    async fn tick(&self, update: ProgressUpdate);
}

/// 永远不上报的 sink,给单测和 caller 不关心进度的场景用
pub struct NoopProgressSink;

#[async_trait]
impl DownloadProgressSink for NoopProgressSink {
    async fn tick(&self, _update: ProgressUpdate) {}
}
