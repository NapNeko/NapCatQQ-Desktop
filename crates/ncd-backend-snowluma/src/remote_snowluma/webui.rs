//! 只恢复运行中 SnowLuma 的 HTTP 入口，不改变 QQ、node 或图形会话。

use super::*;
use ncd_host::remote::{TunnelHandle, TunnelSpec};
use ncd_host::{HostError, HostPath};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DiskCredentials {
    password_hash: String,
    password_salt: String,
    #[serde(default)]
    must_change_password: serde_json::Value,
}

pub(super) async fn read_credential_file(
    host: &dyn Host,
    path: &str,
    label: &str,
) -> Result<Option<Vec<u8>>, BotBackendError> {
    // 首次读取还包含最多十秒的 SFTP 初始化，文件预算不能比初始化预算更短。
    let output = tokio::time::timeout(
        Duration::from_secs(20),
        host.read_file(&HostPath::from_posix(path)),
    )
    .await
    .map_err(|_| BotBackendError::Io(format!("读取当前{label}超时，请重试")))?;
    match output {
        Ok(bytes) if bytes.len() <= 64 * 1024 => Ok(Some(bytes.to_vec())),
        Ok(_) => Err(BotBackendError::Io(format!("当前{label}文件过大"))),
        Err(HostError::PathNotFound { .. }) => Ok(None),
        Err(error) => Err(BotBackendError::Io(format!("读取当前{label}失败：{error}"))),
    }
}

pub(super) struct WebuiTunnel {
    pub(super) handle: TunnelHandle,
    pub(super) remote_port: u16,
    pub(super) remote_host: String,
    pub(super) password: String,
}

impl RemoteSnowLumaDaemon {
    pub async fn prepare_webui(&self) -> Result<(u16, String), BotBackendError> {
        self.in_current_session(async {
            let _stack_guard = self.stack_bootstrap.lock().await;
            let host = self.current_host().await;
            let remote_port = self.resolve_webui_port(host.as_ref()).await?;
            let remote_host = self
                .webui_endpoint
                .lock()
                .await
                .as_ref()
                .map(|endpoint| endpoint.host.clone())
                .ok_or_else(|| BotBackendError::Io("WebUI 监听地址尚未确认".into()))?;
            let (password, _) = self.read_current_webui_credentials(host.as_ref()).await?;
            let mut cached = self.webui_tunnel.lock().await;
            if let Some(tunnel) = cached.as_mut() {
                if tunnel.remote_port == remote_port
                    && tunnel.remote_host == remote_host
                    && verify_webui_http(tunnel.handle.local_port()).await.is_ok()
                {
                    tunnel.password = password.clone();
                    return Ok((tunnel.handle.local_port(), password));
                }
            }
            // This handle belongs to the daemon, independently of the VNC bundle's leases.
            *cached = None;
            let mut spec = TunnelSpec::local_to_remote(0, remote_port);
            spec.remote_host = remote_host.clone();
            let handle = host
                .open_tunnel(spec)
                .await
                .map_err(|error| BotBackendError::Io(format!("SnowLuma WebUI 隧道: {error}")))?;
            verify_webui_http(handle.local_port()).await?;
            let local_port = handle.local_port();
            *cached = Some(WebuiTunnel {
                handle,
                remote_port,
                remote_host,
                password: password.clone(),
            });
            Ok((local_port, password))
        })
        .await
    }

    pub(super) async fn read_current_webui_credentials(
        &self,
        host: &dyn Host,
    ) -> Result<(String, Vec<String>), BotBackendError> {
        let paths = &self.layout.paths;
        let fallback_path = paths
            .snowluma_dir
            .rsplit_once('/')
            .filter(|(parent, _)| !parent.is_empty())
            .map(|(parent, _)| format!("{parent}/webui.secret"))
            .filter(|path| path != &paths.webui_secret);
        let config_path = format!("{}/webui.json", paths.config_dir);
        // Independent tiny files share one SFTP session; an exists round-trip adds no proof.
        let (primary, fallback, config) = tokio::try_join!(
            read_credential_file(host, &paths.webui_secret, "WebUI 凭据"),
            async {
                match &fallback_path {
                    Some(path) => read_credential_file(host, path, "WebUI 备用凭据").await,
                    None => Ok(None),
                }
            },
            read_credential_file(host, &config_path, "WebUI 密码配置"),
        )?;
        let secret_paths = fallback_path
            .filter(|_| fallback.is_some())
            .into_iter()
            .collect();
        let candidates = [primary, fallback]
            .into_iter()
            .flatten()
            .map(|bytes| String::from_utf8_lossy(&bytes).trim().to_owned())
            .collect::<Vec<_>>();
        let disk = config
            .as_deref()
            .and_then(|bytes| serde_json::from_slice::<DiskCredentials>(bytes).ok())
            .filter(|disk| {
                !disk.password_hash.trim().is_empty() && !disk.password_salt.trim().is_empty()
            });
        // Keep scrypt strength and current-file validation without blocking the async executor.
        let password = tokio::task::spawn_blocking(move || {
            candidates
                .into_iter()
                .find(|candidate| {
                    !candidate.is_empty()
                        && disk.as_ref().is_none_or(|disk| {
                            !disk.must_change_password.as_bool().unwrap_or(false)
                                && crate::snowluma::session::verify_webui_password(
                                    candidate,
                                    &disk.password_hash,
                                    &disk.password_salt,
                                )
                        })
                })
                .unwrap_or_default()
        })
        .await
        .map_err(|_| BotBackendError::Io("校验当前 WebUI 凭据失败".into()))?;
        Ok((password, secret_paths))
    }
}

async fn verify_webui_http(port: u16) -> Result<(), BotBackendError> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|error| BotBackendError::Io(error.to_string()))?;
    // Authentication failures still prove the HTTP endpoint is reachable. Do not login here.
    client
        .get(format!("http://127.0.0.1:{port}/"))
        .send()
        .await
        .map_err(|error| BotBackendError::Io(format!("SnowLuma WebUI 连接失败: {error}")))?;
    Ok(())
}

#[cfg(test)]
#[path = "webui_tests.rs"]
mod tests;
