// BotManager 长任务监听：SnowLuma UI 镜像、进程退出、NapCat login poller

use super::*;

impl<R: BotConfigRepo + 'static, S: ConfigStore + 'static> BotManager<R, S> {
    /// 长任务:镜像 SnowLuma UI 事件到内存表(hydrate),并在 daemon Crashed 时
    /// 级联把同 scope 的 SL actor 标 Crashed。
    /// - ready:subscribe 完成后立刻 signal,bootstrap 须等它再 reconcile attach
    ///   (与 run_napcat_login_listener 对称;broadcast 无 backlog)
    pub async fn run_snowluma_listener(
        self: Arc<Self>,
        ready: Option<tokio::sync::oneshot::Sender<()>>,
    ) {
        use crate::snowluma::DaemonState;

        let mut sub = self.event_bus.subscribe(EventFilter::all());
        if let Some(tx) = ready {
            let _ = tx.send(());
        }
        loop {
            let evt = match sub.next().await {
                Some(e) => e,
                None => break,
            };

            // 1) UI 会话态镜像(供 list_snowluma_ui_snapshot hydrate)
            match &evt {
                DomainEvent::SnowLumaDaemonStateChanged { state, .. } => {
                    self.snowluma_ui.set_daemon_state(*state).await;
                }
                DomainEvent::SnowLumaBotInjected { bot_id, .. } => {
                    self.snowluma_ui.mark_injected(bot_id).await;
                }
                DomainEvent::SnowLumaUinDetected { bot_id, uin } => {
                    self.snowluma_ui.set_uin(bot_id, uin.clone()).await;
                }
                DomainEvent::SnowLumaLoginStateChanged { bot_id, state } => {
                    self.snowluma_ui.set_login_state(bot_id, *state).await;
                }
                DomainEvent::SnowLumaDockerEndpointsReady { bot_id } => {
                    self.snowluma_ui.mark_endpoints_ready(bot_id).await;
                }
                DomainEvent::BotStateChanged { snapshot, .. }
                    if matches!(
                        snapshot.state,
                        BotActorState::Stopped | BotActorState::Crashed
                    ) =>
                {
                    self.snowluma_ui.clear_bot(&snapshot.bot_id).await;
                }
                _ => {}
            }

            // 2) daemon Crashed → 级联 actor
            let DomainEvent::SnowLumaDaemonStateChanged {
                state,
                reason,
                server_id: evt_scope,
                ..
            } = evt
            else {
                continue;
            };
            if state != DaemonState::Crashed {
                continue;
            }
            let snapshots: Vec<BotActorSnapshot> = {
                let actors = self.actors.read().await;
                actors.values().map(|h| h.snapshot()).collect()
            };
            for snap in snapshots {
                if !matches!(snap.state, BotActorState::Starting | BotActorState::Running) {
                    continue;
                }
                let bot_id = snap.bot_id.clone();
                let cfg = match self.get_required_bot_config(&bot_id).await {
                    Ok(c) => c,
                    Err(_) => continue,
                };
                if !matches!(cfg.bot.backend_type, BackendType::SnowLuma) {
                    continue;
                }
                let bot_scope = Self::snowluma_daemon_scope_for_config(&cfg);
                if bot_scope != evt_scope {
                    continue;
                }
                let handle = match self.get_actor(&bot_id).await {
                    Ok(h) => h,
                    Err(_) => continue,
                };
                let reason_str = reason
                    .clone()
                    .unwrap_or_else(|| "snowluma daemon crashed".to_string());
                if let Ok(crashed) = handle.mark_crashed(reason_str.clone()).await {
                    self.publish_state_change(&crashed, "snowluma_daemon_crashed");
                }
                self.event_bus.publish(DomainEvent::bot_error(
                    bot_id,
                    reason_str,
                    Some("SnowLuma daemon 已崩溃，请重启 App".to_string()),
                ));
            }
        }
    }

    /// 订阅运行时事件总线,将 BotProcessExited 转换为 actor 状态机转移:
    /// - 进程正常或异常退出 → 调用 confirm_stopped / mark_crashed
    ///   防止 UI 残留假 Running
    ///   返回的 future 由调用方在合适的运行时上 spawn(例如
    ///   tauri::async_runtime::spawn)它不依赖 tokio current handle
    ///   因此可以在 Tauri setup 回调里安全启动;用 tokio::spawn 在
    ///   没有 tokio 运行时上下文的位置直接跑会 panic
    pub async fn run_runtime_event_listener(self) {
        let mut subscription = self.event_bus.subscribe(EventFilter::kind(
            crate::events::DomainEventKind::BotProcessExited,
        ));
        while let Some(event) = subscription.next().await {
            if let DomainEvent::BotProcessExited {
                bot_id,
                exit_code,
                reason,
            } = event
            {
                self.handle_process_exited(bot_id, exit_code, reason).await;
            }
        }
    }

