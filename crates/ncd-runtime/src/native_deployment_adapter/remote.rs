//! 远端 SSH NativeDeployment -> BotBackend

use std::sync::Arc;

use async_trait::async_trait;
use ncd_backend_napcat::remote_native_launch::{
    napcat_remote_log_path, remote_napcat_running_pid, stop_remote_napcat_on_host,
};
use ncd_deploy::{Deployment, DeploymentError, NativeDeployment};
use ncd_domain::bot_status::BotStatus;
use ncd_domain::ids::BotId;
use ncd_domain::kinds::{BackendKind, StopMode};
use ncd_domain::{BotFlavor, RuntimeTarget};
use ncd_host::{Host, HostError, HostPath};
use ncd_traits::runtime_backend::{
    BotBackend, BotBackendError, BotStartCtx, LogSnapshot, TailOpts,
};

use super::config::{bot_config_for_start, status_for_deployment_state};

#[derive(Clone, Copy)]
enum HostOperation {
    Read,
    Mutation,
}

fn host_error(error: HostError) -> BotBackendError {
    if error.is_disconnect() {
        BotBackendError::RemoteHostTransport(error.to_string())
    } else {
        BotBackendError::Io(error.to_string())
    }
}

fn deployment_error(error: DeploymentError) -> BotBackendError {
    match error {
        DeploymentError::Host(error) => host_error(error),
        DeploymentError::ConfigInvalid(message) => BotBackendError::InvalidConfig(message),
        error => BotBackendError::Io(error.to_string()),
    }
}

async fn run_with_refresh<H, T, F, Fut, Refresh, RefreshFuture>(
    host: H,
    operation: HostOperation,
    op: F,
    refresh: Refresh,
) -> Result<T, BotBackendError>
where
    H: Send,
    F: FnOnce(H) -> Fut + Clone + Send,
    Fut: std::future::Future<Output = Result<T, BotBackendError>> + Send,
    Refresh: FnOnce() -> RefreshFuture + Send,
    RefreshFuture: std::future::Future<Output = Result<H, BotBackendError>> + Send,
{
    match op.clone()(host).await {
        Err(error @ BotBackendError::RemoteHostTransport(_))
            if matches!(operation, HostOperation::Read) =>
        {
            match refresh().await {
                Ok(fresh) => op(fresh).await,
                Err(_) => Err(error),
            }
        }
        result => result,
    }
}

