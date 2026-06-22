//! 下载层错误类型
//!
//! 与 ncd-component::ActionError 解耦:caller 自己决定怎么映射
//! 区分四类,UI 才能给出合适的话术("网速太慢" vs "校验失败" vs "被取消")

use thiserror::Error;

#[derive(Debug, Error)]
pub enum NetworkError {
    /// reqwest / IO 错误:网络层硬故障,调用方应当切镜像或重试
    #[error("http error: {0}")]
    Http(String),

    /// 服务端响应非 2xx / 206
    #[error("http status: {0}")]
    Status(u16),

    /// idle timeout:超过约定时间没有新字节到达;mirror race 据此判失败切下家
    #[error("idle timeout: no bytes for {0:?}")]
    IdleTimeout(std::time::Duration),

    /// 本地 IO 失败(写入 .part 文件 / mkdir 等)
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// SHA256 校验失败,caller 应当删除已下载文件并重试或报错
    #[error("checksum mismatch: expected {expected}, actual {actual}")]
    ChecksumMismatch { expected: String, actual: String },

    /// 全部镜像都跑挂了,race 没有 winner
    #[error("all mirrors failed: {0}")]
    AllMirrorsFailed(String),

    /// 用户主动取消
    #[error("cancelled")]
    Cancelled,

    /// 配置错误(URL 非法 / 切片数 0 等)
    #[error("invalid argument: {0}")]
    InvalidArgument(String),

    /// 流提前结束:服务端在 Content-Length 字节灌完前就 close
    /// 镜像连接被掐断 / 反代提前 EOF / 服务端崩了. 这是 zip "Could not
    /// find EOCD" 一类下游错误的真根因——文件根本没下完整;caller 应当切
    /// 下一个 mirror 并 truncate .part 重下,因为续传 offset 拼错位是
    /// 同样致命的(不同 mirror 缓存内容可能不一致)
    #[error("truncated: downloaded {downloaded} of {total} bytes")]
    Truncated { downloaded: u64, total: u64 },
}

impl From<reqwest::Error> for NetworkError {
    fn from(value: reqwest::Error) -> Self {
        if let Some(status) = value.status() {
            NetworkError::Status(status.as_u16())
        } else {
            NetworkError::Http(value.to_string())
        }
    }
}
