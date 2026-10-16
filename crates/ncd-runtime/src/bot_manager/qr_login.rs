// 扫码会话及错号恢复；只重启由 Desktop 管理的冷启动 Bot。
use super::*;
use ncd_domain::DeploymentType;
use ncd_domain::{SnowLumaQrLoginSnapshot, SnowLumaQrLoginState};

pub(super) struct QrRestart {
    pid: u32,
    revision: u64,
    result: Option<Result<(), String>>,
}

pub(super) type QrRestartTable = Arc<Mutex<HashMap<BotId, QrRestart>>>;

fn restarting_snapshot() -> SnowLumaQrLoginSnapshot {
    let mut snapshot = SnowLumaQrLoginSnapshot::unavailable("登录账号不一致，正在重启 Bot");
    snapshot.state = SnowLumaQrLoginState::Starting;
    snapshot
}

fn mismatch_restart_pid(config: &BotConfig, snapshot: &SnowLumaQrLoginSnapshot) -> Option<u32> {
    let cold = config
        .bot
        .snowluma_start_mode
        .unwrap_or(ncd_domain::SnowLumaStartMode::ColdStart)
        .is_cold();
    (config.bot.backend_type == BackendType::SnowLuma
        && cold
        && snapshot.state == SnowLumaQrLoginState::AccountMismatch
        && snapshot.observer_removed
        && snapshot.uin.as_ref().is_some_and(|uin| {
            !uin.is_empty() && uin != "0" && uin != &config.bot.qq_id.to_string()
        }))
    .then_some(snapshot.qq_pid)
    .flatten()
    .filter(|pid| *pid > 0)
}

impl<R: BotConfigRepo + 'static, S: ConfigStore + 'static> BotManager<R, S> {
    pub fn local_snowluma_qr_login_enabled(&self) -> bool {
        cfg!(windows)
            && self.store.config_dir().parent().is_some_and(|root| {
                crate::snowluma::session::load_snowluma_app_config(
                    &crate::DataPaths::new(root).snowluma_data_dir(),
                )
                .local_qr_login_enabled
            })
    }
    pub fn remote_snowluma_qr_login_enabled(&self) -> bool {
        self.store.config_dir().parent().is_some_and(|root| {
            crate::snowluma::session::load_snowluma_app_config(
                &crate::DataPaths::new(root).snowluma_data_dir(),
            )
            .remote_qr_login_enabled
        })
    }

    pub async fn remote_snowluma_qr_login_snapshot(
        &self,
        bot_id: &BotId,
        refresh: bool,
    ) -> Result<SnowLumaQrLoginSnapshot, BotManagerError> {
        if self
            .qr_login_restarts
            .lock()
            .await
            .get(bot_id)
            .is_some_and(|restart| restart.result.is_none())
        {
            return Ok(restarting_snapshot());
        }
        let mut snapshot = self
            .snowluma_qr_login_snapshot_inner(bot_id, refresh)
            .await?;
        let config = self.get_required_bot_config(bot_id).await?;
        let Some(pid) = mismatch_restart_pid(&config, &snapshot) else {
            return Ok(snapshot);
        };
        let before = self.get_snapshot(bot_id).await?;
        if before.state != BotActorState::Running {
            return Ok(snapshot);
        }
        {
            let mut restarts = self.qr_login_restarts.lock().await;
            if let Some(previous) = restarts.get(bot_id) {
                if previous.result.is_none() {
                    return Ok(restarting_snapshot());
                }
                if previous.pid == pid {
                    snapshot.message = Some(match &previous.result {
                        Some(Err(error)) => format!("登录账号不一致，重启失败：{error}"),
                        _ => "登录账号不一致，请手动重启此 Bot".into(),
                    });
                    return Ok(snapshot);
                }
            }
            restarts.insert(
                bot_id.clone(),
                QrRestart {
                    pid,
                    revision: before.revision,
                    result: None,
                },
            );
        }
        let manager = self.clone();
        let bot_id = bot_id.clone();
        tokio::spawn(async move {
            // 用户在排队期间停止/重启过 Bot 时，旧扫码结果不得再启动它。
            let current = manager.get_snapshot(&bot_id).await;
            let result = if current.is_ok_and(|value| value == before) {
                manager
                    .restart_bot(&bot_id)
                    .await
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            } else {
                Err("Bot 状态已改变，请重新打开扫码".into())
            };
            if let Err(error) = &result {
                manager.event_bus.publish(DomainEvent::bot_error(
                    bot_id.clone(),
                    error.clone(),
                    Some("错号登录恢复失败，请手动重启此 Bot".into()),
                ));
            }
            if let Some(restart) = manager.qr_login_restarts.lock().await.get_mut(&bot_id)
                && restart.pid == pid
                && restart.revision == before.revision
            {
                restart.result = Some(result);
            }
        });
        Ok(restarting_snapshot())
    }

