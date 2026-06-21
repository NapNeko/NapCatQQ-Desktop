use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use ncd_runtime::{
    BackendKind, BotStatus, BroadcastEventBus, DomainEvent, EventBus, RuntimeTarget,
};
#[cfg(test)]
use ncd_runtime::BotId;

// ============================================================
// 前端 IPC contract 数据结构(保留——前端 remote.service.ts 依赖这些 shape)
// ============================================================

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

fn default_remote_port() -> u16 {
    22
}

// ============================================================
// AppRuntime:本地运行时状态轮询(non-remote)
// ============================================================

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
pub struct AppRuntime {
    data_root: PathBuf,
    event_bus: BroadcastEventBus,
    registry: Arc<Mutex<RuntimeRegistry>>,
}

impl AppRuntime {
    pub fn new(data_root: impl Into<PathBuf>, event_bus: BroadcastEventBus) -> Self {
        Self {
            data_root: data_root.into(),
            event_bus,
            registry: Arc::new(Mutex::new(RuntimeRegistry::default())),
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
        // 无远端连接需要清理了——连接缓存已搬到 ServerManager
    }

    async fn has_running_bot(&self) -> bool {
        self.registry
            .lock()
            .await
            .latest_statuses()
            .iter()
            .any(|status| status.state == ncd_runtime::BotActorState::Running)
    }

    #[cfg(test)]
    pub async fn record_external_status_for_test(&self, status: BotStatus) {
        let bot_id = status.bot_id.clone();
        self.registry.lock().await.upsert(&bot_id, Some(status));
    }
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
}
