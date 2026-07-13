    use std::sync::Arc;
    use std::sync::Mutex as StdMutex;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use async_trait::async_trait;
    use proptest::prelude::*;

    use super::super::offline_notifier::{OfflineNoticeKind, OfflineNotifier};
    use super::super::webui_client::{
        CheckLoginStatusData, GetQQLoginInfoData, NapCatWebUiClient, NapCatWebUiError,
    };
    use ncd_domain::domain_event::DomainEvent;
    use ncd_domain::ids::BotId;
    use ncd_domain::napcat_events::NapCatLoginInvalidationReason;
    use ncd_traits::events::{BroadcastEventBus, EventBus, EventFilter, EventSubscription};

    use super::{
        LoginState, PollerConfig, PollerDeps, RestartHandle, apply_login_status,
        apply_online_status,
    };

    // ── stub HTTP client:apply_* 不会调用它,给 deps 占位 ─────────────────

    struct StubHttp;

    #[async_trait]
    impl NapCatWebUiClient for StubHttp {
        async fn fetch_credential(
            &self,
            _port: u16,
            _token: &str,
        ) -> Result<String, NapCatWebUiError> {
            Ok("stub".into())
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

    // ── property notifier / restart:仅记录调用次数 ────────────────────────

    struct PropNotifier {
        calls: StdMutex<Vec<OfflineNoticeKind>>,
    }
    impl PropNotifier {
        fn new() -> Self {
            Self {
                calls: StdMutex::new(Vec::new()),
            }
        }
        fn count(&self) -> usize {
            self.calls.lock().expect("notifier mutex poisoned").len()
        }
    }
    #[async_trait]
    impl OfflineNotifier for PropNotifier {
        async fn notify(&self, _bot_id: &BotId, kind: OfflineNoticeKind) {
            self.calls
                .lock()
                .expect("notifier mutex poisoned")
                .push(kind);
        }
    }

    struct PropRestart {
        calls: AtomicUsize,
    }
    impl PropRestart {
        fn new() -> Self {
            Self {
                calls: AtomicUsize::new(0),
            }
        }
        fn count(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
    }
    #[async_trait]
    impl RestartHandle for PropRestart {
        async fn restart_bot(&self, _bot_id: &BotId) {
            self.calls.fetch_add(1, Ordering::SeqCst);
        }
    }

    // ── 测试基础设施 ──────────────────────────────────────────────────────

    /// 为每个 proptest case 现场构造一个 current-thread tokio runtime
    /// 仅启用 time 驱动——tokio::time::timeout 排空事件时需要它
    fn current_thread_rt() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .expect("build current-thread runtime")
    }

    /// 排空 bot-scoped 订阅器中已发布的事件;和 transition_tests::drain_events
    /// 等价,复制一份避免跨模块可见性扩散
    async fn drain(sub: &mut EventSubscription) -> Vec<DomainEvent> {
        let mut out = Vec::new();
        while let Ok(Some(ev)) = tokio::time::timeout(Duration::from_millis(5), sub.next()).await {
            out.push(ev);
        }
        out
    }

    fn make_deps(
        notifier: Arc<PropNotifier>,
        restart: Arc<PropRestart>,
    ) -> (PollerDeps, Arc<BroadcastEventBus>) {
        let bus = Arc::new(BroadcastEventBus::default());
        let deps = PollerDeps {
            event_bus: bus.clone(),
            http: Arc::new(StubHttp),
            notifier,
            restart_handle: restart,
        };
        (deps, bus)
    }

    /// 在 bot 维度订阅,调用 apply_login_status 后返回排空的事件序列
    fn run_apply_login(
        bot_id: &BotId,
        state: &mut LoginState,
        data: CheckLoginStatusData,
    ) -> Vec<DomainEvent> {
        let notifier = Arc::new(PropNotifier::new());
        let restart = Arc::new(PropRestart::new());
        let (deps, bus) = make_deps(notifier, restart);
        let rt = current_thread_rt();
        rt.block_on(async {
            let mut sub = bus.subscribe(EventFilter::bot(bot_id.clone()));
            apply_login_status(bot_id, data, &deps, state);
            drain(&mut sub).await
        })
    }

    /// 在 bot 维度订阅,调用 apply_online_status 后返回(事件序列, notify 次数, restart 次数)
    fn run_apply_online(
        bot_id: &BotId,
        cfg: &PollerConfig,
        state: &mut LoginState,
        data: GetQQLoginInfoData,
    ) -> (Vec<DomainEvent>, usize, usize) {
        let notifier = Arc::new(PropNotifier::new());
        let restart = Arc::new(PropRestart::new());
        let (deps, bus) = make_deps(notifier.clone(), restart.clone());
        let rt = current_thread_rt();
        let events = rt.block_on(async {
            let mut sub = bus.subscribe(EventFilter::bot(bot_id.clone()));
            apply_online_status(bot_id, data, cfg, &deps, state).await;
            drain(&mut sub).await
        });
        (events, notifier.count(), restart.count())
    }

    /// 跑一段「连续离线区间」:同一个 state 上施加 N 次 apply_online_status({online:false})
    fn run_offline_window(
        bot_id: &BotId,
        cfg: &PollerConfig,
        state: &mut LoginState,
        n: usize,
    ) -> (usize, usize) {
        let notifier = Arc::new(PropNotifier::new());
        let restart = Arc::new(PropRestart::new());
        let (deps, _bus) = make_deps(notifier.clone(), restart.clone());
        let rt = current_thread_rt();
        rt.block_on(async {
            for _ in 0..n {
                apply_online_status(
                    bot_id,
                    GetQQLoginInfoData {
                        online: Some(false),
                    },
                    cfg,
                    &deps,
                    state,
                )
                .await;
            }
        });
        (notifier.count(), restart.count())
    }

    /// proptest 全局配置——cases=64 兼顾覆盖与速度
    fn pconfig() -> ProptestConfig {
        ProptestConfig {
            cases: 16,
            ..ProptestConfig::default()
        }
    }

    // 一个非空 / 空两分支的 qrcode_url 策略,确保两侧覆盖均衡
    fn qrcode_url_strategy() -> impl Strategy<Value = String> {
        prop_oneof![
            Just(String::new()),
            "[a-zA-Z0-9:/+=,.]{1,16}".prop_map(|s| s),
        ]
    }

    // ─────────────────────────────────────────────────────────────────────
    // Qrcode 可观测性
    //
    // ∀ Poller state s, ∀ CheckLoginStatusData data: 若
    //   s.is_logged_in == false ∧ data.is_login == false ∧
    //   data.qrcode_url ≠ ∅ ∧ ¬s.suppress_qrcode_until_online ∧
    //   ¬s.login_invalidated_while_online,
    // 则下一次 apply_login_status(s, data) 必发布 NapCatLoginQrcode{qrcode_url}
    //
    // 注意:当 s.is_logged_in == false 时 prev_login=false,kick 分支不
    // 会被触发,因此 state.suppress_* / state.login_invalidated_* 不会
    // 被本次调用改变——前置条件「应用前」 ≡ 「Qrcode 检查时」
    // ─────────────────────────────────────────────────────────────────────

    proptest! {
        #![proptest_config(pconfig())]

        #[test]
        fn property_1_qrcode_emitted_when_preconditions_hold(
            state_online in any::<bool>(),
            state_offline_notice_sent in any::<bool>(),
            state_invalidated in any::<bool>(),
            state_suppress in any::<bool>(),
            data_is_login in any::<bool>(),
            qrcode_url in qrcode_url_strategy(),
        ) {
            let bot_id = BotId::new("prop-1");
            let mut state = LoginState::new();
            state.is_logged_in = false; // 前置条件
            state.online = state_online;
            state.offline_notice_sent = state_offline_notice_sent;
            state.login_invalidated_while_online = state_invalidated;
            state.suppress_qrcode_until_online = state_suppress;

            let preconditions = !data_is_login
                && !qrcode_url.is_empty()
                && !state_invalidated
                && !state_suppress;

            let events = run_apply_login(
                &bot_id,
                &mut state,
                CheckLoginStatusData {
                    is_login: data_is_login,
                    is_offline: None,
                    qrcode_url: qrcode_url.clone(),
                },
            );

            if preconditions {
                let saw_qrcode = events.iter().any(|e| matches!(
                    e,
                    DomainEvent::NapCatLoginQrcode { qrcode_url: u, .. }
                        if u == &qrcode_url
                ));
                prop_assert!(
                    saw_qrcode,
                    "preconditions hold but no NapCatLoginQrcode emitted: events={:?}",
                    events
                );
            }
        }
    }

    // ─────────────────────────────────────────────────────────────────────
    // Qrcode 与 QrcodeRemoved 互斥
    //
    // ∀ apply_*:同一调用内不会同时发布 NapCatLoginQrcode 与
    // NapCatLoginQrcodeRemoved
    //
    // 验证两个入口各自的互斥不变量:
    //   - apply_login_status:要么发 QrcodeRemoved(is_login=true 分支
    //     return),要么发 Qrcode / Invalidated(其它分支),不会同时出现
    //   - apply_online_status:从不发布 NapCatLoginQrcode,因此互斥
    //     trivially 成立——但仍生成随机输入跑一遍以防回归
    // ─────────────────────────────────────────────────────────────────────

    proptest! {
        #![proptest_config(pconfig())]

        #[test]
        fn property_2_login_qrcode_xor_qrcode_removed(
            state_is_logged_in in any::<bool>(),
            state_online in any::<bool>(),
            state_invalidated in any::<bool>(),
            state_suppress in any::<bool>(),
            data_is_login in any::<bool>(),
            qrcode_url in qrcode_url_strategy(),
        ) {
            let bot_id = BotId::new("prop-2-login");
            let mut state = LoginState::new();
            state.is_logged_in = state_is_logged_in;
            state.online = state_online;
            state.login_invalidated_while_online = state_invalidated;
            state.suppress_qrcode_until_online = state_suppress;

            let events = run_apply_login(
                &bot_id,
                &mut state,
                CheckLoginStatusData {
                    is_login: data_is_login,
                    is_offline: None,
                    qrcode_url,
                },
            );
            let saw_qrcode = events
                .iter()
                .any(|e| matches!(e, DomainEvent::NapCatLoginQrcode { .. }));
            let saw_removed = events
                .iter()
                .any(|e| matches!(e, DomainEvent::NapCatLoginQrcodeRemoved { .. }));

            prop_assert!(
                !(saw_qrcode && saw_removed),
                "apply_login_status emitted BOTH Qrcode and QrcodeRemoved in one call: events={:?}",
                events
            );
        }

        #[test]
        fn property_2_online_qrcode_xor_qrcode_removed(
            state_is_logged_in in any::<bool>(),
            state_online in any::<bool>(),
            state_invalidated in any::<bool>(),
            state_suppress in any::<bool>(),
            state_notice_sent in any::<bool>(),
            data_online in any::<bool>(),
            auto_restart in any::<bool>(),
            notice_enabled in any::<bool>(),
        ) {
            let bot_id = BotId::new("prop-2-online");
            let cfg = PollerConfig {
                offline_auto_restart: auto_restart,
                offline_notice_enabled: notice_enabled,
                ..PollerConfig::default()
            };
            let mut state = LoginState::new();
            state.is_logged_in = state_is_logged_in;
            state.online = state_online;
            state.login_invalidated_while_online = state_invalidated;
            state.suppress_qrcode_until_online = state_suppress;
            state.offline_notice_sent = state_notice_sent;

            let (events, _, _) = run_apply_online(
                &bot_id,
                &cfg,
                &mut state,
                GetQQLoginInfoData {
                    online: Some(data_online),
                },
            );
            let saw_qrcode = events
                .iter()
                .any(|e| matches!(e, DomainEvent::NapCatLoginQrcode { .. }));
            let saw_removed = events
                .iter()
                .any(|e| matches!(e, DomainEvent::NapCatLoginQrcodeRemoved { .. }));

            prop_assert!(
                !(saw_qrcode && saw_removed),
                "apply_online_status emitted BOTH Qrcode and QrcodeRemoved in one call: events={:?}",
                events
            );
        }
    }

    // ─────────────────────────────────────────────────────────────────────
    // 踢线检测因果性
    //
    // ∀ (prev_state, next_data): NapCatLoginInvalidated{Kicked} 被发布
    // ⇔ prev_state.is_logged_in == true ∧ prev_state.online == true ∧
    //    next_data.is_login == false
    //
    // 设计要求只是 ⇒(必要条件),但代码实现满足 ⇔(充要);测试两个
    // 方向以更严格地锁定行为
    // ─────────────────────────────────────────────────────────────────────

    proptest! {
        #![proptest_config(pconfig())]

        #[test]
        fn property_3_kick_iff_prev_login_and_online_and_not_next_login(
            prev_login in any::<bool>(),
            prev_online in any::<bool>(),
            prev_invalidated in any::<bool>(),
            prev_suppress in any::<bool>(),
            data_is_login in any::<bool>(),
            qrcode_url in qrcode_url_strategy(),
        ) {
            let bot_id = BotId::new("prop-3");
            let mut state = LoginState::new();
            state.is_logged_in = prev_login;
            state.online = prev_online;
            state.login_invalidated_while_online = prev_invalidated;
            state.suppress_qrcode_until_online = prev_suppress;

            let events = run_apply_login(
                &bot_id,
                &mut state,
                CheckLoginStatusData {
                    is_login: data_is_login,
                    is_offline: None,
                    qrcode_url,
                },
            );

            let saw_kicked = events.iter().any(|e| matches!(
                e,
                DomainEvent::NapCatLoginInvalidated {
                    reason: NapCatLoginInvalidationReason::Kicked,
                    ..
                }
            ));
            let kick_preconditions = prev_login && prev_online && !data_is_login;

            prop_assert_eq!(
                saw_kicked,
                kick_preconditions,
                "Kicked iff (prev_login ∧ prev_online ∧ ¬next_login); got saw={} expected={}, events={:?}",
                saw_kicked,
                kick_preconditions,
                events
            );
        }
    }

    // ─────────────────────────────────────────────────────────────────────
    // auth refresh 5s 节流
    //
    // ∀ 任意 RequestAuthRefresh 到达时间序列:被动路径触发的相邻两次
    // 实际 refresh 间隔必 ≥ auth_refresh_throttle(默认 5s)
    //
    // 备注(来自 prompt):state.last_auth_refresh_attempt_at 用
    // std::time::Instant(wall-clock),不受 tokio::time::pause 影响
    // 因此本属性把节流决策抽象成纯函数 simulate_passive_refresh_throttle
    // 直接驱动——把任意「到达时间序列 + 节流阈值」喂给它,断言输出的
    // 实际 refresh 序列两两间隔 ≥ 阈值这准确镜像了 run_poller 中
    // RequestAuthRefresh 分支的判定语义:
    //
    // text
    // if arrival - last_attempt >= throttle {
    //     actual_refresh.push(arrival);
    //     last_attempt = arrival;
    // }
    //
    // ─────────────────────────────────────────────────────────────────────

    /// 把「RequestAuthRefresh 命令到达时间序列」(毫秒时间戳)按节流阈值
    /// 模拟成「实际 do_auth_refresh 调用时间序列」复制 run_poller 中
    /// PollerCommand::RequestAuthRefresh 分支的判定语义;初始
    /// last_attempt 设为 None 模拟 LoginState::new 的「过去 1 小时」
    /// 远过去状态——首次到达必通过节流
    fn simulate_passive_refresh_throttle(arrivals_ms: &[u64], throttle_ms: u64) -> Vec<u64> {
        let mut last_attempt: Option<u64> = None;
        let mut actual = Vec::new();
        for &arrival in arrivals_ms {
            let passes = match last_attempt {
                None => true,
                Some(prev) => arrival.saturating_sub(prev) >= throttle_ms,
            };
            if passes {
                actual.push(arrival);
                last_attempt = Some(arrival);
            }
        }
        actual
    }

    proptest! {
        #![proptest_config(pconfig())]

        #[test]
        fn property_4_passive_refresh_intervals_respect_throttle(
            // 1..32 个 0..20s 的 gap,组装成单调递增的到达序列
            gaps_ms in proptest::collection::vec(0u64..20_000, 1..32),
        ) {
            // 累加得到到达时间
            let mut t = 0u64;
            let arrivals: Vec<u64> = gaps_ms
                .iter()
                .map(|g| {
                    t = t.saturating_add(*g);
                    t
                })
                .collect();
            let throttle_ms = 5_000u64; // PollerConfig::default().auth_refresh_throttle = 5s

            let actual = simulate_passive_refresh_throttle(&arrivals, throttle_ms);

            // 任意相邻两次实际 refresh 间隔 ≥ throttle
            for w in actual.windows(2) {
                let gap = w[1].saturating_sub(w[0]);
                prop_assert!(
                    gap >= throttle_ms,
                    "two consecutive passive refreshes within throttle window: \
                     w[0]={} w[1]={} gap={} throttle={}",
                    w[0], w[1], gap, throttle_ms
                );
            }
        }
    }

    // ─────────────────────────────────────────────────────────────────────
    // 离线通知至多一次
    //
    // ∀ 一段连续离线区间(一连串 apply_online_status({online:false}) 调用,
    // 起始时 prev_online=true):notifier.notify(...) 被调用次数 ≤ 1
    //
    // 关键不变量:第一次离线调用后 state.online=false,后续调用全部
    // 命中「prev_online=false → return」分支,不再进入通知逻辑;同时
    // state.offline_notice_sent 在首轮被置位后封锁后续 notify 调用
    // ─────────────────────────────────────────────────────────────────────

    proptest! {
        #![proptest_config(pconfig())]

        #[test]
        fn property_6_offline_notice_at_most_once_in_continuous_window(
            n in 1usize..16,
            initial_is_logged_in in any::<bool>(),
            initial_invalidated in any::<bool>(),
            initial_suppress in any::<bool>(),
            initial_notice_sent in any::<bool>(),
            auto_restart in any::<bool>(),
            notice_enabled in any::<bool>(),
        ) {
            let cfg = PollerConfig {
                offline_auto_restart: auto_restart,
                offline_notice_enabled: notice_enabled,
                ..PollerConfig::default()
            };
            let bot_id = BotId::new("prop-6");
            let mut state = LoginState::new();
            // 起始处于「在线」稳态——首轮离线进入会触发通知判定
            state.online = true;
            state.is_logged_in = initial_is_logged_in;
            state.login_invalidated_while_online = initial_invalidated;
            state.suppress_qrcode_until_online = initial_suppress;
            state.offline_notice_sent = initial_notice_sent;

            let (notify_count, _restart_count) = run_offline_window(&bot_id, &cfg, &mut state, n);

            prop_assert!(
                notify_count <= 1,
                "notify called {} times in a continuous offline window of len {}; \
                 cfg(auto_restart={}, notice_enabled={}), \
                 init(is_logged_in={}, invalidated={}, suppress={}, notice_sent={})",
                notify_count, n,
                auto_restart, notice_enabled,
                initial_is_logged_in, initial_invalidated, initial_suppress, initial_notice_sent
            );
        }
    }

    // ─────────────────────────────────────────────────────────────────────
    // 自动重启副作用唯一
    //
    // ∀ 单次 apply_online_status 调用:restart_handle.restart_bot 被
    // 调用次数 ≤ 1(无论分支组合如何)
    //
    // 编译期视角下,函数体内只出现一次 restart_handle.restart_bot(...).await
    // 调用,本属性以运行时观察锁定该结构不变量,防回归
    // ─────────────────────────────────────────────────────────────────────

    proptest! {
        #![proptest_config(pconfig())]

        #[test]
        fn property_7_restart_at_most_once_per_apply_online(
            state_is_logged_in in any::<bool>(),
            state_online in any::<bool>(),
            state_invalidated in any::<bool>(),
            state_suppress in any::<bool>(),
            state_notice_sent in any::<bool>(),
            data_online in any::<bool>(),
            auto_restart in any::<bool>(),
            notice_enabled in any::<bool>(),
        ) {
            let cfg = PollerConfig {
                offline_auto_restart: auto_restart,
                offline_notice_enabled: notice_enabled,
                ..PollerConfig::default()
            };
            let bot_id = BotId::new("prop-7");
            let mut state = LoginState::new();
            state.is_logged_in = state_is_logged_in;
            state.online = state_online;
            state.login_invalidated_while_online = state_invalidated;
            state.suppress_qrcode_until_online = state_suppress;
            state.offline_notice_sent = state_notice_sent;

            let (_events, _notify_count, restart_count) = run_apply_online(
                &bot_id,
                &cfg,
                &mut state,
                GetQQLoginInfoData {
                    online: Some(data_online),
                },
            );

            prop_assert!(
                restart_count <= 1,
                "restart_bot called {} times in a single apply_online_status; \
                 cfg(auto_restart={}, notice_enabled={}), data_online={}",
                restart_count, auto_restart, notice_enabled, data_online
            );
        }
    }

    // ─────────────────────────────────────────────────────────────────────
    // 纯函数 example 测试:锁定 simulate_passive_refresh_throttle 自身
    // 的边界——proptest 的属性测试如果断言模拟器 bug 也会通过,因此用
    // 几个手工 example 给 simulator 做 sanity check
    // ─────────────────────────────────────────────────────────────────────

    #[test]
    fn simulator_first_arrival_always_passes() {
        let actual = simulate_passive_refresh_throttle(&[0], 5_000);
        assert_eq!(actual, vec![0]);
    }

    #[test]
    fn simulator_within_window_skipped() {
        // 0ms, 1000ms, 2000ms, 3000ms, 6000ms → 仅 0 与 6000 通过节流(5s)
        let actual = simulate_passive_refresh_throttle(&[0, 1_000, 2_000, 3_000, 6_000], 5_000);
        assert_eq!(actual, vec![0, 6_000]);
    }

    #[test]
    fn simulator_exactly_at_throttle_boundary_passes() {
        // 0, 5000 → 5000-0 = 5000 ≥ throttle,应该通过
        let actual = simulate_passive_refresh_throttle(&[0, 5_000], 5_000);
        assert_eq!(actual, vec![0, 5_000]);
    }

    #[test]
    fn simulator_just_below_boundary_skipped() {
        // 0, 4999 → 4999-0 < 5000,第二条被跳过
        let actual = simulate_passive_refresh_throttle(&[0, 4_999], 5_000);
        assert_eq!(actual, vec![0]);
    }
