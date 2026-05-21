use std::collections::BTreeMap;

use ncd_core::{
    BackendKind, BotActorHandle, BotActorState, BotBackend, BotId, BotRuntimeConfig,
    BotStartCtx, BroadcastEventBus, DomainEvent, EventBus, EventFilter, LocalRuntimeBackend,
    RuntimeTarget, TailOpts,
};
use tempfile::tempdir;

#[tokio::test]
async fn runtime_backend_syncs_runtime_defaults() {
    let root = tempdir().unwrap();
    let backend = LocalRuntimeBackend::new(root.path(), "backend-1");
    let cfg = BotRuntimeConfig {
        bot_id: BotId::new("10001"),
        config_path: root.path().join("runtime/config/bots/10001.json"),
        backend_kind: BackendKind::Local,
        flavor: ncd_core::BotFlavor::NapCat,
        runtime_target: RuntimeTarget::Local,
        launch_command: vec!["rustc".to_string(), "--version".to_string()],
        working_dir: None,
        log_path: None,
        environment: BTreeMap::new(),
    };

    let synced = backend
        .sync_runtime_config(BotId::new("10001"), &cfg)
        .await
        .unwrap();

    assert!(synced.log_path.is_some());
    let loaded = backend.read_config(BotId::new("10001")).await.unwrap();
    assert_eq!(loaded.bot_id.as_str(), "10001");
    assert!(loaded.log_path.is_some());
}

#[tokio::test]
async fn runtime_backend_keeps_log_buffer_and_tail_consistent() {
    let root = tempdir().unwrap();
    let backend = LocalRuntimeBackend::new(root.path(), "backend-1");
    let bot_id = BotId::new("10002");

    backend.append_log_line(&bot_id, "first").await.unwrap();
    backend.append_log_line(&bot_id, "second").await.unwrap();
    backend.append_log_line(&bot_id, "third").await.unwrap();

    let snapshot = backend.tail_log(bot_id.clone(), TailOpts { lines: 2 }).await.unwrap();
    assert_eq!(snapshot.lines, vec!["second".to_string(), "third".to_string()]);
    assert_eq!(snapshot.total_lines, 3);
}

#[tokio::test]
async fn event_bus_and_bot_actor_stay_serializable() {
    let bus = BroadcastEventBus::default();
    let mut subscription = bus.subscribe(EventFilter::kind(ncd_core::DomainEventKind::TaskProgress));

    bus.publish(DomainEvent::task_progress("p1-demo", 80, "almost done"));
    let event = subscription.next().await.expect("expected task progress");
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("task_progress"));
    assert!(json.contains("almost done"));
}

#[tokio::test]
async fn bot_actor_snapshot_round_trip_and_running_state_are_stable() {
    let actor = BotActorHandle::spawn("10003");
    let starting = actor.request_start().await.unwrap();
    assert_eq!(starting.state, BotActorState::Starting);
    let running = actor.confirm_running().await.unwrap();
    assert_eq!(running.state, BotActorState::Running);
    assert!(running.state.is_active());
    let _ = actor.shutdown().await.unwrap();
}

#[tokio::test]
async fn runtime_backend_start_and_stop_updates_status() {
    let root = tempdir().unwrap();
    let backend = LocalRuntimeBackend::new(root.path(), "backend-1");
    let cfg = BotRuntimeConfig {
        bot_id: BotId::new("10004"),
        config_path: root.path().join("runtime/config/bots/10004.json"),
        backend_kind: BackendKind::Local,
        flavor: ncd_core::BotFlavor::NapCat,
        runtime_target: RuntimeTarget::Local,
        launch_command: vec!["rustc".to_string(), "--version".to_string()],
        working_dir: None,
        log_path: None,
        environment: BTreeMap::new(),
    };

    let started = backend.start(&BotStartCtx { config: cfg }).await.unwrap();
    assert_eq!(started.state, BotActorState::Running);
    let status = backend.status(BotId::new("10004")).await.unwrap();
    assert_eq!(status.state, BotActorState::Running);
    backend.stop(BotId::new("10004"), ncd_core::StopMode::Force).await.unwrap();
    let status = backend.status(BotId::new("10004")).await.unwrap();
    assert_eq!(status.state, BotActorState::Stopped);
}
