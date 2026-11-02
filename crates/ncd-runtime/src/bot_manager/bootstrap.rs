//! Actor 启动恢复、远端会话接管及断线清理。

use super::{BatchResult, BootstrapResult, BotManager, BotManagerError, MAX_BOTS};
use crate::bootstrap_reconcile::BootstrapReconciler;
use crate::bot_actor::BotActorHandle;
use ncd_domain::bot_config::{BackendType, BotConfig};
use ncd_domain::ids::BotId;
use ncd_domain::{DeploymentType, RuntimeScenario};
use ncd_traits::{BotConfigRepo, ConfigStore};
use std::collections::HashSet;
use std::sync::Arc;
use tracing::{info, warn};

impl<R: BotConfigRepo + 'static, S: ConfigStore + 'static> BotManager<R, S> {
    pub(super) fn bootstrap_reconciler(&self) -> BootstrapReconciler<R> {
        BootstrapReconciler::new(
            Arc::clone(&self.actors),
            self.host_resolver.clone(),
            Arc::clone(&self.event_bus),
            self.runtime_router(),
            self.remote_runtime_sessions(),
            Arc::clone(&self.reconcile_locks),
        )
    }

    /// 导入已有远端 Bot 后：若 QQ/容器仍在跑，把 Actor 接到运行态（不重新启动）。
    pub async fn reconcile_remote_runtime_for(
        &self,
        bot_ids: &[BotId],
    ) -> Result<Vec<BotId>, BotManagerError> {
        if bot_ids.is_empty() {
            return Ok(Vec::new());
        }
        let wanted: HashSet<_> = bot_ids.iter().cloned().collect();
        let configs = self.repo.list().await?;
        // 单主进程归因兜底按全量配置计数，而不是本次导入子集：
        // 只导 1 个但该主机已配多个 SL Bot 时不允许猜归因
        let snowluma_counts = crate::bootstrap_reconcile::count_native_snowluma_bots(&configs);
        let subset: Vec<BotConfig> = configs
            .into_iter()
            .filter(|c| wanted.contains(&BotId::new(c.bot.qq_id.to_string())))
            .collect();
        let done = self
            .bootstrap_reconciler()
            .reconcile_bootstrap_bots(&subset, &[], &snowluma_counts)
            .await;
        Ok(done.into_iter().collect())
    }

    /// 远端 SnowLuma WebUI 手动恢复，复用正在运行的 QQ/容器。
    pub async fn retry_snowluma_ui(&self, bot_id: &BotId) -> Result<(), BotManagerError> {
        let config = self.get_required_bot_config(bot_id).await?;
        if config.bot.backend_type != BackendType::SnowLuma
            || (config.bot.runtime_target.is_local()
                && config.bot.deployment_type == DeploymentType::Native)
        {
            return Err(BotManagerError::Render(
                "仅远端 SnowLuma 支持手动恢复 WebUI".to_string(),
            ));
        }

        let docker = config.bot.deployment_type == ncd_domain::DeploymentType::Docker;
        if !docker {
            self.prepare_snowluma_webui(bot_id).await?;
        }
        self.reconcile_remote_runtime_for(std::slice::from_ref(bot_id))
            .await?;
        // Reconcile may clear local handles when authentication or attachment fails.
        self.prepare_snowluma_webui(bot_id).await.map(|_| ())
    }

    /// 绑定在该主机上的所有远端 bot(Docker / Native)补跑一轮运行态恢复。
    pub async fn reconcile_remote_runtimes_for_server(
        &self,
        target_server_id: &str,
    ) -> Result<Vec<BotId>, BotManagerError> {
        let updates = self
            .server_manager
            .as_ref()
            .map(|servers| servers.connection_updates())
            .or_else(|| {
                self.host_resolver
                    .as_ref()
                    .and_then(|resolver| resolver.connection_updates())
            });
        let Some(mut updates) = updates else {
            return self.reconcile_server_inner(target_server_id).await;
        };
        let generation = updates
            .borrow()
            .get(target_server_id)
            .map(|state| state.generation);
        let superseded = async {
            while updates.changed().await.is_ok() {
                if updates
                    .borrow()
                    .get(target_server_id)
                    .map(|state| state.generation)
                    != generation
                {
                    break;
                }
            }
        };
        tokio::select! {
            result = self.reconcile_server_inner(target_server_id) => result,
            _ = superseded => Err(BotManagerError::Render("远端连接已换代，旧恢复已取消".into())),
        }
    }

