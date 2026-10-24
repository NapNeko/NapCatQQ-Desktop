use super::*;

#[tokio::test]
async fn stop_errors_restore_a_refreshable_state_without_claiming_exit() {
    for (index, failure) in [
        "pre-status",
        "post-status",
        "stop",
        "still-running",
        "panic",
    ]
    .into_iter()
    .enumerate()
    {
        let temp = ncd_test_support::TempWorkspace::new().unwrap();
        let (_, _, backend, manager) = make_manager(temp.path());
        let id = if failure == "panic" {
            19997
        } else {
            10300 + index as u64
        };
        let bot = BotId::new(id.to_string());
        manager
            .upsert_bot_config(bot_config(id, "stop recovery"))
            .await
            .unwrap();
        manager.start_bot(&bot).await.unwrap();
        match failure {
            "pre-status" => backend.status_failures.lock().await.push_back(true),
            "post-status" => backend.status_failures.lock().await.extend([false, true]),
            "stop" => backend.fail_next_stop(bot.clone()).await,
            "still-running" => {
                backend
                    .leave_running_after_stop
                    .lock()
                    .await
                    .insert(bot.clone());
            }
            _ => {}
        }
        assert!(manager.stop_bot(&bot).await.is_err(), "case={failure}");
        let restored = manager.get_snapshot(&bot).await.unwrap();
        assert_eq!(restored.state, BotActorState::Running, "case={failure}");
        assert!(
            restored
                .last_error
                .as_deref()
                .unwrap()
                .contains("停止未完成")
        );
        let refreshed = manager.refresh_statuses().await.unwrap();
        assert!(refreshed.succeeded.contains(&bot), "case={failure}");
        let expected = if failure == "post-status" {
            BotActorState::Stopped
        } else {
            BotActorState::Running
        };
        assert_eq!(manager.get_snapshot(&bot).await.unwrap().state, expected);
    }
}

#[tokio::test(start_paused = true)]
async fn stop_timeout_cancels_the_operation_and_releases_the_gate_for_a_retry() {
    let temp = ncd_test_support::TempWorkspace::new().unwrap();
    let (_, _, backend, manager) = make_manager(temp.path());
    let bot = BotId::new("10310");
    manager
        .upsert_bot_config(bot_config(10310, "stop timeout"))
        .await
        .unwrap();
    manager.start_bot(&bot).await.unwrap();
    let release = backend.block_next_stop().await;
    let error = manager.stop_bot(&bot).await.unwrap_err();
    assert!(error.to_string().contains("超时"));
    assert_eq!(
        manager.get_snapshot(&bot).await.unwrap().state,
        BotActorState::Running
    );
    assert!(
        release.send(()).is_err(),
        "timed-out stop future must not survive to mutate runtime later"
    );
    assert!(backend.running.lock().await.contains(&bot));
    let retried = tokio::time::timeout(std::time::Duration::from_secs(5), manager.stop_bot(&bot))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(retried.state, BotActorState::Stopped);
}
