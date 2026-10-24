use super::*;

impl<R: BotConfigRepo + 'static, S: ConfigStore + 'static> BotManager<R, S> {
    /// 准备 WebUI 入口，不尝试登录或恢复 QQ 图形会话。
    pub async fn prepare_snowluma_webui(
        &self,
        bot_id: &BotId,
    ) -> Result<(u16, String), BotManagerError> {
        let config = self.get_required_bot_config(bot_id).await?;
        if config.bot.backend_type != BackendType::SnowLuma {
            return Err(BotManagerError::Render("当前 Bot 不是 SnowLuma".into()));
        }
        match RuntimeScenario::from_config(&config)? {
            RuntimeScenario::RemoteNative { server_id, .. } => {
                let host = if let Some(manager) = &self.server_manager {
                    manager
                        .get_live_host(&server_id)
                        .await
                        .map_err(BotManagerError::RemoteRead)?
                } else {
                    let resolver = self.host_resolver.as_ref().ok_or_else(|| {
                        BotManagerError::RemoteRead("远端主机解析器未就绪".into())
                    })?;
                    let host = resolver
                        .resolve(&config.bot.runtime_target)
                        .await
                        .map_err(|error| BotManagerError::RemoteRead(error.to_string()))?;
                    if host.is_healthy().await {
                        host
                    } else {
                        resolver
                            .refresh(&config.bot.runtime_target)
                            .await
                            .map_err(|error| BotManagerError::RemoteRead(error.to_string()))?
                    }
                };
                if host.os() == ncd_host::Os::Windows {
                    return self
                        .windows_bots
                        .snowluma_endpoint(&server_id)
                        .await
                        .map(|(port, password, _)| (port, password))
                        .ok_or_else(|| {
                            BotManagerError::Render("SnowLuma 远端隧道尚未就绪".into())
                        });
                }
                let daemon = self
                    .runtime_router()
                    .remote_snowluma_daemon_for_server(&server_id, host)
                    .await?;
                daemon.prepare_webui().await.map_err(Into::into)
            }
            RuntimeScenario::RemoteDocker { .. } => self
                .snowluma_docker_endpoints(bot_id)
                .await
                .map(|endpoint| (endpoint.webui_local_port, endpoint.webui_password))
                .ok_or_else(|| {
                    BotManagerError::Render(
                        "SnowLuma Docker 隧道未就绪，请确认 Bot 已启动且运行宿主为远端 SSH".into(),
                    )
                }),
            RuntimeScenario::LocalNative { .. } => {
                crate::local_snowluma_webui_endpoint(self.store.root())
                    .map_err(BotManagerError::Render)
            }
        }
    }
}
