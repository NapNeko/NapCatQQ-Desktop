use super::*;
use crate::native_deployment_adapter::docker_helpers::tests::host;

fn token_watch(published: Option<&str>) -> NapcatTokenWatch {
    NapcatTokenWatch {
        container: "ncbot-10001".into(),
        local_port: 16099,
        remote_webui: 6099,
        published: published.map(str::to_string),
        latest_log: Arc::new(std::sync::Mutex::new(None)),
        recheck: TokenRecheck::default(),
    }
}

#[tokio::test(start_paused = true)]
async fn rejected_token_recovers_after_container_file_becomes_readable() {
    let bus = BroadcastEventBus::default();
    let bot_id = BotId::new("10001");
    let mut events = bus.subscribe(EventFilter::bot("10001"));
    let mut watch = token_watch(Some("stale-store"));
    watch
        .recheck
        .on_auth_issue(Some(NapCatWebuiAuthIssue::TokenRejected));
    *watch.latest_log.lock().unwrap() = Some("old-log".into());

    watch.tick(&host(None, None), &bus, &bot_id).await;
    assert_eq!(watch.published.as_deref(), Some("stale-store"));
    tokio::time::advance(Duration::from_secs(30)).await;
    watch
        .tick(&host(Some(r#"{"token":"actual"}"#), None), &bus, &bot_id)
        .await;
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(1), events.next())
            .await
            .expect("recovered token must be published"),
        Some(DomainEvent::napcat_webui_available_remote(
            bot_id.clone(),
            16099,
            6099,
            "actual"
        ))
    );

    watch.recheck.on_auth_issue(None);
    tokio::time::advance(Duration::from_secs(30)).await;
    watch.tick(&host(None, None), &bus, &bot_id).await;
    assert_eq!(watch.published.as_deref(), Some("actual"));
}

#[tokio::test(start_paused = true)]
async fn missing_initial_token_can_publish_without_a_log_line() {
    let bus = BroadcastEventBus::default();
    let bot_id = BotId::new("10001");
    let mut events = bus.subscribe(EventFilter::bot("10001"));
    let mut watch = token_watch(None);
    watch.tick(&host(None, None), &bus, &bot_id).await;
    tokio::time::advance(Duration::from_secs(30)).await;
    watch
        .tick(&host(Some(r#"{"token":"ready"}"#), None), &bus, &bot_id)
        .await;
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(1), events.next())
            .await
            .expect("initial token must be published"),
        Some(DomainEvent::napcat_webui_available_remote(
            bot_id, 16099, 6099, "ready"
        ))
    );
}

#[tokio::test]
async fn auth_subscription_delivers_rejection_and_recovery_for_its_bot() {
    let bus = BroadcastEventBus::default();
    let mut sub = bus.subscribe(EventFilter {
        bot_id: Some(BotId::new("10001")),
        kind: Some(DomainEventKind::NapCatWebuiAuthChanged),
    });
    bus.publish(DomainEvent::napcat_webui_auth_changed(
        "other",
        Some(NapCatWebuiAuthIssue::TokenRejected),
    ));
    bus.publish(DomainEvent::napcat_webui_auth_changed(
        "10001",
        Some(NapCatWebuiAuthIssue::TokenRejected),
    ));
    assert_eq!(
        wait_auth_issue(Some(&mut sub)).await,
        Some(NapCatWebuiAuthIssue::TokenRejected)
    );
    bus.publish(DomainEvent::napcat_webui_auth_changed("10001", None));
    assert_eq!(wait_auth_issue(Some(&mut sub)).await, None);
}

#[tokio::test]
async fn closing_scan_invalidates_late_generation_and_session_shutdown_removes_client() {
    let registry = DockerBotSessionRegistry::new();
    let bot_id = BotId::new("10001");
    let client: Arc<dyn SnowLumaWebUiClient> =
        Arc::new(ReqwestSnowLumaWebUiClient::new(9, String::new()).expect("client construction"));
    registry.sessions.lock().await.insert(
        bot_id.clone(),
        Arc::new(DockerBotSession {
            inner: Mutex::new(SessionInner {
                tunnels: Vec::new(),
                log_task: None,
                watch_task: None,
                snowluma: None,
                snowluma_poller: None,
                snowluma_client: Some(Arc::clone(&client)),
                qr_generation: 0,
                qr_login_detected: false,
                qr_refresh_pending: false,
                stop_expected: Arc::new(AtomicBool::new(false)),
            }),
        }),
    );
    let (_, generation, _) = registry
        .snowluma_client(&bot_id)
        .await
        .expect("active session");
    registry.request_qr_refresh(&bot_id).await;
    assert_eq!(
        registry.take_qr_refresh(&bot_id, generation, &client).await,
        Some(true)
    );
    assert_eq!(
        registry.take_qr_refresh(&bot_id, generation, &client).await,
        Some(false)
    );
    registry.cancel_qr_login(&bot_id).await;
    registry
        .mark_qr_login_detected(&bot_id, generation, &client)
        .await;
    let (_, current, detected) = registry
        .snowluma_client(&bot_id)
        .await
        .expect("active session");
    assert_ne!(generation, current);
    assert!(
        !detected,
        "late prior-session status cannot update the current scan"
    );
    assert_eq!(
        registry.take_qr_refresh(&bot_id, generation, &client).await,
        None
    );
    registry
        .mark_qr_login_detected(&bot_id, current, &client)
        .await;
    assert!(
        registry
            .snowluma_client(&bot_id)
            .await
            .expect("active session")
            .2
    );
    registry.shutdown_bot(&bot_id).await;
    assert!(registry.snowluma_client(&bot_id).await.is_none());
}
