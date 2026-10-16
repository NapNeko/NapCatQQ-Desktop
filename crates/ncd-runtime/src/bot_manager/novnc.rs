use super::*;
use tokio_tungstenite::tungstenite::{Message, client::IntoClientRequest};

impl<R: BotConfigRepo + 'static, S: ConfigStore + 'static> BotManager<R, S> {
    pub async fn prepare_snowluma_novnc(
        &self,
        bot_id: &BotId,
    ) -> Result<(u16, String), BotManagerError> {
        let config = self.get_required_bot_config(bot_id).await?;
        if config.bot.backend_type != BackendType::SnowLuma {
            return Err(BotManagerError::Render("当前 Bot 不是 SnowLuma".into()));
        }
        let (port, password) = match RuntimeScenario::from_config(&config)? {
            RuntimeScenario::RemoteNative { server_id, .. } => {
                if let Some(resolver) = &self.host_resolver {
                    let host = resolver
                        .resolve(&config.bot.runtime_target)
                        .await
                        .map_err(|error| BotManagerError::RemoteRead(error.to_string()))?;
                    if host.os() == ncd_host::Os::Windows {
                        return Err(BotManagerError::Render(
                            "Windows 原生实例请使用二维码窗口；需要操作 QQ 界面时，请连接目标用户的远程桌面".into(),
                        ));
                    }
                }
                // 路由会在 SSH 重连后替换 daemon 中的旧 Host。
                self.backend_for_config(&config).await?;
                let daemon = self
                    .remote_snowluma_daemons
                    .lock()
                    .await
                    .get(&server_id)
                    .cloned()
                    .ok_or_else(|| BotManagerError::Render("远端 SnowLuma 尚未就绪".into()))?;
                let endpoint = daemon.ensure_novnc_ready().await?;
                (endpoint.novnc_local_port, endpoint.vnc_password)
            }
            RuntimeScenario::RemoteDocker { .. } => {
                if self.snowluma_docker_endpoints(bot_id).await.is_none() {
                    self.reconcile_remote_runtime_for(std::slice::from_ref(bot_id))
                        .await?;
                }
                let endpoint = self
                    .snowluma_docker_endpoints(bot_id)
                    .await
                    .ok_or_else(|| {
                        BotManagerError::Render("noVNC 隧道未就绪，请确认 Bot 已启动".into())
                    })?;
                (endpoint.novnc_local_port, endpoint.vnc_password)
            }
            _ => {
                return Err(BotManagerError::Render(
                    "仅远端 SnowLuma 支持 noVNC 扫码".into(),
                ));
            }
        };
        verify_novnc(port).await.map_err(BotManagerError::Render)?;
        Ok((port, password))
    }
}

async fn verify_novnc(port: u16) -> Result<(), String> {
    let check = async {
        let response = reqwest::Client::builder()
            .no_proxy()
            .build()
            .map_err(|e| e.to_string())?
            .get(format!("http://127.0.0.1:{port}/vnc.html"))
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !response.status().is_success() {
            return Err("noVNC 页面不可用".to_string());
        }
        let mut request = format!("ws://127.0.0.1:{port}/websockify")
            .into_client_request()
            .map_err(|e| e.to_string())?;
        request
            .headers_mut()
            .insert("Sec-WebSocket-Protocol", "binary".parse().unwrap());
        let (mut socket, _) = tokio_tungstenite::connect_async(request)
            .await
            .map_err(|e| e.to_string())?;
        let mut banner = Vec::with_capacity(12);
        while let Some(message) = socket.next().await {
            match message.map_err(|e| e.to_string())? {
                Message::Binary(bytes) => {
                    banner.extend(bytes.iter().take(12 - banner.len()));
                    if banner.len() == 12 {
                        // HTTP 页面活着不代表代理后面的 VNC 服务可用。
                        if banner.starts_with(b"RFB 003.") && banner[11] == b'\n' {
                            return Ok(());
                        }
                        return Err("VNC 握手异常".into());
                    }
                }
                Message::Ping(_) | Message::Pong(_) => {}
                _ => break,
            }
        }
        Err("VNC 服务未响应".into())
    };
    match tokio::time::timeout(Duration::from_secs(5), check).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(format!("QQ 界面连接失败：{error}。请检查远端 VNC 服务")),
        Err(_) => Err("QQ 界面连接超时，请检查主机连接与远端 VNC 服务".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::SinkExt;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    async fn server(banner: Option<&'static [u8]>) -> (u16, tokio::task::JoinHandle<()>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let task = tokio::spawn(async move {
            let (mut http, _) = listener.accept().await.unwrap();
            let mut request = [0; 2048];
            http.read(&mut request).await.unwrap();
            http.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                .await
                .unwrap();
            drop(http);
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_hdr_async(
                stream,
                |_: &tokio_tungstenite::tungstenite::handshake::server::Request,
                 mut response: tokio_tungstenite::tungstenite::handshake::server::Response| {
                    response.headers_mut().insert("Sec-WebSocket-Protocol", "binary".parse().unwrap());
                    Ok(response)
                },
            ).await.unwrap();
            if let Some(banner) = banner {
                socket
                    .send(Message::Binary(banner[..4].to_vec()))
                    .await
                    .unwrap();
                socket
                    .send(Message::Binary(banner[4..].to_vec()))
                    .await
                    .unwrap();
            } else {
                socket.close(None).await.unwrap();
            }
        });
        (port, task)
    }

    #[tokio::test]
    async fn ready_requires_vnc_banner_through_websocket() {
        let (port, task) = server(Some(b"RFB 003.008\n")).await;
        verify_novnc(port).await.unwrap();
        task.await.unwrap();
    }

    #[tokio::test]
    async fn html_without_vnc_backend_is_not_ready() {
        let (port, task) = server(None).await;
        assert!(
            verify_novnc(port)
                .await
                .unwrap_err()
                .contains("VNC 服务未响应")
        );
        task.await.unwrap();
    }

    #[tokio::test]
    async fn wrong_backend_is_not_ready() {
        let (port, task) = server(Some(b"HTTP/1.1 404")).await;
        assert!(
            verify_novnc(port)
                .await
                .unwrap_err()
                .contains("VNC 握手异常")
        );
        task.await.unwrap();
    }
}
