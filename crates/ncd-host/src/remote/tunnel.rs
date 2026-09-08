//! 端口转发 / 隧道
//!
//! `-L`：本机听口经 russh direct-tcpip 连到远端 loopback（WebUI / P0 对接）。
//! `-R`：远端听口经 tcpip-forward 连回本机 loopback（P1：远端 Bot → 本机应用）。

use std::sync::Arc;

use tokio::sync::Notify;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TunnelDirection {
    #[default]
    LocalToRemote,
    RemoteToLocal,
}

/// 隧道规格(描述一条要建立的隧道)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TunnelSpec {
    /// 本地绑定 / 目标地址(默认 127.0.0.1)
    pub local_host: String,
    /// `-L`：本机听口（0 = OS 分配）；`-R`：本机目标口
    pub local_port: u16,
    /// 远端绑定 / 目标 host（从 SSH 服务器视角看）
    pub remote_host: String,
    /// `-L`：远端目标口；`-R`：远端听口（0 = sshd 分配）
    pub remote_port: u16,
    pub direction: TunnelDirection,
}

impl TunnelSpec {
    /// 把远端 127.0.0.1:remote_port 暴露为本地 127.0.0.1:local_port
    pub fn local_to_remote(local_port: u16, remote_port: u16) -> Self {
        Self {
            local_host: "127.0.0.1".to_string(),
            local_port,
            remote_host: "127.0.0.1".to_string(),
            remote_port,
            direction: TunnelDirection::LocalToRemote,
        }
    }

    /// 把本机 127.0.0.1:local_port 暴露为远端 127.0.0.1:remote_listen
    pub fn remote_to_local(remote_listen: u16, local_port: u16) -> Self {
        Self {
            local_host: "127.0.0.1".to_string(),
            local_port,
            remote_host: "127.0.0.1".to_string(),
            remote_port: remote_listen,
            direction: TunnelDirection::RemoteToLocal,
        }
    }
}

/// 隧道句柄Drop 时关闭隧道
pub struct TunnelHandle {
    /// `-L` 实际绑定的本地端口(若 spec.local_port==0,这里是 OS 分配的真实端口)
    pub local_port: u16,
    /// `-R` 远端听口（Bot URL 用）；`-L` 为 0
    pub remote_listen_port: u16,
    /// 给 acceptor / 取消任务发关闭通知
    pub(crate) shutdown: Arc<Notify>,
    /// 持有后台任务,Drop 时它会因 shutdown 唤醒退出
    pub(crate) _task: tokio::task::JoinHandle<()>,
}

impl TunnelHandle {
    pub fn local_port(&self) -> u16 {
        self.local_port
    }

    pub fn remote_listen_port(&self) -> u16 {
        self.remote_listen_port
    }

    /// 主动关闭隧道(也可以直接 drop)
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
        assert_eq!(spec.direction, TunnelDirection::LocalToRemote);
    }

    #[test]
    fn remote_to_local_uses_loopback() {
        let spec = TunnelSpec::remote_to_local(0, 7777);
        assert_eq!(spec.local_host, "127.0.0.1");
        assert_eq!(spec.remote_host, "127.0.0.1");
        assert_eq!(spec.local_port, 7777);
        assert_eq!(spec.remote_port, 0);
        assert_eq!(spec.direction, TunnelDirection::RemoteToLocal);
    }
}
