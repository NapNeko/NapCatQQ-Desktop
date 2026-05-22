use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use ncd_core::{
    BackendKind, BotBackend, BotBackendError, BotFlavor, BotId, BotRuntimeConfig, BotStartCtx,
    BotStatus, BroadcastEventBus, DomainEvent, EventBus, LocalRuntimeBackend, MockRemoteHost,
    RemoteFileEntry, RemoteHost, RemoteHostError, RuntimeTarget, StopMode,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpawnLocalBotRequest {
    pub bot_id: String,
    #[serde(default = "default_bot_flavor")]
    pub flavor: BotFlavor,
    pub launch_command: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<PathBuf>,
    #[serde(default)]
    pub environment: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StopLocalBotRequest {
    pub bot_id: String,
    #[serde(default)]
    pub mode: StopMode,
}

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
    flavor: BotFlavor,
    latest_status: Option<BotStatus>,
}

#[derive(Debug, Default)]
struct RuntimeRegistry {
    records: BTreeMap<String, RuntimeRecord>,
}

impl RuntimeRegistry {
    fn upsert(&mut self, bot_id: &BotId, flavor: BotFlavor, status: Option<BotStatus>) {
        self.records.insert(
            bot_id.as_str().to_string(),
            RuntimeRecord {
                flavor,
                latest_status: status,
            },
        );
    }

    fn flavor_for(&self, bot_id: &BotId) -> Option<BotFlavor> {
        self.records
            .get(bot_id.as_str())
            .map(|record| record.flavor)
    }

    fn status_for(&self, bot_id: &BotId) -> Option<&BotStatus> {
        self.records
            .get(bot_id.as_str())
            .and_then(|record| record.latest_status.as_ref())
    }

    fn known_bots(&self) -> Vec<(BotId, BotFlavor)> {
        self.records
            .iter()
            .map(|(bot_id, record)| (BotId::new(bot_id.clone()), record.flavor))
            .collect()
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
    local_napcat_backend: Arc<LocalRuntimeBackend>,
    local_snowluma_backend: Arc<LocalRuntimeBackend>,
    event_bus: BroadcastEventBus,
    registry: Arc<Mutex<RuntimeRegistry>>,
    remote_services: Arc<Mutex<BTreeMap<String, RemoteRuntimeService>>>,
}

impl AppRuntime {
    pub fn new(data_root: impl Into<PathBuf>, event_bus: BroadcastEventBus) -> Self {
        let data_root = data_root.into();
        Self {
            local_napcat_backend: Arc::new(LocalRuntimeBackend::new_with_flavor(
                &data_root,
                "local-napcat",
                BotFlavor::NapCat,
            )),
            local_snowluma_backend: Arc::new(LocalRuntimeBackend::new_with_flavor(
                &data_root,
                "local-snowluma",
                BotFlavor::SnowLuma,
            )),
            data_root,
            event_bus,
            registry: Arc::new(Mutex::new(RuntimeRegistry::default())),
            remote_services: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    pub fn data_root(&self) -> &Path {
        &self.data_root
    }

    fn backend_for(&self, flavor: BotFlavor) -> &LocalRuntimeBackend {
        match flavor {
            BotFlavor::NapCat => &self.local_napcat_backend,
            BotFlavor::SnowLuma => &self.local_snowluma_backend,
        }
    }

    async fn record_bot(&self, bot_id: &BotId, flavor: BotFlavor, status: Option<BotStatus>) {
        self.registry.lock().await.upsert(bot_id, flavor, status);
    }

    fn emit_status(&self, status: BotStatus, source: impl Into<String>) {
        self.event_bus
            .publish(DomainEvent::bot_status_changed(status, source));
    }

    pub async fn spawn_local_bot(
        &self,
        request: SpawnLocalBotRequest,
    ) -> Result<BotStatus, String> {
        validate_bot_id(&request.bot_id)?;
        validate_launch_command(&request.launch_command)?;

        let bot_id = BotId::new(request.bot_id);
        let flavor = request.flavor;
        let backend = self.backend_for(flavor);
        let config = BotRuntimeConfig {
            bot_id: bot_id.clone(),
            config_path: BotRuntimeConfig::default_path(&self.data_root, bot_id.clone())
                .config_path,
            backend_kind: BackendKind::Local,
            flavor,
            runtime_target: RuntimeTarget::Local,
            launch_command: request.launch_command,
            working_dir: request.working_dir,
            log_path: None,
            environment: request.environment,
        }
        .with_runtime_defaults(&self.data_root);

        backend
            .start(&BotStartCtx { config })
            .await
            .map_err(map_backend_error)?;
        let status = backend
            .status(bot_id.clone())
            .await
            .map_err(map_backend_error)?;
        self.record_bot(&bot_id, flavor, Some(status.clone())).await;
        self.emit_status(status.clone(), "spawn_local_bot");
        Ok(status)
    }

    pub async fn stop_local_bot(&self, request: StopLocalBotRequest) -> Result<(), String> {
        validate_bot_id(&request.bot_id)?;
        let bot_id = BotId::new(request.bot_id);
        let flavor = self.flavor_for(&bot_id).await;
        let backend = self.backend_for(flavor);
        backend
            .stop(bot_id.clone(), request.mode)
            .await
            .map_err(map_backend_error)?;
        let status = backend
            .status(bot_id.clone())
            .await
            .map_err(map_backend_error)?;
        self.record_bot(&bot_id, flavor, Some(status.clone())).await;
        self.emit_status(status, "stop_local_bot");
        Ok(())
    }

    pub async fn get_all_bot_statuses(&self) -> Vec<BotStatus> {
        let napcat_statuses = self.local_napcat_backend.list_running().await;
        let snowluma_statuses = self.local_snowluma_backend.list_running().await;
        let mut registry = self.registry.lock().await;
        for status in &napcat_statuses {
            registry.upsert(&status.bot_id, BotFlavor::NapCat, Some(status.clone()));
        }
        for status in &snowluma_statuses {
            registry.upsert(&status.bot_id, BotFlavor::SnowLuma, Some(status.clone()));
        }

        let mut statuses = registry.latest_statuses();
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
        for (bot_id, flavor) in self.known_bots().await {
            let backend = self.backend_for(flavor);
            let Ok(status) = backend.status(bot_id.clone()).await else {
                continue;
            };
            if self
                .record_status_change(&bot_id, flavor, status.clone())
                .await
            {
                self.emit_status(status, "runtime_watch");
            }
        }
    }

    pub async fn watcher_interval_secs(&self) -> u64 {
        if self.has_running_bot().await { 2 } else { 10 }
    }

    pub async fn shutdown(&self) {
        for (bot_id, flavor) in self.known_bots().await {
            let backend = self.backend_for(flavor);
            if let Ok(status) = backend.status(bot_id.clone()).await
                && status.state == ncd_core::BotActorState::Running
            {
                let _ = backend.stop(bot_id.clone(), StopMode::Force).await;
                let stopped = BotStatus::stopped(bot_id.clone());
                self.record_bot(&bot_id, flavor, Some(stopped.clone()))
                    .await;
                self.emit_status(stopped, "runtime_shutdown");
            }
        }
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

    async fn known_bots(&self) -> Vec<(BotId, BotFlavor)> {
        self.registry.lock().await.known_bots()
    }

    async fn record_status_change(
        &self,
        bot_id: &BotId,
        flavor: BotFlavor,
        status: BotStatus,
    ) -> bool {
        let mut registry = self.registry.lock().await;
        let changed = registry
            .status_for(bot_id)
            .map(|previous| previous != &status)
            .unwrap_or(true);
        registry.upsert(bot_id, flavor, Some(status));
        changed
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

    async fn flavor_for(&self, bot_id: &BotId) -> BotFlavor {
        if let Some(flavor) = self.registry.lock().await.flavor_for(bot_id) {
            return flavor;
        }

        let path = BotRuntimeConfig::default_path(&self.data_root, bot_id.clone()).config_path;
        match tokio::fs::read_to_string(&path)
            .await
            .ok()
            .and_then(|text| serde_json::from_str::<BotRuntimeConfig>(&text).ok())
            .map(|cfg| cfg.flavor)
        {
            Some(flavor) => flavor,
            None => BotFlavor::NapCat,
        }
    }
}

fn default_bot_flavor() -> BotFlavor {
    BotFlavor::NapCat
}

fn default_remote_port() -> u16 {
    22
}

fn validate_bot_id(bot_id: &str) -> Result<(), String> {
    let trimmed = bot_id.trim();
    if trimmed.is_empty() {
        return Err("bot_id 不能为空".to_string());
    }
    if trimmed.contains('/') || trimmed.contains('\\') || trimmed.contains("..") {
        return Err("bot_id 不能包含路径分隔符".to_string());
    }
    Ok(())
}

fn validate_launch_command(command: &[String]) -> Result<(), String> {
    if command.is_empty() {
        return Err("launch_command 不能为空".to_string());
    }
    Ok(())
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

fn map_backend_error(error: BotBackendError) -> String {
    error.to_string()
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

    fn sleep_command() -> Vec<String> {
        if cfg!(windows) {
            vec![
                "timeout".to_string(),
                "/T".to_string(),
                "2".to_string(),
                "/NOBREAK".to_string(),
            ]
        } else {
            vec!["sleep".to_string(), "2".to_string()]
        }
    }

    #[tokio::test]
    async fn spawn_and_list_running_local_bot() {
        let root = tempdir().unwrap();
        let bus = ncd_core::BroadcastEventBus::default();
        let runtime = AppRuntime::new(root.path(), bus.clone());

        let status = runtime
            .spawn_local_bot(SpawnLocalBotRequest {
                bot_id: "10001".to_string(),
                flavor: BotFlavor::NapCat,
                launch_command: sleep_command(),
                working_dir: None,
                environment: BTreeMap::new(),
            })
            .await
            .unwrap();

        assert_eq!(status.bot_id.as_str(), "10001");
        assert_eq!(status.state, ncd_core::BotActorState::Running);

        let statuses = runtime.get_all_bot_statuses().await;
        assert_eq!(statuses.len(), 1);
        assert_eq!(statuses[0].bot_id.as_str(), "10001");

        runtime
            .stop_local_bot(StopLocalBotRequest {
                bot_id: "10001".to_string(),
                mode: StopMode::Force,
            })
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn routes_bots_by_flavor() {
        let root = tempdir().unwrap();
        let bus = ncd_core::BroadcastEventBus::default();
        let runtime = AppRuntime::new(root.path(), bus.clone());

        let napcat = runtime
            .spawn_local_bot(SpawnLocalBotRequest {
                bot_id: "10003".to_string(),
                flavor: BotFlavor::NapCat,
                launch_command: sleep_command(),
                working_dir: None,
                environment: BTreeMap::new(),
            })
            .await
            .unwrap();
        assert_eq!(
            napcat.extra.get("flavor").and_then(|value| value.as_str()),
            Some("napcat")
        );

        let snowluma = runtime
            .spawn_local_bot(SpawnLocalBotRequest {
                bot_id: "10004".to_string(),
                flavor: BotFlavor::SnowLuma,
                launch_command: sleep_command(),
                working_dir: None,
                environment: BTreeMap::new(),
            })
            .await
            .unwrap();
        assert_eq!(
            snowluma
                .extra
                .get("flavor")
                .and_then(|value| value.as_str()),
            Some("snowluma")
        );

        runtime
            .stop_local_bot(StopLocalBotRequest {
                bot_id: "10003".to_string(),
                mode: StopMode::Force,
            })
            .await
            .unwrap();
        runtime
            .stop_local_bot(StopLocalBotRequest {
                bot_id: "10004".to_string(),
                mode: StopMode::Force,
            })
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn rejects_invalid_bot_id() {
        let root = tempdir().unwrap();
        let bus = ncd_core::BroadcastEventBus::default();
        let runtime = AppRuntime::new(root.path(), bus.clone());

        let error = runtime
            .spawn_local_bot(SpawnLocalBotRequest {
                bot_id: "../escape".to_string(),
                flavor: BotFlavor::NapCat,
                launch_command: vec!["sleep".to_string(), "1".to_string()],
                working_dir: None,
                environment: BTreeMap::new(),
            })
            .await
            .unwrap_err();

        assert!(error.contains("路径分隔符"));
    }

    #[tokio::test]
    async fn stop_routes_snowluma_without_falling_back_to_napcat() {
        let root = tempdir().unwrap();
        let bus = ncd_core::BroadcastEventBus::default();
        let runtime = AppRuntime::new(root.path(), bus.clone());

        runtime
            .spawn_local_bot(SpawnLocalBotRequest {
                bot_id: "10005".to_string(),
                flavor: BotFlavor::SnowLuma,
                launch_command: sleep_command(),
                working_dir: None,
                environment: BTreeMap::new(),
            })
            .await
            .unwrap();

        let config_path =
            BotRuntimeConfig::default_path(root.path(), BotId::new("10005")).config_path;
        tokio::fs::remove_file(&config_path).await.unwrap();

        runtime
            .stop_local_bot(StopLocalBotRequest {
                bot_id: "10005".to_string(),
                mode: StopMode::Force,
            })
            .await
            .unwrap();

        let statuses = runtime.get_all_bot_statuses().await;
        let status = statuses
            .iter()
            .find(|status| status.bot_id.as_str() == "10005")
            .unwrap();
        assert_eq!(status.state, ncd_core::BotActorState::Stopped);
    }

    #[tokio::test]
    async fn fallback_to_napcat_when_registry_and_config_are_missing() {
        let root = tempdir().unwrap();
        let bus = ncd_core::BroadcastEventBus::default();
        let runtime = AppRuntime::new(root.path(), bus.clone());

        let flavor = runtime.flavor_for(&BotId::new("10006")).await;
        assert_eq!(flavor, BotFlavor::NapCat);
    }

    #[tokio::test]
    async fn runtime_watch_emits_only_changed_status() {
        let root = tempdir().unwrap();
        let bus = ncd_core::BroadcastEventBus::default();
        let runtime = AppRuntime::new(root.path(), bus.clone());
        let mut subscription = bus.subscribe(ncd_core::EventFilter::kind(
            ncd_core::DomainEventKind::BotStatusChanged,
        ));

        runtime
            .spawn_local_bot(SpawnLocalBotRequest {
                bot_id: "10008".to_string(),
                flavor: BotFlavor::NapCat,
                launch_command: sleep_command(),
                working_dir: None,
                environment: BTreeMap::new(),
            })
            .await
            .unwrap();
        let _ = subscription.next().await.expect("expected spawn event");

        runtime.publish_runtime_status_changes().await;
        let unchanged =
            tokio::time::timeout(std::time::Duration::from_millis(50), subscription.next()).await;
        assert!(unchanged.is_err());

        runtime
            .stop_local_bot(StopLocalBotRequest {
                bot_id: "10008".to_string(),
                mode: StopMode::Force,
            })
            .await
            .unwrap();
        let _ = subscription.next().await.expect("expected stop event");

        runtime.publish_runtime_status_changes().await;
        let unchanged =
            tokio::time::timeout(std::time::Duration::from_millis(50), subscription.next()).await;
        assert!(unchanged.is_err());
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