    /// 在当前 tokio 运行时上 spawn 事件监听任务
    /// 仅在调用方已处于 tokio 运行时上下文(#[tokio::test] 或被
    /// tauri::async_runtime::spawn 包过的 future)中使用;在 Tauri
    /// setup 这种无 tokio handle 的位置请改用:
    /// ignore
    /// let manager = bot_manager.clone()
    /// tauri::async_runtime::spawn(async move {
    /// (*manager).clone().run_runtime_event_listener().await
    /// })
    ///
    pub fn spawn_runtime_event_listener(&self) {
        let manager = self.clone();
        tokio::spawn(manager.run_runtime_event_listener());
    }

    async fn handle_process_exited(
        &self,
        bot_id: BotId,
        exit_code: Option<i32>,
        reason: Option<String>,
    ) {
        let handle = {
            let actors = self.actors.read().await;
            actors.get(&bot_id).cloned()
        };
        let Some(handle) = handle else {
            return;
        };

        let snapshot = handle.snapshot();
        match snapshot.state {
            // 主动停止流程:进程退出意味着 stop 完成
            BotActorState::Stopping => {
                if let Ok(updated) = handle.confirm_stopped().await {
                    self.publish_state_change(&updated, "process_exited");
                }
            }
            // 已运行中却收到退出事件:进程被外部 kill 或自身崩溃
            BotActorState::Running => {
                let detail = match (exit_code, reason.as_deref()) {
                    (Some(0), _) => "process exited with code 0".to_string(),
                    (Some(code), _) => format!("process exited with code {code}"),
                    (None, Some(reason)) => format!("process terminated: {reason}"),
                    (None, None) => "process terminated by signal".to_string(),
                };
                if let Ok(updated) = handle.mark_crashed(detail.clone()).await {
                    self.publish_state_change(&updated, "process_exited");
                    self.event_bus.publish(DomainEvent::bot_error(
                        bot_id,
                        detail,
                        Some("Bot 进程已退出，请检查日志或手动重启。".to_string()),
                    ));
                }
            }
            // Starting:可能是 restart 路径里 fast-path confirm_stopped 后转过来的,
            // 旧 backend 的 spawn_exit_watcher 还在 wait 旧 child handle,wait 返回时
            // 发出来的 exit 事件其实指向的是上一轮已被 force kill 的进程如果在这里
            // mark_crashed 会把刚 Starting 的新进程误标崩溃直接忽略最稳
            // 真正"启动失败"的情况由 start_bot 里 backend.start 的 Err 分支处理
            BotActorState::Starting => {}
            // 已是 Stopped / Crashed / Repairing:不再做转移,避免无效转移报错
            _ => {}
        }
    }

    /// 处理 NapCatWebuiAvailable 事件:为给定 Bot 创建/替换 NapCatLoginPoller
    /// 行为:
    /// - repo.get(bot_id) 不到对应配置时直接 return(不报错),避免在
    ///   配置删除后还接到延迟的 WebuiAvailable 事件时崩溃
    /// - 从 poller_settings.read().await 取最新值组装 PollerConfig:
    /// - login_check_interval ← settings.bot_login_check_interval_ms
    /// - unlogged_interval 固定 1s
    /// - auth_refresh_period 30 min;auth_refresh_throttle 5s;http_timeout 5s
    /// - offline_auto_restart ← bot_cfg.bot.offline_auto_restart
    /// - offline_notice_enabled = bot_cfg.advanced.offline_notice
    ///   App 级桌面通知总开关由注入的 OfflineNotifier 自行判断
    /// - 旧 Poller 先 dispose(取消其 CancellationToken 并触发 Drop 兜底)
    ///   再插入新实例,保证不会同时存在两个 Poller 抢同一 BotId 的事件
    ///   restart_handle 通过 Arc::clone(self) as Arc<dyn RestartHandle> 注入
    ///   利用本类型的 impl RestartHandle for BotManager(见文件末尾)
    pub async fn handle_webui_available(
        self: &Arc<Self>,
        bot_id: BotId,
        port: u16,
        token: String,
        host_port: Option<u16>,
    ) {
        // 0. 先把 (port, token[, host_port]) 落进 endpoint 表
        //    port: Desktop 本机可达(本机进程口/远端隧道口),login_poller 用
        //    host_port: 远端真实监听口,ncd-watch 用;本机为 None
        self.napcat_endpoints
            .insert(
                bot_id.clone(),
                NapCatEndpoint {
                    port,
                    host_port,
                    token: token.clone(),
                },
            )
            .await;

        // 1. 取 BotConfig;解析失败或不存在时静默 return(事件可能晚到)
        let qq_id: u64 = match bot_id.as_str().parse() {
            Ok(v) => v,
            Err(_) => return,
        };
        let bot_cfg = match self.repo.get(qq_id).await {
            Ok(Some(cfg)) => cfg,
            _ => return,
        };

        let settings = self.poller_settings.read().await.clone();
        // 离线通知开关:Bot 高级「掉线时下发桌面通知」;Poller 据此调用 OfflineNotifier
        let cfg = PollerConfig {
            login_check_interval: Duration::from_millis(settings.bot_login_check_interval_ms),
            unlogged_interval: Duration::from_secs(1),
            auth_refresh_period: Duration::from_secs(30 * 60),
            auth_refresh_throttle: Duration::from_secs(5),
            http_timeout: Duration::from_secs(5),
            offline_auto_restart: bot_cfg.bot.offline_auto_restart,
            offline_notice_enabled: bot_cfg.advanced.offline_notice,
        };

        // 3. 注入依赖restart_handle 把 BotManager 自身作为 RestartHandle
        let deps = PollerDeps {
            event_bus: Arc::clone(&self.event_bus),
            http: Arc::clone(&self.webui_client),
            notifier: Arc::clone(&self.offline_notifier),
            restart_handle: Arc::clone(self) as Arc<dyn RestartHandle>,
        };

        // 4. 替换旧 Poller,再插入新实例
        let mut pollers = self.login_pollers.write().await;
        if let Some(old) = pollers.remove(&bot_id) {
            old.dispose();
        }
        let poller = NapCatLoginPoller::spawn(bot_id.clone(), port, token, cfg, deps);
        pollers.insert(bot_id, poller);
    }

