// 手机退出后 QQ 可重建主进程；扫码重登只接管具有精确账号参数的冷启动实例。
use super::*;
use crate::snowluma::webui_client::HookProcessStatus;

pub(super) enum QrReloginPreparation {
    Unchanged,
    Waiting,
    Bound(TrackedQq, Arc<QrActivity>),
}

impl RemoteSnowLumaBackend {
    pub(super) async fn prepare_qr_relogin(
        &self,
        bot_id: &BotId,
        tracked: &TrackedQq,
        activity: &Arc<QrActivity>,
        request_close_revision: u32,
    ) -> Result<QrReloginPreparation, BotBackendError> {
        if activity.cleanup_failed.load(Ordering::SeqCst) {
            return Ok(QrReloginPreparation::Unchanged);
        }
        let Some(old_client) = self.login_clients.lock().await.get(bot_id).cloned() else {
            return Ok(QrReloginPreparation::Waiting);
        };
        let mut client = Arc::clone(&old_client);
        let mut processes = client
            .list_processes()
            .await
            .map_err(|error| BotBackendError::Io(error.to_string()))?;
        let original = processes.iter().find(|process| process.pid == tracked.pid);
        let host = self.daemon.current_host().await;
        let old_alive = host
            .exists(&HostPath::from_posix(format!("/proc/{}", tracked.pid)))
            .await
            .map_err(|error| BotBackendError::Io(error.to_string()))?;
        let old_current = old_alive
            && login_helper::process_start_time(host.as_ref(), tracked.pid).await?
                == tracked.start_time;
        // 登录事实优先；错号由原有门控处理，不在这里覆盖成功/确认状态。
        if old_current && original.is_some_and(|process| process.logged_in == Some(true)) {
            return Ok(QrReloginPreparation::Unchanged);
        }
        let has_terminal = activity
            .terminal
            .lock()
            .await
            .as_ref()
            .is_some_and(|snapshot| {
                snapshot.observer_removed
                    && matches!(
                        snapshot.state,
                        SnowLumaQrLoginState::LoggedIn
                            | SnowLumaQrLoginState::Cancelled
                            | SnowLumaQrLoginState::Unavailable
                    )
            });
        let unhooked = original.is_none_or(|process| {
            process.logged_in == Some(false)
                && process.injected != Some(true)
                && matches!(
                    process.status,
                    HookProcessStatus::Available
                        | HookProcessStatus::Disconnected
                        | HookProcessStatus::Error
                )
        });
        if old_current
            && (!has_terminal && (!unhooked || activity.recovery_started.load(Ordering::SeqCst)))
        {
            return Ok(QrReloginPreparation::Unchanged);
        }
        let qq_id = bot_id
            .as_str()
            .parse::<u64>()
            .map_err(|_| BotBackendError::InvalidConfig("Bot 账号无效".into()))?;
        let Some(pid) = super::super::inject::remote_qq_relogin_pid(
            host.as_ref(),
            qq_id,
            &self.daemon.layout().qq_bin,
        )
        .await?
        else {
            if !old_current {
                return Err(BotBackendError::Io("未找到此 Bot 的 QQ 重登进程".into()));
            }
            return Ok(QrReloginPreparation::Unchanged);
        };
        if old_current && pid != tracked.pid {
            return Ok(QrReloginPreparation::Unchanged);
        }
        let current = TrackedQq {
            pid,
            start_time: login_helper::process_start_time(host.as_ref(), pid).await?,
        };
        if pid != tracked.pid && self.daemon.webui_uses_pid_namespace().await {
            if activity.close_revision.load(Ordering::SeqCst) != request_close_revision {
                return Ok(QrReloginPreparation::Unchanged);
            }
            let Some(endpoints) = self.daemon.tunnel_endpoints().await else {
                return Ok(QrReloginPreparation::Waiting);
            };
            let webui_pid = self
                .daemon
                .webui_process_pid(pid, &current.start_time)
                .await?;
            // The old per-Bot client intentionally cannot expose the replacement PID.
            client = super::super::inject::inject_via_tunnel(&endpoints, pid, webui_pid).await?;
            processes = client
                .list_processes()
                .await
                .map_err(|error| BotBackendError::Io(error.to_string()))?;
        }
        let process = processes.iter().find(|process| process.pid == pid);
        if process.is_none() {
            return Ok(QrReloginPreparation::Waiting);
        }
        if current == *tracked && process.is_some_and(|process| process.logged_in != Some(false)) {
            return Ok(QrReloginPreparation::Unchanged);
        }
        if activity.close_revision.load(Ordering::SeqCst) != request_close_revision {
            return Ok(QrReloginPreparation::Unchanged);
        }
        if process.is_some_and(|process| {
            process.logged_in == Some(false) && process.injected != Some(true)
        }) {
            if let Err(error) = client.load_process(pid).await {
                if !matches!(&error, crate::snowluma::error::SnowLumaWebUiError::ServerRejected { message, .. }
                    if message.to_lowercase().contains("already"))
                {
                    return Err(BotBackendError::Io(error.to_string()));
                }
            }
            // 注入动作和辅助程序绑定各有超时，分轮执行避免占满一个前端请求。
            return Ok(
                if activity.close_revision.load(Ordering::SeqCst) == request_close_revision {
                    QrReloginPreparation::Waiting
                } else {
                    QrReloginPreparation::Unchanged
                },
            );
        }
        let renewed = Arc::new(QrActivity::new());
        renewed.refresh_pending.store(true, Ordering::SeqCst);
        let session = LoginSession::attach_running(
            host.as_ref(),
            self.daemon.paths(),
            self.daemon.layout(),
            bot_id,
            pid,
            &current.start_time,
            renewed.cancel.clone(),
        )
        .await?
        .ok_or_else(|| BotBackendError::Io("QQ 未提供可用扫码控件".into()))?;
        if activity.close_revision.load(Ordering::SeqCst) != request_close_revision {
            renewed.cancel.cancel();
            return Ok(QrReloginPreparation::Unchanged);
        }
        renewed.recovery_started.store(true, Ordering::SeqCst);
        if login_helper::process_start_time(host.as_ref(), pid).await? != current.start_time {
            return Err(BotBackendError::Io("扫码重登期间 QQ 进程已改变".into()));
        }
        let mut activities = self.qr_activity.lock().await;
        let mut pids = self.login_pids.lock().await;
        if activities
            .get(bot_id)
            .is_none_or(|value| !Arc::ptr_eq(value, activity))
            || pids.get(bot_id) != Some(tracked)
            || activity.close_revision.load(Ordering::SeqCst) != request_close_revision
            || self
                .login_clients
                .lock()
                .await
                .get(bot_id)
                .is_none_or(|value| !Arc::ptr_eq(value, &old_client))
        {
            return Ok(QrReloginPreparation::Unchanged);
        }
        activity.cancel.cancel();
        pids.insert(bot_id.clone(), current.clone());
        self.login_sessions
            .lock()
            .await
            .insert(bot_id.clone(), session);
        activities.insert(bot_id.clone(), renewed.clone());
        drop(pids);
        drop(activities);
        remember_remote_bot_pid(host.as_ref(), self.daemon.paths(), bot_id.as_str(), pid).await?;
        if current != *tracked {
            self.replace_poller(
                bot_id.clone(),
                pid,
                PollerDeps {
                    event_bus: Arc::clone(&self.event_bus),
                    http: client,
                    proc_tree: Arc::new(
                        crate::snowluma::linux_proc_probe::LinuxSinglePidProbe::new(pid),
                    ),
                    expected_uin: Some(bot_id.as_str().into()),
                },
            )
            .await;
        }
        tracing::info!(bot_id = %bot_id, old_pid = tracked.pid, pid, "QQ 重登扫码已绑定当前主进程");
        Ok(QrReloginPreparation::Bound(current, renewed))
    }
}
