//! Bot 启动与重启编排，包含启动代次、取消和进程提交确认。

use super::runtime_gate::describe_not_ready;
use super::{BotManager, BotManagerError};
use crate::bot_actor::{BotActorHandle, BotActorSnapshot, BotActorState};
use crate::events::{DomainEvent, EventBus};
use crate::runtime_launch_plan::RuntimeLaunchPlanError;
use crate::snowluma::AgreementsPayload;
use ncd_domain::bot_config::{BackendType, BotConfig};
use ncd_domain::ids::BotId;
use ncd_domain::kinds::{RuntimeTarget, StopMode};
use ncd_domain::{DeploymentType, RuntimeScenario};
use ncd_traits::runtime_backend::{BotBackend, BotBackendError, BotRuntimeConfig, BotStartCtx};
use ncd_traits::{BotConfigRepo, ConfigStore};
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info, warn};

impl<R: BotConfigRepo + 'static, S: ConfigStore + 'static> BotManager<R, S> {
    /// 带用户决议启动 Bot
    ///
    /// decisions 来自前端 ConfigDriftDialog,包含:
    /// - AcceptExternal { file, path, value }:渲染输出时把对应 path 覆盖为外部值
    /// - DropAdded { file, path }:不保留新增字段(覆盖时这些字段不出现在 existing 里即可)
    /// - KeepAdded / UseInternal:无需特别处理(默认行为)
    pub async fn start_bot_with_decisions(
        &self,
        bot_id: &BotId,
        decisions: &[crate::config_drift::DriftDecision],
    ) -> Result<BotActorSnapshot, BotManagerError> {
        use crate::config_drift::DriftDecision;

        let _migration_guard = self.guard_protocol_mutation(bot_id)?;

        let handle = self.get_actor(bot_id).await?;
        let current = handle.snapshot();
        if matches!(
            current.state,
            BotActorState::Starting | BotActorState::Running
        ) {
            return Ok(current);
        }

        // 把 AcceptExternal 的决议转成 overrides map
        let mut overrides: std::collections::HashMap<String, Vec<(String, serde_json::Value)>> =
            std::collections::HashMap::new();
        for d in decisions {
            if let DriftDecision::AcceptExternal { file, path, value } = d {
                overrides
                    .entry(file.clone())
                    .or_default()
                    .push((path.clone(), value.clone()));
            }
        }

        // DropAdded 的决议:从 existing 文件里删掉对应 key 再渲染这里不做
        // 额外处理——因为 render_with_existing 只合并 existing 里在 known_keys
        // outside known_keys 的顶层字段如果要 drop 某个顶层扩展字段,需要从 existing map
        // 里主动 remove但当前 render_backend_config 是从磁盘现读的 existing,
        // 要 drop 的字段仍然在磁盘文件里
        //
        // 处理方式:把 DropAdded 转成"用 null 覆盖"→ set_value_at_dot_path 遇到
        // null 会 remove 该 key,效果等价于"用户选择丢弃"
        for d in decisions {
            if let DriftDecision::DropAdded { file, path } = d {
                overrides
                    .entry(file.clone())
                    .or_default()
                    .push((path.clone(), serde_json::Value::Null));
            }
        }

        let config = self.get_required_bot_config(bot_id).await?;
        // 协议门禁必须在 request_start 之前：未同意不应进入 Starting，更不能 mark_crashed
        self.ensure_snowluma_agreements_ready(&config).await?;
        self.ensure_runtime_ready(&config).await?;
        self.render_backend_config(bot_id, &config, &overrides)
            .await?;

        let (starting, advanced) = handle.request_start_transition().await?;
        if !advanced {
            return Ok(starting);
        }
        self.publish_state_change(&starting, "start_requested");
        self.start_runtime_from_starting(bot_id, &handle, &config, starting.revision)
            .await
    }