    async fn snowluma_qr_login_snapshot_inner(
        &self,
        bot_id: &BotId,
        refresh: bool,
    ) -> Result<SnowLumaQrLoginSnapshot, BotManagerError> {
        let config = self.get_required_bot_config(bot_id).await?;
        if config.bot.backend_type == BackendType::SnowLuma
            && config.bot.runtime_target.is_local()
            && config.bot.deployment_type == DeploymentType::Native
        {
            if !self.local_snowluma_qr_login_enabled() {
                let mut snapshot = SnowLumaQrLoginSnapshot::unavailable("Windows 简化扫码未启用");
                snapshot.state = SnowLumaQrLoginState::Disabled;
                return Ok(snapshot);
            }
            return Ok(self
                .backend_for_config(&config)
                .await?
                .qr_login_snapshot(bot_id, refresh)
                .await?);
        }
        if config.bot.backend_type != BackendType::SnowLuma || config.bot.runtime_target.is_local()
        {
            return Ok(SnowLumaQrLoginSnapshot::unavailable(
                "仅远端 SnowLuma 支持简化扫码",
            ));
        }
        if !self.remote_snowluma_qr_login_enabled() {
            let mut snapshot = SnowLumaQrLoginSnapshot::unavailable("未启用远端简化扫码");
            snapshot.state = SnowLumaQrLoginState::Disabled;
            return Ok(snapshot);
        }
        let scenario = RuntimeScenario::from_config(&config)?;
        if matches!(scenario, RuntimeScenario::RemoteNative { .. }) {
            if let Some(resolver) = &self.host_resolver {
                let host = resolver
                    .resolve(&config.bot.runtime_target)
                    .await
                    .map_err(|error| BotManagerError::RemoteRead(error.to_string()))?;
                if host.os() == ncd_host::Os::Windows {
                    return Ok(self
                        .backend_for_config(&config)
                        .await?
                        .qr_login_snapshot(bot_id, refresh)
                        .await?);
                }
            }
        }
        if matches!(scenario, RuntimeScenario::RemoteDocker { .. }) {
            let backend = self.backend_for_config(&config).await?;
            if refresh {
                self.docker_sessions.request_qr_refresh(bot_id).await;
            }
            let Some((client, generation, login_detected)) = self
                .remote_runtime_sessions()
                .snowluma_docker_client(bot_id)
                .await
            else {
                let mut snapshot = SnowLumaQrLoginSnapshot::unavailable("等待 SL 连接");
                snapshot.state = SnowLumaQrLoginState::Starting;
                return Ok(snapshot);
            };
            let resolver = self
                .host_resolver
                .as_ref()
                .ok_or_else(|| BotManagerError::RemoteRead("主机尚未就绪".into()))?;
            let host = resolver
                .resolve(&config.bot.runtime_target)
                .await
                .map_err(|e| BotManagerError::RemoteRead(e.to_string()))?;
            let Some(pid) = ncd_backend_snowluma::docker_login::DockerLoginSession::bound_pid(
                host.as_ref(),
                bot_id,
            )
            .await?
            else {
                return Ok(SnowLumaQrLoginSnapshot::unavailable(
                    "容器扫码会话未就绪，请使用 VNC",
                ));
            };
            let before = client
                .list_processes()
                .await
                .map_err(|e| BotManagerError::RemoteRead(e.to_string()))?;
            if let Some(snapshot) = ncd_backend_snowluma::qr_guard::process_gate(
                before.iter().find(|p| p.pid == pid),
                pid,
                bot_id.as_str(),
            ) {
                if snapshot.state == SnowLumaQrLoginState::LoginDetected {
                    self.remote_runtime_sessions()
                        .mark_docker_qr_login_detected(bot_id, generation, &client)
                        .await;
                }
                if matches!(
                    snapshot.state,
                    SnowLumaQrLoginState::LoggedIn | SnowLumaQrLoginState::AccountMismatch
                ) {
                    backend.close_qr_login(bot_id).await?;
                }
                return Ok(snapshot);
            }
            if login_detected {
                let mut snapshot = SnowLumaQrLoginSnapshot::unavailable("等待 SL 确认登录账号");
                snapshot.state = SnowLumaQrLoginState::LoginDetected;
                snapshot.qq_pid = Some(pid);
                return Ok(snapshot);
            }
            let Some(effective_refresh) = self
                .docker_sessions
                .take_qr_refresh(bot_id, generation, &client)
                .await
            else {
                let mut snapshot = SnowLumaQrLoginSnapshot::unavailable("扫码会话已关闭");
                snapshot.state = SnowLumaQrLoginState::Cancelled;
                return Ok(snapshot);
            };
            let mut snapshot = backend.qr_login_snapshot(bot_id, effective_refresh).await?;
            let after = client
                .list_processes()
                .await
                .map_err(|e| BotManagerError::RemoteRead(e.to_string()))?;
            if let Some(gated) = ncd_backend_snowluma::qr_guard::process_gate(
                after.iter().find(|p| p.pid == pid),
                pid,
                bot_id.as_str(),
            ) {
                snapshot = gated;
                if matches!(
                    snapshot.state,
                    SnowLumaQrLoginState::LoggedIn | SnowLumaQrLoginState::AccountMismatch
                ) {
                    backend.close_qr_login(bot_id).await?;
                }
            }
            if snapshot
                .qq_pid
                .is_some_and(|snapshot_pid| snapshot_pid != pid)
            {
                return Ok(SnowLumaQrLoginSnapshot::unavailable("容器 QQ 会话已改变"));
            }
            let current = self
                .remote_runtime_sessions()
                .snowluma_docker_client(bot_id)
                .await;
            if !current.is_some_and(|(current_client, current_generation, _)| {
                generation == current_generation && Arc::ptr_eq(&client, &current_client)
            }) {
                snapshot.state = SnowLumaQrLoginState::Cancelled;
                snapshot.qrcode_payload = None;
            }
            if snapshot.state == SnowLumaQrLoginState::LoginDetected {
                self.remote_runtime_sessions()
                    .mark_docker_qr_login_detected(bot_id, generation, &client)
                    .await;
            }
            Ok(snapshot)
        } else {
            self.native_remote_qr_snapshot(bot_id, refresh, scenario)
                .await
        }
    }

