    use super::*;

    use std::sync::Mutex as StdMutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::super::offline_notifier::OfflineNoticeKind;
    use ncd_traits::events::EventFilter;

    // ── mock client:login + online 响应序列 ──────────────────────────────

    struct StatusMockClient {
        login_responses: StdMutex<Vec<Result<CheckLoginStatusData, NapCatWebUiError>>>,
        online_responses: StdMutex<Vec<Result<GetQQLoginInfoData, NapCatWebUiError>>>,
        login_calls: AtomicUsize,
        online_calls: AtomicUsize,
        fetch_calls: AtomicUsize,
    }

    impl StatusMockClient {
        fn new(
            login_responses: Vec<Result<CheckLoginStatusData, NapCatWebUiError>>,
            online_responses: Vec<Result<GetQQLoginInfoData, NapCatWebUiError>>,
        ) -> Self {
            Self {
                login_responses: StdMutex::new(login_responses),
                online_responses: StdMutex::new(online_responses),
                login_calls: AtomicUsize::new(0),
                online_calls: AtomicUsize::new(0),
                fetch_calls: AtomicUsize::new(0),
            }
        }

        fn login_calls(&self) -> usize {
            self.login_calls.load(Ordering::SeqCst)
        }

        fn online_calls(&self) -> usize {
            self.online_calls.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl NapCatWebUiClient for StatusMockClient {
        async fn fetch_credential(
            &self,
            _port: u16,
            _token: &str,
        ) -> Result<String, NapCatWebUiError> {
            self.fetch_calls.fetch_add(1, Ordering::SeqCst);
            Ok("test-cred".into())
        }

        async fn check_login_status(
            &self,
            _port: u16,
            _auth: &str,
        ) -> Result<CheckLoginStatusData, NapCatWebUiError> {
            self.login_calls.fetch_add(1, Ordering::SeqCst);
            let mut q = self.login_responses.lock().expect("login mutex poisoned");
            q.pop()
                .unwrap_or_else(|| Err(NapCatWebUiError::Decode("login queue empty".into())))
        }

        async fn check_online_status(
            &self,
            _port: u16,
            _auth: &str,
        ) -> Result<GetQQLoginInfoData, NapCatWebUiError> {
            self.online_calls.fetch_add(1, Ordering::SeqCst);
            let mut q = self.online_responses.lock().expect("online mutex poisoned");
            q.pop()
                .unwrap_or_else(|| Err(NapCatWebUiError::Decode("online queue empty".into())))
        }

        async fn set_ob11_config(
            &self,
            _port: u16,
            _auth: &str,
            _config_json: &str,
        ) -> Result<(), NapCatWebUiError> {
            Ok(())
        }
    }

    // ── recording notifier:记录 (bot_id, kind) 调用 ──────────────────────

    struct RecordingNotifier {
        calls: StdMutex<Vec<(BotId, OfflineNoticeKind)>>,
    }

    impl RecordingNotifier {
        fn new() -> Self {
            Self {
                calls: StdMutex::new(Vec::new()),
            }
        }

        fn calls(&self) -> Vec<(BotId, OfflineNoticeKind)> {
            self.calls.lock().expect("notifier mutex poisoned").clone()
        }
    }

    #[async_trait]
    impl OfflineNotifier for RecordingNotifier {
        async fn notify(&self, bot_id: &BotId, kind: OfflineNoticeKind) {
            self.calls
                .lock()
                .expect("notifier mutex poisoned")
                .push((bot_id.clone(), kind));
        }
    }

    // ── recording restart handle:记录 restart_bot 调用 ──────────────────

    struct RecordingRestartHandle {
        calls: AtomicUsize,
    }

    impl RecordingRestartHandle {
        fn new() -> Self {
            Self {
                calls: AtomicUsize::new(0),
            }
        }

        fn calls(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl RestartHandle for RecordingRestartHandle {
        async fn restart_bot(&self, _bot_id: &BotId) {
            self.calls.fetch_add(1, Ordering::SeqCst);
        }
    }

    // ── 测试辅助:构造 deps + 排空事件 ────────────────────────────────────

    fn status_test_deps(
        client: Arc<StatusMockClient>,
        notifier: Arc<RecordingNotifier>,
        restart: Arc<RecordingRestartHandle>,
    ) -> (PollerDeps, Arc<BroadcastEventBus>) {
        let bus = Arc::new(BroadcastEventBus::default());
        let deps = PollerDeps {
            event_bus: bus.clone(),
            http: client,
            notifier,
            restart_handle: restart,
        };
        (deps, bus)
    }

    /// 排空 bot-scoped 订阅器中已发布的事件每次循环都用极短超时探测,
    /// 直到 next() 不再立即返回事件为止
    async fn drain_events(sub: &mut ncd_traits::events::EventSubscription) -> Vec<DomainEvent> {
        let mut out = Vec::new();
        loop {
            match tokio::time::timeout(Duration::from_millis(5), sub.next()).await {
                Ok(Some(ev)) => out.push(ev),
                Ok(None) => break, // closed
                Err(_) => break,   // no more events available within timeout
            }
        }
        out
    }

    // =====================================================================
    // do_status_poll
    // =====================================================================

    /// state.auth.is_none() → 通过 cmd channel 触发
    /// RequestAuthRefresh,不发起任何 HTTP 请求
    #[tokio::test]
    async fn status_poll_without_auth_requests_refresh_and_skips_http() {
        let bot_id = BotId::new("no-auth");
        let cfg = PollerConfig::default();
        let client = Arc::new(StatusMockClient::new(vec![], vec![]));
        let (deps, bus) = status_test_deps(
            client.clone(),
            Arc::new(RecordingNotifier::new()),
            Arc::new(RecordingRestartHandle::new()),
        );
        let mut sub = bus.subscribe(EventFilter::bot("no-auth"));
        let (cmd_tx, mut cmd_rx) = mpsc::channel::<PollerCommand>(8);

        let mut state = LoginState::new();
        assert!(state.auth.is_none());

        do_status_poll(&bot_id, 6099, &cfg, &deps, &mut state, &cmd_tx).await;

        // 没有任何 HTTP 调用
        assert_eq!(client.login_calls(), 0);
        assert_eq!(client.online_calls(), 0);
        // 命令通道有一条 RequestAuthRefresh
        match cmd_rx.try_recv() {
            Ok(PollerCommand::RequestAuthRefresh) => {}
            other => panic!("expected RequestAuthRefresh, got {other:?}"),
        }
        // 没有任何 DomainEvent 被发布
        let events = drain_events(&mut sub).await;
        assert!(events.is_empty(), "no events expected, got {events:?}");
    }

    /// state.auth.is_some() → tokio::join! 并发发起两路
    /// HTTP 请求,每路调用恰好 1 次
    #[tokio::test]
    async fn status_poll_with_auth_calls_both_endpoints_concurrently() {
        let bot_id = BotId::new("with-auth");
        let cfg = PollerConfig::default();
        let client = Arc::new(StatusMockClient::new(
            vec![Ok(CheckLoginStatusData {
                is_login: true,
                is_offline: None,
                qrcode_url: String::new(),
            })],
            vec![Ok(GetQQLoginInfoData { online: Some(true) })],
        ));
        let (deps, _bus) = status_test_deps(
            client.clone(),
            Arc::new(RecordingNotifier::new()),
            Arc::new(RecordingRestartHandle::new()),
        );
        let (cmd_tx, mut cmd_rx) = mpsc::channel::<PollerCommand>(8);

        let mut state = LoginState::new();
        state.auth = Some("bearer-xyz".into());

        do_status_poll(&bot_id, 6099, &cfg, &deps, &mut state, &cmd_tx).await;

        assert_eq!(client.login_calls(), 1);
        assert_eq!(client.online_calls(), 1);
        // 成功路径不会触发 RequestAuthRefresh
        assert!(cmd_rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn status_poll_login_true_marks_online_even_when_info_online_missing() {
        let bot_id = BotId::new("login-derived-online");
        let cfg = PollerConfig::default();
        let client = Arc::new(StatusMockClient::new(
            vec![Ok(CheckLoginStatusData {
                is_login: true,
                is_offline: None,
                qrcode_url: String::new(),
            })],
            vec![Ok(GetQQLoginInfoData { online: None })],
        ));
        let (deps, bus) = status_test_deps(
            client,
            Arc::new(RecordingNotifier::new()),
            Arc::new(RecordingRestartHandle::new()),
        );
        let mut sub = bus.subscribe(EventFilter::bot("login-derived-online"));
        let (cmd_tx, _cmd_rx) = mpsc::channel::<PollerCommand>(8);
        let mut state = LoginState::new();
        state.auth = Some("bearer".into());

        do_status_poll(&bot_id, 6099, &cfg, &deps, &mut state, &cmd_tx).await;

        assert!(state.online);
        let events = drain_events(&mut sub).await;
        assert!(
            events
                .iter()
                .any(|e| matches!(e, DomainEvent::NapCatLoginOnline { online: true, .. })),
            "expected derived online=true event, got {events:?}"
        );
    }

    #[tokio::test]
    async fn status_poll_is_offline_marks_offline_even_when_info_online_missing() {
        let bot_id = BotId::new("login-derived-offline");
        let cfg = PollerConfig::default();
        let client = Arc::new(StatusMockClient::new(
            vec![Ok(CheckLoginStatusData {
                is_login: false,
                is_offline: Some(true),
                qrcode_url: String::new(),
            })],
            vec![Ok(GetQQLoginInfoData { online: None })],
        ));
        let (deps, bus) = status_test_deps(
            client,
            Arc::new(RecordingNotifier::new()),
            Arc::new(RecordingRestartHandle::new()),
        );
        let mut sub = bus.subscribe(EventFilter::bot("login-derived-offline"));
        let (cmd_tx, _cmd_rx) = mpsc::channel::<PollerCommand>(8);
        let mut state = LoginState::new();
        state.auth = Some("bearer".into());
        state.online = true;

        do_status_poll(&bot_id, 6099, &cfg, &deps, &mut state, &cmd_tx).await;

        assert!(!state.online);
        let events = drain_events(&mut sub).await;
        assert!(
            events
                .iter()
                .any(|e| matches!(e, DomainEvent::NapCatLoginOnline { online: false, .. })),
            "expected derived online=false event, got {events:?}"
        );
    }

    /// login_res 返回 Unauthorized → 触发
    /// RequestAuthRefresh,且本轮不再处理 online_res(不调用
    /// apply_login_status / apply_online_status,不发布任何事件)
    #[tokio::test]
    async fn status_poll_login_unauthorized_triggers_refresh_and_skips_apply() {
        let bot_id = BotId::new("login-401");
        let cfg = PollerConfig::default();
        let client = Arc::new(StatusMockClient::new(
            vec![Err(NapCatWebUiError::Unauthorized(401))],
            vec![Ok(GetQQLoginInfoData { online: Some(true) })],
        ));
        let (deps, bus) = status_test_deps(
            client.clone(),
            Arc::new(RecordingNotifier::new()),
            Arc::new(RecordingRestartHandle::new()),
        );
        let mut sub = bus.subscribe(EventFilter::bot("login-401"));
        let (cmd_tx, mut cmd_rx) = mpsc::channel::<PollerCommand>(8);

        let mut state = LoginState::new();
        state.auth = Some("bearer".into());

        do_status_poll(&bot_id, 6099, &cfg, &deps, &mut state, &cmd_tx).await;

        // 命令通道收到 RequestAuthRefresh
        match cmd_rx.try_recv() {
            Ok(PollerCommand::RequestAuthRefresh) => {}
            other => panic!("expected RequestAuthRefresh, got {other:?}"),
        }
        // 早退后即使 online 返回成功也不会被 apply(不发布 NapCatLoginOnline)
        let events = drain_events(&mut sub).await;
        assert!(
            events.is_empty(),
            "login Unauthorized must skip online apply, got {events:?}"
        );
        assert!(!state.online);
    }

    /// login_res 返回非 Unauthorized 错误 → 仅 warn,
    /// 不触发 RequestAuthRefresh,不影响 online_res 路径
    #[tokio::test]
    async fn status_poll_login_other_error_warns_and_continues_to_online() {
        let bot_id = BotId::new("login-500");
        let cfg = PollerConfig::default();
        let client = Arc::new(StatusMockClient::new(
            vec![Err(NapCatWebUiError::Status(500))],
            vec![Ok(GetQQLoginInfoData { online: Some(true) })],
        ));
        let (deps, bus) = status_test_deps(
            client.clone(),
            Arc::new(RecordingNotifier::new()),
            Arc::new(RecordingRestartHandle::new()),
        );
        let mut sub = bus.subscribe(EventFilter::bot("login-500"));
        let (cmd_tx, mut cmd_rx) = mpsc::channel::<PollerCommand>(8);

        let mut state = LoginState::new();
        state.auth = Some("bearer".into());

        do_status_poll(&bot_id, 6099, &cfg, &deps, &mut state, &cmd_tx).await;

        // 不应触发 RequestAuthRefresh
        assert!(
            cmd_rx.try_recv().is_err(),
            "non-Unauthorized error must not trigger refresh"
        );
        // online_res 路径仍执行,发了一条 NapCatLoginOnline
        let events = drain_events(&mut sub).await;
        assert_eq!(events.len(), 1, "expected single NapCatLoginOnline event");
        match &events[0] {
            DomainEvent::NapCatLoginOnline { online, .. } => assert!(*online),
            other => panic!("expected NapCatLoginOnline, got {other:?}"),
        }
        assert!(state.online);
    }

    /// online_res 返回 Unauthorized → 触发
    /// RequestAuthRefreshlogin_res 已成功 apply,对应的事件正常发布
    #[tokio::test]
    async fn status_poll_online_unauthorized_triggers_refresh() {
        let bot_id = BotId::new("online-401");
        let cfg = PollerConfig::default();
        let client = Arc::new(StatusMockClient::new(
            vec![Ok(CheckLoginStatusData {
                is_login: true,
                is_offline: None,
                qrcode_url: String::new(),
            })],
            vec![Err(NapCatWebUiError::Unauthorized(403))],
        ));
        let (deps, _bus) = status_test_deps(
            client.clone(),
            Arc::new(RecordingNotifier::new()),
            Arc::new(RecordingRestartHandle::new()),
        );
        let (cmd_tx, mut cmd_rx) = mpsc::channel::<PollerCommand>(8);

        let mut state = LoginState::new();
        state.auth = Some("bearer".into());

        do_status_poll(&bot_id, 6099, &cfg, &deps, &mut state, &cmd_tx).await;

        // login apply 已经把 is_logged_in 置 true
        assert!(state.is_logged_in);
        // 命令通道收到 RequestAuthRefresh(来自 online 路径)
        match cmd_rx.try_recv() {
            Ok(PollerCommand::RequestAuthRefresh) => {}
            other => panic!("expected RequestAuthRefresh, got {other:?}"),
        }
    }

    // =====================================================================
    // apply_login_status
    // =====================================================================

    /// is_login=true → 清除 invalidation/suppress flag,
    /// 发布 NapCatLoginQrcodeRemoved,return(不再发 Qrcode)
    #[tokio::test]
    async fn apply_login_logged_in_clears_flags_and_emits_qrcode_removed() {
        let bot_id = BotId::new("logged-in");
        let bus = Arc::new(BroadcastEventBus::default());
        let mut sub = bus.subscribe(EventFilter::bot("logged-in"));
        let deps = PollerDeps {
            event_bus: bus.clone(),
            http: Arc::new(StatusMockClient::new(vec![], vec![])),
            notifier: Arc::new(RecordingNotifier::new()),
            restart_handle: Arc::new(RecordingRestartHandle::new()),
        };

        let mut state = LoginState::new();
        // 预置脏 flag 验证清除语义
        state.login_invalidated_while_online = true;
        state.suppress_qrcode_until_online = true;

        apply_login_status(
            &bot_id,
            CheckLoginStatusData {
                is_login: true,
                is_offline: None,
                // 即便给了 qrcode_url,已登录分支也必须忽略它
                qrcode_url: "data:image/png;base64,SHOULD_BE_IGNORED".into(),
            },
            &deps,
            &mut state,
        );

        assert!(state.is_logged_in);
        assert!(!state.login_invalidated_while_online);
        assert!(!state.suppress_qrcode_until_online);

        let events = drain_events(&mut sub).await;
        assert_eq!(events.len(), 1, "expected single QrcodeRemoved event");
        match &events[0] {
            DomainEvent::NapCatLoginQrcodeRemoved { bot_id: b } => {
                assert_eq!(b, &bot_id);
            }
            other => panic!("expected NapCatLoginQrcodeRemoved, got {other:?}"),
        }
    }

    /// prev_login=true ∧ state.online=true ∧
    /// is_login=false → 设置 login_invalidated_while_online=true,
    /// 发 NapCatLoginInvalidated{Kicked},且二维码被同一调用抑制
    #[tokio::test]
    async fn apply_login_kick_during_online_emits_invalidated_and_suppresses_qrcode() {
        let bot_id = BotId::new("kicked");
        let bus = Arc::new(BroadcastEventBus::default());
        let mut sub = bus.subscribe(EventFilter::bot("kicked"));
        let deps = PollerDeps {
            event_bus: bus.clone(),
            http: Arc::new(StatusMockClient::new(vec![], vec![])),
            notifier: Arc::new(RecordingNotifier::new()),
            restart_handle: Arc::new(RecordingRestartHandle::new()),
        };

        let mut state = LoginState::new();
        // 上一轮处于 online + logged_in 的稳态
        state.is_logged_in = true;
        state.online = true;

        apply_login_status(
            &bot_id,
            CheckLoginStatusData {
                is_login: false,
                is_offline: None,
                // 服务器把 qrcode_url 也回带了,但本调用因 invalidation flag
                // 必须抑制 Qrcode(与 QrcodeRemoved 互斥)
                qrcode_url: "data:image/png;base64,QR".into(),
            },
            &deps,
            &mut state,
        );

        assert!(!state.is_logged_in);
        assert!(state.login_invalidated_while_online);

        let events = drain_events(&mut sub).await;
        // 仅 1 个 Invalidated 事件,没有 Qrcode(互斥不变量)
        assert_eq!(events.len(), 1);
        match &events[0] {
            DomainEvent::NapCatLoginInvalidated { bot_id: b, reason } => {
                assert_eq!(b, &bot_id);
                assert_eq!(*reason, NapCatLoginInvalidationReason::Kicked);
            }
            other => panic!("expected NapCatLoginInvalidated, got {other:?}"),
        }
    }

    /// is_login=false ∧ qrcode_url 非空 ∧
    /// !login_invalidated_while_online ∧ !suppress_qrcode_until_online →
    /// 发 NapCatLoginQrcode
    #[tokio::test]
    async fn apply_login_unlogged_with_qrcode_emits_qrcode() {
        let bot_id = BotId::new("first-login");
        let bus = Arc::new(BroadcastEventBus::default());
        let mut sub = bus.subscribe(EventFilter::bot("first-login"));
        let deps = PollerDeps {
            event_bus: bus.clone(),
            http: Arc::new(StatusMockClient::new(vec![], vec![])),
            notifier: Arc::new(RecordingNotifier::new()),
            restart_handle: Arc::new(RecordingRestartHandle::new()),
        };

        let mut state = LoginState::new();
        // 干净状态:未登录 + 不在线

        apply_login_status(
            &bot_id,
            CheckLoginStatusData {
                is_login: false,
                is_offline: None,
                qrcode_url: "data:image/png;base64,QR1".into(),
            },
            &deps,
            &mut state,
        );

        assert!(!state.is_logged_in);
        let events = drain_events(&mut sub).await;
        assert_eq!(events.len(), 1);
        match &events[0] {
            DomainEvent::NapCatLoginQrcode {
                bot_id: b,
                qrcode_url,
            } => {
                assert_eq!(b, &bot_id);
                assert_eq!(qrcode_url, "data:image/png;base64,QR1");
            }
            other => panic!("expected NapCatLoginQrcode, got {other:?}"),
        }
    }

    /// login_invalidated_while_online=true → 必须抑制
    /// NapCatLoginQrcode 事件(即使 qrcode_url 非空)
    #[tokio::test]
    async fn apply_login_suppresses_qrcode_when_invalidated_while_online() {
        let bot_id = BotId::new("suppress-by-invalidated");
        let bus = Arc::new(BroadcastEventBus::default());
        let mut sub = bus.subscribe(EventFilter::bot("suppress-by-invalidated"));
        let deps = PollerDeps {
            event_bus: bus.clone(),
            http: Arc::new(StatusMockClient::new(vec![], vec![])),
            notifier: Arc::new(RecordingNotifier::new()),
            restart_handle: Arc::new(RecordingRestartHandle::new()),
        };

        let mut state = LoginState::new();
        // 上一轮已不在线(online=false),所以这次 is_login=false 不会再次
        // 触发 Kicked 事件——只需验证「flag 抑制」语义
        state.login_invalidated_while_online = true;

        apply_login_status(
            &bot_id,
            CheckLoginStatusData {
                is_login: false,
                is_offline: None,
                qrcode_url: "data:image/png;base64,SUPPRESSED".into(),
            },
            &deps,
            &mut state,
        );

        let events = drain_events(&mut sub).await;
        assert!(
            events.is_empty(),
            "Qrcode must be suppressed when login_invalidated_while_online; got {events:?}"
        );
    }

    /// suppress_qrcode_until_online=true → 同样抑制
    /// NapCatLoginQrcode
    #[tokio::test]
    async fn apply_login_suppresses_qrcode_when_suppress_until_online() {
        let bot_id = BotId::new("suppress-by-flag");
        let bus = Arc::new(BroadcastEventBus::default());
        let mut sub = bus.subscribe(EventFilter::bot("suppress-by-flag"));
        let deps = PollerDeps {
            event_bus: bus.clone(),
            http: Arc::new(StatusMockClient::new(vec![], vec![])),
            notifier: Arc::new(RecordingNotifier::new()),
            restart_handle: Arc::new(RecordingRestartHandle::new()),
        };

        let mut state = LoginState::new();
        state.suppress_qrcode_until_online = true;

        apply_login_status(
            &bot_id,
            CheckLoginStatusData {
                is_login: false,
                is_offline: None,
                qrcode_url: "data:image/png;base64,STILL_SUPPRESSED".into(),
            },
            &deps,
            &mut state,
        );

        let events = drain_events(&mut sub).await;
        assert!(
            events.is_empty(),
            "Qrcode must be suppressed; got {events:?}"
        );
    }

    /// is_login=false ∧ qrcode_url="" → 没有可发的二维码,无事件
    #[tokio::test]
    async fn apply_login_unlogged_with_empty_qrcode_emits_nothing() {
        let bot_id = BotId::new("no-qr");
        let bus = Arc::new(BroadcastEventBus::default());
        let mut sub = bus.subscribe(EventFilter::bot("no-qr"));
        let deps = PollerDeps {
            event_bus: bus.clone(),
            http: Arc::new(StatusMockClient::new(vec![], vec![])),
            notifier: Arc::new(RecordingNotifier::new()),
            restart_handle: Arc::new(RecordingRestartHandle::new()),
        };

        let mut state = LoginState::new();

        apply_login_status(
            &bot_id,
            CheckLoginStatusData {
                is_login: false,
                is_offline: None,
                qrcode_url: String::new(),
            },
            &deps,
            &mut state,
        );

        let events = drain_events(&mut sub).await;
        assert!(events.is_empty());
    }

    // =====================================================================
    // apply_online_status
    // =====================================================================

    /// online 缺失表示未知:不能当作 false,否则启动早期 / 远端直跑
    /// OneBot selfInfo 尚未初始化时会被误报为未登录。
    #[tokio::test]
    async fn apply_online_unknown_noops_without_event_or_side_effects() {
        let bot_id = BotId::new("unknown-online");
        let cfg = PollerConfig {
            offline_auto_restart: true,
            offline_notice_enabled: true,
            ..PollerConfig::default()
        };
        let bus = Arc::new(BroadcastEventBus::default());
        let mut sub = bus.subscribe(EventFilter::bot("unknown-online"));
        let notifier = Arc::new(RecordingNotifier::new());
        let restart = Arc::new(RecordingRestartHandle::new());
        let deps = PollerDeps {
            event_bus: bus.clone(),
            http: Arc::new(StatusMockClient::new(vec![], vec![])),
            notifier: notifier.clone(),
            restart_handle: restart.clone(),
        };

        let mut state = LoginState::new();
        state.is_logged_in = true;
        state.online = true;

        apply_online_status(
            &bot_id,
            GetQQLoginInfoData { online: None },
            &cfg,
            &deps,
            &mut state,
        )
        .await;

        assert!(state.online);
        assert!(state.is_logged_in);
        assert!(notifier.calls().is_empty());
        assert_eq!(restart.calls(), 0);
        let events = drain_events(&mut sub).await;
        assert!(
            events.is_empty(),
            "unknown online must not emit events: {events:?}"
        );
    }

    /// online=true → 总是先发 NapCatLoginOnline{true};
    /// 若本离线区间曾发过离线通知,再补一次 Recovered,然后清三个 flag。
    #[tokio::test]
    async fn apply_online_true_resets_flags_and_emits_online_event() {
        let bot_id = BotId::new("back-online");
        let cfg = PollerConfig::default();
        let bus = Arc::new(BroadcastEventBus::default());
        let mut sub = bus.subscribe(EventFilter::bot("back-online"));
        let notifier = Arc::new(RecordingNotifier::new());
        let restart = Arc::new(RecordingRestartHandle::new());
        let deps = PollerDeps {
            event_bus: bus.clone(),
            http: Arc::new(StatusMockClient::new(vec![], vec![])),
            notifier: notifier.clone(),
            restart_handle: restart.clone(),
        };

        let mut state = LoginState::new();
        // 预置「曾发过通知 + 处于 kicked + 抑制二维码」
        state.offline_notice_sent = true;
        state.login_invalidated_while_online = true;
        state.suppress_qrcode_until_online = true;

        apply_online_status(
            &bot_id,
            GetQQLoginInfoData { online: Some(true) },
            &cfg,
            &deps,
            &mut state,
        )
        .await;

        // flags 全部清零
        assert!(!state.offline_notice_sent);
        assert!(!state.login_invalidated_while_online);
        assert!(!state.suppress_qrcode_until_online);
        assert!(state.online);
        // 曾发过离线通知再上线 → Recovered;不触发 restart
        assert_eq!(
            notifier.calls(),
            vec![(bot_id.clone(), OfflineNoticeKind::Recovered)]
        );
        assert_eq!(restart.calls(), 0);
        // 仅 1 个事件:NapCatLoginOnline{true}
        let events = drain_events(&mut sub).await;
        assert_eq!(events.len(), 1);
        match &events[0] {
            DomainEvent::NapCatLoginOnline { online, .. } => assert!(*online),
            other => panic!("expected NapCatLoginOnline, got {other:?}"),
        }
    }

    /// prev_online=false ∧ online=false → 持续离线,仅发
    /// NapCatLoginOnline{false},不触发任何副作用
    #[tokio::test]
    async fn apply_online_persistent_offline_emits_event_only_no_side_effects() {
        let bot_id = BotId::new("persistent-offline");
        let cfg = PollerConfig {
            offline_auto_restart: true,
            offline_notice_enabled: true,
            ..PollerConfig::default()
        };
        let bus = Arc::new(BroadcastEventBus::default());
        let mut sub = bus.subscribe(EventFilter::bot("persistent-offline"));
        let notifier = Arc::new(RecordingNotifier::new());
        let restart = Arc::new(RecordingRestartHandle::new());
        let deps = PollerDeps {
            event_bus: bus.clone(),
            http: Arc::new(StatusMockClient::new(vec![], vec![])),
            notifier: notifier.clone(),
            restart_handle: restart.clone(),
        };

        let mut state = LoginState::new();
        // prev_online = false(默认),所以即使 auto_restart=true 也不应触发

        apply_online_status(
            &bot_id,
            GetQQLoginInfoData {
                online: Some(false),
            },
            &cfg,
            &deps,
            &mut state,
        )
        .await;

        let events = drain_events(&mut sub).await;
        assert_eq!(events.len(), 1, "expected only NapCatLoginOnline{{false}}");
        match &events[0] {
            DomainEvent::NapCatLoginOnline { online, .. } => assert!(!*online),
            other => panic!("expected NapCatLoginOnline, got {other:?}"),
        }
        assert!(notifier.calls().is_empty());
        assert_eq!(restart.calls(), 0);
    }

    /// prev_online=true ∧ online=false ∧ kicked=true →
    /// suppress_qrcode_until_online=true,清 login_invalidated_while_online,
    /// 发 NapCatLoginQrcodeRemovedoffline_auto_restart=true 时还会
    /// 触发一次 restart(属于「踢线后自动重启」分支)
    #[tokio::test]
    async fn apply_online_kicked_then_offline_suppresses_qrcode_and_auto_restarts() {
        let bot_id = BotId::new("kicked-offline");
        let cfg = PollerConfig {
            offline_auto_restart: true,
            offline_notice_enabled: true,
            ..PollerConfig::default()
        };
        let bus = Arc::new(BroadcastEventBus::default());
        let mut sub = bus.subscribe(EventFilter::bot("kicked-offline"));
        let notifier = Arc::new(RecordingNotifier::new());
        let restart = Arc::new(RecordingRestartHandle::new());
        let deps = PollerDeps {
            event_bus: bus.clone(),
            http: Arc::new(StatusMockClient::new(vec![], vec![])),
            notifier: notifier.clone(),
            restart_handle: restart.clone(),
        };

        let mut state = LoginState::new();
        // 上一轮:online=true,且 apply_login_status 已把 invalidated_while_online
        // 置为 true(踢线第一阶段)
        state.online = true;
        state.login_invalidated_while_online = true;
        // is_logged_in 当前已被 apply_login_status 设为 false(被踢后)

        apply_online_status(
            &bot_id,
            GetQQLoginInfoData {
                online: Some(false),
            },
            &cfg,
            &deps,
            &mut state,
        )
        .await;

        // suppress 标记开启,invalidation 标记被清
        assert!(state.suppress_qrcode_until_online);
        assert!(!state.login_invalidated_while_online);
        assert!(!state.online);

        // 应至少发 2 个事件:NapCatLoginOnline{false} + NapCatLoginQrcodeRemoved
        let events = drain_events(&mut sub).await;
        assert!(
            events
                .iter()
                .any(|e| matches!(e, DomainEvent::NapCatLoginOnline { online: false, .. })),
            "expected NapCatLoginOnline{{false}} in {events:?}"
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, DomainEvent::NapCatLoginQrcodeRemoved { .. })),
            "expected NapCatLoginQrcodeRemoved in {events:?}"
        );

        // 自动重启分支:restart 至多 1 次
        assert_eq!(
            restart.calls(),
            1,
            "auto_restart must call restart_bot once"
        );
        // 通知至多 1 次(AutoRestart kind)
        let notice_calls = notifier.calls();
        assert_eq!(notice_calls.len(), 1);
        assert_eq!(notice_calls[0].1, OfflineNoticeKind::AutoRestart);
        assert!(state.offline_notice_sent);
    }

    /// prev_online=true ∧ online=false ∧ is_logged_in=true ∧
    /// kicked=false ∧ offline_auto_restart=true → 发通知(AutoRestart)+
    /// restart_bot(普通在线掉线后自动重启)
    #[tokio::test]
    async fn apply_online_logged_offline_with_auto_restart_notifies_and_restarts() {
        let bot_id = BotId::new("logged-offline-auto");
        let cfg = PollerConfig {
            offline_auto_restart: true,
            offline_notice_enabled: true,
            ..PollerConfig::default()
        };
        let bus = Arc::new(BroadcastEventBus::default());
        let _sub = bus.subscribe(EventFilter::bot("logged-offline-auto"));
        let notifier = Arc::new(RecordingNotifier::new());
        let restart = Arc::new(RecordingRestartHandle::new());
        let deps = PollerDeps {
            event_bus: bus.clone(),
            http: Arc::new(StatusMockClient::new(vec![], vec![])),
            notifier: notifier.clone(),
            restart_handle: restart.clone(),
        };

        let mut state = LoginState::new();
        state.online = true;
        state.is_logged_in = true; // 已登录但掉线(非踢线)

        apply_online_status(
            &bot_id,
            GetQQLoginInfoData {
                online: Some(false),
            },
            &cfg,
            &deps,
            &mut state,
        )
        .await;

        let calls = notifier.calls();
        assert_eq!(calls.len(), 1, "auto_restart should notify once");
        assert_eq!(calls[0].1, OfflineNoticeKind::AutoRestart);
        assert!(state.offline_notice_sent);
        assert_eq!(restart.calls(), 1);
    }

    /// offline_notice_sent=true → 不再调 notify,
    /// 但 auto_restart=true 仍会调 restart_bot(一次)
    #[tokio::test]
    async fn apply_online_offline_with_notice_already_sent_skips_notify() {
        let bot_id = BotId::new("notice-already");
        let cfg = PollerConfig {
            offline_auto_restart: true,
            offline_notice_enabled: true,
            ..PollerConfig::default()
        };
        let bus = Arc::new(BroadcastEventBus::default());
        let _sub = bus.subscribe(EventFilter::bot("notice-already"));
        let notifier = Arc::new(RecordingNotifier::new());
        let restart = Arc::new(RecordingRestartHandle::new());
        let deps = PollerDeps {
            event_bus: bus.clone(),
            http: Arc::new(StatusMockClient::new(vec![], vec![])),
            notifier: notifier.clone(),
            restart_handle: restart.clone(),
        };

        let mut state = LoginState::new();
        state.online = true;
        state.is_logged_in = true;
        state.offline_notice_sent = true; // 上次离线已通知过

        apply_online_status(
            &bot_id,
            GetQQLoginInfoData {
                online: Some(false),
            },
            &cfg,
            &deps,
            &mut state,
        )
        .await;

        assert!(notifier.calls().is_empty(), "must not notify twice");
        assert_eq!(restart.calls(), 1, "restart still fires per offline event");
    }

    /// offline_notice_enabled=false → 即使首次离线也
    /// 不调 notify,但 auto_restart=true 仍触发 restart
    #[tokio::test]
    async fn apply_online_offline_with_notice_disabled_skips_notify_but_restarts() {
        let bot_id = BotId::new("notice-disabled");
        let cfg = PollerConfig {
            offline_auto_restart: true,
            offline_notice_enabled: false, // 全局通知未开启
            ..PollerConfig::default()
        };
        let bus = Arc::new(BroadcastEventBus::default());
        let notifier = Arc::new(RecordingNotifier::new());
        let restart = Arc::new(RecordingRestartHandle::new());
        let deps = PollerDeps {
            event_bus: bus.clone(),
            http: Arc::new(StatusMockClient::new(vec![], vec![])),
            notifier: notifier.clone(),
            restart_handle: restart.clone(),
        };

        let mut state = LoginState::new();
        state.online = true;
        state.is_logged_in = true;

        apply_online_status(
            &bot_id,
            GetQQLoginInfoData {
                online: Some(false),
            },
            &cfg,
            &deps,
            &mut state,
        )
        .await;

        assert!(notifier.calls().is_empty());
        assert_eq!(restart.calls(), 1);
        // notice_sent 在通知未开启路径下保持 false(没人「发过」)
        assert!(!state.offline_notice_sent);
    }

    /// offline_auto_restart=false ∧ is_logged_in=true ∧
    /// offline_notice_enabled=true → 仅发 Manual 通知,不调 restart_bot
    #[tokio::test]
    async fn apply_online_logged_offline_manual_path_notifies_no_restart() {
        let bot_id = BotId::new("manual-path");
        let cfg = PollerConfig {
            offline_auto_restart: false,
            offline_notice_enabled: true,
            ..PollerConfig::default()
        };
        let bus = Arc::new(BroadcastEventBus::default());
        let notifier = Arc::new(RecordingNotifier::new());
        let restart = Arc::new(RecordingRestartHandle::new());
        let deps = PollerDeps {
            event_bus: bus.clone(),
            http: Arc::new(StatusMockClient::new(vec![], vec![])),
            notifier: notifier.clone(),
            restart_handle: restart.clone(),
        };

        let mut state = LoginState::new();
        state.online = true;
        state.is_logged_in = true;

        apply_online_status(
            &bot_id,
            GetQQLoginInfoData {
                online: Some(false),
            },
            &cfg,
            &deps,
            &mut state,
        )
        .await;

        let calls = notifier.calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].1, OfflineNoticeKind::Manual);
        assert!(state.offline_notice_sent);
        assert_eq!(restart.calls(), 0, "manual path must NOT restart");
    }

    /// 「未登录 + 不在线 + ¬kicked」 → 等扫码,发 NapCatLoginOnline{false}
    /// 后 return,无任何副作用即使 auto_restart=true 也不应触发 restart
    #[tokio::test]
    async fn apply_online_unlogged_offline_no_kick_no_side_effects() {
        let bot_id = BotId::new("unlogged-offline");
        let cfg = PollerConfig {
            offline_auto_restart: true,
            offline_notice_enabled: true,
            ..PollerConfig::default()
        };
        let bus = Arc::new(BroadcastEventBus::default());
        let mut sub = bus.subscribe(EventFilter::bot("unlogged-offline"));
        let notifier = Arc::new(RecordingNotifier::new());
        let restart = Arc::new(RecordingRestartHandle::new());
        let deps = PollerDeps {
            event_bus: bus.clone(),
            http: Arc::new(StatusMockClient::new(vec![], vec![])),
            notifier: notifier.clone(),
            restart_handle: restart.clone(),
        };

        let mut state = LoginState::new();
        // 上一轮在线,但本轮未登录(is_logged_in=false)+ 没被踢
        state.online = true;

        apply_online_status(
            &bot_id,
            GetQQLoginInfoData {
                online: Some(false),
            },
            &cfg,
            &deps,
            &mut state,
        )
        .await;

        let events = drain_events(&mut sub).await;
        assert_eq!(events.len(), 1, "expected only NapCatLoginOnline{{false}}");
        assert!(notifier.calls().is_empty());
        assert_eq!(restart.calls(), 0, "unlogged+!kicked must not restart");
    }