    /// 启动指定 Bot(无 drift 决议版本,等价于全部 UseInternal)
    /// 前置条件:Actor 已存在且处于可启动状态(Stopped / Crashed)
    pub async fn start_bot(&self, bot_id: &BotId) -> Result<BotActorSnapshot, BotManagerError> {
        let _migration_guard = self.guard_protocol_mutation(bot_id)?;
        info!(target: "ncd_runtime::bot_manager", bot_id = %bot_id, "收到启动 Bot 请求");
        let handle = self.get_actor(bot_id).await?;
        let current = handle.snapshot();
        // Repeated starts must not repeat remote agreement/runtime probes or rendering.
        if matches!(
            current.state,
            BotActorState::Starting | BotActorState::Running
        ) {
            return Ok(current);
        }
        let config = self.get_required_bot_config(bot_id).await?;
        // 协议门禁必须在 request_start 之前：未同意不应进入 Starting，更不能 mark_crashed
        self.ensure_snowluma_agreements_ready(&config).await?;
        self.ensure_runtime_ready(&config).await?;
        self.render_backend_config(bot_id, &config, &std::collections::HashMap::new())
            .await?;

        let (starting, advanced) = handle.request_start_transition().await?;
        if !advanced {
            return Ok(starting);
        }
        self.publish_state_change(&starting, "start_requested");
        self.start_runtime_from_starting(bot_id, &handle, &config, starting.revision)
            .await
    }

    /// 重启沿用停止的确认与失败收敛；Stopping 时只排队，不并发重复停机。
    pub async fn restart_bot(&self, bot_id: &BotId) -> Result<BotActorSnapshot, BotManagerError> {
        let _migration_guard = self.guard_protocol_mutation(bot_id)?;
        info!(target: "ncd_runtime::bot_manager", bot_id = %bot_id, "收到重启 Bot 请求");
        let handle = self.get_actor(bot_id).await?;
        let snap = handle.snapshot();
        match snap.state {
            BotActorState::Running | BotActorState::Starting => {
                let stopping = handle.request_restart().await?;
                self.publish_state_change(&stopping, "restart_requested");
                self.finish_requested_stop(bot_id, &handle, snap, stopping, None)
                    .await
            }
            BotActorState::Stopped | BotActorState::Crashed => self.start_bot(bot_id).await,
            BotActorState::Stopping => {
                let stopping = handle.request_restart().await?;
                self.publish_state_change(&stopping, "restart_requested");
                Ok(stopping)
            }
            BotActorState::Repairing => Err(BotManagerError::InvalidState {
                bot_id: bot_id.clone(),
                state: snap.state,
                action: "restart",
            }),
        }
    }

    /// 启动编排仅负责将准备好的环境交给 QQ 进程或共享 SnowLuma daemon。
    async fn apply_local_metrics(
        &self,
        bot_id: &BotId,
        config: &BotConfig,
        runtime_config: &mut BotRuntimeConfig,
    ) {
        if !config.bot.runtime_target.is_local()
            || config.bot.deployment_type == DeploymentType::Docker
            || (config.bot.backend_type == BackendType::SnowLuma && self.snowluma_daemon.is_none())
        {
            return;
        }
        let Some(data_root) = self.store.config_dir().parent().map(|p| p.to_path_buf()) else {
            return;
        };
        match super::local_metrics::prepare_local_metrics(data_root, bot_id.clone(), config.clone())
            .await
        {
            Ok(environment) => match config.bot.backend_type {
                BackendType::NapCat => runtime_config.environment.extend(environment),
                BackendType::SnowLuma => {
                    if let Some(daemon) = &self.snowluma_daemon {
                        daemon.set_metrics_child_env(Some(environment));
                    }
                }
            },
            Err(error) => warn!(
                target: "ncd_runtime::bot_manager", bot_id = %bot_id, %error,
                "metrics inject prepare failed"
            ),
        }
    }

    fn build_runtime_config(&self, bot_id: &BotId, config: &BotConfig) -> BotRuntimeConfig {
        BotRuntimeConfig::default_path(self.store.root(), bot_id.clone())
            .with_runtime_defaults(self.store.root())
            .with_bot_config(config)
    }

    pub(super) fn snowluma_daemon_scope_for_config(config: &BotConfig) -> Option<String> {
        if config.bot.backend_type != BackendType::SnowLuma
            || config.bot.deployment_type == DeploymentType::Docker
        {
            return None;
        }
        match &config.bot.runtime_target {
            RuntimeTarget::Local => Some(DomainEvent::SNOWLUMA_DAEMON_SCOPE_LOCAL.to_string()),
            RuntimeTarget::Server(sid) => Some(sid.clone()),
        }
    }

