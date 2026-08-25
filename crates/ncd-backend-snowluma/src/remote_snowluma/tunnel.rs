//! 远端 SnowLuma Native:SSH 本地转发 WebUI / noVNC
//!
//! WebUI / noVNC 远端端口均由调用方探测实际监听后传入，
//! 不写死 5099 / 6081。对齐 legacy SnowLumaTunnelManager:
//! 多 Bot 同 server_id 共享隧道,引用计数归零后关闭

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
    remote_novnc_port: u16,
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
    /// `remote_webui_port` / `remote_novnc_port` 必须是探测到的实际口，不能假定默认值。
    pub async fn acquire(
        &self,
        server_id: &str,
        host: &dyn Host,
        webui_password: String,
        vnc_password: String,
        remote_webui_port: u16,
        remote_novnc_port: u16,
    ) -> Result<RemoteSnowLumaTunnelEndpoints, HostError> {
        let mut guard = self.by_server.lock().await;
        if let Some(bundle) = guard.get(server_id) {
            if tunnel_remote_ports_changed(
                bundle.remote_webui_port,
                remote_webui_port,
                bundle.remote_novnc_port,
                remote_novnc_port,
            ) {
                tracing::info!(
                    target: "ncd_runtime::remote_snowluma",
                    server_id,
                    old_webui = bundle.remote_webui_port,
                    new_webui = remote_webui_port,
                    "SnowLuma 远端端口已变，重建 SSH 隧道"
                );
                guard.remove(server_id);
            }
        }
        if let Some(bundle) = guard.get_mut(server_id) {
            // 复用隧道句柄，但密码以本次从 secret 读到的为准（接管会换密）。
            bundle.webui_password = adopt_incoming_secret(&bundle.webui_password, &webui_password);
            bundle.vnc_password = adopt_incoming_secret(&bundle.vnc_password, &vnc_password);
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
        let remote_novnc = if remote_novnc_port == 0 {
            REMOTE_NOVNC_PORT
        } else {
            remote_novnc_port
        };
        let webui = open_tunnel_preferred(host, PREFERRED_WEBUI_LOCAL_PORT, remote_webui).await?;
        let novnc = open_tunnel_preferred(host, PREFERRED_NOVNC_LOCAL_PORT, remote_novnc).await?;

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
                remote_novnc_port: remote_novnc,
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

/// 空串表示这次没读到 secret，保留缓存；非空则覆盖（接管换密）。
fn adopt_incoming_secret(cached: &str, incoming: &str) -> String {
    if incoming.is_empty() {
        cached.to_string()
    } else {
        incoming.to_string()
    }
}

fn tunnel_remote_ports_changed(
    existing_webui: u16,
    requested_webui: u16,
    existing_novnc: u16,
    requested_novnc: u16,
) -> bool {
    (requested_webui != 0 && existing_webui != requested_webui)
        || (requested_novnc != 0 && existing_novnc != requested_novnc)
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

#[cfg(test)]
mod tests {
    use super::adopt_incoming_secret;

    #[test]
    fn adopt_incoming_secret_keeps_cache_when_empty() {
        assert_eq!(adopt_incoming_secret("cached", ""), "cached");
        assert_eq!(adopt_incoming_secret("cached", "fresh"), "fresh");
        assert_eq!(adopt_incoming_secret("", "fresh"), "fresh");
    }

    #[test]
    fn rebuild_tunnel_when_webui_port_changes() {
        assert!(super::tunnel_remote_ports_changed(5099, 13105, 6081, 6081));
        assert!(!super::tunnel_remote_ports_changed(
            13105, 13105, 6081, 6081
        ));
        assert!(!super::tunnel_remote_ports_changed(13105, 0, 6081, 0));
        assert!(super::tunnel_remote_ports_changed(13105, 13105, 6081, 6082));
    }
}
