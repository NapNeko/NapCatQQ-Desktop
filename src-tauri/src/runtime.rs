use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use ncd_core::{
    BackendKind, BotBackend, BotBackendError, BotFlavor, BotId, BotRuntimeConfig, BotStartCtx,
    BotStatus, BroadcastEventBus, DomainEvent, EventBus, LocalRuntimeBackend, RuntimeTarget,
    StopMode,
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

#[derive(Debug, Clone)]
pub struct AppRuntime {
    data_root: PathBuf,
    local_napcat_backend: Arc<LocalRuntimeBackend>,
    local_snowluma_backend: Arc<LocalRuntimeBackend>,
    event_bus: BroadcastEventBus,
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
        let status = backend.status(bot_id.clone()).await.map_err(map_backend_error)?;
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
        if let Some(status) = backend.status(bot_id.clone()).await.ok() {
            self.emit_status(status, "stop_local_bot");
        }
        Ok(())
    }

    pub async fn get_all_bot_statuses(&self) -> Vec<BotStatus> {
        let mut statuses = self.local_napcat_backend.list_running().await;
        statuses.extend(self.local_snowluma_backend.list_running().await);
        statuses
    }

    pub async fn publish_runtime_statuses(&self) {
        for status in self.get_all_bot_statuses().await {
            self.emit_status(status, "runtime_poll");
        }
    }

    async fn flavor_for(&self, bot_id: &BotId) -> BotFlavor {
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

fn map_backend_error(error: BotBackendError) -> String {
    error.to_string()
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
    async fn emits_status_events_for_running_bots() {
        let root = tempdir().unwrap();
        let bus = ncd_core::BroadcastEventBus::default();
        let runtime = AppRuntime::new(root.path(), bus.clone());

        runtime
            .spawn_local_bot(SpawnLocalBotRequest {
                bot_id: "10002".to_string(),
                flavor: BotFlavor::NapCat,
                launch_command: sleep_command(),
                working_dir: None,
                environment: BTreeMap::new(),
            })
            .await
            .unwrap();

        let mut subscription = runtime.event_bus.subscribe(ncd_core::EventFilter::kind(
            ncd_core::DomainEventKind::BotStatusChanged,
        ));
        runtime.publish_runtime_statuses().await;
        let event = subscription.next().await.expect("expected status event");
        match event {
            ncd_core::DomainEvent::BotStatusChanged { status, .. } => {
                assert_eq!(status.bot_id.as_str(), "10002");
            }
            other => panic!("unexpected event: {other:?}"),
        }

        runtime
            .stop_local_bot(StopLocalBotRequest {
                bot_id: "10002".to_string(),
                mode: StopMode::Force,
            })
            .await
            .unwrap();
    }
}