    fn skip_post_start_status_recheck(config: &BotConfig) -> bool {
        config.bot.backend_type == BackendType::SnowLuma
            && config.bot.runtime_target.is_local()
            && config.bot.deployment_type == DeploymentType::Native
    }

    async fn ensure_snowluma_agreements_ready(
        &self,
        config: &BotConfig,
    ) -> Result<(), BotManagerError> {
        if config.bot.backend_type != BackendType::SnowLuma {
            return Ok(());
        }
        if let Some(payload) = self.snowluma_agreements().prepare(config).await?
            && payload.consent_required
        {
            return Err(Self::snowluma_consent_required_error(&payload.version));
        }
        Ok(())
    }

    /// 框架 + Run 依赖预检。探测失败不拦(交给真正启动去报错),只有确认缺东西才拦
    async fn ensure_runtime_ready(&self, config: &BotConfig) -> Result<(), BotManagerError> {
        let Some(gate) = self.runtime_gate.as_ref() else {
            return Ok(());
        };
        let readiness = match gate.check(config).await {
            Ok(Some(r)) => r,
            Ok(None) => return Ok(()),
            Err(err) => {
                warn!(
                    target: "ncd_runtime::bot_manager",
                    error = %err,
                    "runtime readiness probe failed; continuing start"
                );
                return Ok(());
            }
        };
        let local = matches!(config.bot.runtime_target, RuntimeTarget::Local);
        match describe_not_ready(&readiness, local) {
            Some(reason) => Err(BotManagerError::RuntimeNotReady(reason)),
            None => Ok(()),
        }
    }

    fn snowluma_consent_required_error(version: &str) -> BotManagerError {
        BotManagerError::Runtime(BotBackendError::Io(format!(
            "SNOWLUMA_CONSENT_REQUIRED: SnowLuma agreements version {version} requires consent"
        )))
    }

    /// 协议未签导致的启动中止：Starting → Stopped，不标 Crashed，避免 UI 协议弹窗与崩溃告警打架。
    async fn abort_start_for_consent(
        &self,
        bot_id: &BotId,
        handle: &BotActorHandle,
        revision: u64,
        err: BotManagerError,
    ) -> BotManagerError {
        let message = err.to_string();
        if let Ok(Some(stopped)) = handle.abort_start(revision, None).await {
            self.publish_state_change(&stopped, "start_aborted_consent");
        }
        self.event_bus.publish(DomainEvent::bot_error(
            bot_id.clone(),
            message,
            Some("请先阅读并同意 SnowLuma 用户协议与隐私政策后重试启动。".to_string()),
        ));
        err
    }

    async fn snowluma_docker_agreements_after_attach(
        &self,
        config: &BotConfig,
    ) -> Result<Option<AgreementsPayload>, BotManagerError> {
        let scenario = RuntimeScenario::from_config(config)?;
        if !scenario.is_docker() || scenario.backend_type() != BackendType::SnowLuma {
            return Ok(None);
        }
        self.snowluma_agreements().prepare(config).await
    }

    pub(super) async fn start_runtime_from_starting(
        &self,
        bot_id: &BotId,
        handle: &BotActorHandle,
        config: &BotConfig,
        revision: u64,
    ) -> Result<BotActorSnapshot, BotManagerError> {
        use futures_util::FutureExt;

        let cancel = handle.cancellation_token();
        let mut commit = None;
        let start = async {
            let operation = tokio::select! {
                biased;
                _ = cancel.cancelled() => return Err(BotManagerError::Cancelled),
                guard = crate::bootstrap_reconcile::lock_bot_reconcile(&self.reconcile_locks, bot_id) => Arc::new(guard),
            };
            let current = handle.snapshot();
            if current.state != BotActorState::Starting || current.revision != revision {
                return Err(BotManagerError::Cancelled);
            }
            self.remote_runtime_sessions()
                .prepare_before_runtime_start(config)
                .await;
            self.start_runtime_generation(
                handle,
                config,
                revision,
                cancel.clone(),
                operation,
                &mut commit,
            )
            .await
        };
        let outcome = match tokio::time::timeout(
            Duration::from_secs(180),
            std::panic::AssertUnwindSafe(start).catch_unwind(),
        )
        .await
        {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(BotManagerError::TaskJoinFailed(
                "启动 Bot 时后端异常退出".into(),
            )),
            Err(_) => Err(BotManagerError::RemoteRead(
                "启动 Bot 超时；已发起的进程将等待本轮启动返回后确认清理，请刷新后重试".into(),
            )),
        };
        drop(commit);
        if let Err(error) = &outcome {
            if let Some(failed) = handle
                .abort_start(revision, Some(error.to_string()))
                .await?
            {
                self.publish_state_change(&failed, "start_failed");
                self.event_bus.publish(DomainEvent::bot_error(
                    bot_id.clone(),
                    error.to_string(),
                    Some("请刷新运行状态后重试".into()),
                ));
            }
        }
        outcome
    }

