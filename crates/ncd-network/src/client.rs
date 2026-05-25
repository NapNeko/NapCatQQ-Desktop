//! 全工程共享的 [`reqwest::Client`]。
//!
//! 默认配置：
//! - `connect_timeout(10s)`：DNS + TLS 握手不超过 10 秒；超过认为网络层不可达
//! - 不设总 `timeout`：大文件下载本来就可能跑 30 分钟，总超时与 idle timeout
//!   是两件事；idle timeout 由 [`crate::download`] 在 chunk 循环里自己计
//! - `pool_idle_timeout(60s)`：连接池保活 1 分钟，多镜像 race 后续请求免握手
//! - `gzip(true)`：API 端点（GitHub releases）默认压缩
//! - `rustls-tls`：不依赖系统 OpenSSL，Tauri 包体可控
//!
//! 调用方：除 WebUI 客户端（127.0.0.1，不复用此 client）外，所有外网下载 /
//! GitHub API 调用都应走本 client，避免每次构造新 client 浪费连接池。

use std::sync::OnceLock;
use std::time::Duration;

use reqwest::Client;

use crate::error::NetworkError;

const DEFAULT_USER_AGENT: &str =
    concat!("NapCatQQ-Desktop/", env!("CARGO_PKG_VERSION"), " (ncd-network)");

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const POOL_IDLE_TIMEOUT: Duration = Duration::from_secs(60);

static SHARED: OnceLock<Client> = OnceLock::new();

/// 拿到全工程共享的 [`Client`]。第一次调用时构造，失败 panic（极少发生，
/// 通常是 rustls root store 加载失败），之后所有调用返回同一实例。
///
/// # Panics
/// 仅在 `reqwest::Client::builder().build()` 失败时 panic。这种失败说明
/// 进程根本起不来 HTTP 栈，让进程立即崩溃比让每个 caller 处理 Result 更
/// 合理。
pub fn shared_client() -> &'static Client {
    SHARED.get_or_init(|| {
        build_default_client().expect("ncd-network: 共享 reqwest::Client 构造失败")
    })
}

/// 仅供测试：构造一份独立 client，跳过 OnceLock 缓存。
pub fn build_default_client() -> Result<Client, NetworkError> {
    Client::builder()
        .user_agent(DEFAULT_USER_AGENT)
        .connect_timeout(CONNECT_TIMEOUT)
        .pool_idle_timeout(POOL_IDLE_TIMEOUT)
        .gzip(true)
        .build()
        .map_err(NetworkError::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_returns_same_instance() {
        // 两次调用拿到同一个 Arc 内部指针。
        let a = shared_client() as *const Client;
        let b = shared_client() as *const Client;
        assert_eq!(a, b);
    }

    #[test]
    fn build_default_client_succeeds() {
        let client = build_default_client().expect("默认 client 应当构造成功");
        // builder 成功即可，不发请求。
        let _ = client;
    }
}
