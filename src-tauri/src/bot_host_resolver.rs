//! TauriHostResolver:把 BotConfig 的 RuntimeTarget 解析成 Arc<dyn Host>,
//! 供 BotManager 在启动 bot 时按"在哪台机器跑"取 host
//!
//! Local -> 本机 LocalWindowsHost;Server(id) -> ServerManager 已连接/现连的
//! RemoteLinuxHost(单飞 + keyring 凭据自动连)与组件页/Docker 页的
//! host_resolve::resolve_host_with_autoconnect 同源,只是入口是 RuntimeTarget
//! 而非 host_id 字符串

use std::sync::Arc;

use async_trait::async_trait;
use ncd_domain::RuntimeTarget;
use ncd_host::Host;
use ncd_runtime::{HostResolveError, HostResolver, ServerManager};

pub struct TauriHostResolver {
    server_manager: Arc<ServerManager>,
    local: Arc<dyn Host>,
}

impl TauriHostResolver {
    pub fn new(server_manager: Arc<ServerManager>, local: Arc<dyn Host>) -> Self {
        Self {
            server_manager,
            local,
        }
    }
}

#[async_trait]
impl HostResolver for TauriHostResolver {
    async fn resolve(&self, target: &RuntimeTarget) -> Result<Arc<dyn Host>, HostResolveError> {
        match target {
            RuntimeTarget::Local => Ok(Arc::clone(&self.local)),
            RuntimeTarget::Server(id) => self
                .server_manager
                .ensure_connected(id)
                .await
                .map_err(HostResolveError::from),
        }
    }

    async fn refresh(&self, target: &RuntimeTarget) -> Result<Arc<dyn Host>, HostResolveError> {
        match target {
            RuntimeTarget::Local => Ok(Arc::clone(&self.local)),
            RuntimeTarget::Server(id) => self
                .server_manager
                .refresh_host(id)
                .await
                .map_err(HostResolveError::from),
        }
    }
}
