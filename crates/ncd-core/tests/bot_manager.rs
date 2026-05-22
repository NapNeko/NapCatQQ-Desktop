use std::sync::Arc;

use ncd_core::{
    AdvancedConfig, AutoRestartSchedule, BackendType, BotActorState, BotBasicConfig, BotConfig,
    BotConfigRepo, BotId, BotManager, BotManagerError, BroadcastEventBus, ConnectConfig,
    EventBus, EventFilter, LocalBotConfigRepo, LocalConfigStore, NapCatConfigRenderer,
    RuntimeTarget, SecretStore, SecretStoreImpl,
};

fn bot_config(qq_id: u64, name: &str) -> BotConfig {
    BotConfig {
        bot: BotBasicConfig {
            name: name.to_string(),
            qq_id,
            music_sign_url: String::new(),
            auto_restart_schedule: AutoRestartSchedule::default(),
            offline_auto_restart: false,
            runtime_target: RuntimeTarget::Local,
            backend_type: BackendType::NapCat,
        },
        connect: ConnectConfig::default(),
        advanced: AdvancedConfig::default(),
    }
}

fn bot_config_auto_start(qq_id: u64, name: &str) -> BotConfig {
    let mut config = bot_config(qq_id, name);
    config.advanced.auto_start = true;
    config
}

fn make_manager(
    root: &std::path::Path,
) -> (
    Arc<LocalConfigStore>,
    Arc<LocalBotConfigRepo<LocalConfigStore>>,
    BotManager<LocalBotConfigRepo<LocalConfigStore>, LocalConfigStore>,
) {
    let store = Arc::new(LocalConfigStore::new(root));
    let secrets: Arc<dyn SecretStore + Send + Sync> = Arc::new(
        SecretStoreImpl::new_with_force_fallback(root.join("secrets"), true),
    );
    let repo = Arc::new(LocalBotConfigRepo::new(Arc::clone(&store), secrets));
    let renderer = Arc::new(NapCatConfigRenderer::new(root.join("napcat_config")));
    let event_bus = Arc::new(BroadcastEventBus::default());
    let manager = BotManager::new(
        Arc::clone(&repo),
        Arc::clone(&store),
        renderer,
        event_bus,
    );
    (store, repo, manager)
}

// ─── 4 开上限 ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn upsert_enforces_4_bot_limit() {
    let temp = tempfile::tempdir().unwrap();
    let (_, _, manager) = make_manager(temp.path());

    for i in 1..=4 {
        manager
            .upsert_bot_config(bot_config(10000 + i, &format!("bot-{i}")))
            .await
            .unwrap();
    }

    assert_eq!(manager.bot_count().await, 4);

    let err = manager
        .upsert_bot_config(bot_config(10005, "bot-5"))
        .await
        .unwrap_err();

    assert!(matches!(err, BotManagerError::BotLimitReached));
    assert_eq!(manager.bot_count().await, 4);
}

#[tokio::test]
async fn upsert_existing_bot_does_not_count_toward_limit() {
    let temp = tempfile::tempdir().unwrap();
    let (_, _, manager) = make_manager(temp.path());

    for i in 1..=4 {
        manager
            .upsert_bot_config(bot_config(10000 + i, &format!("bot-{i}")))
            .await
            .unwrap();
    }

    // 更新已有的 Bot 不应触发上限
    manager
        .upsert_bot_config(bot_config(10001, "bot-1-updated"))
        .await
        .unwrap();

    assert_eq!(manager.bot_count().await, 4);
}

// ─── 启停状态流转 ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn start_and_stop_transitions() {
    let temp = tempfile::tempdir().unwrap();
    let (_, _, manager) = make_manager(temp.path());

    let bot_id = BotId::new("10001");
    manager.upsert_bot_config(bot_config(10001, "bot")).await.unwrap();

    // 初始状态 Stopped
    let snap = manager.get_snapshot(&bot_id).await.unwrap();
    assert_eq!(snap.state, BotActorState::Stopped);

    // 启动 → Starting
    let snap = manager.start_bot(&bot_id).await.unwrap();
    assert_eq!(snap.state, BotActorState::Starting);

    // 停止 → Stopping
    let snap = manager.stop_bot(&bot_id).await.unwrap();
    assert_eq!(snap.state, BotActorState::Stopping);
}

