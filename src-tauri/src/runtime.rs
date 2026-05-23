use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use ncd_core::{
    BackendKind, BotId, BotStatus, BroadcastEventBus, DomainEvent, EventBus, MockRemoteHost,
    RemoteFileEntry, RemoteHost, RemoteHostError, RuntimeTarget,
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

#[derive(Debug, Clone)]
struct RemoteRuntimeService {
    remote_id: String,
    host: Arc<MockRemoteHost>,
    connection: RemoteHostConnectionInfo,
}

impl RemoteRuntimeService {
    fn new(connection: RemoteHostConnectionInfo, host: Arc<MockRemoteHost>) -> Self {
        Self {
            remote_id: connection.remote_id.clone(),
            host,
            connection,
        }
    }

    async fn list_remote_files(&self, path: &str) -> Result<Vec<RemoteFileEntry>, String> {
        self.host
            .list_dir(path)
            .await
            .map_err(map_remote_host_error)
    }

    async fn get_runtime_status(
        &self,
        bot_id: &str,
    ) -> Result<RemoteRuntimeStatusResponse, String> {
        let bot_id = BotId::new(bot_id.to_string());
        let status = match self.host.process_tree(bot_id.clone()).await {
            Ok(tree) => {
                let mut status = BotStatus::running(bot_id.clone(), tree.root.pid, 0);
                status.extra.insert(
                    "backend_kind".to_string(),
                    serde_json::Value::String(BackendKind::RemoteSsh.as_str().to_string()),
                );
                status.extra.insert(
                    "runtime_target".to_string(),
                    serde_json::Value::String(
                        match RuntimeTarget::server(self.remote_id.clone()) {
                            RuntimeTarget::Local => "local".to_string(),
                            RuntimeTarget::Server(id) => id,
                        },
                    ),
                );
                status.extra.insert(
                    "process_name".to_string(),
                    serde_json::Value::String(tree.root.name),
                );
                status
            }
            Err(RemoteHostError::ProcessTreeFailed(_)) | Err(RemoteHostError::NotFound(_)) => {
                BotStatus::stopped(bot_id.clone())
            }
            Err(error) => return Err(map_remote_host_error(error)),
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
#[derive(Debug, Clone)]
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

        let connection = RemoteHostConnectionInfo {
            remote_id: request.remote_id.clone(),
            host: request.host,
            port: if request.port == 0 {
                default_remote_port()
            } else {
                request.port
            },
            username: request.username,
            webui_url: request.webui_url,
        };
        let host = Arc::new(MockRemoteHost::new());
        self.remote_services.lock().await.insert(
            request.remote_id.clone(),
            RemoteRuntimeService::new(connection.clone(), host),
        );
        Ok(connection)
    }

    pub async fn list_remote_files(
        &self,
        request: ListRemoteFilesRequest,
    ) -> Result<Vec<RemoteFileEntry>, String> {
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
            .any(|status| status.state == ncd_core::BotActorState::Running)
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

fn map_remote_host_error(error: RemoteHostError) -> String {
    match error {
        RemoteHostError::Unavailable => "远端主机不可用".to_string(),
        RemoteHostError::NotFound(path) => format!("远端资源未找到: {path}"),
        RemoteHostError::CommandFailed(message) => format!("远端命令失败: {message}"),
        RemoteHostError::TunnelFailed(message) => format!("远端隧道失败: {message}"),
        RemoteHostError::ProcessTreeFailed(message) => format!("远端进程树读取失败: {message}"),
        RemoteHostError::Io(message) => format!("远端 IO 失败: {message}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn runtime_status_publication_uses_external_records_only() {
        let root = tempdir().unwrap();
        let bus = ncd_core::BroadcastEventBus::default();
        let runtime = AppRuntime::new(root.path(), bus.clone());
        let mut subscription = bus.subscribe(ncd_core::EventFilter::kind(
            ncd_core::DomainEventKind::BotStatusChanged,
        ));

        runtime
            .record_external_status_for_test(BotStatus::running("10008", 42, 1))
            .await;

        runtime.publish_runtime_status_changes().await;
        let event = subscription.next().await.expect("expected status event");
        assert_eq!(event.bot_id().map(BotId::as_str), Some("10008"));
    }
    #[tokio::test]
    async fn connect_and_query_remote_runtime_contract() {
        let root = tempdir().unwrap();
        let bus = ncd_core::BroadcastEventBus::default();
        let runtime = AppRuntime::new(root.path(), bus.clone());

        let connection = runtime
            .connect_remote_host(ConnectRemoteHostRequest {
                remote_id: "remote-a".to_string(),
                host: "127.0.0.1".to_string(),
                port: 22,
                username: "napcat".to_string(),
                password: None,
                webui_url: Some("http://127.0.0.1:3000".to_string()),
            })
            .await
            .unwrap();
        assert_eq!(connection.remote_id, "remote-a");

        let files = runtime
            .list_remote_files(ListRemoteFilesRequest {
                remote_id: "remote-a".to_string(),
                path: "/etc".to_string(),
            })
            .await
            .unwrap();
        assert!(files.is_empty());

        let status = runtime
            .get_remote_runtime_status(GetRemoteRuntimeStatusRequest {
                remote_id: "remote-a".to_string(),
                bot_id: "20001".to_string(),
            })
            .await
            .unwrap();
        assert_eq!(status.remote_id, "remote-a");
        assert_eq!(status.bot_id, "20001");

        let webui = runtime
            .get_remote_webui_endpoint(GetRemoteWebuiEndpointRequest {
                remote_id: "remote-a".to_string(),
                bot_id: "20001".to_string(),
            })
            .await
            .unwrap();
        assert_eq!(webui.remote_id, "remote-a");
        assert_eq!(webui.webui_url.as_deref(), Some("http://127.0.0.1:3000"));
    }
}