    async fn start_runtime_generation(
        &self,
        handle: &BotActorHandle,
        config: &BotConfig,
        revision: u64,
        cancel: tokio_util::sync::CancellationToken,
        operation: Arc<tokio::sync::OwnedMutexGuard<()>>,
        commit: &mut Option<tokio::sync::oneshot::Sender<()>>,
    ) -> Result<BotActorSnapshot, BotManagerError> {
        let bot_id = handle.bot_id();
        let scenario = RuntimeScenario::from_config(config)?;
        self.write_watch_intent(config, true).await;
        let mut runtime_config = match &scenario {
            RuntimeScenario::LocalNative { .. } => {
                let base = self.build_runtime_config(bot_id, config);
                match self.launch_planner.build_plan(bot_id, config).await {
                    Ok(plan) => plan.into_runtime_config(base),
                    Err(err) => {
                        let message = err.to_string();
                        error!(
                            target: "ncd_runtime::bot_manager",
                            bot_id = %bot_id,
                            err = %message,
                            "构造 Bot 启动计划失败"
                        );
                        let hint = match &err {
                            RuntimeLaunchPlanError::SnowLumaNodeMissing(path) => Some(format!(
                                "未在 {} 找到 SnowLuma 所需 Node.js 运行时。Full 包应包含内置 node.exe，Lite 包需要可用的 Node.js；请先安装 Node.js 组件或在设置中配置 Node.js 环境。",
                                path.display()
                            )),
                            RuntimeLaunchPlanError::SnowLumaInvalidStartMode(detail) => {
                                Some(format!(
                                    "SnowLuma 启动参数无效：{detail}。请在 Bot 配置中检查启动模式。"
                                ))
                            }
                            RuntimeLaunchPlanError::MissingFile { .. } => Some(
                                "NapCat 运行时组件缺失，请先在「设置」页安装运行时。".to_string(),
                            ),
                            _ => Some(
                                "启动计划构造失败：请检查后端类型与运行时安装状态。".to_string(),
                            ),
                        };
                        if let Some(crashed) =
                            handle.abort_start(revision, Some(message.clone())).await?
                        {
                            self.publish_state_change(&crashed, "start_failed");
                            self.event_bus.publish(DomainEvent::bot_error(
                                bot_id.clone(),
                                message,
                                hint,
                            ));
                        }
                        return Err(BotManagerError::Render(err.to_string()));
                    }
                }
            }
            RuntimeScenario::LocalDocker { .. }
            | RuntimeScenario::RemoteNative { .. }
            | RuntimeScenario::RemoteDocker { .. } => self.build_runtime_config(bot_id, config),
        };
        // 必须在 build_plan 之后：plan 会把 loadNapCat.js 写成无探针基线，
        // 若先 inject 再 build_plan，require(探针) 会被覆盖，永远写不出 net-stats。
        self.apply_local_metrics(bot_id, config, &mut runtime_config)
            .await;

        // 二次兜底：start_bot 入口已检查；Docker attach 后另有路径。
        // 若仍命中，回 Stopped 而不是 Crashed（协议未签 ≠ 运行崩溃）。
        if let Err(err) = self.ensure_snowluma_agreements_ready(config).await {
            if cancel.is_cancelled() {
                return Err(BotManagerError::Cancelled);
            }
            return Err(self
                .abort_start_for_consent(bot_id, handle, revision, err)
                .await);
        }

        let backend = match self.backend_for_config(config).await {
            Ok(b) => b,
            Err(err) => {
                if cancel.is_cancelled() {
                    return Err(BotManagerError::Cancelled);
                }
                let message = err.to_string();
                if let Some(crashed) = handle.abort_start(revision, Some(message.clone())).await? {
                    self.publish_state_change(&crashed, "start_failed");
                    self.event_bus
                        .publish(DomainEvent::bot_error(bot_id.clone(), message, None));
                }
                return Err(err);
            }
        };
        if cancel.is_cancelled() || handle.snapshot().revision != revision {
            return Err(BotManagerError::Cancelled);
        }
        let (result, acknowledge) = self.spawn_backend_start(
            Arc::clone(&backend),
            BotStartCtx {
                config: runtime_config,
                bot_config: Some(config.clone()),
            },
            handle.clone(),
            revision,
            Arc::clone(&operation),
            cancel.clone(),
        )?;
        *commit = Some(acknowledge);
        let result = result
            .await
            .map_err(|_| BotManagerError::TaskJoinFailed("启动 Bot 任务提前结束".into()))?;
        if cancel.is_cancelled() || handle.snapshot().revision != revision {
            return Err(BotManagerError::Cancelled);
        }
        match result {
            Ok(status) => {
                // 防快速退出竞态:backend.start Ok 只代表 spawn / compose up 成功,
                // 进程可能在 confirm_running 之前就崩了Starting 阶段的
                // BotProcessExited 被 handle_process_exited 有意忽略(无法区分本轮新
                // 进程退出与 restart fast-path 旧进程退出),所以这里启动后立即复查一次
                // backend.status:若已落到 Stopped / Crashed,直接按崩溃收口,避免 actor
                // 与 UI 停在假 Running复查本身报错时不阻断(查不到不代表没起来)
                // 远端 SL / 本机 SL:启动后立刻 status 复查易误判(远端 status 文件,本机 processes 写入竞态)
                if !Self::skip_post_start_status_recheck(config) {
                    let mut fail_detail = None;
                    for pass in 0..2u8 {
                        if pass == 1 {
                            tokio::time::sleep(Duration::from_millis(400)).await;
                        }
                        if let Ok(observed) = backend.status(bot_id.clone()).await {
                            if matches!(
                                observed.state,
                                BotActorState::Stopped | BotActorState::Crashed
                            ) {
                                fail_detail = Some(
                                    observed
                                        .extra
                                        .get("reason")
                                        .and_then(|v| v.as_str())
                                        .map(str::to_string)
                                        .unwrap_or_else(|| "进程启动后立即退出".to_string()),
                                );
                                break;
                            }
                        }
                    }
                    if let Some(detail) = fail_detail {
                        if let Some(crashed) =
                            handle.abort_start(revision, Some(detail.clone())).await?
                        {
                            self.publish_state_change(&crashed, "start_failed");
                            self.event_bus.publish(DomainEvent::bot_error(
                                bot_id.clone(),
                                detail.clone(),
                                Some(
                                    "Bot 启动后立即退出,请检查启动命令、运行时依赖与日志。"
                                        .to_string(),
                                ),
                            ));
                        }
                        return Err(BotManagerError::Render(detail));
                    }
                }
                let mut attached_after_runtime_start = false;
                if matches!(
                    &scenario,
                    RuntimeScenario::LocalDocker {
                        backend: BackendType::SnowLuma
                    } | RuntimeScenario::RemoteDocker {
                        backend: BackendType::SnowLuma,
                        ..
                    }
                ) {
                    self.remote_runtime_sessions()
                        .attach_after_runtime_start(bot_id, config)
                        .await;
                    attached_after_runtime_start = true;
                    if let Some(payload) =
                        self.snowluma_docker_agreements_after_attach(config).await?
                        && payload.consent_required
                    {
                        if cancel.is_cancelled() {
                            return Err(BotManagerError::Cancelled);
                        }
                        backend.stop(bot_id.clone(), StopMode::Force).await?;
                        if backend.status(bot_id.clone()).await?.state != BotActorState::Stopped {
                            return Err(BotManagerError::RemoteRead(
                                "协议检查未通过，且运行进程尚未确认停止，请刷新后重试".into(),
                            ));
                        }
                        if let Some(commit) = commit.take() {
                            let _ = commit.send(());
                        }
                        let err = Self::snowluma_consent_required_error(&payload.version);
                        return Err(self
                            .abort_start_for_consent(bot_id, handle, revision, err)
                            .await);
                    }
                }
                if cancel.is_cancelled() {
                    return Err(BotManagerError::Cancelled);
                }
                let running = handle
                    .confirm_running_revision(revision)
                    .await?
                    .ok_or(BotManagerError::Cancelled)?;
                if let Some(commit) = commit.take() {
                    let _ = commit.send(());
                }
                self.event_bus
                    .publish(DomainEvent::bot_status_changed(status, "runtime_start"));
                self.publish_state_change(&running, "start_completed");
                if !attached_after_runtime_start {
                    self.remote_runtime_sessions()
                        .attach_after_runtime_start(bot_id, config)
                        .await;
                }
                info!(
                    target: "ncd_runtime::bot_manager",
                    bot_id = %bot_id,
                    "Bot 已启动并就绪"
                );
                Ok(running)
            }
            Err(err) => {
                if cancel.is_cancelled() {
                    return Err(BotManagerError::Cancelled);
                }
                let message = match &err {
                    BotManagerError::Runtime(error) => error.to_string(),
                    error => error.to_string(),
                };
                error!(
                    target: "ncd_runtime::bot_manager",
                    bot_id = %bot_id,
                    err = %message,
                    "启动 Bot 失败（后端 start 报错）"
                );
                if let Some(crashed) = handle.abort_start(revision, Some(message.clone())).await? {
                    self.publish_state_change(&crashed, "start_failed");
                    self.event_bus.publish(DomainEvent::bot_error(
                        bot_id.clone(),
                        message,
                        Some("请在运行时设置中配置 NapCat/SnowLuma 启动命令后重试".to_string()),
                    ));
                }
                Err(err)
            }
        }
    }