#[tokio::test]
async fn start_nonexistent_bot_returns_not_found() {
    let temp = tempfile::tempdir().unwrap();
    let (_, _, manager) = make_manager(temp.path());

    let err = manager.start_bot(&BotId::new("99999")).await.unwrap_err();
    assert!(matches!(err, BotManagerError::BotNotFound(_)));
}

// ─── 批量并发 ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn batch_start_starts_multiple_bots_concurrently() {
    let temp = tempfile::tempdir().unwrap();
    let (_, _, manager) = make_manager(temp.path());

    let ids: Vec<BotId> = (1..=3)
        .map(|i| {
            BotId::new(format!("{}", 10000 + i))
        })
        .collect();

    for i in 1..=3u64 {
        manager
            .upsert_bot_config(bot_config(10000 + i, &format!("bot-{i}")))
            .await
            .unwrap();
    }

    let result = manager.batch_start(&ids).await.unwrap();
    assert_eq!(result.succeeded.len(), 3);
    assert!(result.failed.is_empty());

    // 所有 bot 都进入 Starting
    for id in &ids {
        let snap = manager.get_snapshot(id).await.unwrap();
        assert_eq!(snap.state, BotActorState::Starting);
    }
}

#[tokio::test]
async fn batch_stop_stops_running_bots() {
    let temp = tempfile::tempdir().unwrap();
    let (_, _, manager) = make_manager(temp.path());

    let ids: Vec<BotId> = (1..=2)
        .map(|i| BotId::new(format!("{}", 10000 + i)))
        .collect();

    for i in 1..=2u64 {
        manager
            .upsert_bot_config(bot_config(10000 + i, &format!("bot-{i}")))
            .await
            .unwrap();
    }

    // 先启动
    manager.batch_start(&ids).await.unwrap();

    // 再停止
    let result = manager.batch_stop(&ids).await.unwrap();
    assert_eq!(result.succeeded.len(), 2);
    assert!(result.failed.is_empty());

    for id in &ids {
        let snap = manager.get_snapshot(id).await.unwrap();
        assert_eq!(snap.state, BotActorState::Stopping);
    }
}

// ─── 配置变更热推送 ──────────────────────────────────────────────────────────

#[tokio::test]
async fn upsert_running_bot_triggers_restart() {
    let temp = tempfile::tempdir().unwrap();
    let (_, _, manager) = make_manager(temp.path());

    let bot_id = BotId::new("10001");
    manager.upsert_bot_config(bot_config(10001, "bot")).await.unwrap();

    // 启动 → Starting
    manager.start_bot(&bot_id).await.unwrap();
    let snap = manager.get_snapshot(&bot_id).await.unwrap();
    assert_eq!(snap.state, BotActorState::Starting);

    // 由于我们没有直接暴露 actor handle 的 confirm_running，
    // 我们验证 Starting 状态下 upsert 也会触发 restart
    let snap = manager
        .upsert_bot_config(bot_config(10001, "bot-updated"))
        .await
        .unwrap();

    // Starting 状态 + request_restart → Stopping (pending_restart=true)
    assert_eq!(snap.state, BotActorState::Stopping);
    assert!(snap.pending_restart);
}

#[tokio::test]
async fn upsert_stopped_bot_does_not_restart() {
    let temp = tempfile::tempdir().unwrap();
    let (_, _, manager) = make_manager(temp.path());

    manager.upsert_bot_config(bot_config(10001, "bot")).await.unwrap();

    // Bot 是 Stopped，更新配置不应触发 restart
    let snap = manager
        .upsert_bot_config(bot_config(10001, "bot-updated"))
        .await
        .unwrap();
    assert_eq!(snap.state, BotActorState::Stopped);
}

