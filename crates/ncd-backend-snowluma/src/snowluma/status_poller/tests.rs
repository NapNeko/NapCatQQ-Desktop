    use super::*;

    use std::collections::VecDeque;

    use async_trait::async_trait;
    use tokio::sync::Mutex as TokioMutex;

    use crate::snowluma::error::SnowLumaWebUiError;
    use crate::snowluma::proc_tree::MockProcessTreeProbe;
    use crate::snowluma::webui_client::AuthState;
    use ncd_domain::domain_event::DomainEventKind;
    use ncd_traits::events::EventFilter;

    // --- is_real_uin ---

    #[test]
    fn is_real_uin_handles_edge_cases() {
        assert!(!is_real_uin(""));
        assert!(!is_real_uin("0"));
        assert!(!is_real_uin("abc12345"));
        assert!(!is_real_uin("1234")); // len < 5
        assert!(is_real_uin("12345"));
        assert!(is_real_uin("100200"));
    }

    // --- 测试用 Mock client ---

    #[derive(Default)]
    struct MockBehavior {
        processes_responses: VecDeque<Result<Vec<HookProcessInfo>, SnowLumaWebUiError>>,
        qq_responses: VecDeque<Result<Vec<OneBotInstanceInfo>, SnowLumaWebUiError>>,
        probe_responses: VecDeque<Result<Option<QqPortLoginInfo>, SnowLumaWebUiError>>,
        last_processes: Option<Result<Vec<HookProcessInfo>, SnowLumaWebUiError>>,
        last_qq: Option<Result<Vec<OneBotInstanceInfo>, SnowLumaWebUiError>>,
        last_probe: Option<Result<Option<QqPortLoginInfo>, SnowLumaWebUiError>>,
    }

    fn clone_webui_error(e: &SnowLumaWebUiError) -> SnowLumaWebUiError {
        match e {
            SnowLumaWebUiError::Status {
                endpoint,
                status,
                message,
            } => SnowLumaWebUiError::Status {
                endpoint: endpoint.clone(),
                status: *status,
                message: message.clone(),
            },
            SnowLumaWebUiError::Timeout { endpoint } => SnowLumaWebUiError::Timeout {
                endpoint: endpoint.clone(),
            },
            SnowLumaWebUiError::Http { endpoint, cause } => SnowLumaWebUiError::Http {
                endpoint: endpoint.clone(),
                cause: cause.clone(),
            },
            SnowLumaWebUiError::Decode { endpoint, message } => SnowLumaWebUiError::Decode {
                endpoint: endpoint.clone(),
                message: message.clone(),
            },
            SnowLumaWebUiError::NotReady(d, errs) => SnowLumaWebUiError::NotReady(*d, errs.clone()),
            SnowLumaWebUiError::LoginFailed(msg) => SnowLumaWebUiError::LoginFailed(msg.clone()),
            SnowLumaWebUiError::ServerRejected { endpoint, message } => {
                SnowLumaWebUiError::ServerRejected {
                    endpoint: endpoint.clone(),
                    message: message.clone(),
                }
            }
        }
    }

    fn clone_proc_result(
        r: &Result<Vec<HookProcessInfo>, SnowLumaWebUiError>,
    ) -> Result<Vec<HookProcessInfo>, SnowLumaWebUiError> {
        match r {
            Ok(v) => Ok(v.clone()),
            Err(e) => Err(clone_webui_error(e)),
        }
    }

    fn clone_qq_result(
        r: &Result<Vec<OneBotInstanceInfo>, SnowLumaWebUiError>,
    ) -> Result<Vec<OneBotInstanceInfo>, SnowLumaWebUiError> {
        match r {
            Ok(v) => Ok(v.clone()),
            Err(e) => Err(clone_webui_error(e)),
        }
    }

    fn clone_probe_result(
        r: &Result<Option<QqPortLoginInfo>, SnowLumaWebUiError>,
    ) -> Result<Option<QqPortLoginInfo>, SnowLumaWebUiError> {
        match r {
            Ok(v) => Ok(v.clone()),
            Err(e) => Err(clone_webui_error(e)),
        }
    }

    struct MockClient {
        behavior: Arc<TokioMutex<MockBehavior>>,
    }

    impl MockClient {
        fn new() -> (Arc<Self>, Arc<TokioMutex<MockBehavior>>) {
            let behavior = Arc::new(TokioMutex::new(MockBehavior::default()));
            (
                Arc::new(Self {
                    behavior: Arc::clone(&behavior),
                }),
                behavior,
            )
        }
    }

    #[async_trait]
    impl SnowLumaWebUiClient for MockClient {
        async fn wait_ready(
            &self,
            _timeout: Duration,
            _dead_check: Box<dyn Fn() -> bool + Send + Sync>,
        ) -> Result<(), SnowLumaWebUiError> {
            Ok(())
        }
        async fn login(&self) -> Result<(), SnowLumaWebUiError> {
            Ok(())
        }
        async fn logout(&self) -> Result<(), SnowLumaWebUiError> {
            Ok(())
        }
        async fn list_processes(&self) -> Result<Vec<HookProcessInfo>, SnowLumaWebUiError> {
            let mut behavior = self.behavior.lock().await;
            if let Some(front) = behavior.processes_responses.pop_front() {
                behavior.last_processes = Some(clone_proc_result(&front));
                front
            } else if let Some(last) = &behavior.last_processes {
                clone_proc_result(last)
            } else {
                Ok(Vec::new())
            }
        }
        async fn list_qq_instances(&self) -> Result<Vec<OneBotInstanceInfo>, SnowLumaWebUiError> {
            let mut behavior = self.behavior.lock().await;
            if let Some(front) = behavior.qq_responses.pop_front() {
                behavior.last_qq = Some(clone_qq_result(&front));
                front
            } else if let Some(last) = &behavior.last_qq {
                clone_qq_result(last)
            } else {
                Ok(Vec::new())
            }
        }
        async fn probe_process_login_info(
            &self,
            _pid: u32,
        ) -> Result<Option<QqPortLoginInfo>, SnowLumaWebUiError> {
            let mut behavior = self.behavior.lock().await;
            if let Some(front) = behavior.probe_responses.pop_front() {
                behavior.last_probe = Some(clone_probe_result(&front));
                front
            } else if let Some(last) = &behavior.last_probe {
                clone_probe_result(last)
            } else {
                Ok(None)
            }
        }
        async fn load_process(&self, _pid: u32) -> Result<HookProcessInfo, SnowLumaWebUiError> {
            Err(SnowLumaWebUiError::ServerRejected {
                endpoint: "<mock>".into(),
                message: "n/a".into(),
            })
        }
        async fn unload_process(&self, _pid: u32) -> Result<HookProcessInfo, SnowLumaWebUiError> {
            Err(SnowLumaWebUiError::ServerRejected {
                endpoint: "<mock>".into(),
                message: "n/a".into(),
            })
        }
        async fn get_auth_state(&self) -> Result<AuthState, SnowLumaWebUiError> {
            Ok(AuthState::default())
        }

        async fn get_agreements(
            &self,
        ) -> Result<crate::snowluma::webui_client::AgreementsPayload, SnowLumaWebUiError> {
            Ok(crate::snowluma::webui_client::AgreementsPayload {
                version: "mock".into(),
                consent_required: false,
                documents: Vec::new(),
            })
        }

        async fn record_agreement_consent(&self, _version: &str) -> Result<(), SnowLumaWebUiError> {
            Ok(())
        }

        async fn update_onebot_config(
            &self,
            _uin: &str,
            _config: &serde_json::Value,
        ) -> Result<bool, SnowLumaWebUiError> {
            Ok(true)
        }
    }

    fn proc(pid: u32, uin: &str, status: HookProcessStatus) -> HookProcessInfo {
        HookProcessInfo {
            pid,
            name: "QQ.exe".into(),
            path: "C:/qq".into(),
            uin: uin.into(),
            status,
            error: String::new(),
        }
    }

    fn instance(uin: &str) -> OneBotInstanceInfo {
        OneBotInstanceInfo {
            uin: uin.into(),
            nickname: "test".into(),
        }
    }

    fn probe_info(pid_port: u16, uin: &str, logged_in: bool) -> QqPortLoginInfo {
        QqPortLoginInfo {
            port: pid_port,
            uin: uin.into(),
            uid: None,
            nickname: None,
            logged_in,
        }
    }

    // 占位:后续追加测试用例

    // ----- UIN 锁定行为(纯函数 + tick_once 集成) -----

    #[test]
    fn try_lock_uin_strategy_a_proc_tree_match() {
        let processes = vec![
            proc(99999, "100200", HookProcessStatus::Loaded),
            proc(12346, "100200", HookProcessStatus::Loaded),
        ];
        let qq_instances = vec![];
        let candidates: BTreeSet<u32> = [12345u32, 12346u32].into_iter().collect();
        assert_eq!(
            try_lock_uin(&processes, &qq_instances, &candidates, None),
            Some("100200".to_string())
        );
    }

    #[test]
    fn try_lock_uin_expected_matches_process_even_when_pid_differs() {
        let processes = vec![proc(140661, "572381217", HookProcessStatus::Online)];
        let qq_instances = vec![];
        let candidates = BTreeSet::from([12345u32]);
        assert_eq!(
            try_lock_uin(&processes, &qq_instances, &candidates, Some("572381217")),
            Some("572381217".to_string())
        );
    }

    #[test]
    fn try_lock_uin_expected_refuses_wrong_candidate_uin() {
        let processes = vec![proc(12346, "999999", HookProcessStatus::Online)];
        let qq_instances = vec![];
        let candidates = BTreeSet::from([12346u32]);
        assert_eq!(
            try_lock_uin(&processes, &qq_instances, &candidates, Some("100200")),
            None
        );
    }

    #[test]
    fn try_lock_uin_expected_matches_qq_list_among_multiple_instances() {
        let processes = vec![proc(140661, "0", HookProcessStatus::Loaded)];
        let qq_instances = vec![instance("999999"), instance("572381217")];
        let candidates = BTreeSet::from([12345u32]);
        assert_eq!(
            try_lock_uin(&processes, &qq_instances, &candidates, Some("572381217")),
            Some("572381217".to_string())
        );
    }

    #[test]
    fn try_lock_uin_strategy_b_processes_empty_single_qq_instance() {
        let processes: Vec<HookProcessInfo> = vec![];
        let qq_instances = vec![instance("100200")];
        let candidates = BTreeSet::from([12345u32]);
        assert_eq!(
            try_lock_uin(&processes, &qq_instances, &candidates, None),
            Some("100200".to_string())
        );
    }

    #[test]
    fn try_lock_uin_refuses_with_multiple_qq_instances() {
        let processes: Vec<HookProcessInfo> = vec![];
        let qq_instances = vec![instance("100200"), instance("999999")];
        let candidates = BTreeSet::from([12345u32]);
        assert_eq!(
            try_lock_uin(&processes, &qq_instances, &candidates, None),
            None
        );
    }

    #[test]
    fn try_lock_uin_refuses_when_processes_non_empty_no_candidate_match() {
        // processes 非空但无 candidate 命中 → 不应走 fallback B
        let processes = vec![proc(77777, "100200", HookProcessStatus::Loaded)];
        let qq_instances = vec![instance("100200")];
        let candidates = BTreeSet::from([12345u32]);
        assert_eq!(
            try_lock_uin(&processes, &qq_instances, &candidates, None),
            None
        );
    }

    #[test]
    fn try_lock_uin_rejects_invalid_uin_field() {
        let processes = vec![proc(12346, "0", HookProcessStatus::Loaded)];
        let qq_instances = vec![];
        let candidates = BTreeSet::from([12346u32]);
        assert_eq!(
            try_lock_uin(&processes, &qq_instances, &candidates, None),
            None
        );
    }

    #[test]
    fn try_lock_uin_strategy_c_single_qq_instance_when_candidate_uin_still_invalid() {
        let processes = vec![proc(12346, "0", HookProcessStatus::Loaded)];
        let qq_instances = vec![instance("100200")];
        let candidates = BTreeSet::from([12346u32]);
        assert_eq!(
            try_lock_uin(&processes, &qq_instances, &candidates, None),
            Some("100200".to_string())
        );
    }

    #[test]
    fn try_lock_uin_strategy_c_refuses_when_any_process_already_has_real_uin() {
        let processes = vec![
            proc(12346, "0", HookProcessStatus::Loaded),
            proc(77777, "999999", HookProcessStatus::Online),
        ];
        let qq_instances = vec![instance("100200")];
        let candidates = BTreeSet::from([12346u32]);
        assert_eq!(
            try_lock_uin(&processes, &qq_instances, &candidates, None),
            None
        );
    }

    // ----- 状态合成 6 分支 -----

    #[test]
    fn synthesize_state_logged_in_when_any_online() {
        let p1 = proc(1, "100200", HookProcessStatus::Loaded);
        let p2 = proc(2, "100200", HookProcessStatus::Online);
        let matched = vec![&p1, &p2];
        assert_eq!(
            synthesize_state(&matched, true),
            Some(SnowLumaLoginState::LoggedIn)
        );
    }

    #[test]
    fn synthesize_state_waiting_for_qr_scan_when_any_loaded() {
        let p1 = proc(1, "100200", HookProcessStatus::Connecting);
        let p2 = proc(2, "100200", HookProcessStatus::Loaded);
        let matched = vec![&p1, &p2];
        assert_eq!(
            synthesize_state(&matched, false),
            Some(SnowLumaLoginState::WaitingForQrScan)
        );
    }

    #[test]
    fn synthesize_state_starting_when_any_in_progress() {
        let p1 = proc(1, "100200", HookProcessStatus::Available);
        let matched = vec![&p1];
        assert_eq!(
            synthesize_state(&matched, false),
            Some(SnowLumaLoginState::Starting)
        );

        let p2 = proc(2, "100200", HookProcessStatus::Loading);
        let matched = vec![&p2];
        assert_eq!(
            synthesize_state(&matched, false),
            Some(SnowLumaLoginState::Starting)
        );

        let p3 = proc(3, "100200", HookProcessStatus::Connecting);
        let matched = vec![&p3];
        assert_eq!(
            synthesize_state(&matched, false),
            Some(SnowLumaLoginState::Starting)
        );
    }

    #[test]
    fn synthesize_state_disconnected_only_with_explicit_disconnected() {
        let p1 = proc(1, "100200", HookProcessStatus::Error);
        let p2 = proc(2, "100200", HookProcessStatus::Disconnected);
        let matched = vec![&p1, &p2];
        assert_eq!(
            synthesize_state(&matched, false),
            Some(SnowLumaLoginState::Disconnected)
        );

        let error_only = vec![&p1];
        assert_eq!(
            synthesize_state(&error_only, false),
            None,
            "上游 Error 是注入异常，不携带已登录后掉线语义"
        );
    }

    #[test]
    fn synthesize_state_logged_in_when_qq_list_has_uin_even_if_hook_disconnected() {
        let p1 = proc(1, "100200", HookProcessStatus::Disconnected);
        let matched = vec![&p1];
        assert_eq!(
            synthesize_state(&matched, true),
            Some(SnowLumaLoginState::LoggedIn)
        );
    }

    #[test]
    fn synthesize_state_fallback_logged_in_when_matched_empty_but_qq_has_uin() {
        let matched: Vec<&HookProcessInfo> = vec![];
        assert_eq!(
            synthesize_state(&matched, true),
            Some(SnowLumaLoginState::LoggedIn)
        );
    }

    #[test]
    fn synthesize_state_none_when_matched_empty_and_qq_lacks_uin() {
        let matched: Vec<&HookProcessInfo> = vec![];
        assert_eq!(synthesize_state(&matched, false), None);
    }

    // ----- 主循环 tick_once 行为:用 deps 注入 mock 直接触发单轮 -----

    fn build_test_deps(
        client: Arc<dyn SnowLumaWebUiClient>,
        proc_tree: Arc<dyn ProcessTreeProbe>,
    ) -> (PollerDeps, Arc<BroadcastEventBus>) {
        build_test_deps_with_expected(client, proc_tree, None)
    }

    fn build_test_deps_with_expected(
        client: Arc<dyn SnowLumaWebUiClient>,
        proc_tree: Arc<dyn ProcessTreeProbe>,
        expected_uin: Option<String>,
    ) -> (PollerDeps, Arc<BroadcastEventBus>) {
        let bus = Arc::new(BroadcastEventBus::default());
        let deps = PollerDeps {
            event_bus: Arc::clone(&bus),
            http: client,
            proc_tree,
            expected_uin,
        };
        (deps, bus)
    }

    #[tokio::test]
    async fn tick_once_locks_uin_and_emits_logged_in() {
        let (client, behavior) = MockClient::new();
        {
            let mut b = behavior.lock().await;
            b.processes_responses.push_back(Ok(vec![proc(
                12346,
                "100200",
                HookProcessStatus::Online,
            )]));
            b.qq_responses.push_back(Ok(vec![instance("100200")]));
        }
        let probe: Arc<dyn ProcessTreeProbe> =
            Arc::new(MockProcessTreeProbe::with_set([12345u32, 12346u32]));
        let (deps, bus) = build_test_deps(client, probe);

        let bot_id = BotId::new("10001");
        let mut sub = bus.subscribe(EventFilter::all());
        let mut state = PollerState::new(12345);
        tick_once(&bot_id, &deps, &mut state).await;

        assert_eq!(state.uin.as_deref(), Some("100200"));
        assert_eq!(state.last_state, Some(SnowLumaLoginState::LoggedIn));

        // 收事件:UinDetected → LoginStateChanged{LoggedIn} → PidSetChanged
        let mut got_uin = false;
        let mut got_state = false;
        let mut got_pids = false;
        for _ in 0..3 {
            let evt = tokio::time::timeout(Duration::from_secs(1), sub.next())
                .await
                .expect("event in 1s")
                .expect("subscription open");
            match evt {
                DomainEvent::SnowLumaUinDetected { uin, .. } => {
                    assert_eq!(uin, "100200");
                    got_uin = true;
                }
                DomainEvent::SnowLumaLoginStateChanged { state, .. } => {
                    assert_eq!(state, SnowLumaLoginState::LoggedIn);
                    got_state = true;
                }
                DomainEvent::SnowLumaPidSetChanged { pids, .. } => {
                    assert_eq!(pids, vec![12346]);
                    got_pids = true;
                }
                other => panic!("unexpected event {other:?}"),
            }
        }
        assert!(got_uin && got_state && got_pids);
    }

    #[tokio::test]
    async fn tick_once_locks_expected_uin_when_webui_pid_differs_from_launch_pid() {
        let (client, behavior) = MockClient::new();
        {
            let mut b = behavior.lock().await;
            b.processes_responses.push_back(Ok(vec![proc(
                140661,
                "572381217",
                HookProcessStatus::Online,
            )]));
            b.qq_responses.push_back(Ok(vec![instance("572381217")]));
        }
        let probe: Arc<dyn ProcessTreeProbe> = Arc::new(MockProcessTreeProbe::with_set([12345u32]));
        let (deps, bus) =
            build_test_deps_with_expected(client, probe, Some("572381217".to_string()));

        let bot_id = BotId::new("572381217");
        let mut sub = bus.subscribe(EventFilter::all());
        let mut state = PollerState::new(12345);
        tick_once(&bot_id, &deps, &mut state).await;

        assert_eq!(state.uin.as_deref(), Some("572381217"));
        assert_eq!(state.locked_pid, Some(140661));
        assert_eq!(state.last_state, Some(SnowLumaLoginState::LoggedIn));

        let mut got_uin = false;
        let mut got_state = false;
        let mut got_pids = false;
        for _ in 0..3 {
            let evt = tokio::time::timeout(Duration::from_secs(1), sub.next())
                .await
                .expect("event in 1s")
                .expect("subscription open");
            match evt {
                DomainEvent::SnowLumaUinDetected { uin, .. } => {
                    assert_eq!(uin, "572381217");
                    got_uin = true;
                }
                DomainEvent::SnowLumaLoginStateChanged { state, .. } => {
                    assert_eq!(state, SnowLumaLoginState::LoggedIn);
                    got_state = true;
                }
                DomainEvent::SnowLumaPidSetChanged { pids, .. } => {
                    assert_eq!(pids, vec![140661]);
                    got_pids = true;
                }
                other => panic!("unexpected event {other:?}"),
            }
        }
        assert!(got_uin && got_state && got_pids);
    }

    #[tokio::test]
    async fn tick_once_locks_uin_from_qq_list_after_manual_scan() {
        let (client, behavior) = MockClient::new();
        {
            let mut b = behavior.lock().await;
            b.processes_responses
                .push_back(Ok(vec![proc(12346, "0", HookProcessStatus::Loaded)]));
            b.qq_responses.push_back(Ok(vec![]));
            b.processes_responses
                .push_back(Ok(vec![proc(12346, "0", HookProcessStatus::Loaded)]));
            b.qq_responses.push_back(Ok(vec![instance("100200")]));
        }
        let probe: Arc<dyn ProcessTreeProbe> =
            Arc::new(MockProcessTreeProbe::with_set([12345u32, 12346u32]));
        let (deps, bus) = build_test_deps(client, probe);

        let bot_id = BotId::new("10001");
        let mut sub = bus.subscribe(EventFilter::all());
        let mut state = PollerState::new(12345);

        tick_once(&bot_id, &deps, &mut state).await;
        assert_eq!(state.uin, None);
        assert_eq!(
            state.last_state,
            Some(SnowLumaLoginState::WaitingForQrScan)
        );
        let event = tokio::time::timeout(Duration::from_secs(1), sub.next())
            .await
            .expect("pre-login state")
            .expect("subscription open");
        match event {
            DomainEvent::SnowLumaLoginStateChanged { state, .. } => {
                assert_eq!(state, SnowLumaLoginState::WaitingForQrScan);
            }
            other => panic!("expected LoginStateChanged, got {other:?}"),
        }

        tick_once(&bot_id, &deps, &mut state).await;
        assert_eq!(state.uin.as_deref(), Some("100200"));
        assert_eq!(state.last_state, Some(SnowLumaLoginState::LoggedIn));

        let evt = tokio::time::timeout(Duration::from_secs(1), sub.next())
            .await
            .expect("UIN detected within 1s")
            .expect("subscription open");
        match evt {
            DomainEvent::SnowLumaUinDetected { uin, .. } => assert_eq!(uin, "100200"),
            other => panic!("expected UinDetected, got {other:?}"),
        }

        let evt = tokio::time::timeout(Duration::from_secs(1), sub.next())
            .await
            .expect("LoggedIn within 1s")
            .expect("subscription open");
        match evt {
            DomainEvent::SnowLumaLoginStateChanged { state, .. } => {
                assert_eq!(state, SnowLumaLoginState::LoggedIn);
            }
            other => panic!("expected LoginStateChanged, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn tick_once_locks_uin_from_probe_login_when_hook_stays_loaded() {
        let (client, behavior) = MockClient::new();
        {
            let mut b = behavior.lock().await;
            b.processes_responses
                .push_back(Ok(vec![proc(12346, "0", HookProcessStatus::Loaded)]));
            b.qq_responses.push_back(Ok(vec![]));
            b.probe_responses
                .push_back(Ok(Some(probe_info(4301, "100200", true))));
        }
        let probe: Arc<dyn ProcessTreeProbe> =
            Arc::new(MockProcessTreeProbe::with_set([12345u32, 12346u32]));
        let (deps, bus) = build_test_deps(client, probe);

        let bot_id = BotId::new("10001");
        let mut sub = bus.subscribe(EventFilter::all());
        let mut state = PollerState::new(12345);
        tick_once(&bot_id, &deps, &mut state).await;

        assert_eq!(state.uin.as_deref(), Some("100200"));
        assert_eq!(state.locked_pid, Some(12346));
        assert_eq!(state.last_state, Some(SnowLumaLoginState::LoggedIn));

        let evt = tokio::time::timeout(Duration::from_secs(1), sub.next())
            .await
            .expect("UIN detected within 1s")
            .expect("subscription open");
        match evt {
            DomainEvent::SnowLumaUinDetected { uin, .. } => assert_eq!(uin, "100200"),
            other => panic!("expected UinDetected, got {other:?}"),
        }

        let evt = tokio::time::timeout(Duration::from_secs(1), sub.next())
            .await
            .expect("LoggedIn within 1s")
            .expect("subscription open");
        match evt {
            DomainEvent::SnowLumaLoginStateChanged { state, .. } => {
                assert_eq!(state, SnowLumaLoginState::LoggedIn);
            }
            other => panic!("expected LoginStateChanged, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn tick_once_keeps_logged_in_via_probe_when_hook_uin_remains_invalid() {
        let (client, behavior) = MockClient::new();
        {
            let mut b = behavior.lock().await;
            b.processes_responses
                .push_back(Ok(vec![proc(12346, "0", HookProcessStatus::Loaded)]));
            b.qq_responses.push_back(Ok(vec![]));
            b.probe_responses
                .push_back(Ok(Some(probe_info(4301, "100200", true))));
            b.processes_responses
                .push_back(Ok(vec![proc(12346, "0", HookProcessStatus::Loaded)]));
            b.qq_responses.push_back(Ok(vec![]));
            b.probe_responses
                .push_back(Ok(Some(probe_info(4301, "100200", true))));
        }
        let probe: Arc<dyn ProcessTreeProbe> =
            Arc::new(MockProcessTreeProbe::with_set([12345u32, 12346u32]));
        let (deps, bus) = build_test_deps(client, probe);

        let bot_id = BotId::new("10001");
        let mut sub = bus.subscribe(EventFilter::kind(
            DomainEventKind::SnowLumaLoginStateChanged,
        ));
        let mut state = PollerState::new(12345);

        tick_once(&bot_id, &deps, &mut state).await;
        let evt = tokio::time::timeout(Duration::from_secs(1), sub.next())
            .await
            .expect("first LoggedIn within 1s")
            .expect("subscription open");
        match evt {
            DomainEvent::SnowLumaLoginStateChanged { state, .. } => {
                assert_eq!(state, SnowLumaLoginState::LoggedIn);
            }
            other => panic!("expected LoginStateChanged, got {other:?}"),
        }

        tick_once(&bot_id, &deps, &mut state).await;
        assert_eq!(state.last_state, Some(SnowLumaLoginState::LoggedIn));
        let r = tokio::time::timeout(Duration::from_millis(200), sub.next()).await;
        assert!(r.is_err(), "no downgrade while probe still sees logged in");
    }

    #[tokio::test]
    async fn tick_once_consecutive_failures_emit_probe_unavailable_only_once() {
        let (client, behavior) = MockClient::new();
        {
            let mut b = behavior.lock().await;
            // 让两端都失败,复用最后一条
            b.processes_responses
                .push_back(Err(SnowLumaWebUiError::Timeout {
                    endpoint: "/api/processes".into(),
                }));
            b.qq_responses.push_back(Err(SnowLumaWebUiError::Timeout {
                endpoint: "/api/qq-list".into(),
            }));
        }
        let probe: Arc<dyn ProcessTreeProbe> = Arc::new(MockProcessTreeProbe::new());
        let (deps, bus) = build_test_deps(client, probe);

        let bot_id = BotId::new("10001");
        let mut sub = bus.subscribe(EventFilter::kind(
            DomainEventKind::SnowLumaLoginProbeUnavailable,
        ));
        let mut state = PollerState::new(12345);
        state.last_state = Some(SnowLumaLoginState::LoggedIn);

        // 第一次失败:未达门限,不应发任何事件
        tick_once(&bot_id, &deps, &mut state).await;
        let r = tokio::time::timeout(Duration::from_millis(200), sub.next()).await;
        assert!(r.is_err(), "no event before threshold");

        // 第 2 次失败:达到门限,只报告探测不可用
        tick_once(&bot_id, &deps, &mut state).await;
        let evt = tokio::time::timeout(Duration::from_secs(1), sub.next())
            .await
            .expect("probe unavailable within 1s")
            .expect("subscription open");
        match evt {
            DomainEvent::SnowLumaLoginProbeUnavailable { bot_id: id } => {
                assert_eq!(id, bot_id);
            }
            other => panic!("expected LoginProbeUnavailable, got {other:?}"),
        }
        assert_eq!(state.last_state, Some(SnowLumaLoginState::LoggedIn));

        // 后续失败不重复发，也不能把 last_state 改成 Disconnected
        tick_once(&bot_id, &deps, &mut state).await;
        tick_once(&bot_id, &deps, &mut state).await;
        let r = tokio::time::timeout(Duration::from_millis(200), sub.next()).await;
        assert!(r.is_err(), "no duplicate probe unavailable after threshold");
        assert_eq!(state.last_state, Some(SnowLumaLoginState::LoggedIn));
    }

    #[tokio::test]
    async fn tick_once_pid_set_change_emits_event() {
        let (client, behavior) = MockClient::new();
        let probe: Arc<dyn ProcessTreeProbe> = Arc::new(MockProcessTreeProbe::with_set([
            12345u32, 12346u32, 12347u32,
        ]));
        let (deps, bus) = build_test_deps(client, probe);
        let bot_id = BotId::new("10001");
        let mut sub = bus.subscribe(EventFilter::kind(DomainEventKind::SnowLumaPidSetChanged));
        let mut state = PollerState::new(12345);

        // 第 1 轮:matched 集合 {12346}
        {
            let mut b = behavior.lock().await;
            b.processes_responses.push_back(Ok(vec![proc(
                12346,
                "100200",
                HookProcessStatus::Online,
            )]));
            b.qq_responses.push_back(Ok(vec![]));
        }
        tick_once(&bot_id, &deps, &mut state).await;
        let evt = tokio::time::timeout(Duration::from_secs(1), sub.next())
            .await
            .expect("first PidSet event")
            .expect("open");
        match evt {
            DomainEvent::SnowLumaPidSetChanged { pids, .. } => {
                assert_eq!(pids, vec![12346]);
            }
            o => panic!("{o:?}"),
        }

        // 第 2 轮:matched 集合变成 {12346, 12347}
        {
            let mut b = behavior.lock().await;
            b.processes_responses.push_back(Ok(vec![
                proc(12346, "100200", HookProcessStatus::Online),
                proc(12347, "100200", HookProcessStatus::Online),
            ]));
            b.qq_responses.push_back(Ok(vec![]));
        }
        tick_once(&bot_id, &deps, &mut state).await;
        let evt = tokio::time::timeout(Duration::from_secs(1), sub.next())
            .await
            .expect("second PidSet event")
            .expect("open");
        match evt {
            DomainEvent::SnowLumaPidSetChanged { pids, .. } => {
                assert_eq!(pids, vec![12346, 12347]);
            }
            o => panic!("{o:?}"),
        }
    }

    #[tokio::test]
    async fn tick_once_reports_waiting_for_qr_before_uin_is_locked() {
        let (client, behavior) = MockClient::new();
        {
            let mut b = behavior.lock().await;
            b.processes_responses
                .push_back(Ok(vec![proc(12346, "0", HookProcessStatus::Loaded)]));
            b.qq_responses.push_back(Ok(vec![]));
        }
        let probe: Arc<dyn ProcessTreeProbe> =
            Arc::new(MockProcessTreeProbe::with_set([12345u32, 12346u32]));
        let (deps, bus) = build_test_deps(client, probe);
        let bot_id = BotId::new("10001");
        let mut sub = bus.subscribe(EventFilter::kind(
            DomainEventKind::SnowLumaLoginStateChanged,
        ));
        let mut state = PollerState::new(12345);

        tick_once(&bot_id, &deps, &mut state).await;

        let event = tokio::time::timeout(Duration::from_secs(1), sub.next())
            .await
            .expect("login state event")
            .expect("subscription open");
        match event {
            DomainEvent::SnowLumaLoginStateChanged { state: next, .. } => {
                assert_eq!(next, SnowLumaLoginState::WaitingForQrScan);
            }
            other => panic!("expected LoginStateChanged, got {other:?}"),
        }
        assert_eq!(state.uin, None);
        assert_eq!(state.last_state, Some(SnowLumaLoginState::WaitingForQrScan));
    }

    #[tokio::test]
    async fn recovered_probe_republishes_same_confirmed_state() {
        let bus = Arc::new(BroadcastEventBus::default());
        let mut sub = bus.subscribe(EventFilter::kind(
            DomainEventKind::SnowLumaLoginStateChanged,
        ));
        let mut state = PollerState::new(12345);
        let (client, _b) = MockClient::new();
        let probe: Arc<dyn ProcessTreeProbe> = Arc::new(MockProcessTreeProbe::new());
        let deps = PollerDeps {
            event_bus: Arc::clone(&bus),
            http: client,
            proc_tree: probe,
            expected_uin: None,
        };
        let bot_id = BotId::new("10001");

        state.last_state = Some(SnowLumaLoginState::LoggedIn);
        state.probe_unavailable_published = true;
        publish_login_state_if_needed(
            &bot_id,
            &deps,
            &mut state,
            SnowLumaLoginState::LoggedIn,
        );
        let evt = tokio::time::timeout(Duration::from_secs(1), sub.next())
            .await
            .expect("recovered state")
            .expect("open");
        match evt {
            DomainEvent::SnowLumaLoginStateChanged { state: s, .. } => {
                assert_eq!(s, SnowLumaLoginState::LoggedIn);
            }
            o => panic!("{o:?}"),
        }
        assert!(!state.probe_unavailable_published);

        // 恢复事件已补发，相同状态不再重复。
        publish_login_state_if_needed(
            &bot_id,
            &deps,
            &mut state,
            SnowLumaLoginState::LoggedIn,
        );
        let r = tokio::time::timeout(Duration::from_millis(200), sub.next()).await;
        assert!(r.is_err());
    }

    /// 曾在线(LoggedIn)后全信号消失:matched 空 + qq-list 空 + probe 无。
    /// 连续 NO_SIGNAL_THRESHOLD 轮才发 Disconnected;前两轮不报(防抖)
    #[tokio::test]
    async fn tick_once_emits_disconnected_after_signal_loss_threshold() {
        let (client, behavior) = MockClient::new();
        {
            let mut b = behavior.lock().await;
            // 全信号消失:processes 空 + qq-list 空(MockClient 复用 last,一轮即固定)
            b.processes_responses.push_back(Ok(vec![]));
            b.qq_responses.push_back(Ok(vec![]));
        }
        let probe: Arc<dyn ProcessTreeProbe> = Arc::new(MockProcessTreeProbe::new());
        let (deps, bus) = build_test_deps(client, probe);

        let bot_id = BotId::new("10001");
        let mut sub = bus.subscribe(EventFilter::kind(
            DomainEventKind::SnowLumaLoginStateChanged,
        ));
        let mut state = PollerState::new(12345);
        // 预置:已锁 UIN + locked_pid + 曾在线
        state.uin = Some("100200".to_string());
        state.locked_pid = Some(12346);
        state.last_state = Some(SnowLumaLoginState::LoggedIn);

        // 第 1/2 轮:consecutive_no_signal 1,2 < 3,不发事件
        tick_once(&bot_id, &deps, &mut state).await;
        tick_once(&bot_id, &deps, &mut state).await;
        let r = tokio::time::timeout(Duration::from_millis(200), sub.next()).await;
        assert!(r.is_err(), "前两轮不应发事件(防抖)");

        // 第 3 轮:达阈值,发 Disconnected
        tick_once(&bot_id, &deps, &mut state).await;
        let evt = tokio::time::timeout(Duration::from_secs(1), sub.next())
            .await
            .expect("Disconnected within 1s")
            .expect("open");
        match evt {
            DomainEvent::SnowLumaLoginStateChanged { state: s, .. } => {
                assert_eq!(s, SnowLumaLoginState::Disconnected);
            }
            o => panic!("expected Disconnected, got {o:?}"),
        }
    }

    /// UIN 已锁但 locked_pid None(hook 未 login,process.uin 空)→ probe 候选
    /// pid 补上 locked_pid,probe 成功隐含 logged_in → 合成 LoggedIn
    #[tokio::test]
    async fn tick_once_probes_to_recover_locked_pid_when_uin_locked_but_pid_missing() {
        let (client, behavior) = MockClient::new();
        {
            let mut b = behavior.lock().await;
            // process.uin="0"(hook 未 login),但 probe 能拿到真实 uin
            b.processes_responses
                .push_back(Ok(vec![proc(12346, "0", HookProcessStatus::Loaded)]));
            b.qq_responses.push_back(Ok(vec![]));
            b.probe_responses
                .push_back(Ok(Some(probe_info(4301, "100200", true))));
        }
        let probe: Arc<dyn ProcessTreeProbe> = Arc::new(MockProcessTreeProbe::new());
        let (deps, bus) = build_test_deps(client, probe);

        let bot_id = BotId::new("10001");
        let mut sub = bus.subscribe(EventFilter::kind(
            DomainEventKind::SnowLumaLoginStateChanged,
        ));
        let mut state = PollerState::new(12345);
        state.uin = Some("100200".to_string());
        state.locked_pid = None; // 关键:UIN 已锁但 pid 没
        state.last_state = None;

        tick_once(&bot_id, &deps, &mut state).await;

        // probe 补上了 locked_pid
        assert_eq!(state.locked_pid, Some(12346));
        // probe_has_uin=true → 合成 LoggedIn
        assert_eq!(state.last_state, Some(SnowLumaLoginState::LoggedIn));

        let evt = tokio::time::timeout(Duration::from_secs(1), sub.next())
            .await
            .expect("LoggedIn within 1s")
            .expect("open");
        match evt {
            DomainEvent::SnowLumaLoginStateChanged { state: s, .. } => {
                assert_eq!(s, SnowLumaLoginState::LoggedIn);
            }
            o => panic!("expected LoggedIn, got {o:?}"),
        }
    }