    fn spawn_backend_start(
        &self,
        backend: Arc<dyn BotBackend>,
        context: BotStartCtx,
        actor: BotActorHandle,
        revision: u64,
        operation: Arc<tokio::sync::OwnedMutexGuard<()>>,
        cancel: tokio_util::sync::CancellationToken,
    ) -> Result<
        (
            tokio::sync::oneshot::Receiver<
                Result<ncd_traits::runtime_backend::BotStatus, BotManagerError>,
            >,
            tokio::sync::oneshot::Sender<()>,
        ),
        BotManagerError,
    > {
        use futures_util::FutureExt;

        let starting = actor.snapshot();
        if starting.state != BotActorState::Starting || starting.revision != revision {
            return Err(BotManagerError::Cancelled);
        }
        let token_generation = starting.token_generation;
        let mutation = self.guard_protocol_mutation(actor.bot_id())?;
        let (result_tx, result_rx) = tokio::sync::oneshot::channel();
        let (commit_tx, commit_rx) = tokio::sync::oneshot::channel();
        let event_bus = Arc::clone(&self.event_bus);
        tokio::spawn(async move {
            // 保留本代操作锁直到后端返回及提交确认；取消调用方不能遗弃已发起的启动。
            let _operation = operation;
            let _mutation = mutation;
            let bot_id = context.config.bot_id.clone();
            let result =
                match std::panic::AssertUnwindSafe(backend.start_cancellable(&context, cancel))
                    .catch_unwind()
                    .await
                {
                    Ok(result) => result.map_err(BotManagerError::Runtime),
                    Err(_) => Err(BotManagerError::TaskJoinFailed(
                        "启动 Bot 时后端异常退出".into(),
                    )),
                };
            let started = result.as_ref().ok().cloned();
            let delivered = result_tx.send(result).is_ok();
            let Some(started) = started else {
                return;
            };
            if delivered && commit_rx.await.is_ok() {
                return;
            }
            let cleanup = backend.cleanup_unacknowledged_start(&started);
            let confirmed = matches!(
                tokio::time::timeout(
                    Duration::from_secs(30),
                    std::panic::AssertUnwindSafe(cleanup).catch_unwind(),
                )
                .await,
                Ok(Ok(Ok(())))
            );
            if !confirmed {
                warn!(target: "ncd_runtime::bot_manager", bot_id = %bot_id,
                    "取消启动后的进程未确认停止，请刷新后重试");
            }
            if let Ok(Some(failed)) = actor
                .settle_unacknowledged_start(revision, token_generation, confirmed)
                .await
            {
                event_bus.publish(DomainEvent::bot_state_changed(failed, "start_cancelled"));
            }
        });
        Ok((result_rx, commit_tx))
    }
}
