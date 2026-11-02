//! 转发使用独立 SSH transport，监听器持有并回收所有活动连接。

use super::*;
use tokio::task::JoinSet;

const CHANNEL_TIMEOUT: Duration = Duration::from_secs(5);
const OPEN_TIMEOUT: Duration = Duration::from_secs(15);
const CLOSE_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_CONNECTIONS: usize = 64;

impl RemoteLinuxHost {
    pub async fn open_tunnel(&self, spec: TunnelSpec) -> Result<TunnelHandle, HostError> {
        tokio::time::timeout(OPEN_TIMEOUT, async {
            let transport = self.forwarding_transport().await?;
            match spec.direction {
                TunnelDirection::LocalToRemote => transport.open_local_to_remote(spec).await,
                TunnelDirection::RemoteToLocal => {
                    // 请求已送到 sshd 后，由任务确认结果或关闭连接，调用者取消不能留下远端听口。
                    tokio::spawn(async move { transport.open_remote_to_local(spec).await })
                        .await
                        .map_err(|error| HostError::Io(std::io::Error::other(error)))?
                }
            }
        })
        .await
        .map_err(|_| HostError::Timeout {
            operation: "tunnel_open",
        })?
    }

    async fn forwarding_transport(&self) -> Result<Arc<Self>, HostError> {
        if self.closed.is_cancelled() {
            return Err(HostError::remote_disconnected("connection retired"));
        }
        let mut transport = self.forward_transport.lock().await;
        if let Some(host) = transport.as_ref() {
            if host.is_healthy().await {
                return Ok(Arc::clone(host));
            }
        }
        let config = self
            .config
            .clone()
            .with_connect_timeout(Duration::from_secs(10))
            .with_keepalive(Some(Duration::from_secs(10)));
        let mut host = tokio::time::timeout(
            Duration::from_secs(10),
            Self::connect_transport(self.id.clone(), config),
        )
        .await
        .map_err(|_| HostError::Timeout {
            operation: "tunnel_connect",
        })??;
        host.closed = self.closed.child_token();
        host.transport_changes = self.transport_changes.clone();
        if host.closed.is_cancelled() {
            host.invalidate_main_handle().await;
            return Err(HostError::Cancelled);
        }
        let host = Arc::new(host);
        *transport = Some(Arc::clone(&host));
        Ok(host)
    }

    async fn open_local_to_remote(
        self: Arc<Self>,
        spec: TunnelSpec,
    ) -> Result<TunnelHandle, HostError> {
        let listener = TcpListener::bind((spec.local_host.as_str(), spec.local_port)).await?;
        let local_port = listener.local_addr()?.port();
        let shutdown = Arc::new(Notify::new());
        let signal = Arc::clone(&shutdown);
        let transport_closed = Some(self.closed.clone());
        let task = tokio::spawn(async move {
            let mut connections = JoinSet::new();
            let connection_stop = self.closed.child_token();
            let mut health = tokio::time::interval(Duration::from_secs(1));
            loop {
                tokio::select! {
                    biased;
                    _ = signal.notified() => break,
                    _ = self.closed.cancelled() => break,
                    _ = health.tick() => { if !self.is_healthy().await { self.retire_forwarding().await; break; } },
                    Some(_) = connections.join_next(), if !connections.is_empty() => {},
                    incoming = listener.accept(), if connections.len() < MAX_CONNECTIONS => {
                        let Ok((incoming, peer)) = incoming else { break; };
                        let host = Arc::clone(&self);
                        let target = spec.remote_host.clone();
                        let port = spec.remote_port;
                        let stop = connection_stop.clone();
                        connections.spawn(async move {
                            let opening = tokio::time::timeout(CHANNEL_TIMEOUT, async {
                                let guard = host.handle.read().await;
                                let session = guard.as_ref().ok_or_else(|| HostError::remote_disconnected("connection retired"))?;
                                session.channel_open_direct_tcpip(target, u32::from(port), "127.0.0.1", u32::from(peer.port()))
                                    .await.map_err(|error| HostError::Io(std::io::Error::other(error)))
                            });
                            let opened = tokio::select! {
                                // russh 确认前没有 channel id，丢弃 Future 无法关闭迟到的通道。
                                _ = stop.cancelled() => { host.retire_forwarding().await; return; },
                                opened = opening => opened,
                            };
                            match opened {
                                Ok(Ok(channel)) => {
                                    pump_channel(incoming, channel, stop).await;
                                }
                                Ok(Err(error)) => tracing::debug!(%error, "SSH 转发目标暂不可用"),
                                Err(_) => {
                                    tracing::debug!("SSH 转发通道建立超时");
                                    host.retire_forwarding().await;
                                },
                            }
                        });
                    }
                }
            }
            self.close_connections(&mut connections, &connection_stop)
                .await;
        });
        Ok(TunnelHandle {
            local_port,
            remote_listen_port: 0,
            shutdown,
            closed: Default::default(),
            _task: task,
            transport_closed,
        })
    }

