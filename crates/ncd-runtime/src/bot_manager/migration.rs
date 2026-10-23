// 跨服务器迁移的协议端预检、停机核实与生命周期隔离。
use super::*;
use ncd_domain::{DeploymentType, SnowLumaStartMode};
use ncd_host::{Host, HostCommand, HostPath, Os};

tokio::task_local! {
    static MIGRATION_PROTOCOL_BOT: BotId;
}

pub(crate) async fn migration_protocol_scope<T>(
    bot_id: &BotId,
    future: impl std::future::Future<Output = T>,
) -> T {
    MIGRATION_PROTOCOL_BOT.scope(bot_id.clone(), future).await
}

#[derive(Default)]
pub(super) struct MigrationProtocolGate {
    reserved: HashSet<BotId>,
    mutations: HashMap<BotId, usize>,
    replacements: usize,
}

impl MigrationProtocolGate {
    fn reserve(&mut self, bot_id: &BotId) -> Result<(), String> {
        if self.reserved.contains(bot_id) {
            return Err("此 Bot 已被迁移任务锁定".into());
        }
        if self.replacements > 0 || self.mutations.contains_key(bot_id) {
            return Err("此 Bot 正在保存配置或启停，请稍后重试迁移".into());
        }
        self.reserved.insert(bot_id.clone());
        Ok(())
    }
}

pub(super) struct ProtocolMutationGuard {
    gate: Arc<std::sync::Mutex<MigrationProtocolGate>>,
    bot_id: Option<BotId>,
}

impl Drop for ProtocolMutationGuard {
    fn drop(&mut self) {
        if let Ok(mut gate) = self.gate.lock() {
            if let Some(bot_id) = &self.bot_id {
                if let Some(count) = gate.mutations.get_mut(bot_id) {
                    *count -= 1;
                    if *count == 0 {
                        gate.mutations.remove(bot_id);
                    }
                }
            } else {
                gate.replacements -= 1;
            }
        }
    }
}

fn migration_config_blockers(config: &BotConfig) -> Vec<String> {
    let mut blockers = Vec::new();
    if config.bot.runtime_target.server_id().is_none() {
        blockers.push("迁移暂只支持远程服务器上的协议端".into());
    }
    if config.bot.deployment_type != DeploymentType::Native {
        blockers.push("迁移暂只支持直接运行的协议端，Docker 数据卷尚不支持".into());
    }
    if config.bot.backend_type == BackendType::SnowLuma
        && config
            .bot
            .snowluma_start_mode
            .unwrap_or(SnowLumaStartMode::ColdStart)
            .is_hot()
    {
        blockers.push("SnowLuma 热启动的 QQ 由用户管理，请先改为冷启动后再迁移".into());
    }
    blockers
}

fn lock_error() -> BotManagerError {
    BotManagerError::RemoteRead("协议端迁移锁不可用".into())
}

fn acquire_protocol_mutation(
    shared: &Arc<std::sync::Mutex<MigrationProtocolGate>>,
    bot_id: &BotId,
) -> Result<Option<ProtocolMutationGuard>, BotManagerError> {
    if MIGRATION_PROTOCOL_BOT
        .try_with(|id| id == bot_id)
        .unwrap_or(false)
    {
        return Ok(None);
    }
    let mut gate = shared.lock().map_err(|_| lock_error())?;
    if gate.reserved.contains(bot_id) {
        return Err(BotManagerError::RemoteRead(
            "此 Bot 正在迁移，请在迁移完成或回滚后操作".into(),
        ));
    }
    *gate.mutations.entry(bot_id.clone()).or_default() += 1;
    Ok(Some(ProtocolMutationGuard {
        gate: Arc::clone(shared),
        bot_id: Some(bot_id.clone()),
    }))
}

