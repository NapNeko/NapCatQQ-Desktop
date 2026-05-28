use ncd_runtime::{BotActorSnapshot, BotConfig, BotId, LogSnapshot};
use ncd_runtime::config_drift::{ConfigDrift, DriftDecision};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tauri::State;

use crate::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchResultResponse {
    pub succeeded: Vec<String>,
    pub failed: Vec<(String, String)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootstrapResultResponse {
    pub started: BatchResultResponse,
    pub skipped: Vec<String>,
}

fn map_err(err: ncd_runtime::BotManagerError) -> String {
    err.to_string()
}

fn batch_to_response(result: ncd_runtime::BatchResult) -> BatchResultResponse {
    BatchResultResponse {
        succeeded: result
            .succeeded
            .into_iter()
            .map(|id| id.to_string())
            .collect(),
        failed: result
            .failed
            .into_iter()
            .map(|(id, err)| (id.to_string(), err.to_string()))
            .collect(),
    }
}

#[tauri::command]
pub async fn bootstrap_bot_manager(
    state: State<'_, AppState>,
) -> Result<BootstrapResultResponse, String> {
    let result = state.bot_manager.bootstrap().await.map_err(map_err)?;
    Ok(BootstrapResultResponse {
        started: batch_to_response(result.started),
        skipped: result
            .skipped
            .into_iter()
            .map(|id| id.to_string())
            .collect(),
    })
}

#[tauri::command]
pub async fn list_bot_snapshots(
    state: State<'_, AppState>,
) -> Result<Vec<BotActorSnapshot>, String> {
    Ok(state.bot_manager.list_snapshots().await)
}

#[tauri::command]
pub async fn get_bot_snapshot(
    state: State<'_, AppState>,
    bot_id: String,
) -> Result<BotActorSnapshot, String> {
    state
        .bot_manager
        .get_snapshot(&BotId::new(bot_id))
        .await
        .map_err(map_err)
}

#[tauri::command]
pub async fn get_bot_config(
    state: State<'_, AppState>,
    bot_id: String,
) -> Result<Option<BotConfig>, String> {
    state
        .bot_manager
        .get_bot_config(&BotId::new(bot_id))
        .await
        .map_err(map_err)
}

/// 批量拉所有 Bot 的 backend_type，避免列表页对每个 bot 调 `get_bot_config` 的 N+1。
/// 返回的 key 是 BotId 字符串（数字 QQID）。
#[tauri::command]
pub async fn list_bot_flavors(
    state: State<'_, AppState>,
) -> Result<HashMap<String, ncd_runtime::BackendType>, String> {
    state.bot_manager.list_bot_flavors().await.map_err(map_err)
}

#[tauri::command]
pub async fn upsert_bot_config(
    state: State<'_, AppState>,
    config: BotConfig,
) -> Result<BotActorSnapshot, String> {
    state
        .bot_manager
        .upsert_bot_config(config)
        .await
        .map_err(map_err)
}

#[tauri::command]
pub async fn delete_bot_config(state: State<'_, AppState>, bot_id: String) -> Result<(), String> {
    state
        .bot_manager
        .delete_bot_config(&BotId::new(bot_id))
        .await
        .map_err(map_err)
}

#[tauri::command]
pub async fn start_bot(
    state: State<'_, AppState>,
    bot_id: String,
) -> Result<BotActorSnapshot, String> {
    state
        .bot_manager
        .start_bot(&BotId::new(bot_id))
        .await
        .map_err(map_err)
}

/// 启动前检测派生配置文件是否被外部修改。返回 `None` 表示无差异可直接启动。
#[tauri::command]
pub async fn detect_bot_config_drift(
    state: State<'_, AppState>,
    bot_id: String,
) -> Result<Option<ConfigDrift>, String> {
    state
        .bot_manager
        .detect_config_drift(&BotId::new(bot_id))
        .await
        .map_err(map_err)
}

/// 带用户决议启动 Bot。前端在 ConfigDriftDialog 确认后调此命令。
#[tauri::command]
pub async fn start_bot_with_drift_decisions(
    state: State<'_, AppState>,
    bot_id: String,
    decisions: Vec<DriftDecision>,
) -> Result<BotActorSnapshot, String> {
    state
        .bot_manager
        .start_bot_with_decisions(&BotId::new(bot_id), &decisions)
        .await
        .map_err(map_err)
}

/// 带用户决议保存配置。前端保存时如果检测到 drift 并确认了决议后调此命令。
#[tauri::command]
pub async fn upsert_bot_config_with_decisions(
    state: State<'_, AppState>,
    config: BotConfig,
    decisions: Vec<DriftDecision>,
) -> Result<BotActorSnapshot, String> {
    use ncd_runtime::config_drift::DriftDecision as DD;
    let mut overrides: std::collections::HashMap<String, Vec<(String, serde_json::Value)>> =
        std::collections::HashMap::new();
    for d in &decisions {
        match d {
            DD::AcceptExternal { file, path, value } => {
                overrides.entry(file.clone()).or_default().push((path.clone(), value.clone()));
            }
            DD::DropAdded { file, path } => {
                overrides
                    .entry(file.clone())
                    .or_default()
                    .push((path.clone(), serde_json::Value::Null));
            }
            _ => {}
        }
    }
    state
        .bot_manager
        .upsert_bot_config_with_overrides(config, &overrides)
        .await
        .map_err(map_err)
}

#[tauri::command]
pub async fn stop_bot(
    state: State<'_, AppState>,
    bot_id: String,
) -> Result<BotActorSnapshot, String> {
    state
        .bot_manager
        .stop_bot(&BotId::new(bot_id))
        .await
        .map_err(map_err)
}

#[tauri::command]
pub async fn batch_start_bots(
    state: State<'_, AppState>,
    bot_ids: Vec<String>,
) -> Result<BatchResultResponse, String> {
    let ids: Vec<BotId> = bot_ids.into_iter().map(BotId::new).collect();
    let result = state.bot_manager.batch_start(&ids).await.map_err(map_err)?;
    Ok(batch_to_response(result))
}

#[tauri::command]
pub async fn batch_stop_bots(
    state: State<'_, AppState>,
    bot_ids: Vec<String>,
) -> Result<BatchResultResponse, String> {
    let ids: Vec<BotId> = bot_ids.into_iter().map(BotId::new).collect();
    let result = state.bot_manager.batch_stop(&ids).await.map_err(map_err)?;
    Ok(batch_to_response(result))
}

#[tauri::command]
pub async fn batch_delete_bots(
    state: State<'_, AppState>,
    bot_ids: Vec<String>,
) -> Result<BatchResultResponse, String> {
    let ids: Vec<BotId> = bot_ids.into_iter().map(BotId::new).collect();
    let result = state
        .bot_manager
        .batch_delete(&ids)
        .await
        .map_err(map_err)?;
    Ok(batch_to_response(result))
}

#[tauri::command]
pub async fn count_bot_configs(state: State<'_, AppState>) -> Result<usize, String> {
    Ok(state.bot_manager.bot_count().await)
}

#[tauri::command]
pub async fn active_bot_count(state: State<'_, AppState>) -> Result<usize, String> {
    Ok(state.bot_manager.active_count().await)
}

/// 拉取指定 Bot 的最近 `lines` 行日志快照（默认 1000 行）。
///
/// 用于 BotLogPage 开页时一次性加载历史；后续增量靠订阅 `log_appended` Tauri
/// 事件即可。对齐 legacy `NapCatQQProcessLog.get_log_content` 行为。
#[tauri::command]
pub async fn tail_bot_log(
    state: State<'_, AppState>,
    bot_id: String,
    lines: Option<usize>,
) -> Result<LogSnapshot, String> {
    let lines = lines.unwrap_or(1000);
    state
        .bot_manager
        .tail_log(&BotId::new(bot_id), lines)
        .await
        .map_err(map_err)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use ncd_runtime::{
        AdvancedConfig, BotActorState, BotBasicConfig, BotConfig, BotManager, BroadcastEventBus,
        ConfigStore, ConnectConfig, DispatchRenderer, DomainEventKind, EventBus, EventFilter,
        LocalBotConfigRepo, LocalConfigStore, SecretStoreImpl,
    };
    use tempfile::tempdir;

    use crate::AppState;

    fn make_test_state(root: &std::path::Path) -> (AppState, BroadcastEventBus) {
        let bus = BroadcastEventBus::default();
        let runtime = crate::runtime::AppRuntime::new(root, bus.clone());
        let store = Arc::new(LocalConfigStore::new(root));
        let secrets: Arc<dyn ncd_runtime::SecretStore + Send + Sync> =
            Arc::new(SecretStoreImpl::new(root.join("secrets")));
        let repo = Arc::new(LocalBotConfigRepo::new(Arc::clone(&store), secrets));
        let renderer = Arc::new(DispatchRenderer::new(
            store.config_dir(),
            store.config_dir(),
        ));
        let backend = Arc::new(ncd_runtime::LocalRuntimeBackend::new(root, "test-local"));
        let launch_planner = Arc::new(ncd_runtime::FileSystemRuntimeLaunchPlanner::new(
            root.join("runtime"),
        ));
        let webui_client: Arc<dyn ncd_runtime::NapCatWebUiClient> =
            Arc::new(ncd_runtime::ReqwestNapCatWebUiClient::new().expect("init webui client"));
        let offline_notifier: Arc<dyn ncd_runtime::OfflineNotifier> =
            Arc::new(ncd_runtime::NoopOfflineNotifier);
        let poller_settings = Arc::new(tokio::sync::RwLock::new(
            ncd_runtime::WebUiPollerSettings::default(),
        ));
        let bot_manager = Arc::new(BotManager::new(
            repo,
            Arc::clone(&store),
            renderer,
            backend,
            launch_planner,
            Arc::new(bus.clone()),
            webui_client,
            offline_notifier,
            poller_settings,
        ));
        let state = AppState {
            data_root: root.to_path_buf(),
            snapshot: ncd_runtime::BootstrapSnapshot::ready(),
            event_bus: bus.clone(),
            runtime,
            bot_manager,
            active_tasks: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
        };
        (state, bus)
    }

    fn sample_bot_config(qq_id: u64) -> BotConfig {
        BotConfig {
            bot: BotBasicConfig {
                name: format!("bot-{qq_id}"),
                qq_id,
                music_sign_url: String::new(),
                auto_restart_schedule: Default::default(),
                offline_auto_restart: false,
                runtime_target: ncd_runtime::RuntimeTarget::Local,
                backend_type: ncd_runtime::BackendType::NapCat,
                snowluma_start_mode: None,
            },
            connect: ConnectConfig::default(),
            advanced: AdvancedConfig::default(),
        }
    }

    #[tokio::test]
    async fn upsert_and_list_snapshots() {
        let root = tempdir().unwrap();
        let (state, _bus) = make_test_state(root.path());

        let snapshot = state
            .bot_manager
            .upsert_bot_config(sample_bot_config(10001))
            .await
            .unwrap();
        assert_eq!(snapshot.bot_id.as_str(), "10001");
        assert_eq!(snapshot.state, BotActorState::Stopped);

        let snapshots = state.bot_manager.list_snapshots().await;
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].bot_id.as_str(), "10001");
    }

    #[tokio::test]
    async fn start_without_launch_command_marks_crashed() {
        let root = tempdir().unwrap();
        let (state, _bus) = make_test_state(root.path());

        state
            .bot_manager
            .upsert_bot_config(sample_bot_config(10002))
            .await
            .unwrap();

        let err = state
            .bot_manager
            .start_bot(&ncd_runtime::BotId::new("10002"))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("NapCatWinBootMain.exe"));

        let snapshot = state
            .bot_manager
            .get_snapshot(&ncd_runtime::BotId::new("10002"))
            .await
            .unwrap();
        assert_eq!(snapshot.state, BotActorState::Crashed);
    }

    #[tokio::test]
    async fn delete_bot_removes_actor() {
        let root = tempdir().unwrap();
        let (state, _bus) = make_test_state(root.path());

        state
            .bot_manager
            .upsert_bot_config(sample_bot_config(10003))
            .await
            .unwrap();
        assert_eq!(state.bot_manager.bot_count().await, 1);

        state
            .bot_manager
            .delete_bot_config(&ncd_runtime::BotId::new("10003"))
            .await
            .unwrap();
        assert_eq!(state.bot_manager.bot_count().await, 0);
    }

    #[tokio::test]
    async fn bot_events_flow_through_shared_event_bus() {
        let root = tempdir().unwrap();
        let (state, bus) = make_test_state(root.path());
        let mut subscription = bus.subscribe(EventFilter::kind(DomainEventKind::BotStateChanged));

        state
            .bot_manager
            .upsert_bot_config(sample_bot_config(10004))
            .await
            .unwrap();

        let event = subscription.next().await.expect("expected state event");
        match event {
            ncd_runtime::DomainEvent::BotStateChanged { snapshot, .. } => {
                assert_eq!(snapshot.bot_id.as_str(), "10004");
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[tokio::test]
    async fn bootstrap_on_empty_repo_succeeds() {
        let root = tempdir().unwrap();
        let (state, _bus) = make_test_state(root.path());

        let result = state.bot_manager.bootstrap().await.unwrap();
        assert!(result.started.succeeded.is_empty());
        assert!(result.skipped.is_empty());
    }

    #[tokio::test]
    async fn batch_start_and_stop() {
        let root = tempdir().unwrap();
        let (state, _bus) = make_test_state(root.path());

        state
            .bot_manager
            .upsert_bot_config(sample_bot_config(10005))
            .await
            .unwrap();
        state
            .bot_manager
            .upsert_bot_config(sample_bot_config(10006))
            .await
            .unwrap();

        let ids = vec![ncd_runtime::BotId::new("10005"), ncd_runtime::BotId::new("10006")];
        let started = state.bot_manager.batch_start(&ids).await.unwrap();
        assert!(started.succeeded.is_empty());
        assert_eq!(started.failed.len(), 2);

        let stopped = state.bot_manager.batch_stop(&ids).await.unwrap();
        assert_eq!(stopped.succeeded.len(), 2);
        assert!(stopped.failed.is_empty());
    }

    #[tokio::test]
    async fn count_and_active_count() {
        let root = tempdir().unwrap();
        let (state, _bus) = make_test_state(root.path());

        assert_eq!(state.bot_manager.bot_count().await, 0);
        assert_eq!(state.bot_manager.active_count().await, 0);

        state
            .bot_manager
            .upsert_bot_config(sample_bot_config(10007))
            .await
            .unwrap();
        assert_eq!(state.bot_manager.bot_count().await, 1);
        assert_eq!(state.bot_manager.active_count().await, 0);

        let err = state
            .bot_manager
            .start_bot(&ncd_runtime::BotId::new("10007"))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("NapCatWinBootMain.exe"));
        assert_eq!(state.bot_manager.active_count().await, 0);
    }
}
