use super::*;

async fn bound() -> (Arc<RemoteSnowLumaBackend>, Arc<MetricsHost>, BotId) {
    let (backend, host, _) = fixture();
    let bot = BotId::new("10001");
    backend.login_pids.lock().await.insert(
        bot.clone(),
        TrackedQq {
            pid: 42,
            start_time: "12345".into(),
        },
    );
    backend
        .start_modes
        .lock()
        .await
        .insert(bot.clone(), SnowLumaStartMode::ColdStart);
    (backend, host, bot)
}

#[tokio::test]
async fn attributed_pending_qq_status_uses_exact_host_identity_without_account_scan() {
    let (backend, host, bot) = bound().await;
    let status = backend.status(bot).await.unwrap();
    assert_eq!(status.pid, Some(42));
    assert_eq!(
        host.qq_operations.lock().unwrap().as_slice(),
        &[("inspect".into(), false, 42, "12345".into())]
    );
    assert!(
        host.commands
            .lock()
            .unwrap()
            .iter()
            .all(|command| !command.contains("os.scandir(\"/proc\")"))
    );
}

#[tokio::test]
async fn status_permission_elevates_only_bound_pid_then_rechecks_original_user_snapshot() {
    let (backend, host, bot) = bound().await;
    host.qq_replies.lock().unwrap().extend([
        "permission".into(),
        "verified".into(),
        "verified".into(),
    ]);
    assert_eq!(backend.status(bot).await.unwrap().pid, Some(42));
    let operations = host.qq_operations.lock().unwrap();
    assert_eq!(
        operations
            .iter()
            .map(|item| (item.0.as_str(), item.1))
            .collect::<Vec<_>>(),
        [("inspect", false), ("inspect", true), ("snapshot", false)]
    );
    assert!(
        operations
            .iter()
            .all(|item| item.2 == 42 && item.3 == "12345")
    );
}

#[tokio::test]
async fn replaced_binding_is_an_error_and_cannot_fall_back_to_an_unrelated_process() {
    let (backend, host, bot) = bound().await;
    host.qq_replies.lock().unwrap().push_back("replaced".into());
    assert!(backend.status(bot.clone()).await.is_err());
    assert_eq!(host.qq_operations.lock().unwrap().len(), 1);
    assert!(backend.login_pids.lock().await.contains_key(&bot));
}

#[tokio::test]
async fn confirmed_stop_removes_binding_only_after_exact_process_confirmation() {
    let (backend, host, bot) = bound().await;
    backend.stop(bot.clone(), StopMode::Force).await.unwrap();
    assert!(!backend.login_pids.lock().await.contains_key(&bot));
    assert!(!backend.start_modes.lock().await.contains_key(&bot));
    assert_eq!(
        host.qq_operations.lock().unwrap().as_slice(),
        &[("stop".into(), false, 42, "12345".into())]
    );
    assert!(
        host.commands
            .lock()
            .unwrap()
            .iter()
            .all(|script| !script.contains("pidfile=\"$RUNTIME_DIR/pid_bot_"))
    );
}

#[tokio::test]
async fn failed_stop_preserves_pid_birth_and_start_mode_for_retry() {
    let (backend, host, bot) = bound().await;
    host.qq_replies
        .lock()
        .unwrap()
        .extend(["permission".into(), "error".into()]);
    assert!(backend.stop(bot.clone(), StopMode::Force).await.is_err());
    assert_eq!(
        backend
            .login_pids
            .lock()
            .await
            .get(&bot)
            .unwrap()
            .start_time,
        "12345"
    );
    assert_eq!(
        backend.start_modes.lock().await.get(&bot),
        Some(&SnowLumaStartMode::ColdStart)
    );
    let operations = host.qq_operations.lock().unwrap();
    assert_eq!(operations.len(), 2);
    assert!(!operations[0].1 && operations[1].1);
    assert!(
        operations
            .iter()
            .all(|item| item.0 == "stop" && item.2 == 42 && item.3 == "12345")
    );
}

