use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use ncd_host::{DirEntry, Host, HostCommand, HostError, HostPath};
use ncd_host::remote::{ConnectionConfig, RemoteLinuxHost, SshCredentials, HostKeyPolicy};
use ncd_runtime::{
    BackendKind, BotId, BotStatus, BroadcastEventBus, DomainEvent, EventBus, RuntimeTarget,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectRemoteHostRequest {
    pub remote_id: String,
    pub host: String,
    #[serde(default = "default_remote_port")]
    pub port: u16,
    #[serde(default)]
    pub username: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub webui_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListRemoteFilesRequest {
    pub remote_id: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetRemoteRuntimeStatusRequest {
    pub remote_id: String,
    pub bot_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetRemoteWebuiEndpointRequest {
    pub remote_id: String,
    pub bot_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteHostConnectionInfo {
    pub remote_id: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub webui_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteRuntimeStatusResponse {
    pub remote_id: String,
    pub bot_id: String,
    pub status: BotStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backend_kind: Option<BackendKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime_target: Option<RuntimeTarget>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteWebuiEndpointResponse {
    pub remote_id: String,
    pub bot_id: String,
    pub webui_url: Option<String>,
}

#[derive(Debug, Clone)]
struct RuntimeRecord {
    latest_status: Option<BotStatus>,
}

#[derive(Debug, Default)]
struct RuntimeRegistry {
    records: BTreeMap<String, RuntimeRecord>,
}

impl RuntimeRegistry {
    #[cfg(test)]
    fn upsert(&mut self, bot_id: &BotId, status: Option<BotStatus>) {
        self.records.insert(
            bot_id.as_str().to_string(),
            RuntimeRecord {
                latest_status: status,
            },
        );
    }

    fn latest_statuses(&self) -> Vec<BotStatus> {
        self.records
            .values()
            .filter_map(|record| record.latest_status.clone())
            .collect()
    }
}

#[derive(Clone)]
struct RemoteRuntimeService {
    remote_id: String,
    host: Arc<dyn Host>,
    connection: RemoteHostConnectionInfo,
}

impl RemoteRuntimeService {
    fn new(connection: RemoteHostConnectionInfo, host: Arc<dyn Host>) -> Self {
        Self {
            remote_id: connection.remote_id.clone(),
            host,
            connection,
        }
    }

    async fn list_remote_files(&self, path: &str) -> Result<Vec<DirEntry>, String> {
        let host_path = HostPath::from_posix(path);
        self.host
            .list_dir(&host_path)
            .await
            .map_err(map_host_error)
    }

    async fn get_runtime_status(
        &self,
        bot_id: &str,
    ) -> Result<RemoteRuntimeStatusResponse, String> {
        let bot_id = BotId::new(bot_id.to_string());
        // 用 pgrep 探测 NapCat 进程是否在跑。轻量级探测，不做完整进程树构建。
        let cmd = HostCommand::new("pgrep")
            .arg("-f")
            .arg(format!("napcat.*{}", bot_id.as_str()));
        let output = self.host.run_to_string(cmd).await;
        let status = match output {
            Ok(out) if out.success() && !out.stdout.trim().is_empty() => {
                let pid: u32 = out.stdout.trim().lines().next()
                    .and_then(|l| l.trim().parse().ok())
                    .unwrap_or(0);
                let mut s = BotStatus::running(bot_id.clone(), pid, 0);
                s.extra.insert(
                    "backend_kind".to_string(),
                    serde_json::Value::String(BackendKind::RemoteSsh.as_str().to_string()),
                );
                s
            }
            _ => BotStatus::stopped(bot_id.clone()),
        };

        Ok(RemoteRuntimeStatusResponse {
            remote_id: self.remote_id.clone(),
            bot_id: bot_id.as_str().to_string(),
            status,
            backend_kind: Some(BackendKind::RemoteSsh),
            runtime_target: Some(RuntimeTarget::server(self.remote_id.clone())),
        })
    }

    async fn get_webui_endpoint(
        &self,
        bot_id: &str,
    ) -> Result<RemoteWebuiEndpointResponse, String> {
        Ok(RemoteWebuiEndpointResponse {
            remote_id: self.remote_id.clone(),
            bot_id: bot_id.to_string(),
            webui_url: self.connection.webui_url.clone(),
        })
    }
}
#[derive(Clone)]
pub struct AppRuntime {
    data_root: PathBuf,
    event_bus: BroadcastEventBus,
    registry: Arc<Mutex<RuntimeRegistry>>,
    remote_services: Arc<Mutex<BTreeMap<String, RemoteRuntimeService>>>,
}

impl AppRuntime {
    pub fn new(data_root: impl Into<PathBuf>, event_bus: BroadcastEventBus) -> Self {
        let data_root = data_root.into();
        Self {
            data_root,
            event_bus,
            registry: Arc::new(Mutex::new(RuntimeRegistry::default())),
            remote_services: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    pub fn data_root(&self) -> &Path {
        &self.data_root
    }

    fn emit_status(&self, status: BotStatus, source: impl Into<String>) {
        self.event_bus
            .publish(DomainEvent::bot_status_changed(status, source));
    }

    pub async fn get_all_bot_statuses(&self) -> Vec<BotStatus> {
        let mut statuses = self.registry.lock().await.latest_statuses();
        statuses.sort_by(|left, right| left.bot_id.as_str().cmp(right.bot_id.as_str()));
        statuses
    }

    pub async fn connect_remote_host(
        &self,
        request: ConnectRemoteHostRequest,
    ) -> Result<RemoteHostConnectionInfo, String> {
        validate_remote_id(&request.remote_id)?;
        validate_remote_host(&request.host)?;
        validate_username(&request.username)?;

        let port = if request.port == 0 { default_remote_port() } else { request.port };

        let credentials = match request.password {
            Some(ref pw) => SshCredentials::password(&request.username, pw),
            None => {
                // 没给密码时尝试默认私钥路径（~/.ssh/id_rsa → id_ed25519）
                let ssh_dir = dirs::home_dir()
                    .ok_or_else(|| "无法确定用户主目录".to_string())?
                    .join(".ssh");
                let key_path = if ssh_dir.join("id_ed25519").exists() {
                    ssh_dir.join("id_ed25519")
                } else if ssh_dir.join("id_rsa").exists() {
                    ssh_dir.join("id_rsa")
                } else {
                    return Err("未提供密码且未找到 ~/.ssh/id_ed25519 或 id_rsa 密钥文件".to_string());
                };
                SshCredentials::key_file(&request.username, key_path, None)
            }
        };

        let config = ConnectionConfig::new(
            &request.host,
            port,
            credentials,
            HostKeyPolicy::Insecure,
        );

        let host = RemoteLinuxHost::connect(&request.remote_id, config)
            .await
            .map_err(|err| format!("SSH 连接失败: {err}"))?;

        let connection = RemoteHostConnectionInfo {
            remote_id: request.remote_id.clone(),
            host: request.host,
            port,
            username: request.username,
            webui_url: request.webui_url,
        };

        self.remote_services.lock().await.insert(
            request.remote_id.clone(),
            RemoteRuntimeService::new(connection.clone(), Arc::new(host)),
        );
        Ok(connection)
    }

    pub async fn list_remote_files(
        &self,
        request: ListRemoteFilesRequest,
    ) -> Result<Vec<DirEntry>, String> {
        let service = self.remote_service(&request.remote_id).await?;
        service.list_remote_files(&request.path).await
    }

    pub async fn get_remote_runtime_status(
        &self,
        request: GetRemoteRuntimeStatusRequest,
    ) -> Result<RemoteRuntimeStatusResponse, String> {
        let service = self.remote_service(&request.remote_id).await?;
        service.get_runtime_status(&request.bot_id).await
    }

    pub async fn publish_runtime_statuses(&self) {
        for status in self.get_all_bot_statuses().await {
            self.emit_status(status, "runtime_poll");
        }
    }

    pub async fn publish_runtime_status_changes(&self) {
        self.publish_runtime_statuses().await;
    }

    pub async fn watcher_interval_secs(&self) -> u64 {
        if self.has_running_bot().await { 2 } else { 10 }
    }

    pub async fn shutdown(&self) {
        self.remote_services.lock().await.clear();
    }

    async fn has_running_bot(&self) -> bool {
        self.registry
            .lock()
            .await
            .latest_statuses()
            .iter()
            .any(|status| status.state == ncd_runtime::BotActorState::Running)
    }

    pub async fn get_remote_webui_endpoint(
        &self,
        request: GetRemoteWebuiEndpointRequest,
    ) -> Result<RemoteWebuiEndpointResponse, String> {
        let service = self.remote_service(&request.remote_id).await?;
        service.get_webui_endpoint(&request.bot_id).await
    }

    async fn remote_service(&self, remote_id: &str) -> Result<RemoteRuntimeService, String> {
        self.remote_services
            .lock()
            .await
            .get(remote_id)
            .cloned()
            .ok_or_else(|| format!("远端主机未连接: {remote_id}"))
    }

    #[cfg(test)]
    pub async fn record_external_status_for_test(&self, status: BotStatus) {
        let bot_id = status.bot_id.clone();
        self.registry.lock().await.upsert(&bot_id, Some(status));
    }
}

fn default_remote_port() -> u16 {
    22
}

fn validate_remote_id(remote_id: &str) -> Result<(), String> {
    if remote_id.trim().is_empty() {
        return Err("remote_id 不能为空".to_string());
    }
    Ok(())
}

fn validate_remote_host(host: &str) -> Result<(), String> {
    if host.trim().is_empty() {
        return Err("host 不能为空".to_string());
    }
    Ok(())
}

fn validate_username(username: &str) -> Result<(), String> {
    if username.trim().is_empty() {
        return Err("username 不能为空".to_string());
    }
    Ok(())
}

fn map_host_error(error: HostError) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn runtime_status_publication_uses_external_records_only() {
        let root = tempdir().unwrap();
        let bus = ncd_runtime::BroadcastEventBus::default();
        let runtime = AppRuntime::new(root.path(), bus.clone());
        let mut subscription = bus.subscribe(ncd_runtime::EventFilter::kind(
            ncd_runtime::DomainEventKind::BotStatusChanged,
        ));

        runtime
            .record_external_status_for_test(BotStatus::running("10008", 42, 1))
            .await;

        runtime.publish_runtime_status_changes().await;
        let event = subscription.next().await.expect("expected status event");
        assert_eq!(event.bot_id().map(BotId::as_str), Some("10008"));
    }

    #[tokio::test]
    async fn connect_rejects_empty_remote_id() {
        let root = tempdir().unwrap();
        let bus = ncd_runtime::BroadcastEventBus::default();
        let runtime = AppRuntime::new(root.path(), bus);
        let result = runtime
            .connect_remote_host(ConnectRemoteHostRequest {
                remote_id: "".to_string(),
                host: "127.0.0.1".to_string(),
                port: 22,
                username: "user".to_string(),
                password: Some("pw".to_string()),
                webui_url: None,
            })
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("remote_id"));
    }

    #[tokio::test]
    async fn connect_rejects_empty_host() {
        let root = tempdir().unwrap();
        let bus = ncd_runtime::BroadcastEventBus::default();
        let runtime = AppRuntime::new(root.path(), bus);
        let result = runtime
            .connect_remote_host(ConnectRemoteHostRequest {
                remote_id: "r1".to_string(),
                host: "".to_string(),
                port: 22,
                username: "user".to_string(),
                password: Some("pw".to_string()),
                webui_url: None,
            })
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("host"));
    }

    #[tokio::test]
    async fn connect_rejects_empty_username() {
        let root = tempdir().unwrap();
        let bus = ncd_runtime::BroadcastEventBus::default();
        let runtime = AppRuntime::new(root.path(), bus);
        let result = runtime
            .connect_remote_host(ConnectRemoteHostRequest {
                remote_id: "r1".to_string(),
                host: "127.0.0.1".to_string(),
                port: 22,
                username: "".to_string(),
                password: Some("pw".to_string()),
                webui_url: None,
            })
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("username"));
    }

    #[tokio::test]
    async fn list_remote_files_returns_err_when_not_connected() {
        let root = tempdir().unwrap();
        let bus = ncd_runtime::BroadcastEventBus::default();
        let runtime = AppRuntime::new(root.path(), bus);
        let result = runtime
            .list_remote_files(ListRemoteFilesRequest {
                remote_id: "ghost".to_string(),
                path: "/etc".to_string(),
            })
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("未连接"));
    }
}