    async fn open_remote_to_local(
        self: Arc<Self>,
        spec: TunnelSpec,
    ) -> Result<TunnelHandle, HostError> {
        if spec.local_port == 0 {
            return Err(HostError::InvalidArgument {
                reason: "remote-forward 本机目标端口无效".into(),
            });
        }
        let opening = tokio::time::timeout(CHANNEL_TIMEOUT, async {
            let mut guard = self.handle.write().await;
            let session = guard
                .as_mut()
                .ok_or_else(|| HostError::remote_disconnected("connection retired"))?;
            session
                .tcpip_forward(spec.remote_host.clone(), u32::from(spec.remote_port))
                .await
                .map_err(|error| HostError::Io(std::io::Error::other(error)))
        })
        .await;
        let replied = match opening {
            Ok(result) => result?,
            Err(_) => {
                self.retire_forwarding().await;
                return Err(HostError::Timeout {
                    operation: "reverse_tunnel_open",
                });
            }
        };
        let remote_port = if spec.remote_port == 0 {
            replied as u16
        } else {
            spec.remote_port
        };
        if remote_port == 0 {
            return Err(HostError::remote_disconnected("sshd 未分配远端转发端口"));
        }
        let (tx, mut rx) = mpsc::channel::<Channel<Msg>>(MAX_CONNECTIONS);
        let key = (spec.remote_host.clone(), remote_port);
        self.remote_forwards.lock().await.insert(key.clone(), tx);
        let shutdown = Arc::new(Notify::new());
        let signal = Arc::clone(&shutdown);
        let local_port = spec.local_port;
        let transport_closed = Some(self.closed.clone());
        let task = tokio::spawn(async move {
            let mut connections = JoinSet::new();
            let connection_stop = self.closed.child_token();
            let mut health = tokio::time::interval(Duration::from_secs(1));
            loop {
                tokio::select! {
                    biased;
                    _ = signal.notified() => break,
                    _ = self.closed.cancelled() => break,
                    _ = health.tick() => { if !self.is_healthy().await { self.retire_forwarding().await; break; } },
                    Some(_) = connections.join_next(), if !connections.is_empty() => {},
                    channel = rx.recv(), if connections.len() < MAX_CONNECTIONS => {
                        let Some(channel) = channel else { break; };
                        let target = (spec.local_host.clone(), local_port);
                        let stop = connection_stop.clone();
                        connections.spawn(async move {
                            let connected = tokio::select! {
                                _ = stop.cancelled() => None,
                                result = tokio::time::timeout(CHANNEL_TIMEOUT, TcpStream::connect(target)) => result.ok().and_then(Result::ok),
                            };
                            if let Some(local) = connected {
                                pump_channel(local, channel, stop).await;
                            } else {
                                let _ = tokio::time::timeout(CLOSE_TIMEOUT, channel.close()).await;
                            }
                        });
                    }
                }
            }
            self.remote_forwards.lock().await.remove(&key);
            rx.close();
            while let Ok(channel) = rx.try_recv() {
                connections.spawn(async move {
                    let _ = tokio::time::timeout(CLOSE_TIMEOUT, channel.close()).await;
                });
            }
            self.close_connections(&mut connections, &connection_stop)
                .await;
            let cancelled = tokio::time::timeout(CLOSE_TIMEOUT, async {
                let guard = self.handle.read().await;
                if let Some(session) = guard.as_ref() {
                    session
                        .cancel_tcpip_forward(spec.remote_host, u32::from(remote_port))
                        .await
                } else {
                    Ok(())
                }
            })
            .await;
            if !matches!(cancelled, Ok(Ok(()))) {
                self.retire_forwarding().await;
            }
        });
        Ok(TunnelHandle {
            local_port,
            remote_listen_port: remote_port,
            shutdown,
            closed: Default::default(),
            _task: task,
            transport_closed,
        })
    }

    async fn close_connections(&self, connections: &mut JoinSet<()>, stop: &CancellationToken) {
        stop.cancel();
        if tokio::time::timeout(Duration::from_secs(3), async {
            while connections.join_next().await.is_some() {}
        })
        .await
        .is_err()
        {
            connections.shutdown().await;
            self.retire_forwarding().await;
        }
    }

    async fn retire_forwarding(&self) {
        self.closed.cancel();
        if !self
            .failure_reported
            .swap(true, std::sync::atomic::Ordering::AcqRel)
        {
            self.transport_changes
                .send_modify(|revision| *revision += 1);
        }
        let _ = tokio::time::timeout(CLOSE_TIMEOUT, self.invalidate_main_handle()).await;
    }
}

async fn pump_channel(mut local: TcpStream, mut channel: Channel<Msg>, stop: CancellationToken) {
    {
        let mut writer = channel.make_writer();
        let mut reader = channel.make_reader();
        let (mut local_read, mut local_write) = local.split();
        tokio::select! {
            _ = stop.cancelled() => {},
            _ = async {
                tokio::try_join!(
                    async { tokio::io::copy(&mut local_read, &mut writer).await?; writer.shutdown().await },
                    async { tokio::io::copy(&mut reader, &mut local_write).await?; local_write.shutdown().await },
                )
            } => {},
        }
    }
    let _ = tokio::time::timeout(CLOSE_TIMEOUT, channel.close()).await;
}
