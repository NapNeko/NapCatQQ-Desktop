    use super::*;

    // -----------------------------------------------------------------------
    // DaemonState 字面量稳定性(继承自 ,签名不变所以保留)
    // -----------------------------------------------------------------------

    #[test]
    fn daemon_state_serializes_as_snake_case() {
        // 防字面量漂移: / 9.1 的前端事件 payload state 字段
        // 严格依赖这五个 snake_case 字面量;这里 lock 一遍
        assert_eq!(
            serde_json::to_string(&DaemonState::Stopped).unwrap(),
            "\"stopped\""
        );
        assert_eq!(
            serde_json::to_string(&DaemonState::Starting).unwrap(),
            "\"starting\""
        );
        assert_eq!(
            serde_json::to_string(&DaemonState::Ready).unwrap(),
            "\"ready\""
        );
        assert_eq!(
            serde_json::to_string(&DaemonState::Stopping).unwrap(),
            "\"stopping\""
        );
        assert_eq!(
            serde_json::to_string(&DaemonState::Crashed).unwrap(),
            "\"crashed\""
        );
    }

    #[test]
    fn daemon_state_round_trips_through_serde() {
        for state in [
            DaemonState::Stopped,
            DaemonState::Starting,
            DaemonState::Ready,
            DaemonState::Stopping,
            DaemonState::Crashed,
        ] {
            let encoded = serde_json::to_string(&state).expect("serialize");
            let decoded: DaemonState = serde_json::from_str(&encoded).expect("deserialize");
            assert_eq!(decoded, state);
        }
    }

    #[test]
    fn daemon_state_derives_copy_and_eq() {
        // Copy / Eq 派生健康度: 要求 Copy(被 DomainEvent 内联)
        // 这里通过类型推断断言
        let s = DaemonState::Ready;
        let copied = s; // 触发 Copy
        assert_eq!(s, copied);
    }

    // -----------------------------------------------------------------------
    // SnowLumaDaemon smoke 测试( deliverable)
    //
    // 完整 starter / waiter / rollback / crash / shutdown 行为覆盖由
    // 接手;本 task 仅验证:
    // 1) new 不做任何 IO 即可构造,初始 state == Stopped
    // 2) 初始 ref_count == 0
    // -----------------------------------------------------------------------

    /// 最小 stub factory:smoke 测试用,永远返回错误,不会被实际调用
    struct StubFactory;

    #[async_trait]
    impl SnowLumaWebUiClientFactory for StubFactory {
        async fn create(
            &self,
            _password: String,
            _port: u16,
        ) -> Result<Arc<dyn SnowLumaWebUiClient>, SnowLumaWebUiError> {
            Err(SnowLumaWebUiError::Http {
                endpoint: "<stub>".into(),
                cause: "stub factory must not be invoked in smoke tests".into(),
            })
        }
    }

    fn build_smoke_daemon() -> Arc<SnowLumaDaemon> {
        let snowluma_data_root = PathBuf::from("/tmp/snowluma-smoke");
        let runtime_root = PathBuf::from("/tmp/snowluma-runtime-smoke");
        let event_bus = Arc::new(BroadcastEventBus::default());
        let factory: Arc<dyn SnowLumaWebUiClientFactory> = Arc::new(StubFactory);
        SnowLumaDaemon::new(snowluma_data_root, runtime_root, event_bus, factory)
    }

    #[tokio::test]
    async fn daemon_initial_state_is_stopped() {
        let daemon = build_smoke_daemon();
        assert_eq!(daemon.state().await, DaemonState::Stopped);
    }

    #[tokio::test]
    async fn daemon_ref_count_is_zero_initially() {
        let daemon = build_smoke_daemon();
        assert_eq!(daemon.ref_count().await, 0);
    }

    /// 编译期断言:SnowLumaWebUiClientFactory 直接以 Arc<dyn SnowLumaWebUiClient>
    /// 暴露 client( 占位的关联类型已经在 移除)这里通过让
    /// 一个 stub factory 真正参与 Arc::new 装箱来锁定签名形态
    #[tokio::test]
    async fn factory_trait_uses_dyn_client_directly() {
        let factory: Arc<dyn SnowLumaWebUiClientFactory> = Arc::new(StubFactory);
        // 调一次 create 拿到 Result,确认返回类型恰好是 Arc<dyn SnowLumaWebUiClient>
        let result: Result<Arc<dyn SnowLumaWebUiClient>, SnowLumaWebUiError> =
            factory.create("x".into(), 5099).await;
        assert!(result.is_err(), "stub factory always errors");
    }

    /// subscribe_logs 应当直接返回一个 broadcast Receiver;不需要 daemon 已启动
    #[tokio::test]
    async fn daemon_subscribe_logs_does_not_require_start() {
        let daemon = build_smoke_daemon();
        let _rx: broadcast::Receiver<String> = daemon.subscribe_logs();
    }

    // -----------------------------------------------------------------------
    // spawn_stdout_reader smoke 测试( deliverable)
    //
    // 通过真起一个会立刻 echo 一行的小子进程,把 ChildStdout 喂给
    // spawn_stdout_reader,断言:
    // 1) log_tx 订阅者收到清洗后的行
    // 2) event_bus 上能收到 SnowLumaDaemonLog variant
    // 3) 子进程退出后 reader 任务自然结束(无 panic,无 hang)
    //
    // SnowLuma daemon 本身仅 Windows 落地,但 spawn_stdout_reader 自身是平台无关的
    // 所以本 smoke 测试在两平台都跑:Windows 用 cmd /C echo,其它走 echo
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn spawn_stdout_reader_forwards_cleaned_line() {
        use ncd_domain::domain_event::DomainEventKind;
        use ncd_traits::events::EventFilter;
        use std::process::Stdio;

        let event_bus = Arc::new(BroadcastEventBus::default());
        let mut bus_sub =
            event_bus.subscribe(EventFilter::kind(DomainEventKind::SnowLumaDaemonLog));
        let (log_tx, mut log_rx) = broadcast::channel::<String>(16);

        // 用一个一定能立即 echo 一行并退出的小子进程:
        // - Windows: cmd /C echo hello-snowluma
        // - 其它: /bin/sh -c "echo hello-snowluma"
        // echo 行尾换行让 lines.next_line() 立刻拿到一整行
        #[cfg(windows)]
        let mut child = tokio::process::Command::new("cmd")
            .args(["/C", "echo hello-snowluma"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn cmd /C echo");
        #[cfg(not(windows))]
        let mut child = tokio::process::Command::new("sh")
            .args(["-c", "echo hello-snowluma"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn sh -c echo");

        let stdout = child.stdout.take().expect("child stdout should be piped");
        let recent: Arc<std::sync::Mutex<std::collections::VecDeque<String>>> =
            Arc::new(std::sync::Mutex::new(std::collections::VecDeque::new()));
        spawn_stdout_reader(stdout, log_tx.clone(), Arc::clone(&event_bus), recent);

        // 1) log_tx 订阅者拿到清洗后的行
        let received = tokio::time::timeout(Duration::from_secs(5), log_rx.recv())
            .await
            .expect("log_tx send should arrive within 5s")
            .expect("log channel still open");
        assert_eq!(received, "hello-snowluma");

        // 2) event_bus 上的 SnowLumaDaemonLog variant 也命中
        let bus_event = tokio::time::timeout(Duration::from_secs(5), bus_sub.next())
            .await
            .expect("event bus should publish within 5s")
            .expect("event subscription still open");
        match bus_event {
            DomainEvent::SnowLumaDaemonLog { line } => assert_eq!(line, "hello-snowluma"),
            other => panic!("expected SnowLumaDaemonLog, got {other:?}"),
        }

        // 3) 子进程退出收尾,reader 任务靠 EOF 自然结束
        let _ = child.wait().await;
    }

    // -----------------------------------------------------------------------
    // deliverable
    //
    // MockSnowLumaWebUiClient + MockFactory:复用 / 3.2 的 trait 边界
    // 注入测试 stub,不走真实 reqwest / wiremock
    //
    // 测试覆盖:
    // - daemon_spawn_failure_rolls_back_to_stopped:empty runtime_root → spawn
    // 失败 → state 回到 Stopped + ref_count == 0 + 收到 Starting/Stopped
    // 两次状态事件
    // - daemon_concurrent_callers_share_starting_then_all_fail:3 个并发
    // ensure_running,starter spawn 失败时其它 waiter 也拿到 Crashed-ish 错误
    // - daemon_release_is_safe_when_ref_count_is_zero:release 多次不 panic,
    // ref_count 始终 == 0
    // - daemon_shutdown_from_stopped_is_noop:build 一个新 daemon 调 shutdown
    // 不 panic,状态保持 Stopped
    // - 属性测试 daemon_release_never_underflows:随机 0..50 次 release
    // ref_count 始终 == 0,不 panic
    //
    // Follow-up:watch_exit 真实 child 监听路径无法在 Mock 层构造(child 是
    // tokio::process::Child 句柄),留给真起 node.exe 的端到端集成测试覆盖
    // watch_exit_emits_crashed_when_child_unexpectedly_exits 行为
    // -----------------------------------------------------------------------

    use ncd_domain::domain_event::DomainEventKind;
    use ncd_traits::events::EventFilter;
    use proptest::prelude::*;
    use tempfile::tempdir;
    use tokio::sync::Mutex as TokioMutex;

    /// MockSnowLumaWebUiClient:用 Arc<TokioMutex<MockBehavior>> 控制每个
    /// trait 方法的返回值其它 daemon 测试不需要的方法返回简单 Ok / 空
    /// 注:当前 的几个落地测试(spawn 失败 / 并发 / release / shutdown)
    /// 都跑不到 wait_ready / login,因为 spawn 阶段就已失败;mock 仍保留以
    /// 便后续 task 在 Ready 路径覆盖中复用,并满足 trait 完备性编译要求
    #[derive(Default)]
    struct MockBehavior {
        wait_ready_result: Option<Result<(), SnowLumaWebUiError>>,
        login_result: Option<Result<(), SnowLumaWebUiError>>,
        logout_calls: u32,
    }

    struct MockSnowLumaWebUiClient {
        behavior: Arc<TokioMutex<MockBehavior>>,
    }

    #[async_trait]
    impl SnowLumaWebUiClient for MockSnowLumaWebUiClient {
        async fn wait_ready(
            &self,
            _timeout: Duration,
            _dead_check: Box<dyn Fn() -> bool + Send + Sync>,
        ) -> Result<(), SnowLumaWebUiError> {
            let behavior = self.behavior.lock().await;
            match &behavior.wait_ready_result {
                Some(Ok(())) | None => Ok(()),
                Some(Err(e)) => Err(clone_webui_error(e)),
            }
        }

        async fn login(&self) -> Result<(), SnowLumaWebUiError> {
            let behavior = self.behavior.lock().await;
            match &behavior.login_result {
                Some(Ok(())) | None => Ok(()),
                Some(Err(e)) => Err(clone_webui_error(e)),
            }
        }

        async fn logout(&self) -> Result<(), SnowLumaWebUiError> {
            let mut behavior = self.behavior.lock().await;
            behavior.logout_calls = behavior.logout_calls.saturating_add(1);
            Ok(())
        }

        async fn list_processes(
            &self,
        ) -> Result<Vec<crate::snowluma::webui_client::HookProcessInfo>, SnowLumaWebUiError>
        {
            Ok(Vec::new())
        }

        async fn list_qq_instances(
            &self,
        ) -> Result<Vec<crate::snowluma::webui_client::OneBotInstanceInfo>, SnowLumaWebUiError>
        {
            Ok(Vec::new())
        }

        async fn probe_process_login_info(
            &self,
            _pid: u32,
        ) -> Result<Option<crate::snowluma::webui_client::QqPortLoginInfo>, SnowLumaWebUiError>
        {
            Ok(None)
        }

        async fn load_process(
            &self,
            _pid: u32,
        ) -> Result<crate::snowluma::webui_client::HookProcessInfo, SnowLumaWebUiError> {
            Err(SnowLumaWebUiError::ServerRejected {
                endpoint: "<mock>/load".into(),
                message: "mock not configured".into(),
            })
        }

        async fn unload_process(
            &self,
            _pid: u32,
        ) -> Result<crate::snowluma::webui_client::HookProcessInfo, SnowLumaWebUiError> {
            Err(SnowLumaWebUiError::ServerRejected {
                endpoint: "<mock>/unload".into(),
                message: "mock not configured".into(),
            })
        }

        async fn get_auth_state(
            &self,
        ) -> Result<crate::snowluma::webui_client::AuthState, SnowLumaWebUiError> {
            Ok(crate::snowluma::webui_client::AuthState::default())
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

    /// SnowLumaWebUiError 不实现 Clone(含 BTreeMap 字段);测试里需要
    /// 在多次调用之间复用同一个错误模板,这里手写一份字面 clone
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

    /// 返回 MockSnowLumaWebUiClient 的工厂;shared 行为容器让测试用例可在
    /// daemon 构造之后再调整 mock 返回值
    struct MockFactory {
        behavior: Arc<TokioMutex<MockBehavior>>,
    }

    #[async_trait]
    impl SnowLumaWebUiClientFactory for MockFactory {
        async fn create(
            &self,
            _password: String,
            _port: u16,
        ) -> Result<Arc<dyn SnowLumaWebUiClient>, SnowLumaWebUiError> {
            Ok(Arc::new(MockSnowLumaWebUiClient {
                behavior: Arc::clone(&self.behavior),
            }))
        }
    }

    /// 构造 daemon + tempdir 持有句柄runtime_root 为空目录,spawn 时找不到
    /// node.exe 必然失败 —— 这是测试 spawn 失败回滚路径的关键
    /// 同时 snowluma_data_root 也用 tempdir,保证 render_daemon_globals
    /// 写 session.json / runtime.json / webui.json 不污染真实数据根
    /// 返回 (daemon, runtime_dir, snowluma_dir, behavior);测试函数应把 dir
    /// 句柄绑到 _runtime_dir / _snowluma_dir 保活到测试结束
    fn build_test_daemon() -> (
        Arc<SnowLumaDaemon>,
        tempfile::TempDir,
        tempfile::TempDir,
        Arc<TokioMutex<MockBehavior>>,
    ) {
        let runtime_dir = tempdir().expect("tempdir runtime");
        let snowluma_dir = tempdir().expect("tempdir snowluma_data");
        let event_bus = Arc::new(BroadcastEventBus::default());
        let behavior = Arc::new(TokioMutex::new(MockBehavior::default()));
        let factory: Arc<dyn SnowLumaWebUiClientFactory> = Arc::new(MockFactory {
            behavior: Arc::clone(&behavior),
        });
        let daemon = SnowLumaDaemon::new(
            snowluma_dir.path().to_path_buf(),
            runtime_dir.path().to_path_buf(),
            event_bus,
            factory,
        );
        (daemon, runtime_dir, snowluma_dir, behavior)
    }

    /// runtime_root 不含 node.exe → Command::spawn 失败 → starter 路径在
    /// step 3 早 fail → rollback_to_stopped → state == Stopped,ref_count == 0,
    /// last_error 非空事件序列:先 Starting,再 Stopped(reason=Some)
    #[tokio::test]
    async fn daemon_spawn_failure_rolls_back_to_stopped() {
        let (daemon, _runtime_dir, _snowluma_dir, _behavior) = build_test_daemon();
        let mut sub = daemon.event_bus.subscribe(EventFilter::kind(
            DomainEventKind::SnowLumaDaemonStateChanged,
        ));

        let result = daemon.ensure_running(Duration::from_secs(5)).await;
        assert!(
            result.is_err(),
            "spawn must fail without node.exe in runtime_root"
        );

        assert_eq!(daemon.state().await, DaemonState::Stopped);
        assert_eq!(daemon.ref_count().await, 0);

        // 收到 Starting 事件
        let first = tokio::time::timeout(Duration::from_secs(2), sub.next())
            .await
            .expect("starting event within 2s")
            .expect("subscription open");
        match first {
            DomainEvent::SnowLumaDaemonStateChanged {
                state, ref_count, ..
            } => {
                assert_eq!(state, DaemonState::Starting);
                assert_eq!(ref_count, 1);
            }
            other => panic!("expected Starting state event, got {other:?}"),
        }

        // 紧接着 Stopped 事件,reason 携带回滚原因
        let second = tokio::time::timeout(Duration::from_secs(2), sub.next())
            .await
            .expect("stopped event within 2s")
            .expect("subscription open");
        match second {
            DomainEvent::SnowLumaDaemonStateChanged {
                state,
                ref_count,
                reason,
                ..
            } => {
                assert_eq!(state, DaemonState::Stopped);
                assert_eq!(ref_count, 0);
                assert!(reason.is_some(), "rollback should carry reason");
            }
            other => panic!("expected Stopped state event, got {other:?}"),
        }
    }

    /// 3 个并发 caller 同时调 ensure_running:starter 路径 spawn 失败 →
    /// rollback_to_stopped → ref_count 归零 + ready_notify 唤醒 waiter三个
    /// caller 全部得到 Err;最终 state == Stopped,ref_count == 0
    /// 这是 并发 caller 行为的反向覆盖(正向 Ready 路径需要真实
    /// node.exe,留给端到端集成测试)
    #[tokio::test]
    async fn daemon_concurrent_callers_share_starting_then_all_fail() {
        let (daemon, _runtime_dir, _snowluma_dir, _behavior) = build_test_daemon();

        let d1 = Arc::clone(&daemon);
        let d2 = Arc::clone(&daemon);
        let d3 = Arc::clone(&daemon);
        let h1 = tokio::spawn(async move { d1.ensure_running(Duration::from_secs(5)).await });
        let h2 = tokio::spawn(async move { d2.ensure_running(Duration::from_secs(5)).await });
        let h3 = tokio::spawn(async move { d3.ensure_running(Duration::from_secs(5)).await });

        let r1 = h1.await.expect("join h1");
        let r2 = h2.await.expect("join h2");
        let r3 = h3.await.expect("join h3");

        assert!(r1.is_err(), "caller 1 should err");
        assert!(r2.is_err(), "caller 2 should err");
        assert!(r3.is_err(), "caller 3 should err");

        // 全部失败后应回到稳态 Stopped,ref_count 归零(rollback 强制清零)
        assert_eq!(daemon.state().await, DaemonState::Stopped);
        assert_eq!(daemon.ref_count().await, 0);
    }

    /// release 在 ref_count == 0 时是安全的:多次调用不 panic,不 underflow
    /// ref_count 仍然 == 0(saturating_sub 语义)
    #[tokio::test]
    async fn daemon_release_is_safe_when_ref_count_is_zero() {
        let daemon = build_smoke_daemon();
        for _ in 0..10 {
            daemon.release().await;
            assert_eq!(daemon.ref_count().await, 0);
        }
    }

    /// 新构造的 daemon state == Stopped;shutdown 在 Stopped 状态早 return
    /// 不 panic,不发任何状态事件,状态保持 Stopped
    #[tokio::test]
    async fn daemon_shutdown_from_stopped_is_noop() {
        let daemon = build_smoke_daemon();
        daemon.shutdown().await;
        assert_eq!(daemon.state().await, DaemonState::Stopped);
        assert_eq!(daemon.ref_count().await, 0);
    }

    // -------------------------------------------------------------------
    // 属性测试:随机生成 0..50 次 release 操作序列,断言 ref_count 始终 == 0
    // 且不 panic(saturating_sub 单调上界)Validates:
    // 的 release 安全性(即便没有 ensure_running 配对调用,也不能 underflow)
    // -------------------------------------------------------------------

    proptest! {
    #![proptest_config(ProptestConfig {
    cases: 16,
    ..ProptestConfig::default()
    })]

    #[test]
    fn daemon_release_never_underflows(call_count in 0u32..50) {
    // proptest case 内部跑 tokio runtime;用 current_thread 即可
    let rt = tokio::runtime::Builder::new_current_thread()
    .enable_all()
    .build()
    .expect("build rt");
    rt.block_on(async {
    let daemon = build_smoke_daemon();
    for _ in 0..call_count {
    daemon.release().await;
    }
    let final_ref = daemon.ref_count().await;
    prop_assert_eq!(final_ref, 0);
    Ok(())
    }).expect("proptest async block");
    }
    }
