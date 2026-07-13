//! 远端 SSH NativeDeployment -> BotBackend

use std::sync::Arc;

use async_trait::async_trait;
use ncd_backend_napcat::remote_native_launch::{
    RemoteNapcatLayout, napcat_remote_log_path, probe_remote_napcat_layout,
    remote_napcat_running_pid, stop_remote_napcat_on_host,
};
use ncd_deploy::{Deployment, NativeDeployment};
use ncd_domain::bot_status::BotStatus;
use ncd_domain::ids::BotId;
use ncd_domain::kinds::{BackendKind, StopMode};
use ncd_domain::{BotFlavor, RuntimeTarget};
use ncd_host::{Host, HostPath};
use ncd_traits::runtime_backend::{
    BotBackend, BotBackendError, BotRuntimeConfig, BotStartCtx, LogSnapshot, TailOpts,
};

use super::config::{bot_config_for_start, status_for_deployment_state};
use super::log_helpers::remote_tail_log_raw_lines;

/// 远端「直接运行」:每 Bot 绑定一台 Host + 独立 NativeDeployment(translator 写远端路径)
///
/// 不再常驻固定 host 引用,而是持有 resolver + target;在 start/status/stop/tail_log
/// 边界按需取当前应活 host,传输层断连时 refresh 后重试一次。
pub struct RemoteNativeDeploymentBackend {
    deployment: Arc<NativeDeployment>,
    /// 用于在操作边界按需获取/刷新远端 hostServerManager 通过 TauriHostResolver 注入
    resolver: Arc<dyn crate::HostResolver>,
    /// 目标运行宿主(应为 RuntimeTarget::Server(...))
    target: RuntimeTarget,
    backend_id: BotId,
    flavor: BotFlavor,
}

impl RemoteNativeDeploymentBackend {
    pub fn new(
        deployment: Arc<NativeDeployment>,
        resolver: Arc<dyn crate::HostResolver>,
        target: RuntimeTarget,
        backend_id: impl Into<BotId>,
        flavor: BotFlavor,
    ) -> Self {
        Self {
            deployment,
            resolver,
            target,
            backend_id: backend_id.into(),
            flavor,
        }
    }

    /// 便捷方法:通过 resolver 取得当前应活的 host(不触发自愈刷新)
    async fn current_host(&self) -> Result<Arc<dyn Host>, BotBackendError> {
        self.resolver
            .resolve(&self.target)
            .await
            .map_err(|e| BotBackendError::RemoteHostTransport(e.to_string()))
    }

    /// 通过 resolver 取得一个“新鲜”host(会触发底层刷新/重连)
    async fn refreshed_host(&self) -> Result<Arc<dyn Host>, BotBackendError> {
        self.resolver
            .refresh(&self.target)
            .await
            .map_err(|e| BotBackendError::RemoteHostTransport(e.to_string()))
    }

    /// 在操作边界使用:先拿 host 执行 op;失败则 refresh 后再试一次。
    async fn with_host_refresh<F, Fut, T, E>(&self, op: F) -> Result<T, BotBackendError>
    where
        F: FnOnce(Arc<dyn Host>) -> Fut + Clone + Send,
        Fut: std::future::Future<Output = Result<T, E>> + Send,
        E: std::fmt::Debug + Send + 'static,
    {
        let host = self.current_host().await?;
        match op.clone()(host).await {
            Ok(v) => Ok(v),
            Err(_e) => {
                let host2 = self.refreshed_host().await?;
                op(host2)
                    .await
                    .map_err(|e2| BotBackendError::RemoteHostTransport(format!("{:?}", e2)))
            }
        }
    }

    async fn napcat_install_base(&self) -> Result<HostPath, BotBackendError> {
        let host = self.current_host().await?;
        let (home, layout) = probe_remote_napcat_layout(host.as_ref())
            .await
            .map_err(BotBackendError::Io)?;
        match layout {
            RemoteNapcatLayout::System => Ok(HostPath::from_posix("/")),
            RemoteNapcatLayout::Rootless => Ok(HostPath::from_posix(format!("{home}/Napcat"))),
        }
    }
}

#[async_trait]
impl BotBackend for RemoteNativeDeploymentBackend {
    fn id(&self) -> &BotId {
        &self.backend_id
    }

    fn kind(&self) -> BackendKind {
        BackendKind::RemoteSsh
    }

    fn flavor(&self) -> BotFlavor {
        self.flavor
    }

