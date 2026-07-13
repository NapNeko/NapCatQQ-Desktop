// BotManager 长任务监听：SnowLuma UI 镜像、进程退出、NapCat login poller。
// 与 mod.rs 同 impl 块拆分，避免编排主路径与事件环搅在一起。

use super::*;

impl<R: BotConfigRepo + 'static, S: ConfigStore + 'static> BotManager<R, S> {
    // ready: subscribe 完成后立刻 signal；bootstrap 须等它再 reconcile attach
    // （与 run_napcat_login_listener 对称；broadcast 无 backlog）。
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

    // 进程退出 → confirm_stopped / mark_crashed，避免 UI 残留假 Running。
    // 不依赖 current tokio handle，可在 tauri::async_runtime::spawn 里跑。
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

    // 仅在已有 tokio handle 的上下文调用；Tauri setup 请用 async_runtime::spawn。
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
            // Starting: restart fast-path 后旧 child 的 exit 可能晚到；
            // 若 mark_crashed 会误伤新进程。启动失败由 start_bot 的 Err 分支处理。
            BotActorState::Starting => {}
            // Stopped / Crashed / Repairing: 不再转移
            _ => {}
        }
    }

    // WebUI 可用 → 建/换 NapCatLoginPoller。配置已删时静默 return（事件可能晚到）。
    // 旧 poller 先 dispose 再 insert，避免双 poller 抢同一 BotId。
    pub async fn handle_webui_available(
        self: &Arc<Self>,
        bot_id: BotId,
        port: u16,
        token: String,
        host_port: Option<u16>,
    ) {
        // port: Desktop 本机可达；host_port: 远端真实口（ncd-watch）；本机 None。
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

        let Ok(qq_id) = bot_id.as_str().parse::<u64>() else {
            return;
        };
        let bot_cfg = match self.repo.get(qq_id).await {
            Ok(Some(cfg)) => cfg,
            _ => return,
        };

        let settings = self.poller_settings.read().await.clone();
        // offline_notice: Bot 高级「掉线通知」；App 总开关由 OfflineNotifier 自行判断。
        let cfg = PollerConfig {
            login_check_interval: Duration::from_millis(settings.bot_login_check_interval_ms),
            unlogged_interval: Duration::from_secs(1),
            auth_refresh_period: Duration::from_secs(30 * 60),
            auth_refresh_throttle: Duration::from_secs(5),
            http_timeout: Duration::from_secs(5),
            offline_auto_restart: bot_cfg.bot.offline_auto_restart,
            offline_notice_enabled: bot_cfg.advanced.offline_notice,
        };

        let deps = PollerDeps {
            event_bus: Arc::clone(&self.event_bus),
            http: Arc::clone(&self.webui_client),
            notifier: Arc::clone(&self.offline_notifier),
            restart_handle: Arc::clone(self) as Arc<dyn RestartHandle>,
        };

        let mut pollers = self.login_pollers.write().await;
        if let Some(old) = pollers.remove(&bot_id) {
            old.dispose();
        }
        let poller = NapCatLoginPoller::spawn(bot_id.clone(), port, token, cfg, deps);
        pollers.insert(bot_id, poller);
    }

    // 幂等：进程退出 / 删除 / shutdown 时清 poller + endpoint，避免热推送到死端口。
    pub async fn dispose_poller(&self, bot_id: &BotId) {
        let mut pollers = self.login_pollers.write().await;
        if let Some(poller) = pollers.remove(bot_id) {
            poller.dispose();
        }
        drop(pollers);
        self.napcat_endpoints.remove(bot_id).await;
    }

    // WebuiAvailable 建 poller；ProcessExited 回收。ready 后 bootstrap 才能 attach。
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