fn acquire_all_protocol_mutation(
    shared: &Arc<std::sync::Mutex<MigrationProtocolGate>>,
) -> Result<ProtocolMutationGuard, BotManagerError> {
    let mut gate = shared.lock().map_err(|_| lock_error())?;
    if !gate.reserved.is_empty() {
        return Err(BotManagerError::RemoteRead(
            "存在迁移中的 Bot，请先完成迁移或回滚".into(),
        ));
    }
    gate.replacements += 1;
    Ok(ProtocolMutationGuard {
        gate: Arc::clone(shared),
        bot_id: None,
    })
}

async fn remote_home(host: &dyn Host) -> Result<String, String> {
    let output = host
        .run_to_string(
            HostCommand::new("printenv")
                .arg("HOME")
                .timeout(Duration::from_secs(10)),
        )
        .await
        .map_err(|e| format!("读取远端 HOME 失败：{e}"))?;
    let home = output.stdout.trim();
    if !output.success() || !home.starts_with('/') || home.contains(['\n', '\r']) {
        return Err("远端 HOME 无效，无法核实值守配置".into());
    }
    Ok(home.to_string())
}

async fn read_watch_config(
    host: &dyn Host,
    root: &HostPath,
) -> Result<Option<ncd_watch::config::NotifyConfig>, String> {
    let path = root.join("config").join("notify.json");
    if !host.exists(&path).await.map_err(|e| e.to_string())? {
        return Ok(None);
    }
    let bytes = host.read_file(&path).await.map_err(|e| e.to_string())?;
    if bytes.len() > 1024 * 1024 {
        return Err("远端值守配置过大，无法安全核实".into());
    }
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|_| "远端值守配置无效，无法安全核实".into())
}

async fn write_watch_run_intent(
    host: &dyn Host,
    root: &HostPath,
    bot_id: &BotId,
    running: bool,
) -> Result<(), String> {
    let Some(notify) = read_watch_config(host, root).await? else {
        return Ok(());
    };
    if ncd_watch::relogin_config::owns(&notify, bot_id.as_str()) {
        return Err("值守自动重登配置已变化，请先关闭后重试迁移".into());
    }
    let directory = root.join("state").join("intents");
    host.create_dir_all(&directory)
        .await
        .map_err(|e| e.to_string())?;
    let path = directory.join(format!("{bot_id}.json"));
    let temporary = directory.join(format!(".migration-{}.tmp", uuid::Uuid::new_v4()));
    let intent = ncd_watch::relogin_config::WatchRunIntent {
        running,
        reset_generation: uuid::Uuid::new_v4().to_string(),
    };
    let bytes = serde_json::to_vec(&intent).map_err(|e| e.to_string())?;
    host.write_file(&temporary, &bytes)
        .await
        .map_err(|e| e.to_string())?;
    if let Err(error) = host.rename(&temporary, &path).await {
        let _ = host.remove_file(&temporary).await;
        return Err(format!("写入值守启停记录失败：{error}"));
    }
    let bytes = host.read_file(&path).await.map_err(|e| e.to_string())?;
    validate_watch_intent(&bytes, running, &intent.reset_generation)
}

fn validate_watch_intent(
    bytes: &[u8],
    expected_running: bool,
    expected_generation: &str,
) -> Result<(), String> {
    let actual: ncd_watch::relogin_config::WatchRunIntent =
        serde_json::from_slice(bytes).map_err(|_| "值守启停记录无效")?;
    if actual.running != expected_running || actual.reset_generation != expected_generation {
        return Err("值守启停记录已变化，迁移已中止".into());
    }
    Ok(())
}

impl<R: BotConfigRepo + 'static, S: ConfigStore + 'static> BotManager<R, S> {
    pub(super) fn guard_protocol_mutation(
        &self,
        bot_id: &BotId,
    ) -> Result<Option<ProtocolMutationGuard>, BotManagerError> {
        acquire_protocol_mutation(&self.migration_reserved, bot_id)
    }

    pub(super) fn guard_all_protocol_mutation(
        &self,
    ) -> Result<ProtocolMutationGuard, BotManagerError> {
        acquire_all_protocol_mutation(&self.migration_reserved)
    }