    async fn native_remote_qr_snapshot(
        &self,
        bot_id: &BotId,
        refresh: bool,
        scenario: RuntimeScenario,
    ) -> Result<SnowLumaQrLoginSnapshot, BotManagerError> {
        let RuntimeScenario::RemoteNative { server_id, .. } = scenario else {
            return Ok(SnowLumaQrLoginSnapshot::unavailable("请使用 QQ 界面扫码"));
        };
        let backend = self
            .remote_snowluma_backends
            .lock()
            .await
            .get(&server_id)
            .cloned();
        match backend {
            Some(backend) => Ok(backend.qr_login_snapshot(bot_id, refresh).await),
            None => Ok(SnowLumaQrLoginSnapshot::unavailable(
                "扫码会话未就绪，请确认 Bot 已启动",
            )),
        }
    }

    pub async fn close_remote_snowluma_qr_login(
        &self,
        bot_id: &BotId,
    ) -> Result<(), BotManagerError> {
        // 重启已经负责关闭旧观察器；弹窗因 Stopping 卸载不能取消下一代会话。
        if self
            .qr_login_restarts
            .lock()
            .await
            .get(bot_id)
            .is_some_and(|restart| restart.result.is_none())
        {
            return Ok(());
        }
        self.remote_runtime_sessions()
            .cancel_docker_qr_login(bot_id)
            .await;
        if let Some(config) = self.get_bot_config(bot_id).await? {
            if config.bot.backend_type == BackendType::SnowLuma
                && !config.bot.runtime_target.is_local()
                && config.bot.deployment_type == DeploymentType::Native
            {
                if let Some(resolver) = &self.host_resolver {
                    let host = resolver
                        .resolve(&config.bot.runtime_target)
                        .await
                        .map_err(|error| BotManagerError::RemoteRead(error.to_string()))?;
                    if host.os() == ncd_host::Os::Windows {
                        return Ok(self
                            .backend_for_config(&config)
                            .await?
                            .close_qr_login(bot_id)
                            .await?);
                    }
                }
            }
            if config.bot.backend_type == BackendType::SnowLuma
                && (config.bot.deployment_type == DeploymentType::Docker
                    || config.bot.runtime_target.is_local())
            {
                return Ok(self
                    .backend_for_config(&config)
                    .await?
                    .close_qr_login(bot_id)
                    .await?);
            }
        }
        let backends: Vec<_> = self
            .remote_snowluma_backends
            .lock()
            .await
            .values()
            .cloned()
            .collect();
        for backend in backends {
            backend.close_qr_login(bot_id).await?;
        }
        Ok(())
    }

