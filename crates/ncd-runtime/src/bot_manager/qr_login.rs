// 远端扫码命令只访问已有会话，不隐含启动或重启 QQ。
use super::*;
use ncd_domain::DeploymentType;
use ncd_domain::{SnowLumaQrLoginSnapshot, SnowLumaQrLoginState};

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
        self.remote_runtime_sessions()
            .cancel_docker_qr_login(bot_id)
            .await;
        if let Some(config) = self.get_bot_config(bot_id).await? {
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
                if config.bot.backend_type == BackendType::SnowLuma
                    && (config.bot.deployment_type == DeploymentType::Docker
                        || config.bot.runtime_target.is_local())
                {
                    let bot_id = BotId::new(config.bot.qq_id.to_string());
                    let _ = self.close_remote_snowluma_qr_login(&bot_id).await;
                }
            }
        }
    }
}