#[tokio::test]
async fn reused_pid_cannot_be_stopped_or_clear_the_old_binding() {
    let (backend, host, bot) = bound().await;
    host.qq_replies.lock().unwrap().push_back("replaced".into());
    assert!(backend.stop(bot.clone(), StopMode::Force).await.is_err());
    assert_eq!(host.qq_operations.lock().unwrap().len(), 1);
    assert!(backend.login_pids.lock().await.contains_key(&bot));
}

#[tokio::test(start_paused = true)]
async fn status_has_a_total_budget_even_if_a_host_ignores_command_timeout() {
    let (backend, host, bot) = bound().await;
    host.qq_block.store(true, Ordering::SeqCst);
    let began = tokio::time::Instant::now();
    assert!(backend.status(bot).await.is_err());
    assert_eq!(began.elapsed(), Duration::from_secs(15));
}

#[tokio::test(start_paused = true)]
async fn stop_total_budget_keeps_the_verified_binding_for_a_later_retry() {
    let (backend, host, bot) = bound().await;
    host.qq_block.store(true, Ordering::SeqCst);
    let began = tokio::time::Instant::now();
    assert!(backend.stop(bot.clone(), StopMode::Force).await.is_err());
    assert_eq!(began.elapsed(), Duration::from_secs(25));
    assert!(backend.login_pids.lock().await.contains_key(&bot));
}

#[tokio::test]
async fn cold_start_reuses_the_verified_pid_without_reading_a_pidfile_or_spawning() {
    let (_, host, _) = bound().await;
    let paths = SnowLumaRemotePaths::from_workspace_and_dir("/opt/sl", "/opt/sl");
    let layout = RemoteSnowLumaLayout {
        home: "/home/test".into(),
        paths,
        node_bin: "/opt/sl/node".into(),
        qq_bin: "/opt/QQ/qq".into(),
        qq_install_base: "/".into(),
    };
    let pid = super::super::super::orchestrator::bot_cold_start_with_login(
        host.as_ref(),
        &layout,
        "10001",
        "10001",
        None,
        Some(42),
    )
    .await
    .unwrap();
    assert_eq!(pid, 42);
    assert!(host.commands.lock().unwrap().is_empty());
}

#[tokio::test(start_paused = true)]
async fn warm_pid_refresh_uses_fresh_birth_and_bound_http_target_without_sudo_or_scan() {
    let (backend, host, bot) = bound().await;
    let client = install_reusable_poller(&backend, &bot).await;
    assert_eq!(
        backend.reconcile_running_pid(&bot, true).await.unwrap(),
        Some(42)
    );
    assert!(client.requests.load(Ordering::SeqCst) > 0);
    assert!(host.commands.lock().unwrap().is_empty());
    backend.detach_login_pollers().await;
}

#[tokio::test(start_paused = true)]
async fn warm_cached_client_cannot_hide_a_changed_qq_birth() {
    let (backend, host, bot) = bound().await;
    install_reusable_poller(&backend, &bot).await;
    *host.qq_start_time.lock().unwrap() = Some("67890".into());
    host.qq_replies.lock().unwrap().push_back("replaced".into());
    assert!(backend.reconcile_running_pid(&bot, true).await.is_err());
    assert_eq!(host.qq_operations.lock().unwrap().len(), 1);
    backend.detach_login_pollers().await;
}

#[tokio::test(start_paused = true)]
async fn service_supervision_rejection_keeps_working_login_poller_and_process_binding() {
    let (backend, host, bot) = bound().await;
    install_reusable_poller(&backend, &bot).await;
    host.qq_replies
        .lock()
        .unwrap()
        .push_back("managed_service".into());
    let error = backend
        .stop(bot.clone(), StopMode::Force)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("系统服务"));
    assert!(backend.login_pids.lock().await.contains_key(&bot));
    assert!(backend.pollers.lock().await.contains_key(&bot));
    assert!(backend.login_clients.lock().await.contains_key(&bot));
    backend.detach_login_pollers().await;
}

