//! 先准备目标再停止源端，保留原目录和持久化交接记录。

use super::*;
use ncd_domain::bot_migration::{
    BotMigrationApp, BotMigrationJob, BotMigrationPhase, BotMigrationPlan, BotMigrationRequest,
};
use sha2::{Digest, Sha256};

mod journal;
mod transfer;
pub(super) use journal::MigrationState;
use journal::{AppMove, MigrationJournal};

tokio::task_local! { static MIGRATION_APP_CONTEXT: (); }

fn migration_fingerprint(
    config: &BotConfig,
    apps: &[AppInstance],
    target: &str,
) -> Result<String, String> {
    let bytes = serde_json::to_vec(&(config, apps, target)).map_err(|e| e.to_string())?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn without_auto_start(mut config: BotConfig) -> BotConfig {
    config.advanced.auto_start = false;
    config.bot.offline_auto_restart = false;
    config.bot.auto_restart_schedule.enable = false;
    config
}

impl AppManager {
    pub async fn start_bot_migration(
        self: &Arc<Self>,
        request: BotMigrationRequest,
        fingerprint: String,
    ) -> Result<BotMigrationJob, String> {
        let guard = Arc::clone(&self.migration.gate)
            .try_lock_owned()
            .map_err(|_| "已有迁移操作正在进行")?;
        for existing in self.migration_journals().await? {
            if existing.job.phase.is_working()
                || matches!(
                    existing.job.phase,
                    BotMigrationPhase::AwaitingLogin | BotMigrationPhase::RecoveryRequired
                )
                || (existing.job.phase == BotMigrationPhase::Failed
                    && existing.job.rollback_available)
            {
                return Err("请先完成或回退上一个迁移任务".into());
            }
        }
        let cache_lease = self
            .component_executor()
            .map_err(|e| e.to_string())?
            .download_cache()
            .lease()
            .await?;
        let plan = self.preview_bot_migration(request.clone()).await?;
        if !plan.blockers.is_empty() {
            return Err(plan.blockers.join("；"));
        }
        if plan.fingerprint != fingerprint {
            return Err("Bot 或应用配置已变化，请重新检查".into());
        }
        let (source, apps, _) = self.migration_inputs(&request).await?;
        let bot_id = BotId::new(&request.bot_id);
        let id = uuid::Uuid::new_v4().to_string();
        let mut movements = Vec::new();
        for app in apps {
            let target = self
                .migration_app_target(&app, &plan.target_host_id, &id)
                .await?;
            let adopt = self.migration_adopt_snapshot(&app.id).await?;
            movements.push(AppMove {
                source: app,
                target,
                adopt,
            });
        }
        let mut target = without_auto_start(source.clone());
        target.bot.runtime_target = RuntimeTarget::server(&request.target_server_id);
        self.bot_manager
            .migration_reserve_protocol(&bot_id, true)
            .await?;
        let source_was_running = match self.bot_manager.migration_protocol_running(&bot_id).await {
            Ok(running) => running,
            Err(error) => {
                self.bot_manager
                    .migration_reserve_protocol(&bot_id, false)
                    .await?;
                return Err(error);
            }
        };
        let job = BotMigrationJob {
            id: id.clone(),
            bot_id: request.bot_id.clone(),
            source_host_id: plan.source_host_id,
            target_host_id: plan.target_host_id,
            phase: BotMigrationPhase::Preparing,
            message: "准备目标服务器环境".into(),
            apps: plan.apps,
            error: None,
            rollback_available: false,
            target_started: false,
            updated_at_ms: now_ms(),
        };
        let journal = MigrationJournal {
            job: job.clone(),
            source,
            target,
            apps: movements,
            source_was_running,
            source_touched: false,
            config_switched: false,
        };
        if let Err(error) = self.migration_save(&journal).await {
            self.bot_manager
                .migration_reserve_protocol(&bot_id, false)
                .await?;
            return Err(error);
        }
        *self
            .migration
            .active
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some(id.clone());
        let manager = Arc::clone(self);
        tokio::spawn(async move {
            let _guard = guard;
            let _cache_lease = cache_lease;
            crate::bot_manager::migration_protocol_scope(&bot_id, async {
                MIGRATION_APP_CONTEXT
                    .scope((), manager.run_bot_migration(journal, fingerprint))
                    .await;
            })
            .await;
            *manager
                .migration
                .active
                .lock()
                .unwrap_or_else(|e| e.into_inner()) = None;
        });
        Ok(job)
    }

    async fn migration_prepare_component(
        &self,
        component_id: ComponentId,
        host_id: &str,
    ) -> Result<(), String> {
        let host = self
            .resolve_host(host_id)
            .await
            .map_err(|e| e.to_string())?;
        let components = self.component_executor().map_err(|e| e.to_string())?;
        let inputs = components.inputs_for(host_id, host.as_ref(), None).await;
        let task = components
            .submit(ComponentActionRequest {
                component_id,
                host_id: host_id.into(),
                kind: StepKind::EnsureInstalled,
                task_id: None,
                host: Arc::clone(&host),
                inputs,
            })
            .await?;
        loop {
            match components.deployment_tasks().status_of(&task).await {
                Some((ncd_domain::DeploymentTaskStatus::Success, _)) => return Ok(()),
                Some((status, error)) if status.is_terminal() => {
                    return Err(error.unwrap_or_else(|| "目标组件安装未完成".into()));
                }
                Some(_) => tokio::time::sleep(INSTALL_POLL_INTERVAL).await,
                None => return Err("目标组件安装任务已丢失".into()),
            }
        }
    }

    async fn run_bot_migration(&self, mut journal: MigrationJournal, fingerprint: String) {
        let result = self.migration_execute(&mut journal, &fingerprint).await;
        if let Err(error) = result {
            journal.job.error = Some(error);
            let phase = if journal.source_touched {
                BotMigrationPhase::RecoveryRequired
            } else {
                BotMigrationPhase::Failed
            };
            if let Err(error) = self
                .migration_phase(&mut journal, phase, "迁移已暂停，旧目录和任务记录均保留")
                .await
            {
                tracing::error!(%error, "persist migration failure");
            }
            if !journal.source_touched {
                if let Err(error) = self
                    .bot_manager
                    .migration_reserve_protocol(&BotId::new(&journal.job.bot_id), false)
                    .await
                {
                    tracing::error!(%error, "release migration reservation");
                }
            }
        }
    }

    async fn migration_execute(
        &self,
        journal: &mut MigrationJournal,
        fingerprint: &str,
    ) -> Result<(), String> {
        let bot_id = BotId::new(&journal.job.bot_id);
        let component = match journal.source.bot.backend_type {
            BackendType::NapCat => ComponentId::NapCat,
            BackendType::SnowLuma => ComponentId::SnowLuma,
        };
        self.migration_prepare_component(component, &journal.job.target_host_id)
            .await?;
        for app in &journal.apps {
            let dependency = if app.source.framework_id.as_str() == "nonebot2" {
                ComponentId::Uv
            } else {
                ComponentId::NodeJs
            };
            self.migration_prepare_component(dependency, &journal.job.target_host_id)
                .await?;
        }
        let target_host = self
            .resolve_host(&journal.job.target_host_id)
            .await
            .map_err(|e| e.to_string())?;
        if !self
            .component_executor()
            .map_err(|e| e.to_string())?
            .runtime_readiness(component, &journal.job.target_host_id, target_host.as_ref())
            .await?
            .ready()
        {
            return Err("目标协议端依赖尚未就绪，请在组件页处理后重试".into());
        }
        let config_guard = self.framework_config_gate.lock().await;
        let request = BotMigrationRequest {
            bot_id: journal.job.bot_id.clone(),
            target_server_id: server_id_of_host(&journal.job.target_host_id)
                .ok_or("目标服务器无效")?
                .into(),
        };
        let (current, apps, target) = self.migration_inputs(&request).await?;
        if migration_fingerprint(&current, &apps, &target)? != fingerprint {
            return Err("准备期间配置已变化，请重新检查后迁移".into());
        }
        for movement in &journal.apps {
            let host = self
                .resolve_host(&movement.source.host_id)
                .await
                .map_err(|e| e.to_string())?;
            let running = self
                .runtime
                .is_running_readonly(host.as_ref(), &movement.source)
                .await
                .map_err(|e| e.to_string())?;
            if running != (movement.source.state == AppInstanceState::Running) {
                return Err(format!(
                    "{} 的进程与列表状态不一致，请刷新应用端后重试",
                    movement.source.display_name
                ));
            }
        }
        journal.source_touched = true;
        journal.job.rollback_available = true;
        self.migration_phase(
            journal,
            BotMigrationPhase::Stopping,
            "停止源端并确认项目没有其他写入进程",
        )
        .await?;
        self.bot_manager
            .migration_upsert_protocol(without_auto_start(journal.source.clone()))
            .await?;
        for movement in &journal.apps {
            self.migration_stop_app(&movement.source).await?;
        }
        self.bot_manager.migration_stop_protocol(&bot_id).await?;
        self.migration_phase(
            journal,
            BotMigrationPhase::Copying,
            "生成一致快照、传输并逐文件校验",
        )
        .await?;
        for movement in &journal.apps {
            self.migration_copy_app(journal, movement).await?;
        }
        self.migration_phase(
            journal,
            BotMigrationPhase::Rebuilding,
            "按原项目锁文件重建目标依赖",
        )
        .await?;
        for movement in &journal.apps {
            self.migration_rebuild_app(&movement.target).await?;
        }
        // 再核实一次源协议端，配置换主机后 stop 会路由目标，不能留到换完再检查。
        self.bot_manager.migration_stop_protocol(&bot_id).await?;
        journal.config_switched = true;
        self.migration_phase(
            journal,
            BotMigrationPhase::Switching,
            "交接主机配置并重新建立应用端连接",
        )
        .await?;
        self.bot_manager
            .migration_upsert_protocol(journal.target.clone())
            .await?;
        for movement in &journal.apps {
            self.migration_save_adopt_target(&movement.target).await?;
            self.migration_set_app(movement.target.clone()).await?;
            self.migration_relink_app(&movement.target.id, &bot_id)
                .await?;
        }
        drop(config_guard);
        journal.job.target_started = journal.source_was_running
            || journal
                .apps
                .iter()
                .any(|m| m.source.state == AppInstanceState::Running);
        self.migration_phase(
            journal,
            BotMigrationPhase::Starting,
            "启动目标端，确认进程和 QQ 登录状态",
        )
        .await?;
        for movement in &journal.apps {
            if movement.source.state == AppInstanceState::Running {
                self.start_instance(&movement.target.id)
                    .await
                    .map_err(|e| e.to_string())?;
            }
        }
        if journal.source_was_running {
            self.bot_manager.migration_start_protocol(&bot_id).await?;
        }
        let awaiting_message = if journal.source_was_running {
            "已切到目标服务器，正在等待 QQ 在线；需要时请在 Bot 卡扫码登录"
        } else {
            "已切到目标服务器，正在核实目标状态；原先停止的协议端保持停止"
        };
        self.migration_phase(journal, BotMigrationPhase::AwaitingLogin, awaiting_message)
            .await?;
        Ok(())
    }

    pub(super) async fn ensure_framework_write_allowed(&self) -> Result<(), AppFrameworkError> {
        if MIGRATION_APP_CONTEXT.try_with(|_| ()).is_ok() {
            return Ok(());
        }
        for journal in self
            .migration_journals()
            .await
            .map_err(AppFrameworkError::Store)?
        {
            if journal.source_touched
                && (journal.job.phase.is_working()
                    || matches!(
                        journal.job.phase,
                        BotMigrationPhase::AwaitingLogin | BotMigrationPhase::RecoveryRequired
                    ))
            {
                return Err(AppFrameworkError::Validation(
                    "整套 Bot 迁移尚未完成，请先完成迁移或回退".into(),
                ));
            }
        }
        Ok(())
    }

    pub async fn recover_bot_migrations(&self) -> Result<(), String> {
        let journals = match self.migration_journals().await {
            Ok(journals) => journals,
            Err(error) => {
                for config in self.bot_manager.list_bot_configs_for_link().await? {
                    self.bot_manager
                        .migration_reserve_protocol(&BotId::new(config.bot.qq_id.to_string()), true)
                        .await?;
                }
                return Err(error);
            }
        };
        for mut journal in journals {
            if journal.job.phase.is_working()
                || matches!(
                    journal.job.phase,
                    BotMigrationPhase::AwaitingLogin | BotMigrationPhase::RecoveryRequired
                )
            {
                let bot_id = BotId::new(&journal.job.bot_id);
                self.bot_manager
                    .migration_reserve_protocol(&bot_id, true)
                    .await?;
                // 崩溃点可能在写停用意图之前，恢复时先抑制应用自启。
                if journal.source_touched {
                    for movement in &journal.apps {
                        if let Some(current) = self.store.get(&movement.source.id).await {
                            self.store
                                .update(&current.id, |i| i.auto_start = false)
                                .await
                                .map_err(|e| e.to_string())?;
                        }
                    }
                }
                if journal.job.phase.is_working() {
                    journal.job.rollback_available = journal.source_touched;
                    let phase = if journal.source_touched {
                        BotMigrationPhase::RecoveryRequired
                    } else {
                        BotMigrationPhase::Failed
                    };
                    if !journal.source_touched {
                        self.bot_manager
                            .migration_reserve_protocol(&bot_id, false)
                            .await?;
                    }
                    self.migration_phase(
                        &mut journal,
                        phase,
                        "上次迁移被中断，目录保留，请检查记录后回退",
                    )
                    .await?;
                }
            }
        }
        Ok(())
    }

    pub async fn get_bot_migration(
        &self,
        bot_id: String,
    ) -> Result<Option<BotMigrationJob>, String> {
        let Some(mut journal) = self
            .migration_journals()
            .await?
            .into_iter()
            .filter(|j| j.job.bot_id == bot_id)
            .max_by_key(|j| j.job.updated_at_ms)
        else {
            return Ok(None);
        };
        let Ok(_guard) = self.migration.gate.try_lock() else {
            return Ok(Some(journal.job));
        };
        // 轮询可能在 worker 落盘前读到旧阶段，拿到操作锁后必须重读。
        journal = self.migration_load(&journal.job.id).await?;
        let active = self
            .migration
            .active
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_deref()
            == Some(&journal.job.id);
        if journal.job.phase.is_working() && !active {
            journal.job.rollback_available = journal.source_touched;
            let phase = if journal.source_touched {
                BotMigrationPhase::RecoveryRequired
            } else {
                BotMigrationPhase::Failed
            };
            self.migration_phase(&mut journal, phase, "迁移被中断，已保留旧目录和交接记录")
                .await?;
        }
        if journal.job.phase == BotMigrationPhase::AwaitingLogin && !active {
            let _cache_lease = self
                .component_executor()
                .map_err(|e| e.to_string())?
                .download_cache()
                .lease()
                .await?;
            let id = BotId::new(&bot_id);
            crate::bot_manager::migration_protocol_scope(&id, async {
                MIGRATION_APP_CONTEXT
                    .scope((), self.migration_finish_if_ready(&mut journal))
                    .await
            })
            .await?;
        }
        Ok(Some(journal.job))
    }

    pub async fn run_bot_migration_monitor(self: Arc<Self>) {
        let mut previous_error = None;
        loop {
            tokio::time::sleep(Duration::from_secs(3)).await;
            let result = async {
                for journal in self.migration_journals().await? {
                    if journal.job.phase == BotMigrationPhase::AwaitingLogin {
                        self.get_bot_migration(journal.job.bot_id).await?;
                    }
                }
                Ok::<(), String>(())
            }
            .await;
            match result {
                Ok(()) => previous_error = None,
                Err(error) => {
                    if previous_error.as_ref() != Some(&error) {
                        tracing::warn!(%error, "migration completion check pending");
                        previous_error = Some(error);
                    }
                }
            }
        }
    }

    async fn migration_finish_if_ready(
        &self,
        journal: &mut MigrationJournal,
    ) -> Result<(), String> {
        if now_ms().saturating_sub(journal.job.updated_at_ms) < 10_000 {
            return Ok(());
        }
        let id = BotId::new(&journal.job.bot_id);
        if journal.source_was_running && !self.bot_manager.migration_protocol_running(&id).await? {
            journal.job.error = Some("目标协议端进程已退出".into());
            return self
                .migration_phase(
                    journal,
                    BotMigrationPhase::RecoveryRequired,
                    "目标协议端启动后退出，请查看日志或回退",
                )
                .await;
        }
        let mut apps_ready = true;
        for movement in &journal.apps {
            if movement.source.state != AppInstanceState::Running {
                continue;
            }
            let current = self
                .get_instance(&movement.source.id)
                .await
                .map_err(|e| e.to_string())?;
            let host = self
                .resolve_host(&current.host_id)
                .await
                .map_err(|e| e.to_string())?;
            if !self
                .runtime
                .is_running_readonly(host.as_ref(), &current)
                .await
                .map_err(|e| e.to_string())?
            {
                journal.job.error = Some(format!("{} 的目标进程已退出", current.display_name));
                return self
                    .migration_phase(
                        journal,
                        BotMigrationPhase::RecoveryRequired,
                        "目标应用启动后退出，请查看日志或回退",
                    )
                    .await;
            }
            let port = current.port.to_string();
            let output = host.run_to_string(HostCommand::new("python3").arg("-c")
                .arg("import socket,sys; s=socket.socket(); s.settimeout(3); sys.exit(s.connect_ex(('127.0.0.1',int(sys.argv[1]))))")
                .arg(port).timeout(Duration::from_secs(5))).await.map_err(|e| e.to_string())?;
            if !output.success() {
                apps_ready = false;
            }
        }
        // 等待 QQ 登录或某个监听口时，也继续核实其他目标进程，避免退出被掩盖。
        if !apps_ready
            || (journal.source_was_running
                && !self.bot_manager.migration_protocol_online(&id).await?)
        {
            return Ok(());
        }
        let mut config = self
            .bot_manager
            .bot_config(&id)
            .await?
            .ok_or("目标 Bot 配置不存在")?;
        config.advanced.auto_start = journal.source.advanced.auto_start;
        config.bot.offline_auto_restart = journal.source.bot.offline_auto_restart;
        config.bot.auto_restart_schedule = journal.source.bot.auto_restart_schedule.clone();
        self.bot_manager
            .migration_upsert_protocol(config.clone())
            .await?;
        journal.target = config;
        for movement in &journal.apps {
            let mut current = self
                .get_instance(&movement.source.id)
                .await
                .map_err(|e| e.to_string())?;
            current.auto_start = movement.source.auto_start;
            self.migration_set_app(current).await?;
        }
        self.migration_phase(
            journal,
            BotMigrationPhase::Completed,
            "已迁移到目标服务器，旧目录保留",
        )
        .await?;
        self.bot_manager
            .migration_reserve_protocol(&id, false)
            .await
    }

    pub async fn rollback_bot_migration(
        self: &Arc<Self>,
        job_id: String,
        accept_data_loss: bool,
    ) -> Result<BotMigrationJob, String> {
        let guard = Arc::clone(&self.migration.gate)
            .try_lock_owned()
            .map_err(|_| "迁移任务仍在运行")?;
        let mut journal = self.migration_load(&job_id).await?;
        if !journal.job.rollback_available || journal.job.phase == BotMigrationPhase::RolledBack {
            return Err("此任务没有待回退的交接".into());
        }
        if journal.job.target_started && !accept_data_loss {
            return Err("目标端可能已有新数据，回退前请确认不会自动将新数据回拷旧机".into());
        }
        if self.migration_journals().await?.iter().any(|j| {
            j.job.bot_id == journal.job.bot_id && j.job.updated_at_ms > journal.job.updated_at_ms
        }) {
            return Err("此 Bot 已有更新的迁移记录，不能回退旧任务".into());
        }
        let cache_lease = self
            .component_executor()
            .map_err(|e| e.to_string())?
            .download_cache()
            .lease()
            .await?;
        let bot_id = BotId::new(&journal.job.bot_id);
        // 已完成的任务也必须重新取得协议端租约。
        if journal.job.phase == BotMigrationPhase::Completed {
            self.bot_manager
                .migration_reserve_protocol(&bot_id, true)
                .await?;
        }
        self.migration_phase(
            &mut journal,
            BotMigrationPhase::RollingBack,
            "停止目标端并恢复源端配置",
        )
        .await?;
        let job = journal.job.clone();
        *self
            .migration
            .active
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some(job_id);
        let manager = Arc::clone(self);
        tokio::spawn(async move {
            let _guard = guard;
            let _cache_lease = cache_lease;
            let result = crate::bot_manager::migration_protocol_scope(&bot_id, async {
                MIGRATION_APP_CONTEXT
                    .scope((), manager.migration_rollback(&mut journal))
                    .await
            })
            .await;
            if let Err(error) = result {
                journal.job.error = Some(error);
                if let Err(error) = manager
                    .migration_phase(
                        &mut journal,
                        BotMigrationPhase::RecoveryRequired,
                        "回退未完成，请保留两端目录并检查错误",
                    )
                    .await
                {
                    tracing::error!(%error, "persist migration rollback failure");
                }
            }
            *manager
                .migration
                .active
                .lock()
                .unwrap_or_else(|e| e.into_inner()) = None;
        });
        Ok(job)
    }

    async fn migration_rollback(&self, journal: &mut MigrationJournal) -> Result<(), String> {
        let bot_id = BotId::new(&journal.job.bot_id);
        let config_guard = self.framework_config_gate.lock().await;
        let current = self
            .bot_manager
            .bot_config(&bot_id)
            .await?
            .ok_or("Bot 配置不存在")?;
        if current.bot.runtime_target != journal.source.bot.runtime_target
            && current.bot.runtime_target != journal.target.bot.runtime_target
        {
            return Err("Bot 已被移动到其他服务器，不能套用旧迁移记录".into());
        }
        // 即使上次在 start 的 await 中断，也先严格验证当前协议进程退出。
        self.bot_manager.migration_stop_protocol(&bot_id).await?;
        for movement in &journal.apps {
            let current = self
                .get_instance(&movement.source.id)
                .await
                .map_err(|e| e.to_string())?;
            if (current.host_id != movement.source.host_id
                || current.install_dir != movement.source.install_dir)
                && (current.host_id != movement.target.host_id
                    || current.install_dir != movement.target.install_dir)
            {
                return Err("应用端位置已被修改，不能套用旧记录".into());
            }
            self.migration_stop_app(&current).await?;
        }
        self.bot_manager
            .migration_upsert_protocol(without_auto_start(journal.source.clone()))
            .await?;
        for movement in &journal.apps {
            self.migration_restore_adopt(&movement.source, movement.adopt.as_ref())
                .await?;
            let mut original = movement.source.clone();
            original.state = AppInstanceState::Stopped;
            original.auto_start = false;
            self.migration_set_app(original).await?;
        }
        drop(config_guard);
        for movement in &journal.apps {
            if movement.source.state == AppInstanceState::Running {
                self.start_instance(&movement.source.id)
                    .await
                    .map_err(|e| e.to_string())?;
            }
            self.store
                .update(&movement.source.id, |i| {
                    i.auto_start = movement.source.auto_start
                })
                .await
                .map_err(|e| e.to_string())?;
        }
        if journal.source_was_running {
            self.bot_manager.migration_start_protocol(&bot_id).await?;
        }
        self.bot_manager
            .migration_upsert_protocol(journal.source.clone())
            .await?;
        journal.job.rollback_available = false;
        journal.job.error = None;
        self.migration_phase(
            journal,
            BotMigrationPhase::RolledBack,
            "已恢复源端，目标目录和快照保留",
        )
        .await?;
        self.bot_manager
            .migration_reserve_protocol(&bot_id, false)
            .await
    }
    async fn migration_inputs(
        &self,
        request: &BotMigrationRequest,
    ) -> Result<(BotConfig, Vec<AppInstance>, String), String> {
        let config = self
            .bot_manager
            .bot_config(&BotId::new(&request.bot_id))
            .await?
            .ok_or("Bot 配置不存在")?;
        let mut apps: Vec<_> = self
            .store
            .list()
            .await
            .into_iter()
            .filter(|i| {
                i.link
                    .as_ref()
                    .is_some_and(|link| link.bot_id.as_str() == request.bot_id)
            })
            .collect();
        apps.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));
        if request.target_server_id.is_empty()
            || request.target_server_id.contains(['/', '\\', ':'])
        {
            return Err("请选择已登记的目标服务器".into());
        }
        let target = format!("remote:{}", request.target_server_id);
        Ok((config, apps, target))
    }

    pub async fn preview_bot_migration(
        &self,
        request: BotMigrationRequest,
    ) -> Result<BotMigrationPlan, String> {
        let (config, apps, target_host_id) = self.migration_inputs(&request).await?;
        let source_host_id = host_id_of_runtime_target(&config.bot.runtime_target);
        let mut blockers = self
            .bot_manager
            .migration_protocol_preflight(&config)
            .await?;
        if source_host_id == target_host_id {
            blockers.push("源服务器和目标服务器相同".into());
        }
        let target = self
            .resolve_host(&target_host_id)
            .await
            .map_err(|e| e.to_string())?;
        if target.os() != Os::Linux || target.locality() != Locality::Remote {
            blockers.push("整套迁移目前支持 Linux 原生部署的服务器之间迁移".into());
        }
        if !target.command_exists("python3").await {
            blockers.push("目标服务器需要 Python 3 进行快照校验".into());
        }
        let components = self.component_executor().map_err(|e| e.to_string())?;
        let inventory = components
            .inventory()
            .ensure(&request.target_server_id, target.as_ref(), true)
            .await?;
        if inventory
            .bots
            .iter()
            .any(|b| b.qq_id.to_string() == request.bot_id)
        {
            blockers
                .push("目标服务器已有此 QQ 的协议数据，请先处理已有实例，迁移不会覆盖它".into());
        }
        if source_host_id != LOCAL_HOST_ID {
            let source = self
                .resolve_host(&source_host_id)
                .await
                .map_err(|e| e.to_string())?;
            if !source.command_exists("python3").await {
                blockers.push("源服务器需要 Python 3 进行快照检查".into());
            }
            let machine = HostCommand::new("cat")
                .arg("/proc/sys/kernel/random/boot_id")
                .timeout(Duration::from_secs(10));
            let source_machine = source
                .run_to_string(machine.clone())
                .await
                .map_err(|e| e.to_string())?;
            let target_machine = target
                .run_to_string(machine)
                .await
                .map_err(|e| e.to_string())?;
            if !source_machine.success()
                || !target_machine.success()
                || source_machine.stdout.trim().is_empty()
            {
                blockers.push("无法核实源和目标是否为独立服务器".into());
            } else if source_machine.stdout.trim() == target_machine.stdout.trim() {
                blockers.push("两个主机档案连接到同一台服务器".into());
            }
        }
        let mut total_bytes = 0u64;
        for app in &apps {
            if app.host_id != source_host_id || app.placement != AppPlacement::RemoteNative {
                blockers.push(format!(
                    "{} 不在源协议端服务器，暂不能整套迁移",
                    app.display_name
                ));
                continue;
            }
            if app.state == AppInstanceState::Installing || self.install_watched(&app.id) {
                blockers.push(format!("{} 正在安装，请等待任务结束", app.display_name));
                continue;
            }
            match self
                .migration_app_target(app, &target_host_id, "preview")
                .await
            {
                Ok(destination) => {
                    if let Err(error) = self.ensure_port_free(&destination, app.port).await {
                        blockers.push(format!("{}：{error}", app.display_name));
                    }
                    let source = self
                        .resolve_host(&app.host_id)
                        .await
                        .map_err(|e| e.to_string())?;
                    match transfer::run(source.as_ref(), "inspect", &app.install_dir, &[]).await {
                        Ok(report) => {
                            total_bytes = total_bytes.saturating_add(report.bytes);
                            if let Err(error) = transfer::run(
                                target.as_ref(),
                                "target",
                                &destination.install_dir,
                                &[&total_bytes.to_string()],
                            )
                            .await
                            {
                                blockers.push(format!("{}：{error}", app.display_name));
                            }
                        }
                        Err(error) => blockers.push(format!("{}：{error}", app.display_name)),
                    }
                }
                Err(error) => blockers.push(format!("{}：{error}", app.display_name)),
            }
        }
        Ok(BotMigrationPlan {
            bot_id: request.bot_id,
            source_host_id,
            target_host_id: target_host_id.clone(),
            apps: apps
                .iter()
                .map(|a| BotMigrationApp {
                    instance_id: a.id.to_string(),
                    display_name: a.display_name.clone(),
                    framework_id: a.framework_id.to_string(),
                    source_dir: a.install_dir.clone(),
                })
                .collect(),
            blockers,
            notes: vec![
                format!("项目数据约 {:.1} MiB", total_bytes as f64 / 1048576.0),
                "仅迁移 NCD 已关联的项目；项目外数据库、Redis 和额外系统服务需先处理".into(),
            ],
            fingerprint: migration_fingerprint(&config, &apps, &target_host_id)?,
        })
    }
}

#[cfg(test)]
mod tests;
