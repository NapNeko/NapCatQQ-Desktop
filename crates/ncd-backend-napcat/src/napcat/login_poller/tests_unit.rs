    use super::*;

    use super::super::offline_notifier::NoopOfflineNotifier;
    use ncd_traits::events::EventFilter;

    // ── 既有测试:确保骨架未被破坏 ──

    #[test]
    fn login_state_new_initializes_defaults() {
        let state = LoginState::new();

        assert!(state.auth.is_none());
        assert!(!state.is_logged_in);
        assert!(!state.online);
        assert!(!state.offline_notice_sent);
        assert!(!state.login_invalidated_while_online);
        assert!(!state.suppress_qrcode_until_online);
        // 从未尝试 → 首次 RequestAuthRefresh 必过节流
        assert!(state.last_auth_refresh_attempt_at.is_none());
        assert!(state.auth_refresh_throttle_elapsed(Duration::from_secs(5)));
    }

    #[test]
    fn poller_config_defaults_match_design() {
        let cfg = PollerConfig::default();

        assert_eq!(cfg.login_check_interval, Duration::from_millis(5000));
        assert_eq!(cfg.unlogged_interval, Duration::from_secs(1));
        assert_eq!(cfg.auth_refresh_period, Duration::from_secs(30 * 60));
        assert_eq!(cfg.auth_refresh_throttle, Duration::from_secs(5));
        assert_eq!(cfg.http_timeout, Duration::from_secs(5));
        assert!(!cfg.offline_auto_restart);
        assert!(!cfg.offline_notice_enabled);
    }

    #[test]
    fn dispose_cancels_token_and_drop_is_idempotent() {
        let bot_id = BotId::new("10001");
        let cancel = CancellationToken::new();
        let poller = NapCatLoginPoller {
            bot_id: bot_id.clone(),
            cancel: cancel.clone(),
        };

        assert_eq!(poller.bot_id(), &bot_id);
        assert!(!cancel.is_cancelled());

        poller.dispose();
        assert!(cancel.is_cancelled());

        // dispose 多次幂等
        poller.dispose();
        assert!(cancel.is_cancelled());

        // Drop 兜底:再造一个 poller 不调 dispose,drop 后 token 被取消
        let cancel2 = CancellationToken::new();
        {
            let _p = NapCatLoginPoller {
                bot_id,
                cancel: cancel2.clone(),
            };
        }
        assert!(cancel2.is_cancelled());
    }

    // ── 单元测试:adjust_status_interval ──
    //
    // tokio::time::interval 必须在 Tokio runtime 上下文里调用,因此这三个
    // 测试用 #[tokio::test] 而非 #[test]

    #[tokio::test]
    async fn adjust_status_interval_keeps_unlogged_period_when_not_logged_in() {
        let cfg = PollerConfig::default();
        let state = LoginState::new();
        let mut ticker = interval(cfg.unlogged_interval);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let mut current = cfg.unlogged_interval;

        adjust_status_interval(&mut ticker, &mut current, &state, &cfg);

        assert_eq!(current, cfg.unlogged_interval);
    }

    #[tokio::test]
    async fn adjust_status_interval_switches_to_login_period_when_logged_in() {
        let cfg = PollerConfig::default();
        let mut state = LoginState::new();
        state.is_logged_in = true;
        let mut ticker = interval(cfg.unlogged_interval);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let mut current = cfg.unlogged_interval;

        adjust_status_interval(&mut ticker, &mut current, &state, &cfg);

        assert_eq!(current, cfg.login_check_interval);
        assert_ne!(current, cfg.unlogged_interval);
    }

    #[tokio::test]
    async fn adjust_status_interval_switches_back_to_unlogged_period() {
        let cfg = PollerConfig::default();
        let mut state = LoginState::new();
        state.is_logged_in = true;
        let mut ticker = interval(cfg.login_check_interval);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
        let mut current = cfg.login_check_interval;

        // 1) 已登录 → 保持 5s
        adjust_status_interval(&mut ticker, &mut current, &state, &cfg);
        assert_eq!(current, cfg.login_check_interval);

        // 2) 退出登录 → 切回 1s
        state.is_logged_in = false;
        adjust_status_interval(&mut ticker, &mut current, &state, &cfg);
        assert_eq!(current, cfg.unlogged_interval);
    }

    // ── smoke test:取消时主循环退出并补发 QrcodeRemoved ──

    /// 验证取消语义:dispose() 触发的 cancel.cancelled() 分支
    /// (biased; 优先)必须在退出前发布且仅发布一次
    /// NapCatLoginQrcodeRemoved
    ///
    /// 用 start_paused = true 避免真实时间影响 ticker 行为:tokio 的
    /// Interval 首次 tick 在创建时刻立即可用,因此即使时间被冻结,
    /// 主循环仍能完成「初始 setup → 进入 select! 主循环 → 收到取消信号」
    /// 的全过程
    #[tokio::test(start_paused = true)]
    async fn run_poller_publishes_qrcode_removed_on_cancellation() {
        // 此 smoke test 关心的是「取消优先于 ticker」的语义,因此 stub client 让
        // fetch_credential 失败 → state.auth 始终为 None → 初次
        // do_status_poll 走 RequestAuthRefresh 命令路径,不发布任何
        // DomainEvent状态转移分支由 transition_tests 模块单独覆盖
        struct StubClient;
        #[async_trait]
        impl NapCatWebUiClient for StubClient {
            async fn fetch_credential(
                &self,
                _port: u16,
                _token: &str,
            ) -> Result<String, NapCatWebUiError> {
                Err(NapCatWebUiError::Decode("stub".into()))
            }
            async fn check_login_status(
                &self,
                _port: u16,
                _auth: &str,
            ) -> Result<CheckLoginStatusData, NapCatWebUiError> {
                Err(NapCatWebUiError::Decode("stub".into()))
            }
            async fn check_online_status(
                &self,
                _port: u16,
                _auth: &str,
            ) -> Result<GetQQLoginInfoData, NapCatWebUiError> {
                Err(NapCatWebUiError::Decode("stub".into()))
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

        struct StubRestart;
        #[async_trait]
        impl RestartHandle for StubRestart {
            async fn restart_bot(&self, _bot_id: &BotId) {}
        }

        let bus = Arc::new(BroadcastEventBus::default());
        // 必须先订阅再 spawn,否则 broadcast 通道还没有 receiver,
        // 取消时发的事件会被静默丢弃
        let mut sub = bus.subscribe(EventFilter::bot("smoke-bot"));

        let deps = PollerDeps {
            event_bus: bus.clone(),
            http: Arc::new(StubClient),
            notifier: Arc::new(NoopOfflineNotifier),
            restart_handle: Arc::new(StubRestart),
        };

        let poller = NapCatLoginPoller::spawn(
            BotId::new("smoke-bot"),
            6099,
            "smoke-token".to_string(),
            PollerConfig::default(),
            deps,
        );

        // 让 spawn 出去的任务跑完初始 setup 并阻塞在 select! 上
        // yield_now 把当前任务让出给运行时;运行时会调度 poller 任务直到
        // 它再次 park,然后回到测试任务
        tokio::task::yield_now().await;

        // 触发取消;biased; 让 cancel 分支在下一次 select! 中优先 fire
        poller.dispose();

        // sub.next().await 会再次让出执行权,运行时调度 poller 任务,
        // poller 发布 QrcodeRemoved 后 break,事件被广播到订阅者
        let event = sub
            .next()
            .await
            .expect("subscription should yield the terminal event");
        match event {
            DomainEvent::NapCatLoginQrcodeRemoved { bot_id } => {
                assert_eq!(bot_id, BotId::new("smoke-bot"));
            }
            other => panic!("expected NapCatLoginQrcodeRemoved on cancel, got {other:?}"),
        }
    }

    // =====================================================================
    // 单元测试:do_auth_refresh
    // =====================================================================
    //
    // 直接驱动私有的 do_auth_refresh,不走 run_poller,用最小可编译的
    // mock client 控制返回值,避免 ticker / 取消等其他变量干扰

    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// 可编程的 mock client:fetch_credential 返回值由测试预设
    /// check_login_status / check_online_status 返回 default(auth 测试不会触发)
    struct AuthMockClient {
        /// Ok(credential) 或 Err(NapCatWebUiError) 序列;按调用顺序消费
        fetch_responses: Mutex<Vec<Result<String, NapCatWebUiError>>>,
        /// fetch_credential 实际被调用的次数,断言节流 / no-op 路径用
        fetch_calls: AtomicUsize,
    }

    impl AuthMockClient {
        fn new(responses: Vec<Result<String, NapCatWebUiError>>) -> Self {
            Self {
                fetch_responses: Mutex::new(responses),
                fetch_calls: AtomicUsize::new(0),
            }
        }

        fn calls(&self) -> usize {
            self.fetch_calls.load(Ordering::SeqCst)
        }
    }

    #[async_trait]
    impl NapCatWebUiClient for AuthMockClient {
        async fn fetch_credential(
            &self,
            _port: u16,
            _token: &str,
        ) -> Result<String, NapCatWebUiError> {
            self.fetch_calls.fetch_add(1, Ordering::SeqCst);
            let mut queue = self.fetch_responses.lock().expect("mock mutex poisoned");
            queue
                .pop()
                .unwrap_or_else(|| Err(NapCatWebUiError::Decode("queue empty".into())))
        }

        async fn check_login_status(
            &self,
            _port: u16,
            _auth: &str,
        ) -> Result<CheckLoginStatusData, NapCatWebUiError> {
            Ok(CheckLoginStatusData::default())
        }

        async fn check_online_status(
            &self,
            _port: u16,
            _auth: &str,
        ) -> Result<GetQQLoginInfoData, NapCatWebUiError> {
            Ok(GetQQLoginInfoData::default())
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

    struct StubRestart;
    #[async_trait]
    impl RestartHandle for StubRestart {
        async fn restart_bot(&self, _bot_id: &BotId) {}
    }

    /// 构造仅给 do_auth_refresh 用的最小 PollerDeps
    fn auth_test_deps(client: Arc<AuthMockClient>) -> (PollerDeps, Arc<BroadcastEventBus>) {
        let bus = Arc::new(BroadcastEventBus::default());
        let deps = PollerDeps {
            event_bus: bus.clone(),
            http: client,
            notifier: Arc::new(NoopOfflineNotifier),
            restart_handle: Arc::new(StubRestart),
        };
        (deps, bus)
    }

    /// fetch_credential 成功 → state.auth = Some(credential)
    /// last_auth_refresh_attempt_at 在调用前被刷新,
    /// 因此调用结束时 elapsed() 远小于 auth_refresh_throttle
    #[tokio::test]
    async fn do_auth_refresh_success_writes_auth_and_updates_timestamp() {
        let bot_id = BotId::new("auth-success");
        let cfg = PollerConfig::default();
        let client = Arc::new(AuthMockClient::new(vec![Ok("bearer-xyz".into())]));
        let (deps, _bus) = auth_test_deps(client.clone());
        let mut state = LoginState::new();
        assert!(state.last_auth_refresh_attempt_at.is_none());

        do_auth_refresh(&bot_id, 6099, "smoke-token", &cfg, &deps, &mut state).await;

        assert_eq!(state.auth.as_deref(), Some("bearer-xyz"));
        assert_eq!(client.calls(), 1);
        let ts = state
            .last_auth_refresh_attempt_at
            .expect("last_auth_refresh_attempt_at must be set after attempt");
        assert!(
            ts.elapsed() < cfg.auth_refresh_throttle,
            "post-call elapsed must be within throttle window"
        );
        assert!(!state.auth_refresh_throttle_elapsed(cfg.auth_refresh_throttle));
    }

    /// fetch_credential 失败仅 tracing::warn!,
    /// 不发布 DomainEvent::BotError,不修改 state.auth,但 timestamp
    /// 仍在调用前被刷新(节流计时从「尝试」时刻起算,与失败无关)
    #[tokio::test]
    async fn do_auth_refresh_failure_leaves_auth_untouched_and_emits_no_bot_error() {
        let bot_id = BotId::new("auth-fail");
        let cfg = PollerConfig::default();
        let client = Arc::new(AuthMockClient::new(vec![Err(NapCatWebUiError::Timeout)]));
        let (deps, bus) = auth_test_deps(client.clone());
        // 订阅 bot-scoped 事件流:捕获任何 BotError 即视为违规
        let mut sub = bus.subscribe(EventFilter::bot("auth-fail"));

        let mut state = LoginState::new();
        assert!(state.last_auth_refresh_attempt_at.is_none());

        do_auth_refresh(&bot_id, 6099, "smoke-token", &cfg, &deps, &mut state).await;

        // auth 保持调用前值不动(None)
        assert!(
            state.auth.is_none(),
            "auth must remain unchanged on failure"
        );
        assert_eq!(client.calls(), 1);
        // timestamp 仍被写入(节流计时从「尝试」起算)
        assert!(
            state.last_auth_refresh_attempt_at.is_some(),
            "last_auth_refresh_attempt_at must be set even on failure"
        );

        // 验证没有任何事件被发布到 bot 流(包括 BotError)
        // 用一个极短超时让 broadcast receiver 报告「无消息」
        let recv = tokio::time::timeout(Duration::from_millis(10), sub.next()).await;
        assert!(
            recv.is_err(),
            "do_auth_refresh failure path must not publish any DomainEvent, got {:?}",
            recv
        );
    }

    /// 成功路径覆盖「先有 auth,再次刷新成功」覆盖原值
    #[tokio::test]
    async fn do_auth_refresh_success_overwrites_previous_auth() {
        let bot_id = BotId::new("auth-overwrite");
        let cfg = PollerConfig::default();
        let client = Arc::new(AuthMockClient::new(vec![Ok("new-credential".into())]));
        let (deps, _bus) = auth_test_deps(client);
        let mut state = LoginState::new();
        state.auth = Some("old-credential".into());

        do_auth_refresh(&bot_id, 6099, "smoke-token", &cfg, &deps, &mut state).await;

        assert_eq!(state.auth.as_deref(), Some("new-credential"));
    }

    /// 边界:Unauthorized 错误也走 tracing::warn! 路径,行为与
    /// 其它错误一致——do_auth_refresh 内部对错误类型不做分支
    /// (内部只要求「仅日志」),分流由 caller 负责
    #[tokio::test]
    async fn do_auth_refresh_unauthorized_error_does_not_modify_auth() {
        let bot_id = BotId::new("auth-unauth");
        let cfg = PollerConfig::default();
        let client = Arc::new(AuthMockClient::new(vec![Err(
            NapCatWebUiError::Unauthorized(401),
        )]));
        let (deps, _bus) = auth_test_deps(client.clone());
        let mut state = LoginState::new();
        state.auth = Some("stale-credential".into());

        do_auth_refresh(&bot_id, 6099, "smoke-token", &cfg, &deps, &mut state).await;

        // 失败仅日志,state.auth 保持调用前的值不动
        assert_eq!(state.auth.as_deref(), Some("stale-credential"));
        assert_eq!(client.calls(), 1);
    }

    /// 通过模拟 run_poller 的 RequestAuthRefresh
    /// 分支语义验证 5s 节流——do_auth_refresh 后 timestamp 是「现在」,
    /// 第二次命令到达时 elapsed() < throttle 必命中节流分支被忽略,
    /// state.auth 保持调用前的值不动
    ///
    /// 注:state.last_auth_refresh_attempt_at 使用 std::time::Instant
    /// (wall-clock),不受 tokio::time::pause 影响,因此本测试只覆盖
    /// 「窗口内必须忽略」语义;「窗口外允许刷新」由端到端测试覆盖
    /// (在那里通过真实时间或专用 mock clock 推进)
    #[tokio::test]
    async fn request_auth_refresh_throttle_skips_within_window() {
        // throttle 设大到 60s,保证 ms 级测试内绝不可能自然过期
        let cfg = PollerConfig {
            auth_refresh_period: Duration::from_secs(3600),
            auth_refresh_throttle: Duration::from_secs(60),
            ..PollerConfig::default()
        };

        let client = Arc::new(AuthMockClient::new(vec![Ok("first-cred".into())]));
        let (deps, _bus) = auth_test_deps(client.clone());

        let bot_id = BotId::new("throttle-bot");
        let mut state = LoginState::new();

        // ── 首次:从未尝试 → 必过节流
        assert!(
            state.auth_refresh_throttle_elapsed(cfg.auth_refresh_throttle),
            "first RequestAuthRefresh must pass throttle (never attempted)"
        );
        state.auth = None; // run_poller 在通过节流后清 auth
        do_auth_refresh(&bot_id, 6099, "tok", &cfg, &deps, &mut state).await;
        assert_eq!(client.calls(), 1);
        assert_eq!(state.auth.as_deref(), Some("first-cred"));

        // ── 第二次:模拟立即收到下一条 RequestAuthRefresh
        // timestamp 刚被刷新成现在 → elapsed < 60s throttle → 必须命中节流
        assert!(
            !state.auth_refresh_throttle_elapsed(cfg.auth_refresh_throttle),
            "second RequestAuthRefresh within window must NOT pass"
        );
        // 节流命中分支:直接忽略且不修改 state.auth
        // 断言 fetch 没被再次调用,且 state.auth 仍是上一次写入的值
        assert_eq!(client.calls(), 1, "throttled command must not call fetch");
        assert_eq!(
            state.auth.as_deref(),
            Some("first-cred"),
            "throttled command must NOT clear state.auth"
        );
    }

    /// auth_ticker tick 直接调 do_auth_refresh 不走节流
    /// 验证「即使 timestamp 刚更新过,再次直接调用 do_auth_refresh 仍会
    /// 真正执行 fetch」——节流判定不在 do_auth_refresh 自身,而在 caller
    #[tokio::test]
    async fn do_auth_refresh_does_not_self_throttle() {
        let bot_id = BotId::new("ticker-bot");
        let cfg = PollerConfig::default();
        let client = Arc::new(AuthMockClient::new(vec![
            Ok("second".into()),
            Ok("first".into()),
        ]));
        let (deps, _bus) = auth_test_deps(client.clone());
        let mut state = LoginState::new();

        do_auth_refresh(&bot_id, 6099, "tok", &cfg, &deps, &mut state).await;
        assert_eq!(client.calls(), 1);
        assert_eq!(state.auth.as_deref(), Some("first"));

        // 立刻再次调用——do_auth_refresh 自身不带节流,必须真正 fetch
        // 这对应 auth_ticker 30min tick 直接执行的语义
        do_auth_refresh(&bot_id, 6099, "tok", &cfg, &deps, &mut state).await;
        assert_eq!(client.calls(), 2);
        assert_eq!(state.auth.as_deref(), Some("second"));
    }