async fn hot_bound() -> (
    Arc<RemoteSnowLumaBackend>,
    Arc<MetricsHost>,
    BotId,
    Arc<ReuseClient>,
) {
    let (backend, host, bot) = bound().await;
    backend
        .start_modes
        .lock()
        .await
        .insert(bot.clone(), SnowLumaStartMode::HotStart);
    let client = install_reusable_poller(&backend, &bot).await;
    client.allow_unload.store(true, Ordering::SeqCst);
    (backend, host, bot, client)
}

#[tokio::test(start_paused = true)]
async fn hot_detach_reports_stopped_and_repeated_stop_never_kills_preserved_qq() {
    let (backend, host, bot, client) = hot_bound().await;
    backend.stop(bot.clone(), StopMode::Force).await.unwrap();
    assert_eq!(client.unloads.load(Ordering::SeqCst), 1);
    assert_eq!(
        backend.status(bot.clone()).await.unwrap().state,
        ncd_domain::bot_actor::BotActorState::Stopped
    );
    assert_eq!(
        backend.reconcile_running_pid(&bot, true).await.unwrap(),
        None
    );
    backend
        .attach_reconciled_running(
            bot.clone(),
            42,
            &ncd_test_support::BotConfigBuilder::new().build(),
        )
        .await
        .unwrap();
    backend.stop(bot.clone(), StopMode::Force).await.unwrap();
    assert_eq!(client.unloads.load(Ordering::SeqCst), 1);
    assert_eq!(host.qq_operations.lock().unwrap().len(), 1);
    assert_eq!(host.qq_operations.lock().unwrap()[0].0, "inspect");
    assert_eq!(backend.login_pids.lock().await.get(&bot).unwrap().pid, 42);
    assert_eq!(
        backend.start_modes.lock().await.get(&bot),
        Some(&SnowLumaStartMode::HotStart)
    );
    assert!(backend.hot_detached.lock().await.contains(&bot));
}

#[tokio::test(start_paused = true)]
async fn rejected_hot_unload_preserves_active_session_and_original_boundary() {
    let (backend, host, bot, client) = hot_bound().await;
    client.failed.store(true, Ordering::SeqCst);
    assert!(backend.stop(bot.clone(), StopMode::Force).await.is_err());
    assert!(!backend.hot_detached.lock().await.contains(&bot));
    assert!(backend.pollers.lock().await.contains_key(&bot));
    assert!(backend.login_clients.lock().await.contains_key(&bot));
    assert_eq!(
        backend.start_modes.lock().await.get(&bot),
        Some(&SnowLumaStartMode::HotStart)
    );
    assert_eq!(backend.status(bot.clone()).await.unwrap().pid, Some(42));
    assert!(
        host.qq_operations
            .lock()
            .unwrap()
            .iter()
            .all(|operation| operation.0 != "stop")
    );
    backend.detach_login_pollers().await;
}

#[tokio::test(start_paused = true)]
async fn explicit_hot_start_resolves_the_same_pid_without_recreating_qq() {
    let (backend, host, gates) = fixture();
    let bot = BotId::new("10001");
    backend
        .start_modes
        .lock()
        .await
        .insert(bot.clone(), SnowLumaStartMode::HotStart);
    let client = install_reusable_poller(&backend, &bot).await;
    client.allow_unload.store(true, Ordering::SeqCst);
    backend.stop(bot.clone(), StopMode::Force).await.unwrap();
    host.commands.lock().unwrap().clear();
    host.qq_operations.lock().unwrap().clear();
    let mut config = ncd_test_support::BotConfigBuilder::new().build();
    config.bot.qq_id = 10001;
    config.bot.snowluma_start_mode = Some(SnowLumaStartMode::HotStart);
    let ctx = BotStartCtx {
        config: ncd_traits::runtime_backend::BotRuntimeConfig::default_path(
            "/fixture",
            bot.clone(),
        )
        .with_bot_config(&config),
        bot_config: Some(config),
    };
    let starting = {
        let backend = Arc::clone(&backend);
        tokio::spawn(async move { backend.start(&ctx).await })
    };
    permit(&gates.entered).await;
    assert_eq!(
        host.qq_operations.lock().unwrap().as_slice(),
        &[("inspect".into(), false, 42, "12345".into())]
    );
    assert_eq!(host.commands.lock().unwrap().len(), 1);
    assert!(backend.hot_detached.lock().await.contains(&bot));
    starting.abort();
    assert!(starting.await.unwrap_err().is_cancelled());
    assert_eq!(
        backend.status(bot.clone()).await.unwrap().state,
        ncd_domain::bot_actor::BotActorState::Stopped
    );
}