    pub async fn close_all_remote_snowluma_qr_logins(&self) {
        let backends: Vec<_> = self
            .remote_snowluma_backends
            .lock()
            .await
            .values()
            .cloned()
            .collect();
        for backend in backends {
            backend.close_all_qr_logins().await;
        }
        if let Ok(configs) = self.list_bot_configs().await {
            for config in configs {
                let windows_managed = if let Some(server) = config.bot.runtime_target.server_id() {
                    self.windows_bots
                        .manages(server, config.bot.qq_id, config.bot.backend_type)
                        .await
                } else {
                    false
                };
                if config.bot.backend_type == BackendType::SnowLuma
                    && (config.bot.deployment_type == DeploymentType::Docker
                        || config.bot.runtime_target.is_local()
                        || windows_managed)
                {
                    let bot_id = BotId::new(config.bot.qq_id.to_string());
                    let _ = self.close_remote_snowluma_qr_login(&bot_id).await;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mismatch_restart_requires_owned_process_and_confirmed_wrong_account() {
        let mut config = ncd_test_support::BotConfigBuilder::new()
            .backend_type(BackendType::SnowLuma)
            .build();
        let mut snapshot = SnowLumaQrLoginSnapshot::unavailable("test");
        snapshot.state = SnowLumaQrLoginState::AccountMismatch;
        snapshot.qq_pid = Some(42);
        snapshot.uin = Some("9999999999".into());
        assert_eq!(mismatch_restart_pid(&config, &snapshot), Some(42));
        config.bot.snowluma_start_mode = Some(ncd_domain::SnowLumaStartMode::HotStart);
        assert_eq!(mismatch_restart_pid(&config, &snapshot), None);
        config.bot.snowluma_start_mode = Some(ncd_domain::SnowLumaStartMode::ColdStart);
        snapshot.observer_removed = false;
        assert_eq!(mismatch_restart_pid(&config, &snapshot), None);
        snapshot.observer_removed = true;
        snapshot.uin = Some(config.bot.qq_id.to_string());
        assert_eq!(mismatch_restart_pid(&config, &snapshot), None);
        snapshot.uin = None;
        assert_eq!(mismatch_restart_pid(&config, &snapshot), None);
        snapshot.uin = Some("9999999999".into());
        snapshot.qq_pid = None;
        assert_eq!(mismatch_restart_pid(&config, &snapshot), None);
        snapshot.qq_pid = Some(42);
        for state in [
            SnowLumaQrLoginState::Scanned,
            SnowLumaQrLoginState::LoginDetected,
            SnowLumaQrLoginState::LoggedIn,
            SnowLumaQrLoginState::Unavailable,
        ] {
            snapshot.state = state;
            assert_eq!(mismatch_restart_pid(&config, &snapshot), None);
        }
    }
}
