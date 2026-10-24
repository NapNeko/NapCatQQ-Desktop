use super::*;

async fn wait_for<F: std::future::Future<Output = bool>>(mut ready: impl FnMut() -> F) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !ready().await {
        assert!(
            std::time::Instant::now() < deadline,
            "lifecycle condition did not converge"
        );
        tokio::task::yield_now().await;
    }
}

#[tokio::test]
async fn cancelling_start_waits_for_its_result_and_confirms_the_process_is_stopped() {
    let temp = ncd_test_support::TempWorkspace::new().unwrap();
    let (_, _, backend, manager) = make_manager(temp.path());
    let bot = BotId::new("10400");
    manager
        .upsert_bot_config(bot_config(10400, "start-stop"))
        .await
        .unwrap();
    let (release, entered) = backend.block_next_start().await;
    let start_manager = manager.clone();
    let start_bot = bot.clone();
    let start = tokio::spawn(async move { start_manager.start_bot(&start_bot).await });
    entered.await.unwrap();
    let stop_manager = manager.clone();
    let stop_bot = bot.clone();
    let stop = tokio::spawn(async move { stop_manager.stop_bot(&stop_bot).await });
    wait_for(|| async {
        manager.get_snapshot(&bot).await.unwrap().state == BotActorState::Stopping
    })
    .await;
    assert_eq!(backend.stop_count(bot.clone()).await, 0);
    release.send(()).unwrap();
    assert!(matches!(
        start.await.unwrap(),
        Err(BotManagerError::Cancelled)
    ));
    assert_eq!(stop.await.unwrap().unwrap().state, BotActorState::Stopped);
    assert!(!backend.running.lock().await.contains(&bot));
    assert_eq!(backend.stop_count(bot.clone()).await, 1);
    assert_eq!(
        manager.start_bot(&bot).await.unwrap().state,
        BotActorState::Running
    );
}

#[tokio::test]
async fn restart_errors_restore_a_refreshable_state_without_starting_again() {
    for failure in ["status", "stop", "still-running"] {
        let temp = ncd_test_support::TempWorkspace::new().unwrap();
        let (_, _, backend, manager) = make_manager(temp.path());
        let bot = BotId::new("10401");
        manager
            .upsert_bot_config(bot_config(10401, "restart recovery"))
            .await
            .unwrap();
        manager.start_bot(&bot).await.unwrap();
        match failure {
            "status" => backend.status_failures.lock().await.push_back(true),
            "stop" => backend.fail_next_stop(bot.clone()).await,
            _ => {
                backend
                    .leave_running_after_stop
                    .lock()
                    .await
                    .insert(bot.clone());
            }
        }
        assert!(manager.restart_bot(&bot).await.is_err(), "case={failure}");
        let snapshot = manager.get_snapshot(&bot).await.unwrap();
        assert_eq!(snapshot.state, BotActorState::Running, "case={failure}");
        assert!(!snapshot.pending_restart);
        assert!(snapshot.last_error.is_some());
        assert_eq!(backend.start_count(bot.clone()).await, 1);
        assert!(manager.refresh_statuses().await.unwrap().failed.is_empty());
        backend.leave_running_after_stop.lock().await.remove(&bot);
        assert_eq!(
            manager.restart_bot(&bot).await.unwrap().state,
            BotActorState::Running
        );
        assert_eq!(backend.start_count(bot).await, 2);
    }
}