#[tokio::test(start_paused = true)]
async fn explicit_hot_start_cannot_adopt_a_reused_or_exited_pid() {
    for state in ["replaced", "absent"] {
        let (backend, host, bot, _) = hot_bound().await;
        backend.stop(bot.clone(), StopMode::Force).await.unwrap();
        host.qq_operations.lock().unwrap().clear();
        host.qq_replies.lock().unwrap().push_back(state.into());
        assert!(backend.explicit_start_pid(&bot).await.is_err());
        assert_eq!(host.qq_operations.lock().unwrap().len(), 1);
        assert!(backend.hot_detached.lock().await.contains(&bot));
        assert_eq!(backend.login_pids.lock().await.get(&bot).unwrap().pid, 42);
        assert_eq!(
            backend.status(bot.clone()).await.unwrap().state,
            ncd_domain::bot_actor::BotActorState::Stopped
        );
    }
}

#[tokio::test(start_paused = true)]
async fn one_reconcile_observation_does_not_repeat_birth_or_http_probe_during_attach() {
    let (backend, host, bot) = bound().await;
    let client = install_reusable_poller(&backend, &bot).await;
    let observation = backend
        .observe_reconciled_running(&bot, true)
        .await
        .unwrap();
    assert_eq!(observation.pid(), Some(42));
    let birth_reads = host.birth_reads.load(Ordering::SeqCst);
    let requests = client.requests.load(Ordering::SeqCst);
    assert_eq!(birth_reads, 1);
    backend
        .attach_reconciled_observation(
            bot.clone(),
            observation,
            &ncd_test_support::BotConfigBuilder::new().build(),
        )
        .await
        .unwrap();
    assert_eq!(host.birth_reads.load(Ordering::SeqCst), birth_reads);
    assert_eq!(client.requests.load(Ordering::SeqCst), requests);
    backend.detach_login_pollers().await;
}

#[tokio::test(start_paused = true)]
async fn observation_for_a_replaced_client_requires_a_new_fresh_probe() {
    let (backend, host, bot) = bound().await;
    install_reusable_poller(&backend, &bot).await;
    let observation = backend
        .observe_reconciled_running(&bot, true)
        .await
        .unwrap();
    let client = install_reusable_poller(&backend, &bot).await;
    let requests = client.requests.load(Ordering::SeqCst);
    let birth_reads = host.birth_reads.load(Ordering::SeqCst);
    backend
        .attach_reconciled_observation(
            bot.clone(),
            observation,
            &ncd_test_support::BotConfigBuilder::new().build(),
        )
        .await
        .unwrap();
    assert_eq!(host.birth_reads.load(Ordering::SeqCst), birth_reads + 1);
    assert_eq!(client.requests.load(Ordering::SeqCst), requests + 1);
    backend.detach_login_pollers().await;
}

