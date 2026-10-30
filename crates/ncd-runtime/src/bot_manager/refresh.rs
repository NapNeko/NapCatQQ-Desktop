// 手动刷新运行状态，按主机合并连接探测并汇总每个 Bot 的结果。
use super::*;
use futures_util::stream::{self, StreamExt};

impl<R: BotConfigRepo + 'static, S: ConfigStore + 'static> BotManager<R, S> {
    pub async fn refresh_statuses(&self) -> Result<BatchResult, BotManagerError> {
        let _guard = self
            .refresh_lock
            .try_lock()
            .map_err(|_| BotManagerError::RemoteRead("Bot 状态正在刷新，请稍后再试".into()))?;
        let configs = self.repo.list().await?;
        let counts = crate::bootstrap_reconcile::count_native_snowluma_bots(&configs);
        let mut groups: HashMap<Option<String>, Vec<BotConfig>> = HashMap::new();
        for config in configs {
            let server = match &config.bot.runtime_target {
                RuntimeTarget::Server(id) => Some(id.clone()),
                RuntimeTarget::Local => None,
            };
            groups.entry(server).or_default().push(config);
        }
        let mut tasks = stream::iter(groups)
            .map(|(server, configs)| {
                let counts = &counts;
                async move {
                    let connection = async {
                        if let (Some(id), Some(manager)) = (&server, &self.server_manager) {
                            manager.probe_for_refresh(id).await?;
                        }
                        Ok::<_, String>(())
                    };
                    let connection = tokio::time::timeout(Duration::from_secs(20), connection)
                        .await
                        .unwrap_or_else(|_| Err("远端连接探测超时".into()));
                    stream::iter(configs)
                        .map(|config| {
                            let connection = &connection;
                            async move {
                                let id = BotId::new(config.bot.qq_id.to_string());
                                let result = match connection {
                                    Err(err) => Err(BotManagerError::RemoteRead(err.clone())),
                                    Ok(()) => tokio::time::timeout(
                                        Duration::from_secs(30),
                                        self.refresh_bot_status(&config, counts),
                                    )
                                    .await
                                    .unwrap_or_else(|_| {
                                        Err(BotManagerError::RemoteRead(
                                            "状态探测超时，请检查远端连接后重试".into(),
                                        ))
                                    }),
                                };
                                (id, result)
                            }
                        })
                        .buffer_unordered(4)
                        .collect::<Vec<_>>()
                        .await
                }
            })
            .buffer_unordered(4);
        let mut result = BatchResult {
            succeeded: Vec::new(),
            failed: Vec::new(),
        };
        while let Some(results) = tasks.next().await {
            for (id, outcome) in results {
                match outcome {
                    Ok(()) => result.succeeded.push(id),
                    Err(err) => result.failed.push((id, err)),
                }
            }
        }
        result.succeeded.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        result.failed.sort_by(|a, b| a.0.as_str().cmp(b.0.as_str()));
        Ok(result)
    }

    async fn refresh_bot_status(
        &self,
        config: &BotConfig,
        counts: &HashMap<String, usize>,
    ) -> Result<(), BotManagerError> {
        let id = BotId::new(config.bot.qq_id.to_string());
        if !config.bot.runtime_target.is_local()
            || config.bot.deployment_type == DeploymentType::Docker
        {
            if !self
                .bootstrap_reconciler()
                .reconcile_bot(config, counts, true)
                .await?
            {
                self.dispose_poller(&id).await;
            }
            return Ok(());
        }
        let handle = self.get_actor(&id).await?;
        let before = handle.snapshot();
        if !matches!(
            before.state,
            BotActorState::Stopped | BotActorState::Running | BotActorState::Crashed
        ) {
            return Err(BotManagerError::RemoteRead(
                "Bot 正在启停或修复，请稍后刷新".into(),
            ));
        }
        let status = self
            .backend_for_config(config)
            .await?
            .status(id.clone())
            .await?;
        let snapshot = handle
            .reconcile_observed(before.revision, status.state == BotActorState::Running)
            .await?
            .ok_or_else(|| {
                BotManagerError::RemoteRead("探测期间 Bot 状态已变化，请稍后刷新".into())
            })?;
        self.publish_state_change(&snapshot, "runtime_refreshed");
        self.event_bus
            .publish(DomainEvent::bot_status_changed(status, "runtime_refreshed"));
        if snapshot.state == BotActorState::Running {
            // 本机 NC 沿用现有端点，立即重建登录探测，不等待下一轮定时器。
            if let Some(endpoint) = self.napcat_endpoints.snapshot(&id).await {
                self.event_bus.publish(DomainEvent::napcat_webui_available(
                    id,
                    endpoint.port,
                    endpoint.token,
                ));
            }
        } else {
            self.dispose_poller(&id).await;
        }
        Ok(())
    }
}
