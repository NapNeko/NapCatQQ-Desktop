//! 全工程共享的 reqwest::Client
//!
//! 不设总 timeout(大文件下载可能跑很久,idle timeout 由 download 模块在 chunk
//! 循环自己计);rustls-tls 不依赖系统 OpenSSL,Tauri 包体可控;系统代理只读
//! HTTP_PROXY/HTTPS_PROXY/ALL_PROXY 环境变量,不读 Windows 注册表——国内用户
//! 设 HTTPS_PROXY=http://127.0.0.1:7890 即可走代理
//!
//! 除 WebUI 客户端(127.0.0.1)外,外网下载 / GitHub API 都走本 client 复用连接池

use std::sync::OnceLock;
use std::time::Duration;

use reqwest::Client;

use crate::error::NetworkError;

const DEFAULT_USER_AGENT: &str = concat!(
    "NapCatQQ-Desktop/",
    env!("CARGO_PKG_VERSION"),
    " (ncd-network)"
);

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const POOL_IDLE_TIMEOUT: Duration = Duration::from_secs(60);

static SHARED: OnceLock<Client> = OnceLock::new();

/// 拿全工程共享 Client,首次调用构造,之后返回同一实例
/// 构造失败(rustls root store 加载失败等)直接 panic——HTTP 栈起不来进程没法跑,
/// 比让每个 caller 处理 Result 更合理
#[allow(
    clippy::expect_used,
    reason = "进程级单例 client 构造失败不可恢复，见 shared_client doc"
)]
pub fn shared_client() -> &'static Client {
    SHARED
        .get_or_init(|| build_default_client().expect("ncd-network: 共享 reqwest::Client 构造失败"))
}

/// 仅供测试:构造一份独立 client,跳过 OnceLock 缓存
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
        // 两次调用拿到同一个 Arc 内部指针
        let a = shared_client() as *const Client;
        let b = shared_client() as *const Client;
        assert_eq!(a, b);
    }

    #[test]
    fn build_default_client_succeeds() {
        let client = build_default_client().expect("默认 client 应当构造成功");
        // builder 成功即可,不发请求
        let _ = client;
    }
}