#[tokio::test(start_paused = true)]
async fn observation_cannot_cross_a_host_replacement_or_another_bot() {
    let (backend, _, bot) = bound().await;
    install_reusable_poller(&backend, &bot).await;
    let observation = backend
        .observe_reconciled_running(&bot, true)
        .await
        .unwrap();
    let config = ncd_test_support::BotConfigBuilder::new().build();
    assert!(
        backend
            .attach_reconciled_observation(BotId::new("20002"), observation, &config)
            .await
            .is_err()
    );
    let observation = backend
        .observe_reconciled_running(&bot, true)
        .await
        .unwrap();
    backend
        .daemon
        .replace_host_if_current(Arc::new(MetricsHost::default()), async { true })
        .await
        .unwrap();
    assert!(
        backend
            .attach_reconciled_observation(bot, observation, &config)
            .await
            .is_err()
    );
    backend.detach_login_pollers().await;
}

#[tokio::test(start_paused = true)]
async fn earlier_observation_cannot_reattach_a_subsequently_detached_hot_session() {
    let (backend, _, bot) = bound().await;
    let client = install_reusable_poller(&backend, &bot).await;
    let observation = backend
        .observe_reconciled_running(&bot, true)
        .await
        .unwrap();
    let requests = client.requests.load(Ordering::SeqCst);
    backend.hot_detached.lock().await.insert(bot.clone());
    backend
        .attach_reconciled_observation(
            bot.clone(),
            observation,
            &ncd_test_support::BotConfigBuilder::new().build(),
        )
        .await
        .unwrap();
    assert_eq!(client.requests.load(Ordering::SeqCst), requests);
    assert!(backend.hot_detached.lock().await.contains(&bot));
    backend.detach_login_pollers().await;
}

fn start_context(bot: &BotId, mode: SnowLumaStartMode) -> BotStartCtx {
    let mut config = ncd_test_support::BotConfigBuilder::new().build();
    config.bot.qq_id = bot.as_str().parse().unwrap();
    config.bot.snowluma_start_mode = Some(mode);
    BotStartCtx {
        config: ncd_traits::runtime_backend::BotRuntimeConfig::default_path(
            "/fixture",
            bot.clone(),
        )
        .with_bot_config(&config),
        bot_config: Some(config),
    }
}

#[tokio::test(start_paused = true)]
async fn precancelled_start_does_not_issue_any_host_operation_or_spawn() {
    let (backend, host, _) = fixture();
    let bot = BotId::new("10001");
    let cancel = CancellationToken::new();
    cancel.cancel();
    assert!(
        backend
            .start_cancellable(&start_context(&bot, SnowLumaStartMode::ColdStart), cancel)
            .await
            .is_err()
    );
    assert!(host.commands.lock().unwrap().is_empty());
    assert!(backend.pending_spawns.lock().unwrap().is_empty());
}

#[tokio::test(start_paused = true)]
async fn cancellation_waits_for_the_inflight_metrics_mutation_and_then_cannot_spawn_qq() {
    let (backend, host, gates) = fixture();
    let bot = BotId::new("10001");
    let cancel = CancellationToken::new();
    let task = {
        let backend = Arc::clone(&backend);
        let cancel = cancel.clone();
        let ctx = start_context(&bot, SnowLumaStartMode::ColdStart);
        tokio::spawn(async move { backend.start_cancellable(&ctx, cancel).await })
    };
    permit(&gates.entered).await;
    cancel.cancel();
    tokio::task::yield_now().await;
    assert!(
        !task.is_finished(),
        "a submitted mutating phase must be allowed to finish"
    );
    gates.release.add_permits(1);
    assert!(task.await.unwrap().is_err());
    assert_eq!(
        gates.cancelled.available_permits(),
        0,
        "the mutation future was not dropped"
    );
    assert!(
        host.commands
            .lock()
            .unwrap()
            .iter()
            .all(|command| !command.contains("nohup"))
    );
    assert!(backend.pending_spawns.lock().unwrap().is_empty());
    assert!(!backend.pollers.lock().await.contains_key(&bot));
}