    fn require_migration_reservation(&self, bot_id: &BotId) -> Result<(), String> {
        let gate = self
            .migration_reserved
            .lock()
            .map_err(|_| lock_error().to_string())?;
        if !gate.reserved.contains(bot_id) {
            return Err("协议端尚未锁定，不能执行迁移操作".into());
        }
        Ok(())
    }

    pub(super) fn is_protocol_reserved(&self, bot_id: &BotId) -> bool {
        self.migration_reserved
            .lock()
            .map(|gate| gate.reserved.contains(bot_id))
            .unwrap_or(true)
    }

    pub(super) fn has_protocol_reservations(&self) -> bool {
        self.migration_reserved
            .lock()
            .map(|gate| !gate.reserved.is_empty())
            .unwrap_or(true)
    }

    pub async fn migration_reserve_protocol(
        &self,
        bot_id: &BotId,
        reserve: bool,
    ) -> Result<(), String> {
        if !reserve {
            self.migration_reserved
                .lock()
                .map_err(|_| lock_error().to_string())?
                .reserved
                .remove(bot_id);
            return Ok(());
        }
        // 冷启动恢复先锁住配置，再由 bootstrap 创建 Actor 和只读对账。
        self.get_required_bot_config(bot_id)
            .await
            .map_err(|e| e.to_string())?;
        let handle = self.actors.read().await.get(bot_id).cloned();
        let mut gate = self
            .migration_reserved
            .lock()
            .map_err(|_| lock_error().to_string())?;
        if handle.is_some_and(|handle| {
            !matches!(
                handle.snapshot().state,
                BotActorState::Stopped | BotActorState::Crashed | BotActorState::Running
            )
        }) {
            return Err("此 Bot 正在启停或修复，请稍后重试迁移".into());
        }
        gate.reserve(bot_id)
    }

    async fn migration_host(&self, config: &BotConfig) -> Result<Arc<dyn Host>, String> {
        let resolver = self.host_resolver.as_ref().ok_or("远端主机解析器未就绪")?;
        let host = resolver
            .resolve(&config.bot.runtime_target)
            .await
            .map_err(|e| e.to_string())?;
        if host.os() != Os::Linux {
            return Err("迁移暂只支持 Linux 服务器上的协议端".into());
        }
        Ok(host)
    }

    async fn migration_watch_roots(
        &self,
        config: &BotConfig,
        host: &dyn Host,
    ) -> Result<Vec<HostPath>, String> {
        let home = remote_home(host).await?;
        let mut roots = vec![crate::ncd_watch_sync::remote_watch_root(&home)];
        if let Some(manager) = &self.server_manager {
            if let Some(profile) = manager
                .list_servers()
                .await
                .into_iter()
                .find(|p| Some(p.id.as_str()) == config.bot.runtime_target.server_id())
            {
                let override_root = profile
                    .path_overrides
                    .as_ref()
                    .and_then(|p| p.ncd_watch_root.as_deref());
                let selected_root = profile
                    .inventory
                    .as_ref()
                    .and_then(|i| i.selected.ncd_watch_root.as_deref());
                for root in override_root.into_iter().chain(selected_root) {
                    if !root.starts_with('/') || root.contains(['\n', '\r']) {
                        return Err("远端值守路径无效，无法安全核实".into());
                    }
                    let path = HostPath::from_posix(ncd_domain::normalize_posix(root));
                    if !roots.contains(&path) {
                        roots.push(path);
                    }
                }
            }
        }
        Ok(roots)
    }