    /// 移除并取消指定 Bot 的 NapCatLoginPoller多次调用幂等
    /// 由 run_napcat_login_listener 在 BotProcessExited 事件到达时调用
    /// 也由 delete_bot_internal / shutdown_all 在生命周期收尾时调用
    /// 同步清理 napcat_endpoints 中对应记录,避免后续保存配置查到陈旧端口
    pub async fn dispose_poller(&self, bot_id: &BotId) {
        let mut pollers = self.login_pollers.write().await;
        if let Some(poller) = pollers.remove(bot_id) {
            poller.dispose();
        }
        drop(pollers);
        // endpoint 表与 poller 生命周期严格对齐:bot 进程一旦退出,原来的
        // (port, token) 立即作废(NapCat 重启时 token 会换,端口也可能换),
        // 必须立刻清掉,避免后续 upsert 查到陈旧值打到一个已经死亡的端口
        self.napcat_endpoints.remove(bot_id).await;
    }

    /// 监听 NapCatWebuiAvailable 与 BotProcessExited 两路事件,分别驱动
    /// Poller 的创建与回收
    /// - Arc<Self> 作为 receiver:handle_webui_available 需要把
    ///   Arc<BotManager<R, S>> 转成 Arc<dyn RestartHandle> 注入 PollerDeps
    /// - tokio::select! 同时消费两路 subscription;任一路关闭都会让 else =>
    ///   分支退出循环,避免半挂死
    /// - 调用方(Tauri setup 或测试)通过 tauri::async_runtime::spawn /
    ///   tokio::spawn 启动;与 run_runtime_event_listener 风格一致
    /// - ready:subscribe 完成后立刻 signal,bootstrap 必须等它再 attach,
    ///   否则 broadcast 无 backlog 会丢掉 napcat_webui_available(多实例 port+1)
    pub async fn run_napcat_login_listener(
        self: Arc<Self>,
        ready: Option<tokio::sync::oneshot::Sender<()>>,
    ) {
        // 远端 NC 隧道建不起来 / 连续探活失败:清 poller+endpoint,避免 UI 打死后端口
        {
            let manager = Arc::clone(&self);
            self.remote_native_napcat_sessions
                .set_on_webui_unreachable(Arc::new(move |bot_id| {
                    let manager = Arc::clone(&manager);
                    tokio::spawn(async move {
                        manager.dispose_poller(&bot_id).await;
                        // 不把 actor 标 Crashed;只发 reason 让前端灭 WebUI 按钮
                        if let Ok(snap) = manager.get_snapshot(&bot_id).await {
                            manager.event_bus.publish(DomainEvent::bot_state_changed(
                                snap,
                                "webui_tunnel_unreachable",
                            ));
                        }
                    });
                }))
                .await;
        }

        let mut webui_sub = self
            .event_bus
            .subscribe(EventFilter::kind(DomainEventKind::NapCatWebuiAvailable));
        let mut exit_sub = self
            .event_bus
            .subscribe(EventFilter::kind(DomainEventKind::BotProcessExited));
        if let Some(tx) = ready {
            let _ = tx.send(());
        }
        loop {
            tokio::select! {
                ev = webui_sub.next() => match ev {
                    Some(DomainEvent::NapCatWebuiAvailable {
                        bot_id,
                        port,
                        token,
                        host_port,
                    }) => {
                        self.handle_webui_available(bot_id, port, token, host_port)
                            .await;
                    }
                    Some(_) => continue,
                    None => break,
                },
                ev = exit_sub.next() => match ev {
                    Some(DomainEvent::BotProcessExited { bot_id, .. }) => {
                        self.dispose_poller(&bot_id).await;
                        self.remote_runtime_sessions().shutdown_bot(&bot_id).await;
                    }
                    Some(_) => continue,
                    None => break,
                },
            }
        }
    }
}
