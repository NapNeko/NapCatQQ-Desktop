// 停止失败必须离开过渡态；只有后端明确确认退出才发布停止完成。
use super::*;
use futures_util::FutureExt;

const STOP_BUDGET: Duration = Duration::from_secs(30);
const STOP_CLEANUP_BUDGET: Duration = Duration::from_secs(3);

impl<R: BotConfigRepo + 'static, S: ConfigStore + 'static> BotManager<R, S> {
    pub async fn stop_bot(&self, bot_id: &BotId) -> Result<BotActorSnapshot, BotManagerError> {
        let _migration_guard = self.guard_protocol_mutation(bot_id)?;
        info!(target: "ncd_runtime::bot_manager", bot_id = %bot_id, "收到停止 Bot 请求");
        let handle = self.get_actor(bot_id).await?;
        let before = handle.snapshot();
        let stopping = handle.request_stop().await?;
        self.publish_state_change(&stopping, "stop_requested");
        self.finish_requested_stop(bot_id, &handle, before, stopping, None)
            .await
    }

    pub(super) async fn finish_requested_stop(
        &self,
        bot_id: &BotId,
        handle: &BotActorHandle,
        before: BotActorSnapshot,
        stopping: BotActorSnapshot,
        previous_config: Option<BotConfig>,
    ) -> Result<BotActorSnapshot, BotManagerError> {
        let started = std::time::Instant::now();
        let mut stage = "等待当前 Bot 操作结束";
        let mut operation = None;
        let stop = async {
            // Cancel Starting before waiting: the starter may own this same gate.
            operation = Some(
                crate::bootstrap_reconcile::lock_bot_reconcile(&self.reconcile_locks, bot_id).await,
            );
            let current = handle.snapshot();
            if current.revision != stopping.revision {
                return Ok((current, None));
            }
            stage = "读取 Bot 配置";
            let config = match previous_config {
                Some(config) => config,
                None => self.get_required_bot_config(bot_id).await?,
            };
            self.write_watch_intent(&config, false).await;
            self.remote_runtime_sessions()
                .mark_remote_docker_stop_expected(bot_id)
                .await;
            stage = "连接运行后端";
            let backend = self.backend_for_lifecycle(&config).await?;
            stage = "确认停止前的运行状态";
            let mut status = backend.status(bot_id.clone()).await?;
            if status.state != BotActorState::Stopped {
                stage = "停止运行进程";
                backend.stop(bot_id.clone(), StopMode::Force).await?;
                stage = "确认运行进程已经退出";
                status = backend.status(bot_id.clone()).await?;
                if status.state != BotActorState::Stopped {
                    return Err(BotManagerError::RemoteRead(
                        "停止操作已返回，但运行进程尚未确认退出，请刷新状态后重试".into(),
                    ));
                }
            }
            let current = handle.snapshot();
            let stopped = match current.state {
                BotActorState::Stopping if current.revision == stopping.revision => {
                    match handle.confirm_stopped_revision(stopping.revision).await? {
                        Some(stopped) => stopped,
                        None => return Ok((handle.snapshot(), None)),
                    }
                }
                BotActorState::Stopped => current,
                BotActorState::Crashed if current.revision == stopping.revision => {
                    match handle.reconcile_observed(stopping.revision, false).await? {
                        Some(stopped) => stopped,
                        None => return Ok((handle.snapshot(), None)),
                    }
                }
                BotActorState::Starting
                    if current.last_transition.as_deref() == Some("restart_queued") =>
                {
                    current
                }
                _ => return Ok((current, None)),
            };
            self.event_bus
                .publish(DomainEvent::bot_status_changed(status, "runtime_stop"));
            self.publish_state_change(&stopped, "stop_completed");
            Ok::<_, BotManagerError>((stopped, Some(config)))
        };
        let outcome = match tokio::time::timeout(
            STOP_BUDGET,
            std::panic::AssertUnwindSafe(stop).catch_unwind(),
        )
        .await
        {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(BotManagerError::TaskJoinFailed(
                "停止 Bot 时后端异常退出".into(),
            )),
            Err(_) => Err(BotManagerError::RemoteRead(format!(
                "停止 Bot 超时（{stage}），运行状态尚未确认，请刷新后重试"
            ))),
        };
        info!(
            target: "ncd_runtime::bot_manager",
            bot_id = %bot_id,
            elapsed_ms = started.elapsed().as_millis() as u64,
            "停止 Bot 操作结束"
        );
        match outcome {
            Ok((snapshot, config)) => {
                if let Some(config) = &config {
                    // 此预算独立于进程停止预算；已确认的停止不能因可选清理变失败。
                    let cleanup = async {
                        self.dispose_poller(bot_id).await;
                        self.remote_runtime_sessions().shutdown_bot(bot_id).await;
                        self.refresh_snowluma_metrics_registration(config).await;
                    };
                    if !matches!(
                        tokio::time::timeout(
                            STOP_CLEANUP_BUDGET,
                            std::panic::AssertUnwindSafe(cleanup).catch_unwind(),
                        )
                        .await,
                        Ok(Ok(()))
                    ) {
                        warn!(target: "ncd_runtime::bot_manager", bot_id = %bot_id,
                            "Bot 已确认停止，日志或指标清理未及时完成");
                    }
                }
                // The stop gate must be released before queued restart takes it again.
                drop(operation);
                if snapshot.state == BotActorState::Starting && config.is_some() {
                    let config = match self.get_required_bot_config(bot_id).await {
                        Ok(config) => config,
                        Err(error) => {
                            if let Some(failed) = handle
                                .abort_start(snapshot.revision, Some(error.to_string()))
                                .await?
                            {
                                self.publish_state_change(&failed, "start_failed");
                            }
                            return Err(error);
                        }
                    };
                    return self
                        .start_runtime_from_starting(bot_id, handle, &config, snapshot.revision)
                        .await;
                }
                Ok(snapshot)
            }
            Err(error) => {
                let message = format!("停止未完成（{stage}）：{error}");
                error!(target: "ncd_runtime::bot_manager", bot_id = %bot_id, err = %message, "停止 Bot 未能确认完成");
                if let Some(restored) = handle
                    .abort_stop(
                        stopping.revision,
                        before.state == BotActorState::Running,
                        message.clone(),
                    )
                    .await?
                {
                    self.publish_state_change(&restored, "stop_failed");
                }
                self.event_bus.publish(DomainEvent::bot_error(
                    bot_id.clone(),
                    message,
                    Some("运行状态尚未确认，请刷新状态后重试；未确认停止不会标为已停止".into()),
                ));
                Err(error)
            }
        }
    }
}