    pub async fn migration_protocol_preflight(
        &self,
        config: &BotConfig,
    ) -> Result<Vec<String>, String> {
        let mut blockers = migration_config_blockers(config);
        if !blockers.is_empty() {
            return Ok(blockers);
        }
        let host = self.migration_host(config).await?;
        let output = host.run_to_string(
            HostCommand::new("sh").arg("-c")
                .arg("test -d /proc && test -r /proc/self/stat && command -v pgrep >/dev/null && command -v awk >/dev/null && command -v tr >/dev/null && command -v readlink >/dev/null")
                .timeout(Duration::from_secs(10)),
        ).await.map_err(|e| format!("核实协议端进程探测能力失败：{e}"))?;
        if !output.success() {
            blockers.push(
                "远端缺少可靠的 QQ 进程探测能力，请检查 /proc 和 pgrep、awk、tr、readlink".into(),
            );
        }
        let bot_id = BotId::new(config.bot.qq_id.to_string());
        for root in self.migration_watch_roots(config, host.as_ref()).await? {
            if read_watch_config(host.as_ref(), &root)
                .await?
                .as_ref()
                .is_some_and(|notify| ncd_watch::relogin_config::owns(notify, bot_id.as_str()))
            {
                blockers.push("此 Bot 的值守自动重登仍在启用，请先关闭后再迁移".into());
            }
        }
        if self
            .watch_relogin_owners
            .lock()
            .await
            .get(config.bot.runtime_target.server_id().unwrap_or_default())
            .is_some_and(|ids| ids.iter().any(|id| id == bot_id.as_str()))
        {
            blockers.push("值守仍持有此 Bot 的自动重登，请关闭并等待配置同步后再迁移".into());
        }
        Ok(blockers)
    }