#[tokio::test(start_paused = true)]
async fn a_timed_out_start_retains_its_backend_future_and_cleans_late_success() {
    let temp = ncd_test_support::TempWorkspace::new().unwrap();
    let (_, _, backend, manager) = make_manager(temp.path());
    let bot = BotId::new("10402");
    manager
        .upsert_bot_config(bot_config(10402, "late start"))
        .await
        .unwrap();
    let (release, entered) = backend.block_next_start().await;
    let start_manager = manager.clone();
    let start_bot = bot.clone();
    let start = tokio::spawn(async move { start_manager.start_bot(&start_bot).await });
    entered.await.unwrap();
    tokio::time::advance(std::time::Duration::from_secs(181)).await;
    assert!(
        start
            .await
            .unwrap()
            .unwrap_err()
            .to_string()
            .contains("超时")
    );
    assert_eq!(
        manager.get_snapshot(&bot).await.unwrap().state,
        BotActorState::Crashed
    );
    release
        .send(())
        .expect("timeout must retain the side-effecting backend future");
    wait_for(|| async {
        backend.stop_count(bot.clone()).await == 1 && !backend.running.lock().await.contains(&bot)
    })
    .await;
    assert_eq!(
        manager.get_snapshot(&bot).await.unwrap().state,
        BotActorState::Crashed
    );
    assert!(manager.refresh_statuses().await.unwrap().failed.is_empty());
    assert_eq!(
        manager.get_snapshot(&bot).await.unwrap().state,
        BotActorState::Stopped
    );
    assert_eq!(
        manager.start_bot(&bot).await.unwrap().state,
        BotActorState::Running
    );
}

#[tokio::test(start_paused = true)]
async fn a_replacement_start_waits_for_cancelled_generation_cleanup() {
    let temp = ncd_test_support::TempWorkspace::new().unwrap();
    let (_, _, backend, manager) = make_manager(temp.path());
    let bot = BotId::new("10403");
    manager
        .upsert_bot_config(bot_config(10403, "replacement start"))
        .await
        .unwrap();
    let (release_old, old_entered) = backend.block_next_start().await;
    let old_manager = manager.clone();
    let old_bot = bot.clone();
    let old_start = tokio::spawn(async move { old_manager.start_bot(&old_bot).await });
    old_entered.await.unwrap();
    let stop_manager = manager.clone();
    let stop_bot = bot.clone();
    let stop = tokio::spawn(async move { stop_manager.stop_bot(&stop_bot).await });
    wait_for(|| async {
        manager.get_snapshot(&bot).await.unwrap().state == BotActorState::Stopping
    })
    .await;
    tokio::time::advance(std::time::Duration::from_secs(31)).await;
    assert!(stop.await.unwrap().is_err());
    let (release_new, new_entered) = backend.block_next_start().await;
    let new_manager = manager.clone();
    let new_bot = bot.clone();
    let new_start = tokio::spawn(async move { new_manager.start_bot(&new_bot).await });
    wait_for(|| async {
        manager.get_snapshot(&bot).await.unwrap().state == BotActorState::Starting
    })
    .await;
    let replacement = manager.get_snapshot(&bot).await.unwrap();
    assert_eq!(backend.start_count(bot.clone()).await, 1);
    release_old.send(()).unwrap();
    assert!(matches!(
        old_start.await.unwrap(),
        Err(BotManagerError::Cancelled)
    ));
    new_entered.await.unwrap();
    assert_eq!(backend.stop_count(bot.clone()).await, 1);
    assert_eq!(manager.get_snapshot(&bot).await.unwrap(), replacement);
    release_new.send(()).unwrap();
    assert_eq!(
        new_start.await.unwrap().unwrap().state,
        BotActorState::Running
    );
    assert_eq!(backend.start_count(bot).await, 2);
}

#[tokio::test]
async fn stopping_a_crashed_actor_clears_error_only_after_confirming_exit() {
    let temp = ncd_test_support::TempWorkspace::new().unwrap();
    let (_, _, backend, manager) = make_manager(temp.path());
    let bot = BotId::new("10404");
    manager
        .upsert_bot_config(bot_config(10404, "uncertain start"))
        .await
        .unwrap();
    backend.fail_next_start(bot.clone()).await;
    manager.start_bot(&bot).await.unwrap_err();
    backend.running.lock().await.insert(bot.clone());
    let stopped = manager.stop_bot(&bot).await.unwrap();
    assert_eq!(stopped.state, BotActorState::Stopped);
    assert!(stopped.last_error.is_none());
    assert!(!backend.running.lock().await.contains(&bot));
}