#[tokio::test(start_paused = true)]
async fn cancelling_hot_start_before_load_detaches_existing_hook_and_preserves_user_qq() {
    let (backend, host, gates) = fixture();
    let bot = BotId::new("10001");
    backend
        .start_modes
        .lock()
        .await
        .insert(bot.clone(), SnowLumaStartMode::HotStart);
    let client = install_reusable_poller(&backend, &bot).await;
    client.allow_unload.store(true, Ordering::SeqCst);
    let cancel = CancellationToken::new();
    let task = {
        let backend = Arc::clone(&backend);
        let cancel = cancel.clone();
        let ctx = start_context(&bot, SnowLumaStartMode::HotStart);
        tokio::spawn(async move { backend.start_cancellable(&ctx, cancel).await })
    };
    permit(&gates.entered).await;
    cancel.cancel();
    gates.release.add_permits(1);
    assert!(task.await.unwrap().is_err());
    assert_eq!(client.unloads.load(Ordering::SeqCst), 1);
    assert_eq!(
        backend.status(bot.clone()).await.unwrap().state,
        ncd_domain::bot_actor::BotActorState::Stopped
    );
    assert_eq!(backend.login_pids.lock().await.get(&bot).unwrap().pid, 42);
    assert_eq!(
        backend.start_modes.lock().await.get(&bot),
        Some(&SnowLumaStartMode::HotStart)
    );
    backend.stop(bot, StopMode::Force).await.unwrap();
    assert!(
        host.qq_operations
            .lock()
            .unwrap()
            .iter()
            .all(|operation| operation.0 != "stop")
    );
}

#[tokio::test(start_paused = true)]
async fn replaced_host_start_cannot_restore_or_remove_the_new_sessions_bindings() {
    let (backend, old_host, gates) = fixture();
    let bot = BotId::new("10001");
    install_reusable_poller(&backend, &bot).await;
    let task = {
        let backend = Arc::clone(&backend);
        let ctx = start_context(&bot, SnowLumaStartMode::HotStart);
        tokio::spawn(async move {
            backend
                .start_cancellable(&ctx, CancellationToken::new())
                .await
        })
    };
    permit(&gates.entered).await;
    let new_client = install_reusable_poller(&backend, &bot).await;
    new_client.pid.store(99, Ordering::SeqCst);
    backend.login_pids.lock().await.insert(
        bot.clone(),
        TrackedQq {
            pid: 99,
            start_time: "54321".into(),
        },
    );
    backend.hot_detached.lock().await.insert(bot.clone());
    let new_host = Arc::new(MetricsHost::default());
    backend
        .daemon
        .replace_host_if_current(new_host.clone(), async { true })
        .await
        .unwrap();
    gates.release.add_permits(1);
    assert!(task.await.unwrap().is_err());
    assert_eq!(backend.login_pids.lock().await.get(&bot).unwrap().pid, 99);
    assert!(backend.hot_detached.lock().await.contains(&bot));
    let current = backend
        .login_clients
        .lock()
        .await
        .get(&bot)
        .cloned()
        .unwrap();
    let expected: Arc<dyn SnowLumaWebUiClient> = new_client.clone();
    assert!(Arc::ptr_eq(&current, &expected));
    assert_eq!(new_client.unloads.load(Ordering::SeqCst), 0);
    assert!(new_host.qq_operations.lock().unwrap().is_empty());
    assert!(
        old_host
            .qq_operations
            .lock()
            .unwrap()
            .iter()
            .all(|operation| operation.0 != "stop")
    );
    backend.detach_login_pollers().await;
}

async fn completed_start_fixture(
    mode: SnowLumaStartMode,
) -> (
    Arc<RemoteSnowLumaBackend>,
    Arc<MetricsHost>,
    BotStatus,
    Arc<ReuseClient>,
) {
    let (backend, host, bot) = bound().await;
    let client = install_reusable_poller(&backend, &bot).await;
    client.allow_unload.store(true, Ordering::SeqCst);
    backend.start_modes.lock().await.insert(bot.clone(), mode);
    let status = BotStatus::running(bot.clone(), 42, 0);
    backend.completed_starts.lock().await.insert(
        bot,
        CompletedStart {
            status: status.clone(),
            host: backend.daemon.current_host().await,
            tracked: TrackedQq {
                pid: 42,
                start_time: "12345".into(),
            },
            mode,
            client: client.clone(),
        },
    );
    (backend, host, status, client)
}

