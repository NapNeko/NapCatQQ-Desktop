//! 端口转发 / 隧道
//!
//! `-L`：本机听口经 russh direct-tcpip 连到远端 loopback（WebUI / P0 对接）。
//! `-R`：远端听口经 tcpip-forward 连回本机 loopback（P1：远端 Bot → 本机应用）。

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

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
    pub(crate) closed: AtomicBool,
    /// 持有后台任务,Drop 时它会因 shutdown 唤醒退出
    pub(crate) _task: tokio::task::JoinHandle<()>,
}

impl TunnelHandle {
    /// 本机 TCP 转发，用于 Docker 网桥到宿主机 loopback 的连接。
    pub async fn local_forward(spec: TunnelSpec) -> Result<Self, crate::HostError> {
        if spec.direction != TunnelDirection::LocalToRemote {
            return Err(crate::HostError::Unsupported {
                operation: "local_reverse_tunnel",
            });
        }
        let listener =
            tokio::net::TcpListener::bind((spec.local_host.as_str(), spec.local_port)).await?;
        let local_port = listener.local_addr()?.port();
        let shutdown = Arc::new(Notify::new());
        let signal = Arc::clone(&shutdown);
        let task = tokio::spawn(async move {
            let mut connections = tokio::task::JoinSet::new();
            loop {
                tokio::select! {
                    biased;
                    _ = signal.notified() => break,
                    Some(_) = connections.join_next(), if !connections.is_empty() => {},
                    accepted = listener.accept() => {
                        let Ok((mut incoming, _)) = accepted else { break; };
                        let target = (spec.remote_host.clone(), spec.remote_port);
                        connections.spawn(async move {
                            match tokio::net::TcpStream::connect(target).await {
                                Ok(mut outgoing) => { let _ = tokio::io::copy_bidirectional(&mut incoming, &mut outgoing).await; }
                                Err(error) => tracing::debug!(%error, "local tunnel target unavailable"),
                            }
                        });
                    }
                }
            }
            // JoinSet 释放时中止当前连接，关闭句柄也会关闭已建立的 TCP 流。
        });
        Ok(Self {
            local_port,
            remote_listen_port: 0,
            shutdown,
            closed: AtomicBool::new(false),
            _task: task,
        })
    }

    /// 不带后台转发任务的句柄：给自己不泵数据的主机实现用（上层编排的测试替身只需要记下口）。
    /// 要在 tokio 运行时里调用
    pub fn detached(local_port: u16, remote_listen_port: u16) -> Self {
        let shutdown = Arc::new(Notify::new());
        let shutdown_for_task = Arc::clone(&shutdown);
        Self {
            local_port,
            remote_listen_port,
            shutdown,
            closed: AtomicBool::new(false),
            _task: tokio::spawn(async move {
                shutdown_for_task.notified().await;
            }),
        }
    }

    pub fn local_port(&self) -> u16 {
        self.local_port
    }

    pub fn remote_listen_port(&self) -> u16 {
        self.remote_listen_port
    }

    /// 主动关闭立即可见；后台任务退出的隧道也不应继续用于连接计划。
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Acquire) || self._task.is_finished()
    }

    /// 主动关闭隧道(也可以直接 drop)
    pub fn close(&self) {
        self.closed.store(true, Ordering::Release);
        self.shutdown.notify_waiters();
        // 任务可能尚未首次轮询，保留一个通知使稍后注册的等待者也能退出。
        self.shutdown.notify_one();
    }
}

impl Drop for TunnelHandle {
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn local_forward_relays_bytes_and_closes_active_connections() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        tokio::time::timeout(std::time::Duration::from_secs(3), async {
            let target = tokio::net::TcpListener::bind(("127.0.0.1", 0))
                .await
                .unwrap();
            let port = target.local_addr().unwrap().port();
            let server = tokio::spawn(async move {
                let (mut stream, _) = target.accept().await.unwrap();
                let mut message = [0; 4];
                stream.read_exact(&mut message).await.unwrap();
                assert_eq!(&message, b"ping");
                stream.write_all(b"pong").await.unwrap();
                assert_eq!(stream.read(&mut message).await.unwrap(), 0);
            });
            let handle = TunnelHandle::local_forward(TunnelSpec::local_to_remote(0, port))
                .await
                .unwrap();
            let mut client = tokio::net::TcpStream::connect(("127.0.0.1", handle.local_port()))
                .await
                .unwrap();
            client.write_all(b"ping").await.unwrap();
            let mut reply = [0; 4];
            client.read_exact(&mut reply).await.unwrap();
            assert_eq!(&reply, b"pong");
            handle.close();
            assert!(handle.is_closed());
            assert_eq!(client.read(&mut reply).await.unwrap(), 0);
            server.await.unwrap();
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn detached_handle_stays_live_until_closed() {
        let handle = TunnelHandle::detached(47099, 0);
        tokio::task::yield_now().await;
        assert!(!handle.is_closed());
        handle.close();
        assert!(handle.is_closed());
    }

    #[tokio::test]
    async fn dropped_handle_shuts_down_even_before_task_starts() {
        let handle = TunnelHandle::detached(47099, 0);
        let task = handle._task.abort_handle();
        drop(handle);
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while !task.is_finished() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn finished_task_marks_handle_closed() {
        let handle = TunnelHandle {
            local_port: 47099,
            remote_listen_port: 0,
            shutdown: Arc::new(Notify::new()),
            closed: AtomicBool::new(false),
            _task: tokio::spawn(async {}),
        };
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while !handle.is_closed() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }

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
