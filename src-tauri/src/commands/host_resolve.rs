//! host_id 字符串 → Arc<dyn Host> 的统一解析。
//!
//! 组件页和 Docker 页都按 host_id 选主机:
//! - "local"          本机 Host
//! - "remote:<id>"    远端 SSH,从 ServerManager 取已建立的连接;缓存未命中时
//!                    用 keyring 缓存凭据自动连一次,省得用户先去远端页点测试。

use std::sync::Arc;

use ncd_host::Host;

use crate::AppState;

/// 解析 host_id 到一个可用的 Host,远端缓存未命中时尝试自动连接。
///
/// 失败时返回人话错误,让前端在那行 host status 上显示原因。
pub async fn resolve_host_with_autoconnect(
    host_id: &str,
    state: &AppState,
) -> Result<Arc<dyn Host>, String> {
    if host_id == "local" {
        return local_host();
    }
    let Some(server_id) = host_id.strip_prefix("remote:") else {
        return Err(format!("unknown host_id: {host_id}"));
    };

    // 缓存命中直接用；未命中走 ServerManager 的单飞连接，避免组件页并发的
    // 多个 detect 各自发起一次 SSH 握手把远端 MaxStartups 打爆。
    state.server_manager.ensure_connected(server_id).await
}

#[cfg(windows)]
pub fn local_host() -> Result<Arc<dyn Host>, String> {
    Ok(Arc::new(ncd_host::local::LocalWindowsHost::new()))
}

#[cfg(not(windows))]
pub fn local_host() -> Result<Arc<dyn Host>, String> {
    Err("local host on non-Windows targets is not yet implemented".to_string())
}

/// 把 host_id 解析成"展示用主机地址",拼 WebUI / noVNC URL 用。
/// 本机返回 127.0.0.1;远端返回 ServerProfile.host(IP 或域名)。
pub async fn host_display_address(host_id: &str, state: &AppState) -> String {
    if let Some(server_id) = host_id.strip_prefix("remote:") {
        let profiles = state.server_manager.list_servers().await;
        if let Some(p) = profiles.into_iter().find(|p| p.id == server_id) {
            return p.host;
        }
    }
    "127.0.0.1".to_string()
}