#[tokio::test(start_paused = true)]
async fn unacknowledged_start_cleanup_never_targets_a_replacement_host_or_its_maps() {
    for mode in [SnowLumaStartMode::ColdStart, SnowLumaStartMode::HotStart] {
        let (backend, old_host, status, old_client) = completed_start_fixture(mode).await;
        let new_host = Arc::new(MetricsHost::default());
        backend
            .daemon
            .replace_host_if_current(new_host.clone(), async { true })
            .await
            .unwrap();
        let new_client = install_reusable_poller(&backend, &status.bot_id).await;
        backend.login_pids.lock().await.insert(
            status.bot_id.clone(),
            TrackedQq {
                pid: 99,
                start_time: "54321".into(),
            },
        );
        backend
            .start_modes
            .lock()
            .await
            .insert(status.bot_id.clone(), SnowLumaStartMode::HotStart);
        backend
            .hot_detached
            .lock()
            .await
            .insert(status.bot_id.clone());
        let result = backend.cleanup_unacknowledged_start(&status).await;
        assert_eq!(result.is_ok(), mode.is_cold());
        let actions = old_host.qq_operations.lock().unwrap().clone();
        if mode.is_cold() {
            assert_eq!(actions, [("stop".into(), false, 42, "12345".into())]);
        } else {
            assert!(actions.is_empty());
        }
        assert!(new_host.qq_operations.lock().unwrap().is_empty());
        assert!(new_host.commands.lock().unwrap().is_empty());
        assert_eq!(old_client.unloads.load(Ordering::SeqCst), 0);
        assert_eq!(new_client.unloads.load(Ordering::SeqCst), 0);
        assert_eq!(
            backend
                .login_pids
                .lock()
                .await
                .get(&status.bot_id)
                .unwrap()
                .pid,
            99
        );
        assert!(backend.hot_detached.lock().await.contains(&status.bot_id));
        let current = backend
            .login_clients
            .lock()
            .await
            .get(&status.bot_id)
            .cloned()
            .unwrap();
        let expected: Arc<dyn SnowLumaWebUiClient> = new_client;
        assert!(Arc::ptr_eq(&current, &expected));
        backend.detach_login_pollers().await;
    }
}

#[tokio::test(start_paused = true)]
async fn unacknowledged_hot_start_unloads_only_its_bound_session_and_preserves_qq() {
    let (backend, host, status, client) =
        completed_start_fixture(SnowLumaStartMode::HotStart).await;
    backend.cleanup_unacknowledged_start(&status).await.unwrap();
    assert_eq!(client.unloads.load(Ordering::SeqCst), 1);
    assert!(host.qq_operations.lock().unwrap().is_empty());
    assert_eq!(
        backend
            .login_pids
            .lock()
            .await
            .get(&status.bot_id)
            .unwrap()
            .pid,
        42
    );
    assert!(backend.hot_detached.lock().await.contains(&status.bot_id));
    assert_eq!(
        backend.status(status.bot_id).await.unwrap().state,
        ncd_domain::bot_actor::BotActorState::Stopped
    );
}

#[tokio::test(start_paused = true)]
async fn unacknowledged_cleanup_rejects_a_status_that_does_not_match_the_success_record() {
    let (backend, host, mut status, client) =
        completed_start_fixture(SnowLumaStartMode::ColdStart).await;
    status.started_at = Some(1);
    assert!(backend.cleanup_unacknowledged_start(&status).await.is_err());
    assert!(host.qq_operations.lock().unwrap().is_empty());
    assert_eq!(client.unloads.load(Ordering::SeqCst), 0);
    assert_eq!(
        backend
            .login_pids
            .lock()
            .await
            .get(&status.bot_id)
            .unwrap()
            .pid,
        42
    );
    backend.detach_login_pollers().await;
}
