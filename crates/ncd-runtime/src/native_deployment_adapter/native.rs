//! 本机 NativeDeployment -> BotBackend 过渡壳

use std::sync::Arc;

use async_trait::async_trait;
use ncd_deploy::{Deployment, NativeDeployment};
use ncd_domain::bot_status::BotStatus;
use ncd_domain::ids::BotId;
use ncd_domain::kinds::{BackendKind, StopMode};
use ncd_domain::BotFlavor;
use ncd_host::Host;
use ncd_traits::runtime_backend::{
    BotBackend, BotBackendError, BotRuntimeConfig, BotStartCtx, LogSnapshot, TailOpts,
};

use super::config::bot_config_for_start;

/// 过渡壳:让 NativeDeployment 穿上 BotBackend trait 的外套
pub struct NativeDeploymentBackend {
    deployment: Arc<NativeDeployment>,
    host: Arc<dyn Host>,
    backend_id: BotId,
    flavor: BotFlavor,
}

impl NativeDeploymentBackend {
    pub fn new(
        deployment: Arc<NativeDeployment>,
        host: Arc<dyn Host>,
        backend_id: impl Into<BotId>,
        flavor: BotFlavor,
    ) -> Self {
        Self {
            deployment,
            host,
            backend_id: backend_id.into(),
            flavor,
        }
    }
}

#[async_trait]
impl BotBackend for NativeDeploymentBackend {
    fn id(&self) -> &BotId {
        &self.backend_id
    }

    fn kind(&self) -> BackendKind {
        BackendKind::Local
    }

    fn flavor(&self) -> BotFlavor {
        self.flavor
    }

    async fn start(&self, ctx: &BotStartCtx) -> Result<BotStatus, BotBackendError> {
        let bot_config = bot_config_for_start(ctx, self.flavor, false)?;

        let handle = self
            .deployment
            .launch(self.host.as_ref(), &bot_config)
            .await
            .map_err(|err| BotBackendError::Io(err.to_string()))?;

        match handle {
            ncd_deploy::DeploymentHandle::Native { pid, started_at } => Ok(BotStatus::running(
                ctx.config.bot_id.clone(),
                pid,
                started_at,
            )),
            _ => Err(BotBackendError::Io("unexpected handle variant".into())),
        }
    }

    async fn stop(&self, bot_id: BotId, mode: StopMode) -> Result<(), BotBackendError> {
        self.deployment
            .stop(self.host.as_ref(), &bot_id, mode)
            .await
            .map_err(|err| BotBackendError::Io(err.to_string()))
    }

    async fn status(&self, bot_id: BotId) -> Result<BotStatus, BotBackendError> {
        let state = self
            .deployment
            .observe(self.host.as_ref(), &bot_id)
            .await
            .map_err(|err| BotBackendError::Io(err.to_string()))?;
        match state {
            ncd_deploy::DeploymentState::Running => {
                // 从 deployment 拿不到精确 pid/started_at,返回 running 用 0 占位
                // BotManager 只看 state 字段做决策,pid 在 start 时已经拿到了
                Ok(BotStatus::running(bot_id, 0, 0))
            }
            _ => Ok(BotStatus::stopped(bot_id)),
        }
    }

    async fn read_config(&self, bot_id: BotId) -> Result<BotRuntimeConfig, BotBackendError> {
        // 过渡期不支持,BotManager 不从 backend 读 config(用 repo 读)
        Err(BotBackendError::ConfigNotFound(bot_id))
    }

    async fn write_config(
        &self,
        _bot_id: BotId,
        _cfg: &BotRuntimeConfig,
    ) -> Result<(), BotBackendError> {
        // 过渡期:config 落盘已由 BotManager 自己做,backend 不需要管
        Ok(())
    }

    async fn tail_log(
        &self,
        bot_id: BotId,
        opts: TailOpts,
    ) -> Result<LogSnapshot, BotBackendError> {
        let snap = self.deployment.tail_log(&bot_id, opts.lines).await;
        Ok(LogSnapshot {
            lines: snap.lines,
            total_lines: snap.total_lines,
        })
    }
}
