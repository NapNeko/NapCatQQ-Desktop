//! 远端 SnowLuma Native:SSH 本地转发 WebUI / noVNC
//!
//! WebUI 远端端口由调用方按 runtime.json / 日志 / node 监听口解析后传入，
//! 不写死 5099。noVNC 仍默认 6081（图形栈由 Desktop 拉起时固定）。
//! 对齐 legacy SnowLumaTunnelManager:多 Bot 同 server_id 共享隧道,引用计数归零后关闭

use std::collections::HashMap;

use ncd_host::remote::{TunnelHandle, TunnelSpec};
use ncd_host::{Host, HostError};
use tokio::sync::Mutex;

pub const REMOTE_WEBUI_PORT: u16 = 5099;
pub const REMOTE_NOVNC_PORT: u16 = 6081;
pub const PREFERRED_WEBUI_LOCAL_PORT: u16 = 47099;
pub const PREFERRED_NOVNC_LOCAL_PORT: u16 = 47609;

#[derive(Debug, Clone)]
pub struct RemoteSnowLumaTunnelEndpoints {
    pub webui_local_port: u16,
    pub novnc_local_port: u16,
    pub webui_password: String,
    pub vnc_password: String,
}

struct TunnelBundle {
    webui: TunnelHandle,
    novnc: TunnelHandle,
    webui_password: String,
    vnc_password: String,
    remote_webui_port: u16,
    refcount: u32,
}

pub struct RemoteSnowLumaTunnelRegistry {
    by_server: Mutex<HashMap<String, TunnelBundle>>,
}

impl Default for RemoteSnowLumaTunnelRegistry {
    fn default() -> Self {
        Self {
            by_server: Mutex::new(HashMap::new()),
        }
    }
}

impl RemoteSnowLumaTunnelRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn endpoints_for_server(
        &self,
        server_id: &str,
    ) -> Option<RemoteSnowLumaTunnelEndpoints> {
        let guard = self.by_server.lock().await;
        guard.get(server_id).map(|b| RemoteSnowLumaTunnelEndpoints {
            webui_local_port: b.webui.local_port(),
            novnc_local_port: b.novnc.local_port(),
            webui_password: b.webui_password.clone(),
            vnc_password: b.vnc_password.clone(),
        })
    }

    /// 隧道 +1;首次建立双隧道密码由调用方在 daemon 就绪后从远端 secret 读出传入。
    /// `remote_webui_port` 必须是探测到的实际口，不能假定 5099。
    pub async fn acquire(
        &self,
        server_id: &str,
        host: &dyn Host,
        webui_password: String,
        vnc_password: String,
        remote_webui_port: u16,
    ) -> Result<RemoteSnowLumaTunnelEndpoints, HostError> {
        let mut guard = self.by_server.lock().await;
        if let Some(bundle) = guard.get_mut(server_id) {
            if remote_webui_port != 0 && bundle.remote_webui_port != remote_webui_port {
                tracing::warn!(
                    target: "ncd_runtime::remote_snowluma",
                    server_id,
                    existing = bundle.remote_webui_port,
                    requested = remote_webui_port,
                    "SnowLuma WebUI 隧道已按先前探测端口建立，忽略本次端口"
                );
            }
            bundle.refcount = bundle.refcount.saturating_add(1);
            return Ok(RemoteSnowLumaTunnelEndpoints {
                webui_local_port: bundle.webui.local_port(),
                novnc_local_port: bundle.novnc.local_port(),
                webui_password: bundle.webui_password.clone(),
                vnc_password: bundle.vnc_password.clone(),
            });
        }

        let remote_webui = if remote_webui_port == 0 {
            REMOTE_WEBUI_PORT
        } else {
            remote_webui_port
        };
        let webui = open_tunnel_preferred(host, PREFERRED_WEBUI_LOCAL_PORT, remote_webui).await?;
        let novnc =
            open_tunnel_preferred(host, PREFERRED_NOVNC_LOCAL_PORT, REMOTE_NOVNC_PORT).await?;

        let eps = RemoteSnowLumaTunnelEndpoints {
            webui_local_port: webui.local_port(),
            novnc_local_port: novnc.local_port(),
            webui_password: webui_password.clone(),
            vnc_password: vnc_password.clone(),
        };
        guard.insert(
            server_id.to_string(),
            TunnelBundle {
                webui,
                novnc,
                webui_password,
                vnc_password,
                remote_webui_port: remote_webui,
                refcount: 1,
            },
        );
        Ok(eps)
    }

    pub async fn release(&self, server_id: &str) {
        let mut guard = self.by_server.lock().await;
        let Some(bundle) = guard.get_mut(server_id) else {
            return;
        };
        if bundle.refcount == 0 {
            return;
        }
        bundle.refcount -= 1;
        if bundle.refcount == 0 {
            guard.remove(server_id);
        }
    }

    pub async fn shutdown_all(&self) {
        self.by_server.lock().await.clear();
    }
}

async fn open_tunnel_preferred(
    host: &dyn Host,
    preferred_local: u16,
    remote_port: u16,
) -> Result<TunnelHandle, HostError> {
    let spec_fixed = TunnelSpec::local_to_remote(preferred_local, remote_port);
    if let Ok(h) = host.open_tunnel(spec_fixed).await {
        return Ok(h);
    }
    let spec_ephemeral = TunnelSpec {
        local_host: "127.0.0.1".to_string(),
        local_port: 0,
        remote_host: "127.0.0.1".to_string(),
        remote_port,
    };
    host.open_tunnel(spec_ephemeral).await
}
