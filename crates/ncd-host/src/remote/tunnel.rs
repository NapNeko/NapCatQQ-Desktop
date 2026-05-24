//! 端口转发 / 隧道。
//!
//! 蓝图 §6 中档实装:把远端 host:port 暴露为本地 127.0.0.1:local_port,
//! 用于 SnowLuma 远端 WebUI / VNC 访问。
//!
//! 实装在 `linux.rs` 中走 russh 的 direct-tcpip channel + `tokio::io::copy_bidirectional`
//! 双向泵。本节只放数据类型。

use std::sync::Arc;

use tokio::sync::Notify;

/// 隧道规格(描述一条要建立的隧道)。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TunnelSpec {
    /// 本地绑定地址(默认 127.0.0.1)
    pub local_host: String,
    /// 本地端口(0 表示让 OS 分配)
    pub local_port: u16,
    /// 远端目标 host(从 SSH 服务器视角看)
    pub remote_host: String,
    /// 远端目标端口
    pub remote_port: u16,
}

impl TunnelSpec {
    /// 把远端 127.0.0.1:remote_port 暴露为本地 127.0.0.1:local_port。
    pub fn local_to_remote(local_port: u16, remote_port: u16) -> Self {
        Self {
            local_host: "127.0.0.1".to_string(),
            local_port,
            remote_host: "127.0.0.1".to_string(),
            remote_port,
        }
    }
}

/// 隧道句柄。Drop 时关闭隧道。
pub struct TunnelHandle {
    /// 实际绑定的本地端口(若 spec.local_port==0,这里是 OS 分配的真实端口)
    pub local_port: u16,
    /// 给 acceptor 任务发关闭通知
    pub(crate) shutdown: Arc<Notify>,
    /// 持有 acceptor 任务句柄,Drop 时它会因 shutdown 唤醒退出
    pub(crate) _task: tokio::task::JoinHandle<()>,
}

impl TunnelHandle {
    pub fn local_port(&self) -> u16 {
        self.local_port
    }

    /// 主动关闭隧道(也可以直接 drop)。
    pub fn close(&self) {
        self.shutdown.notify_waiters();
    }
}

impl Drop for TunnelHandle {
    fn drop(&mut self) {
        self.shutdown.notify_waiters();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_to_remote_uses_loopback() {
        let spec = TunnelSpec::local_to_remote(47099, 5099);
        assert_eq!(spec.local_host, "127.0.0.1");
        assert_eq!(spec.remote_host, "127.0.0.1");
        assert_eq!(spec.local_port, 47099);
        assert_eq!(spec.remote_port, 5099);
    }
}