    pub async fn migration_upsert_protocol(&self, config: BotConfig) -> Result<(), String> {
        let bot_id = BotId::new(config.bot.qq_id.to_string());
        self.require_migration_reservation(&bot_id)?;
        let blockers = migration_config_blockers(&config);
        if !blockers.is_empty() {
            return Err(blockers.join("；"));
        }
        migration_protocol_scope(&bot_id, self.upsert_bot_config(config))
            .await
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    pub async fn migration_stop_protocol(&self, bot_id: &BotId) -> Result<(), String> {
        self.require_migration_reservation(bot_id)?;
        let config = self
            .get_required_bot_config(bot_id)
            .await
            .map_err(|e| e.to_string())?;
        let blockers = self.migration_protocol_preflight(&config).await?;
        if !blockers.is_empty() {
            return Err(blockers.join("；"));
        }
        let host = self.migration_host(&config).await?;
        let roots = self.migration_watch_roots(&config, host.as_ref()).await?;
        let backend = self
            .backend_for_lifecycle(&config)
            .await
            .map_err(|e| e.to_string())?;
        let before = backend
            .status(bot_id.clone())
            .await
            .map_err(|e| e.to_string())?;
        if let Some(error) = before.transport_error {
            return Err(format!("源端状态无法核实：{error}"));
        }
        if !matches!(
            (before.state, before.pid),
            (BotActorState::Stopped, None) | (BotActorState::Running, Some(1..))
        ) {
            return Err("源端进程状态无法可靠核实，已阻止停机".into());
        }
        let actor = self.get_snapshot(bot_id).await.map_err(|e| e.to_string())?;
        if (before.state == BotActorState::Running) != (actor.state == BotActorState::Running) {
            return Err("源端进程与列表状态不一致，请先刷新 Bot 状态后重试".into());
        }
        for root in &roots {
            write_watch_run_intent(host.as_ref(), root, bot_id, false).await?;
        }
        migration_protocol_scope(bot_id, self.stop_bot(bot_id))
            .await
            .map_err(|e| e.to_string())?;
        for root in &roots {
            write_watch_run_intent(host.as_ref(), root, bot_id, false).await?;
        }
        let status = backend
            .status(bot_id.clone())
            .await
            .map_err(|e| e.to_string())?;
        if status.transport_error.is_some()
            || status.state != BotActorState::Stopped
            || status.pid.is_some()
        {
            return Err("旧服务器上的 QQ 进程尚未确认停止，已阻止切换".into());
        }
        if let Some(pid) = before.pid.filter(|pid| *pid > 0) {
            if host
                .exists(&HostPath::from_posix(format!("/proc/{pid}")))
                .await
                .map_err(|e| e.to_string())?
            {
                return Err("旧 QQ 进程仍然存在，已阻止切换".into());
            }
        }
        self.snowluma_ui.clear_bot(bot_id).await;
        Ok(())
    }

    pub async fn migration_start_protocol(&self, bot_id: &BotId) -> Result<(), String> {
        self.require_migration_reservation(bot_id)?;
        let config = self
            .get_required_bot_config(bot_id)
            .await
            .map_err(|e| e.to_string())?;
        let blockers = self.migration_protocol_preflight(&config).await?;
        if !blockers.is_empty() {
            return Err(blockers.join("；"));
        }
        let host = self.migration_host(&config).await?;
        let roots = self.migration_watch_roots(&config, host.as_ref()).await?;
        self.dispose_poller(bot_id).await;
        self.snowluma_ui.clear_bot(bot_id).await;
        migration_protocol_scope(bot_id, self.start_bot(bot_id))
            .await
            .map_err(|e| e.to_string())?;
        for root in &roots {
            write_watch_run_intent(host.as_ref(), root, bot_id, true).await?;
        }
        Ok(())
    }

    pub async fn migration_protocol_running(&self, bot_id: &BotId) -> Result<bool, String> {
        let config = self
            .get_required_bot_config(bot_id)
            .await
            .map_err(|e| e.to_string())?;
        let blockers = migration_config_blockers(&config);
        if !blockers.is_empty() {
            return Err(blockers.join("；"));
        }
        self.migration_host(&config).await?;
        let status = self
            .backend_for_lifecycle(&config)
            .await
            .map_err(|e| e.to_string())?
            .status(bot_id.clone())
            .await
            .map_err(|e| e.to_string())?;
        if let Some(error) = status.transport_error {
            return Err(format!("协议端进程状态无法核实：{error}"));
        }
        let actor = self.get_snapshot(bot_id).await.map_err(|e| e.to_string())?;
        if (status.state == BotActorState::Running) != (actor.state == BotActorState::Running) {
            return Err("协议端进程与列表状态不一致，请先刷新 Bot 状态后重试".into());
        }
        match status.state {
            BotActorState::Running if status.pid.is_some_and(|pid| pid > 0) => Ok(true),
            BotActorState::Stopped if status.pid.is_none() => Ok(false),
            _ => Err("协议端进程状态尚未稳定，请稍后重试".into()),
        }
    }

    pub async fn migration_protocol_online(&self, bot_id: &BotId) -> Result<bool, String> {
        if !self.migration_protocol_running(bot_id).await? {
            return Ok(false);
        }
        let config = self
            .get_required_bot_config(bot_id)
            .await
            .map_err(|e| e.to_string())?;
        match config.bot.backend_type {
            BackendType::NapCat => {
                Ok(self
                    .napcat_endpoints
                    .snapshot(bot_id)
                    .await
                    .is_some_and(|endpoint| {
                        endpoint.online == Some(true) && endpoint.auth_issue.is_none()
                    }))
            }
            BackendType::SnowLuma => {
                let snapshot = self.snowluma_ui.snapshot().await;
                Ok(snapshot.by_bot.get(bot_id).is_some_and(|bot| {
                    bot.uin.as_deref() == Some(bot_id.as_str())
                        && !bot.probe_unavailable
                        && bot.login_state == Some(ncd_domain::SnowLumaLoginState::LoggedIn)
                }))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ncd_traits::runtime_backend::{BotStatus, LogSnapshot, TailOpts};

    struct RecoveryRepo(Vec<BotConfig>);

    #[async_trait]
    impl BotConfigRepo for RecoveryRepo {
        async fn list(&self) -> Result<Vec<BotConfig>, BotConfigError> {
            Ok(self.0.clone())
        }

        async fn get(&self, qq_id: u64) -> Result<Option<BotConfig>, BotConfigError> {
            Ok(self
                .0
                .iter()
                .find(|config| config.bot.qq_id == qq_id)
                .cloned())
        }

        async fn upsert(&self, _config: BotConfig) -> Result<(), BotConfigError> {
            unreachable!("recovery must not rewrite protocol configuration")
        }

        async fn delete(&self, _qq_id: u64) -> Result<bool, BotConfigError> {
            unreachable!("recovery must not delete protocol configuration")
        }

        async fn count(&self) -> Result<usize, BotConfigError> {
            Ok(self.0.len())
        }
    }

    struct RecoveryBackend(BotId);

    #[async_trait]
    impl BotBackend for RecoveryBackend {
        fn id(&self) -> &BotId {
            &self.0
        }

        fn kind(&self) -> ncd_domain::BackendKind {
            ncd_domain::BackendKind::Local
        }

        fn flavor(&self) -> ncd_domain::BotFlavor {
            ncd_domain::BotFlavor::NapCat
        }

        async fn start(&self, _ctx: &BotStartCtx) -> Result<BotStatus, BotBackendError> {
            unreachable!("a reserved protocol must not auto-start during recovery")
        }

        async fn stop(&self, _bot_id: BotId, _mode: StopMode) -> Result<(), BotBackendError> {
            unreachable!("shutdown must preserve the reserved protocol")
        }

        async fn status(&self, bot_id: BotId) -> Result<BotStatus, BotBackendError> {
            Ok(BotStatus::stopped(bot_id))
        }

        async fn tail_log(
            &self,
            _bot_id: BotId,
            _opts: TailOpts,
        ) -> Result<LogSnapshot, BotBackendError> {
            Ok(LogSnapshot {
                lines: Vec::new(),
                total_lines: 0,
            })
        }
    }

    #[tokio::test]
    async fn migration_recovery_reserves_before_bootstrap_and_preserves_other_actors() {
        let root = tempfile::tempdir().unwrap();
        let mut migrating = ncd_test_support::BotConfigBuilder::new()
            .qq_id(10001)
            .runtime_target(RuntimeTarget::server("source"))
            .build();
        migrating.advanced.auto_start = true;
        let mut other = ncd_test_support::BotConfigBuilder::new()
            .qq_id(10002)
            .build();
        other.advanced.auto_start = false;
        let manager = BotManager::new(
            Arc::new(RecoveryRepo(vec![migrating, other])),
            Arc::new(crate::LocalConfigStore::new(root.path())),
            Arc::new(crate::DispatchRenderer::new(root.path(), root.path())),
            Arc::new(RecoveryBackend(BotId::new("recovery"))),
            Arc::new(crate::FileSystemRuntimeLaunchPlanner::new(root.path())),
            Arc::new(BroadcastEventBus::default()),
            Arc::new(crate::ReqwestNapCatWebUiClient::new().unwrap()),
            Arc::new(crate::NoopOfflineNotifier),
            Arc::new(RwLock::new(WebUiPollerSettings::default())),
            Arc::new(RwLock::new(DesktopNotifySettings::default())),
        );
        let reserved = BotId::new("10001");
        assert!(manager.get_snapshot(&reserved).await.is_err());
        manager
            .migration_reserve_protocol(&reserved, true)
            .await
            .unwrap();
        let boot = manager.bootstrap_with_auto_start(true).await.unwrap();
        assert!(boot.started.succeeded.is_empty());
        assert!(boot.started.failed.is_empty());
        assert_eq!(manager.list_snapshots().await.len(), 2);
        assert!(manager.start_bot(&reserved).await.is_err());
        assert!(manager.shutdown_all().await.failed.is_empty());
        let snapshots = manager.list_snapshots().await;
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].bot_id, reserved);
        assert!(manager.is_protocol_reserved(&reserved));
    }

    fn shared_gate() -> Arc<std::sync::Mutex<MigrationProtocolGate>> {
        Arc::new(std::sync::Mutex::new(MigrationProtocolGate::default()))
    }

    #[test]
    fn migration_reservation_waits_for_in_flight_mutation() {
        let gate = shared_gate();
        let bot_id = BotId::new("10001");
        let mutation = acquire_protocol_mutation(&gate, &bot_id).unwrap();
        assert!(gate.lock().unwrap().reserve(&bot_id).is_err());
        drop(mutation);
        assert!(gate.lock().unwrap().reserve(&bot_id).is_ok());
    }

    #[test]
    fn migration_reservation_blocks_bot_mutations_and_bulk_replacement() {
        let gate = shared_gate();
        let bot_id = BotId::new("10001");
        gate.lock().unwrap().reserve(&bot_id).unwrap();
        assert!(acquire_protocol_mutation(&gate, &bot_id).is_err());
        assert!(acquire_all_protocol_mutation(&gate).is_err());
        assert!(acquire_protocol_mutation(&gate, &BotId::new("10002")).is_ok());
    }

    #[test]
    fn migration_reservation_waits_for_bulk_replacement() {
        let gate = shared_gate();
        let bot_id = BotId::new("10001");
        let replacement = acquire_all_protocol_mutation(&gate).unwrap();
        assert!(gate.lock().unwrap().reserve(&bot_id).is_err());
        drop(replacement);
        assert!(gate.lock().unwrap().reserve(&bot_id).is_ok());
    }

    #[tokio::test]
    async fn migration_scope_is_limited_to_one_bot_and_one_task() {
        let gate = shared_gate();
        let first = BotId::new("10001");
        let second = BotId::new("10002");
        {
            let mut locked = gate.lock().unwrap();
            locked.reserve(&first).unwrap();
            locked.reserve(&second).unwrap();
        }
        migration_protocol_scope(&first, async {
            assert!(acquire_protocol_mutation(&gate, &first).unwrap().is_none());
            assert!(acquire_protocol_mutation(&gate, &second).is_err());
            let child_gate = Arc::clone(&gate);
            let child_id = first.clone();
            assert!(
                tokio::spawn(
                    async move { acquire_protocol_mutation(&child_gate, &child_id).is_err() }
                )
                .await
                .unwrap()
            );
        })
        .await;
        assert!(acquire_protocol_mutation(&gate, &first).is_err());
    }

    #[test]
    fn migration_preflight_rejects_hot_start_docker_and_local() {
        let mut config = ncd_test_support::BotConfigBuilder::new()
            .runtime_target(RuntimeTarget::server("source"))
            .backend_type(BackendType::SnowLuma)
            .build();
        assert!(migration_config_blockers(&config).is_empty());
        config.bot.snowluma_start_mode = Some(SnowLumaStartMode::HotStart);
        assert!(!migration_config_blockers(&config).is_empty());
        config.bot.snowluma_start_mode = Some(SnowLumaStartMode::ColdStart);
        config.bot.deployment_type = DeploymentType::Docker;
        assert!(!migration_config_blockers(&config).is_empty());
        config.bot.deployment_type = DeploymentType::Native;
        config.bot.runtime_target = RuntimeTarget::Local;
        assert!(!migration_config_blockers(&config).is_empty());
    }

    #[test]
    fn migration_watch_stop_requires_matching_generation_and_explicit_stop() {
        for bytes in [
            br#"{"running":true,"resetGeneration":"expected"}"#.as_slice(),
            br#"{"running":false,"resetGeneration":"changed"}"#.as_slice(),
            br#"{}"#.as_slice(),
            b"invalid".as_slice(),
        ] {
            assert!(validate_watch_intent(bytes, false, "expected").is_err());
        }
        assert!(
            validate_watch_intent(
                br#"{"running":false,"resetGeneration":"expected"}"#,
                false,
                "expected",
            )
            .is_ok()
        );
        assert!(
            validate_watch_intent(
                br#"{"running":true,"resetGeneration":"expected"}"#,
                true,
                "expected",
            )
            .is_ok()
        );
    }
}
