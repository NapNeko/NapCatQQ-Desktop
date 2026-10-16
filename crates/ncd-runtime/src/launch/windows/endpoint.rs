//! 远端实际 WebUI 端点经 SSH 转发为控制端回环端点。
use ncd_domain::windows_remote::{WindowsAgentAction, WindowsAgentResult};
use ncd_domain::{BackendType, DomainEvent};
use ncd_host::remote::{TunnelHandle, TunnelSpec};
use ncd_traits::EventBus;
use ncd_traits::runtime_backend::BotBackendError;

use super::RemoteWindowsBotBackend;

pub(super) struct Endpoint {
    local_port: u16,
    remote_port: u16,
    secret: String,
    _tunnel: TunnelHandle,
    host: std::sync::Arc<dyn ncd_host::Host>,
}

impl RemoteWindowsBotBackend {
    pub(super) async fn ensure_endpoint(&self) -> Result<(u16, String, u16), BotBackendError> {
        self.check_open()?;
        let host = self.host().await?;
        let endpoint = match self
            .request_on(
                host.as_ref(),
                WindowsAgentAction::BotEndpoint {
                    bot_id: self.id.to_string(),
                },
            )
            .await?
        {
            WindowsAgentResult::BotEndpoint(endpoint) if endpoint.host_port > 0 => endpoint,
            _ => return Err(BotBackendError::Io("Windows Bot WebUI 尚未就绪".into())),
        };
        let mut current = self.endpoint.lock().await;
        self.check_open()?;
        if let Some(current) = current.as_ref() {
            if std::sync::Arc::ptr_eq(&current.host, &host)
                && current.remote_port == endpoint.host_port
                && current.secret == endpoint.secret
            {
                return Ok((
                    current.local_port,
                    current.secret.clone(),
                    current.remote_port,
                ));
            }
        }
        let tunnel = tokio::select! {
            biased;
            _ = self.cancel.cancelled() => return Err(Self::closed_error()),
            tunnel = host.open_tunnel(TunnelSpec::local_to_remote(0, endpoint.host_port)) => tunnel
                .map_err(|error| BotBackendError::RemoteHostTransport(error.to_string()))?,
        };
        self.check_open()?;
        let local_port = tunnel.local_port();
        let value = (local_port, endpoint.secret.clone(), endpoint.host_port);
        *current = Some(Endpoint {
            local_port,
            remote_port: endpoint.host_port,
            secret: endpoint.secret.clone(),
            _tunnel: tunnel,
            host,
        });
        if endpoint.backend == BackendType::NapCat {
            self.bus.publish(DomainEvent::NapCatWebuiAvailable {
                bot_id: self.id.clone(),
                port: local_port,
                token: endpoint.secret,
                host_port: Some(endpoint.host_port),
            });
        }
        Ok(value)
    }
}