    async fn reconcile_server_inner(
        &self,
        target_server_id: &str,
    ) -> Result<Vec<BotId>, BotManagerError> {
        let all = self.repo.list().await?;
        let snowluma_counts = crate::bootstrap_reconcile::count_native_snowluma_bots(&all);
        let subset: Vec<BotConfig> = all
            .into_iter()
            .filter(|c| match RuntimeScenario::from_config(c) {
                Ok(RuntimeScenario::RemoteNative { server_id, .. }) => {
                    server_id.as_str() == target_server_id
                }
                Ok(RuntimeScenario::RemoteDocker { server_id, .. }) => {
                    server_id.as_str() == target_server_id
                }
                _ => false,
            })
            .collect();
        if subset.is_empty() {
            return Ok(Vec::new());
        }
        let done = self
            .bootstrap_reconciler()
            .reconcile_bootstrap_bots(&subset, &[], &snowluma_counts)
            .await;
        Ok(done.into_iter().collect())
    }

    pub(super) async fn detach_remote_snowluma_sessions(&self, server_id: &str) {
        crate::launch::router::cancel_remote_snowluma_initialization(
            &self.remote_snowluma_init_locks,
            Some(server_id),
        )
        .await;
        self.windows_bots.detach_server(server_id).await;
        self.docker_sessions.detach_snowluma_server(server_id).await;
        let daemon = {
            let daemons = self.remote_snowluma_daemons.lock().await;
            daemons.get(server_id).cloned()
        };
        if let Some(daemon) = daemon {
            daemon.detach_local_sessions().await;
        }
        let backend = {
            let backends = self.remote_snowluma_backends.lock().await;
            backends.get(server_id).cloned()
        };
        if let Some(backend) = backend {
            backend.detach_login_pollers().await;
        }
        self.remote_snowluma_tunnels.drop_server(server_id).await;
        info!(
            target: "ncd_runtime::bot_manager",
            server_id,
            "主机连接丢失：已拆远端 SnowLuma 隧道并清 tunnel_eps"
        );
    }

    /// 启动时从持久化配置恢复所有 Bot Actor,并自动启动标记了 auto_start 的 Bot
    /// 返回 BootstrapResult,其中 skipped 包含超出 4 开上限而未注册的 Bot ID
    pub async fn bootstrap(&self) -> Result<BootstrapResult, BotManagerError> {
        self.bootstrap_with_auto_start(true).await
    }

    /// 与 [`Self::bootstrap`] 相同，但可关闭 auto_start（例如 Desktop 协议未同意时）。
    pub async fn bootstrap_with_auto_start(
        &self,
        enable_auto_start: bool,
    ) -> Result<BootstrapResult, BotManagerError> {
        self.pin_container_engines().await?;
        let configs = self.repo.list().await?;
        info!(
            target: "ncd_runtime::bot_manager",
            bot_configs = configs.len(),
            enable_auto_start,
            "启动恢复：正在加载 Bot 配置并注册 Actor"
        );

        let mut skipped: Vec<BotId> = Vec::new();

        // 恢复 Actor(不超过 MAX_BOTS),超出的记入 skipped
        {
            let mut actors = self.actors.write().await;
            for config in &configs {
                let bot_id = BotId::new(config.bot.qq_id.to_string());
                // 重复恢复时已有 Actor 不占新名额，也不能误列 skipped 跳过接管/自动启动。
                if actors.contains_key(&bot_id) {
                    continue;
                }
                if actors.len() >= MAX_BOTS {
                    skipped.push(bot_id);
                    continue;
                }
                actors.insert(bot_id.clone(), BotActorHandle::spawn(bot_id));
            }
        }

        // 远端 Docker:桌面退出后容器可能仍在跑,先 reconcile attach,避免 auto_start 误 remove
        let snowluma_counts = crate::bootstrap_reconcile::count_native_snowluma_bots(&configs);
        let reconciled = self
            .bootstrap_reconciler()
            .reconcile_bootstrap_bots(&configs, &skipped, &snowluma_counts)
            .await;

        // 自动启动(只针对已注册的 actor,skipped / 已 reconcile 的不会被启动)
        let auto_start_ids: Vec<BotId> = if enable_auto_start {
            configs
                .iter()
                .filter(|c| c.advanced.auto_start)
                .map(|c| BotId::new(c.bot.qq_id.to_string()))
                .filter(|id| !skipped.contains(id) && !reconciled.contains(id))
                .filter(|id| !self.is_protocol_reserved(id))
                .collect()
        } else {
            Vec::new()
        };

        let started = if auto_start_ids.is_empty() {
            BatchResult {
                succeeded: Vec::new(),
                failed: Vec::new(),
            }
        } else {
            self.batch_start(&auto_start_ids).await?
        };

        if !skipped.is_empty() {
            warn!(
                target: "ncd_runtime::bot_manager",
                skipped = skipped.len(),
                "部分 Bot 超过单机 4 开上限，未注册 Actor"
            );
        }
        info!(
            target: "ncd_runtime::bot_manager",
            auto_started_ok = started.succeeded.len(),
            auto_started_fail = started.failed.len(),
            skipped = skipped.len(),
            reconciled = reconciled.len(),
            "Bot 启动恢复完成"
        );

        Ok(BootstrapResult { started, skipped })
    }
}