// ─── 自动启动 ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn bootstrap_auto_starts_marked_bots() {
    let temp = tempfile::tempdir().unwrap();
    let (_, repo, manager) = make_manager(temp.path());

    // 先通过 repo 直接写入配置（模拟已有持久化数据）
    repo.upsert(bot_config_auto_start(10001, "auto-bot")).await.unwrap();
    repo.upsert(bot_config(10002, "manual-bot")).await.unwrap();

    let result = manager.bootstrap().await.unwrap();

    // 没有超出上限的 bot
    assert!(result.skipped.is_empty());

    // 只有 auto_start=true 的 bot 被启动
    assert_eq!(result.started.succeeded.len(), 1);
    assert_eq!(result.started.succeeded[0], BotId::new("10001"));

    // 两个 bot 都被注册为 actor
    assert_eq!(manager.bot_count().await, 2);

    // auto-bot 进入 Starting
    let snap = manager.get_snapshot(&BotId::new("10001")).await.unwrap();
    assert_eq!(snap.state, BotActorState::Starting);

    // manual-bot 保持 Stopped
    let snap = manager.get_snapshot(&BotId::new("10002")).await.unwrap();
    assert_eq!(snap.state, BotActorState::Stopped);
}

#[tokio::test]
async fn bootstrap_respects_4_bot_limit_and_reports_skipped() {
    let temp = tempfile::tempdir().unwrap();
    let (_, repo, manager) = make_manager(temp.path());

    for i in 1..=6u64 {
        repo.upsert(bot_config(10000 + i, &format!("bot-{i}"))).await.unwrap();
    }

    let result = manager.bootstrap().await.unwrap();

    // 最多只注册 4 个 actor
    assert_eq!(manager.bot_count().await, 4);

    // 超出的 2 个 bot 被记入 skipped
    assert_eq!(result.skipped.len(), 2);
    assert_eq!(result.skipped[0], BotId::new("10005"));
    assert_eq!(result.skipped[1], BotId::new("10006"));
}

// ─── 删除 ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn delete_bot_removes_actor_and_config() {
    let temp = tempfile::tempdir().unwrap();
    let (_, repo, manager) = make_manager(temp.path());

    manager.upsert_bot_config(bot_config(10001, "bot")).await.unwrap();
    assert_eq!(manager.bot_count().await, 1);

    manager.delete_bot_config(&BotId::new("10001")).await.unwrap();

    assert_eq!(manager.bot_count().await, 0);
    assert_eq!(repo.get(10001).await.unwrap(), None);
}

#[tokio::test]
async fn batch_delete_stops_and_removes_bots() {
    let temp = tempfile::tempdir().unwrap();
    let (_, repo, manager) = make_manager(temp.path());

    for i in 1..=3u64 {
        manager
            .upsert_bot_config(bot_config(10000 + i, &format!("bot-{i}")))
            .await
            .unwrap();
    }

    // 启动前两个
    let ids: Vec<BotId> = vec![BotId::new("10001"), BotId::new("10002")];
    manager.batch_start(&ids).await.unwrap();

    // 批量删除所有 3 个
    let all_ids: Vec<BotId> = (1..=3).map(|i| BotId::new(format!("{}", 10000 + i))).collect();
    let result = manager.batch_delete(&all_ids).await.unwrap();

    assert_eq!(result.succeeded.len(), 3);
    assert!(result.failed.is_empty());
    assert_eq!(manager.bot_count().await, 0);
    assert_eq!(repo.count().await.unwrap(), 0);
}