/// 远端「直接运行」:每 Bot 绑定一台 Host + 独立 NativeDeployment(translator 写远端路径)
///
/// 不再常驻固定 host 引用,而是持有 resolver + target;在 start/status/stop/tail_log
/// 边界按需取当前应活 host；仅读操作在确认断连后重试，写操作不自动重放。
pub struct RemoteNativeDeploymentBackend {
    deployment: Arc<NativeDeployment>,
    /// 用于在操作边界按需获取/刷新远端 hostServerManager 通过 TauriHostResolver 注入
    resolver: Arc<dyn crate::HostResolver>,
    /// 目标运行宿主(应为 RuntimeTarget::Server(...))
    target: RuntimeTarget,
    backend_id: BotId,
    flavor: BotFlavor,
    selected: Option<ncd_domain::RemoteSelectedPaths>,
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
            selected: None,
        }
    }

    pub fn with_selected(mut self, selected: ncd_domain::RemoteSelectedPaths) -> Self {
        self.selected = Some(selected);
        self
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

    async fn with_host_refresh<F, Fut, T>(
        &self,
        operation: HostOperation,
        op: F,
    ) -> Result<T, BotBackendError>
    where
        F: FnOnce(Arc<dyn Host>) -> Fut + Clone + Send,
        Fut: std::future::Future<Output = Result<T, BotBackendError>> + Send,
    {
        let host = self.current_host().await?;
        // A launch may already have succeeded remotely when its reply is lost.
        run_with_refresh(host, operation, op, || self.refreshed_host()).await
    }

    async fn napcat_install_base(&self) -> Result<HostPath, BotBackendError> {
        if let Some(sel) = &self.selected {
            let base =
                ncd_domain::require_qq_install_base(sel).map_err(BotBackendError::InvalidConfig)?;
            return Ok(HostPath::from_posix(base));
        }
        let host = self.current_host().await?;
        match crate::remote::inventory::probe_remote_inventory(host.as_ref(), None, None).await {
            Ok(inv) => {
                let base = ncd_domain::require_qq_install_base(&inv.selected)
                    .map_err(BotBackendError::InvalidConfig)?;
                return Ok(HostPath::from_posix(base));
            }
            Err(e) => Err(BotBackendError::InvalidConfig(format!(
                "远端未发现 QQ 安装树，且库存探测失败: {e}"
            ))),
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
            self.with_host_refresh(HostOperation::Mutation, move |h| async move {
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
            .with_host_refresh(HostOperation::Mutation, |h| async move {
                self.deployment
                    .launch(h.as_ref(), &bot_config)
                    .await
                    .map_err(deployment_error)
            })
            .await?;
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
        self.with_host_refresh(HostOperation::Mutation, |h| {
            let bid = bot_id.clone();
            let m = _mode;
            async move {
                self.deployment
                    .stop(h.as_ref(), &bid, m)
                    .await
                    .map_err(deployment_error)
            }
        })
        .await
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
            .with_host_refresh(HostOperation::Read, |h| {
                let bid = bot_id.clone();
                async move {
                    self.deployment
                        .observe(h.as_ref(), &bid)
                        .await
                        .map_err(deployment_error)
                }
            })
            .await?;
        Ok(status_for_deployment_state(bot_id, state))
    }

    async fn tail_log(
        &self,
        bot_id: BotId,
        opts: TailOpts,
    ) -> Result<LogSnapshot, BotBackendError> {
        if self.flavor != BotFlavor::NapCat {
            return Ok(self.deployment.tail_log(&bot_id, opts.lines).await);
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
            .with_host_refresh(HostOperation::Read, |h| {
                let p = log_path.clone();
                async move {
                    // 只让远端 tail -n,不走 SFTP 整读:崩溃转储能到上百 MB
                    ncd_host::remote_tail_lines(h.as_ref(), &p, raw_n, None)
                        .await
                        .map_err(host_error)
                }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[tokio::test]
    async fn business_errors_do_not_refresh_or_repeat_the_operation() {
        for failure in ["config", "permission", "timeout"] {
            let calls = Arc::new(AtomicUsize::new(0));
            let refreshes = AtomicUsize::new(0);
            let result = run_with_refresh(
                1u8,
                HostOperation::Read,
                {
                    let calls = Arc::clone(&calls);
                    move |_| {
                        calls.fetch_add(1, Ordering::SeqCst);
                        async move {
                            Err::<(), _>(deployment_error(match failure {
                                "config" => DeploymentError::ConfigInvalid("bad config".into()),
                                "permission" => {
                                    DeploymentError::Host(HostError::Io(std::io::Error::new(
                                        std::io::ErrorKind::PermissionDenied,
                                        "denied",
                                    )))
                                }
                                _ => {
                                    DeploymentError::Host(HostError::Timeout { operation: "read" })
                                }
                            }))
                        }
                    }
                },
                || async {
                    refreshes.fetch_add(1, Ordering::SeqCst);
                    Ok(2u8)
                },
            )
            .await;
            if failure == "config" {
                assert!(matches!(result, Err(BotBackendError::InvalidConfig(_))));
            } else {
                assert!(matches!(result, Err(BotBackendError::Io(_))));
            }
            assert_eq!(calls.load(Ordering::SeqCst), 1);
            assert_eq!(refreshes.load(Ordering::SeqCst), 0);
        }
    }

    #[tokio::test]
    async fn typed_transport_read_retries_once_with_the_refreshed_host() {
        let calls = Arc::new(AtomicUsize::new(0));
        let refreshes = AtomicUsize::new(0);
        let result = run_with_refresh(
            1u8,
            HostOperation::Read,
            {
                let calls = Arc::clone(&calls);
                move |host| {
                    let attempt = calls.fetch_add(1, Ordering::SeqCst);
                    async move {
                        if attempt == 0 {
                            assert_eq!(host, 1);
                            Err(host_error(HostError::remote_disconnected(
                                "lost first host",
                            )))
                        } else {
                            assert_eq!(host, 2);
                            Ok(42)
                        }
                    }
                }
            },
            || async {
                refreshes.fetch_add(1, Ordering::SeqCst);
                Ok(2u8)
            },
        )
        .await
        .unwrap();
        assert_eq!(result, 42);
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_eq!(refreshes.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn a_mutation_with_an_unknown_remote_result_is_never_replayed() {
        let calls = Arc::new(AtomicUsize::new(0));
        let refreshes = AtomicUsize::new(0);
        let error = run_with_refresh(
            1u8,
            HostOperation::Mutation,
            {
                let calls = Arc::clone(&calls);
                move |_| {
                    calls.fetch_add(1, Ordering::SeqCst);
                    async {
                        Err::<(), _>(BotBackendError::RemoteHostTransport(
                            "original uncertain write".into(),
                        ))
                    }
                }
            },
            || async {
                refreshes.fetch_add(1, Ordering::SeqCst);
                Ok(2u8)
            },
        )
        .await
        .unwrap_err();
        assert!(
            matches!(error, BotBackendError::RemoteHostTransport(message) if message == "original uncertain write")
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(refreshes.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn a_failed_read_refresh_preserves_the_original_error_without_replaying() {
        let error = run_with_refresh(
            1u8,
            HostOperation::Read,
            |_| async {
                Err::<(), _>(BotBackendError::RemoteHostTransport(
                    "original disconnect".into(),
                ))
            },
            || async {
                Err::<u8, _>(BotBackendError::RemoteHostTransport(
                    "refresh failed".into(),
                ))
            },
        )
        .await
        .unwrap_err();
        assert!(
            matches!(error, BotBackendError::RemoteHostTransport(message) if message == "original disconnect")
        );
    }
}