    async fn start(&self, ctx: &BotStartCtx) -> Result<BotStatus, BotBackendError> {
        let bot_config = bot_config_for_start(ctx, self.flavor, true)?;
        // 远端 nohup 进程不在本机 processes 表里;若上次 stop 不彻底,先按 qq 清掉再起
        if self.flavor == BotFlavor::NapCat {
            let qq_id = bot_config.bot.qq_id;
            self.with_host_refresh(move |h| async move {
                if remote_napcat_running_pid(h.as_ref(), qq_id)
                    .await?
                    .is_some()
                {
                    stop_remote_napcat_on_host(h.as_ref(), qq_id).await?;
                }
                Ok::<(), BotBackendError>(())
            })
            .await?;
        }
        let handle = self
            .with_host_refresh(
                |h| async move { self.deployment.launch(h.as_ref(), &bot_config).await },
            )
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

    async fn stop(&self, bot_id: BotId, _mode: StopMode) -> Result<(), BotBackendError> {
        if self.flavor == BotFlavor::NapCat {
            let qq_id: u64 = bot_id
                .as_str()
                .parse()
                .map_err(|_| BotBackendError::InvalidConfig(format!("invalid bot id: {bot_id}")))?;
            let host = self.current_host().await?;
            stop_remote_napcat_on_host(host.as_ref(), qq_id).await?;
        }
        self.with_host_refresh(|h| {
            let bid = bot_id.clone();
            let m = _mode;
            async move { self.deployment.stop(h.as_ref(), &bid, m).await }
        })
        .await
        .map_err(|err| BotBackendError::Io(err.to_string()))
    }

    async fn status(&self, bot_id: BotId) -> Result<BotStatus, BotBackendError> {
        if self.flavor == BotFlavor::NapCat {
            let qq_id: u64 = bot_id
                .as_str()
                .parse()
                .map_err(|_| BotBackendError::InvalidConfig(format!("invalid bot id: {bot_id}")))?;
            let host = self.current_host().await?;
            if let Some(pid) = remote_napcat_running_pid(host.as_ref(), qq_id).await? {
                return Ok(BotStatus::running(bot_id, pid, 0));
            }
            return Ok(BotStatus::stopped(bot_id));
        }
        let state = self
            .with_host_refresh(|h| {
                let bid = bot_id.clone();
                async move { self.deployment.observe(h.as_ref(), &bid).await }
            })
            .await
            .map_err(|err| BotBackendError::Io(err.to_string()))?;
        Ok(status_for_deployment_state(bot_id, state))
    }

    async fn read_config(&self, bot_id: BotId) -> Result<BotRuntimeConfig, BotBackendError> {
        Err(BotBackendError::ConfigNotFound(bot_id))
    }

    async fn write_config(
        &self,
        _bot_id: BotId,
        _cfg: &BotRuntimeConfig,
    ) -> Result<(), BotBackendError> {
        Ok(())
    }

    async fn tail_log(
        &self,
        bot_id: BotId,
        opts: TailOpts,
    ) -> Result<LogSnapshot, BotBackendError> {
        if self.flavor != BotFlavor::NapCat {
            let snap = self.deployment.tail_log(&bot_id, opts.lines).await;
            return Ok(LogSnapshot {
                lines: snap.lines,
                total_lines: snap.total_lines,
            });
        }
        let qq_id: u64 = bot_id
            .as_str()
            .parse()
            .map_err(|_| BotBackendError::InvalidConfig(format!("invalid bot id: {bot_id}")))?;
        let install_base = self.napcat_install_base().await?;
        let log_path = napcat_remote_log_path(&install_base, qq_id);
        let want = if opts.lines > 0 { opts.lines } else { 1000 };
        let raw_n = want.saturating_mul(5).clamp(800, 20_000);
        // 读日志失败时给空行：UI tail 不应因单次 SSH 抖动整页失败。
        let raw = self
            .with_host_refresh(|h| {
                let p = log_path.clone();
                async move { remote_tail_log_raw_lines(h.as_ref(), &p, raw_n).await }
            })
            .await
            .unwrap_or_default();
        let mut lines = ncd_deploy::filter_napcat_console_lines(raw);
        let total_lines = lines.len();
        if lines.len() > want {
            lines = lines.split_off(lines.len() - want);
        }
        Ok(LogSnapshot { lines, total_lines })
    }
}