// ─── 事件广播 ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn start_bot_publishes_state_change_event() {
    let temp = tempfile::tempdir().unwrap();
    let store = Arc::new(LocalConfigStore::new(temp.path()));
    let secrets: Arc<dyn SecretStore + Send + Sync> = Arc::new(
        SecretStoreImpl::new_with_force_fallback(temp.path().join("secrets"), true),
    );
    let repo = Arc::new(LocalBotConfigRepo::new(Arc::clone(&store), secrets));
    let renderer = Arc::new(NapCatConfigRenderer::new(temp.path().join("napcat_config")));
    let event_bus = Arc::new(BroadcastEventBus::default());

    // 先订阅，再操作
    let mut sub = event_bus.subscribe(EventFilter::bot("10001"));

    let manager = BotManager::new(
        Arc::clone(&repo),
        Arc::clone(&store),
        renderer,
        Arc::clone(&event_bus),
    );

    manager.upsert_bot_config(bot_config(10001, "bot")).await.unwrap();
    manager.start_bot(&BotId::new("10001")).await.unwrap();

    // 应该收到 bot_created 和 start_requested 两个事件
    let event1 = sub.next().await.expect("expected bot_created event");
    let event2 = sub.next().await.expect("expected start_requested event");

    // 验证事件属于正确的 bot
    assert_eq!(event1.bot_id().unwrap().as_str(), "10001");
    assert_eq!(event2.bot_id().unwrap().as_str(), "10001");
}

// ─── list_snapshots ───────────────────────────────────────────────────────────

#[tokio::test]
async fn list_snapshots_returns_all_actors() {
    let temp = tempfile::tempdir().unwrap();
    let (_, _, manager) = make_manager(temp.path());

    for i in 1..=3u64 {
        manager
            .upsert_bot_config(bot_config(10000 + i, &format!("bot-{i}")))
            .await
            .unwrap();
    }

    let snapshots = manager.list_snapshots().await;
    assert_eq!(snapshots.len(), 3);
}

// ─── 持久化一致性回归 ─────────────────────────────────────────────────────────

#[tokio::test]
async fn upsert_persists_before_creating_actor() {
    // 回归测试：upsert 先写 bot.json 再创建 Actor。
    // 验证：upsert 成功后，repo 里已有数据。
    let temp = tempfile::tempdir().unwrap();
    let (_, repo, manager) = make_manager(temp.path());

    manager.upsert_bot_config(bot_config(10001, "bot")).await.unwrap();

    // repo 中必须能读到
    let stored = repo.get(10001).await.unwrap();
    assert!(stored.is_some());
    assert_eq!(stored.unwrap().bot.name, "bot");
}

#[tokio::test]
async fn delete_persists_before_removing_actor() {
    // 回归测试：delete 先删 bot.json 再清理 Actor。
    // 验证：即使 Actor 还在（假设 shutdown 慢），repo 里已无数据。
    let temp = tempfile::tempdir().unwrap();
    let (_, repo, manager) = make_manager(temp.path());

    manager.upsert_bot_config(bot_config(10001, "bot")).await.unwrap();
    manager.delete_bot_config(&BotId::new("10001")).await.unwrap();

    // 持久化已删
    assert_eq!(repo.get(10001).await.unwrap(), None);
    // Actor 也已清除
    assert_eq!(manager.bot_count().await, 0);
}

#[tokio::test]
async fn bootstrap_skipped_bots_are_not_auto_started() {
    // 回归测试：超出上限且标记 auto_start 的 bot 不会被启动。
    let temp = tempfile::tempdir().unwrap();
    let (_, repo, manager) = make_manager(temp.path());

    // 前 4 个不自动启动，第 5 个标记 auto_start
    for i in 1..=4u64 {
        repo.upsert(bot_config(10000 + i, &format!("bot-{i}"))).await.unwrap();
    }
    repo.upsert(bot_config_auto_start(10005, "auto-skipped")).await.unwrap();

    let result = manager.bootstrap().await.unwrap();

    // 第 5 个被跳过
    assert_eq!(result.skipped.len(), 1);
    assert_eq!(result.skipped[0], BotId::new("10005"));

    // 没有 bot 被自动启动（前 4 个不是 auto_start，第 5 个被 skip）
    assert_eq!(result.started.succeeded.len(), 0);
    assert_eq!(manager.bot_count().await, 4);
}
