//! DockerDeployment -> BotBackend 过渡壳

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use ncd_deploy::deployments::docker::resolve_bot_container_name_with_cli;
use ncd_deploy::{
    Deployment, DeploymentHandle, DockerDeployment, NullProgressSink,
    bot_docker_container_candidates, bot_docker_container_name,
};
use ncd_domain::bot_status::BotStatus;
use ncd_domain::ids::BotId;
use ncd_domain::kinds::{BackendKind, StopMode};
use ncd_domain::{BackendType, BotFlavor, ContainerState, SnowLumaQrLoginSnapshot};
use ncd_host::Host;
use ncd_traits::runtime_backend::{
    BotBackend, BotBackendError, BotStartCtx, LogSnapshot, TailOpts,
};
use tracing::warn;

use crate::metrics::{BotRuntimeMetricsPrefs, prepare_docker_metrics_overlay, probe_remote_home};

use super::config::{bot_config_for_start, status_for_deployment_state};
use super::docker_helpers::{render_docker_config_on_host, sync_snowluma_docker_config};

/// 过渡壳:让 DockerDeployment 穿上 BotBackend trait 外套
pub struct DockerDeploymentBackend {
    deployment: Arc<DockerDeployment>,
    host: Arc<dyn Host>,
    backend_id: BotId,
    flavor: BotFlavor,
    /// 本机 data_root：写出探针再 upload；未设则不注入 metrics
    local_data_root: Option<PathBuf>,
    metrics_prefs: BotRuntimeMetricsPrefs,
    snowluma_qr_login: bool,
    qr_login_lock: Arc<tokio::sync::Mutex<()>>,
}

impl DockerDeploymentBackend {
    pub fn new(
        deployment: Arc<DockerDeployment>,
        host: Arc<dyn Host>,
        backend_id: impl Into<BotId>,
        flavor: BotFlavor,
    ) -> Self {
        Self {
            deployment,
            host,
            backend_id: backend_id.into(),
            flavor,
            local_data_root: None,
            metrics_prefs: BotRuntimeMetricsPrefs::default(),
            snowluma_qr_login: false,
            qr_login_lock: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    pub fn with_metrics(
        mut self,
        local_data_root: impl Into<PathBuf>,
        prefs: BotRuntimeMetricsPrefs,
    ) -> Self {
        self.local_data_root = Some(local_data_root.into());
        self.metrics_prefs = prefs;
        self
    }

    pub fn with_snowluma_qr_login(mut self, enabled: bool) -> Self {
        self.snowluma_qr_login = enabled;
        self
    }

    pub fn with_qr_login_lock(mut self, lock: Arc<tokio::sync::Mutex<()>>) -> Self {
        self.qr_login_lock = lock;
        self
    }

    /// 若指标开启：prepare overlay 并 clone 一份带 metrics 的 DockerDeployment
    async fn deployment_with_metrics(
        &self,
        bot_config: &ncd_domain::BotConfig,
    ) -> Arc<DockerDeployment> {
        if !self.metrics_prefs.enabled {
            return Arc::clone(&self.deployment);
        }
        let Some(data_root) = self.local_data_root.as_ref() else {
            return Arc::clone(&self.deployment);
        };

        let metrics_root = if self.host.locality() == ncd_host::Locality::Local {
            self.host
                .target_managed_root()
                .map(|root| root.as_posix().to_owned())
                .ok_or_else(|| "本机 Desktop 数据目录未配置".to_string())
        } else {
            probe_remote_home(self.host.as_ref()).await
        };
        let home = match metrics_root {
            Ok(h) => h,
            Err(err) => {
                warn!(
                    target: "ncd_runtime::docker_metrics",
                    err = %err,
                    "Docker metrics: probe HOME failed"
                );
                return Arc::clone(&self.deployment);
            }
        };

        let name = bot_docker_container_name(
            match self.flavor {
                BotFlavor::SnowLuma => BackendType::SnowLuma,
                BotFlavor::NapCat => BackendType::NapCat,
            },
            bot_config.bot.qq_id,
        );
        let project_dir = match ncd_deploy::docker_project_dir(self.host.as_ref(), &name).await {
            Ok(path) => path,
            Err(error) => {
                warn!(%error, "Docker metrics: project directory unavailable");
                return Arc::clone(&self.deployment);
            }
        };

        // bot_id 与 watch metrics 一致：qq 字符串
        let bot_id = bot_config.bot.qq_id.to_string();
        match prepare_docker_metrics_overlay(
            self.host.as_ref(),
            &home,
            &bot_id,
            bot_config,
            &self.metrics_prefs,
            data_root,
            &project_dir,
            self.flavor,
        )
        .await
        {
            Ok(Some(overlay)) => {
                let dep = (*self.deployment)
                    .clone()
                    .with_metrics_overlay(Some(overlay));
                Arc::new(dep)
            }
            Ok(None) => Arc::clone(&self.deployment),
            Err(err) => {
                warn!(
                    target: "ncd_runtime::docker_metrics",
                    err = %err,
                    "Docker metrics prepare failed; starting without probe"
                );
                Arc::clone(&self.deployment)
            }
        }
    }
}

#[async_trait]
impl BotBackend for DockerDeploymentBackend {
    fn id(&self) -> &BotId {
        &self.backend_id
    }

    fn kind(&self) -> BackendKind {
        match self.host.locality() {
            ncd_host::Locality::Local => BackendKind::Local,
            ncd_host::Locality::Remote => BackendKind::RemoteSsh,
        }
    }

    fn flavor(&self) -> BotFlavor {
        self.flavor
    }

    async fn sync_bot_config(&self, config: &ncd_domain::BotConfig) -> Result<(), BotBackendError> {
        let bot_id = BotId::new(config.bot.qq_id.to_string());
        render_docker_config_on_host(self.host.as_ref(), &bot_id, config).await?;
        if self.flavor == BotFlavor::SnowLuma {
            let cli = self.deployment.cli(self.host.as_ref());
            cli.ensure_daemon_ready()
                .await
                .map_err(|error| BotBackendError::Io(error.to_string()))?;
            let container = resolve_bot_container_name_with_cli(&cli, &bot_id)
                .await
                .map_err(|error| BotBackendError::Io(error.to_string()))?
                .unwrap_or_else(|| DockerDeployment::container_name(config));
            sync_snowluma_docker_config(self.host.as_ref(), &cli, config, &container).await?;
        }
        Ok(())
    }

    async fn start(&self, ctx: &BotStartCtx) -> Result<BotStatus, BotBackendError> {
        let _qr_guard = self.qr_login_lock.lock().await;
        let bot_config = bot_config_for_start(ctx, self.flavor, true)?;
        if self.host.os() != ncd_host::Os::Linux {
            return Err(BotBackendError::InvalidConfig(
                "Docker Bot 仅支持 Linux 主机".into(),
            ));
        }
        let cli = self.deployment.cli(self.host.as_ref());
        cli.ensure_daemon_ready()
            .await
            .map_err(|err| BotBackendError::Io(err.to_string()))?;
        let candidates =
            bot_docker_container_candidates(bot_config.bot.backend_type, bot_config.bot.qq_id);
        let containers = cli
            .list_containers()
            .await
            .map_err(|err| BotBackendError::Io(err.to_string()))?;
        if containers.iter().any(|c| {
            candidates.contains(&c.name)
                && matches!(c.state, ContainerState::Paused | ContainerState::Restarting)
        }) {
            return Err(BotBackendError::InvalidConfig(
                "容器处于暂停或重启中，请先恢复或停止该容器".into(),
            ));
        }
        if containers
            .iter()
            .any(|c| candidates.contains(&c.name) && c.state == ContainerState::Running)
        {
            if self.host.locality() == ncd_host::Locality::Local {
                ncd_deploy::ensure_managed_bot_container(self.host.as_ref(), &bot_config)
                    .await
                    .map_err(|error| BotBackendError::InvalidConfig(error.to_string()))?;
            }
            let started_at = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            return Ok(BotStatus::running(ctx.config.bot_id.clone(), 0, started_at));
        }
        render_docker_config_on_host(self.host.as_ref(), &ctx.config.bot_id, &bot_config).await?;

        // 指标：上传探针 + 写 NC load 覆盖；clone deployment 挂 overlay 再 install
        // 失败只 warn，不阻断启动（与远端原生一致）
        let mut deployment = self.deployment_with_metrics(&bot_config).await;
        let sink = NullProgressSink;
        if let Err(err) = deployment
            .install(self.host.as_ref(), &bot_config, &sink)
            .await
        {
            if self.flavor == BotFlavor::SnowLuma {
                let _ = ncd_backend_snowluma::docker_login::DockerLoginSession::close(
                    self.host.as_ref(),
                    &ctx.config.bot_id,
                )
                .await;
            }
            return Err(BotBackendError::Io(err.to_string()));
        }

        if self.snowluma_qr_login && self.flavor == BotFlavor::SnowLuma {
            match ncd_backend_snowluma::docker_login::DockerLoginSession::prepare(
                self.host.as_ref(),
                &bot_config,
            )
            .await
            {
                Ok(Some(overlay)) => {
                    let base_deployment = Arc::clone(&deployment);
                    deployment = Arc::new((*deployment).clone().with_login_overlay(Some(overlay)));
                    if deployment
                        .install(self.host.as_ref(), &bot_config, &sink)
                        .await
                        .is_err()
                    {
                        let _ =
                            ncd_backend_snowluma::docker_login::DockerLoginSession::abort_prepare(
                                self.host.as_ref(),
                                &ctx.config.bot_id,
                                cli.engine(),
                            )
                            .await;
                        deployment = base_deployment;
                        deployment
                            .install(self.host.as_ref(), &bot_config, &sink)
                            .await
                            .map_err(|err| BotBackendError::Io(err.to_string()))?;
                        warn!(target: "ncd_runtime::docker_login", "Docker 扫码挂载未就绪，已恢复 VNC 登录");
                    }
                }
                Ok(None) => {}
                Err(_) => {
                    let _ = ncd_backend_snowluma::docker_login::DockerLoginSession::abort_prepare(
                        self.host.as_ref(),
                        &ctx.config.bot_id,
                        cli.engine(),
                    )
                    .await;
                    warn!(target: "ncd_runtime::docker_login", "Docker 扫码辅助未就绪，保留 VNC 登录");
                }
            }
        }

        let handle = match deployment.launch(self.host.as_ref(), &bot_config).await {
            Ok(handle) => handle,
            Err(err) => {
                if self.flavor == BotFlavor::SnowLuma {
                    let _ = ncd_backend_snowluma::docker_login::DockerLoginSession::close(
                        self.host.as_ref(),
                        &ctx.config.bot_id,
                    )
                    .await;
                }
                return Err(BotBackendError::Io(err.to_string()));
            }
        };

        if self.flavor == BotFlavor::SnowLuma {
            let container = DockerDeployment::container_name(&bot_config);
            let applied = async {
                sync_snowluma_docker_config(self.host.as_ref(), &cli, &bot_config, &container)
                    .await?;
                // 首次创建时 named volume 到 compose up 后才存在；写入后重启，让协议端
                // 在账号加载之前读取最新配置。运行会话与隧道此时尚未建立。
                cli.lifecycle("restart", &container)
                    .await
                    .map_err(|error| BotBackendError::Io(error.to_string()))
            }
            .await;
            if let Err(error) = applied {
                // 不让失败后留下的 Running 容器在下次 start 时走接管早返，跳过重试。
                if let Err(stop_error) = cli.lifecycle("stop", &container).await {
                    warn!(target: "ncd_runtime::docker_bot", %stop_error, "同步配置失败后停止容器失败");
                }
                let _ = ncd_backend_snowluma::docker_login::DockerLoginSession::close(
                    self.host.as_ref(),
                    &ctx.config.bot_id,
                )
                .await;
                return Err(error);
            }
        }

        match handle {
            DeploymentHandle::Docker { started_at, .. } => {
                Ok(BotStatus::running(ctx.config.bot_id.clone(), 0, started_at))
            }
            _ => Err(BotBackendError::Io("unexpected handle variant".into())),
        }
    }

    async fn stop(&self, bot_id: BotId, mode: StopMode) -> Result<(), BotBackendError> {
        let _qr_guard = self.qr_login_lock.lock().await;
        if self.flavor == BotFlavor::SnowLuma {
            let _ = ncd_backend_snowluma::docker_login::DockerLoginSession::close(
                self.host.as_ref(),
                &bot_id,
            )
            .await;
        }
        self.deployment
            .stop(self.host.as_ref(), &bot_id, mode)
            .await
            .map_err(|err| BotBackendError::Io(err.to_string()))?;
        if self.flavor == BotFlavor::SnowLuma {
            ncd_backend_snowluma::docker_login::DockerLoginSession::close(
                self.host.as_ref(),
                &bot_id,
            )
            .await?;
        }
        Ok(())
    }

    async fn qr_login_snapshot(
        &self,
        bot_id: &BotId,
        refresh: bool,
    ) -> Result<SnowLumaQrLoginSnapshot, BotBackendError> {
        let _qr_guard = self.qr_login_lock.lock().await;
        if self.flavor != BotFlavor::SnowLuma || !self.snowluma_qr_login {
            return Ok(SnowLumaQrLoginSnapshot::unavailable(
                "此容器未启用简化扫码，请使用 VNC",
            ));
        }
        ncd_backend_snowluma::docker_login::DockerLoginSession::snapshot(
            self.host.as_ref(),
            bot_id,
            refresh,
        )
        .await
    }

    async fn close_qr_login(&self, bot_id: &BotId) -> Result<(), BotBackendError> {
        let _qr_guard = self.qr_login_lock.lock().await;
        if self.flavor == BotFlavor::SnowLuma {
            ncd_backend_snowluma::docker_login::DockerLoginSession::close(
                self.host.as_ref(),
                bot_id,
            )
            .await?;
        }
        Ok(())
    }

    async fn status(&self, bot_id: BotId) -> Result<BotStatus, BotBackendError> {
        let state = self
            .deployment
            .observe(self.host.as_ref(), &bot_id)
            .await
            .map_err(|err| BotBackendError::Io(err.to_string()))?;
        Ok(status_for_deployment_state(bot_id, state))
    }

    async fn tail_log(
        &self,
        bot_id: BotId,
        opts: TailOpts,
    ) -> Result<LogSnapshot, BotBackendError> {
        let cli = self.deployment.cli(self.host.as_ref());
        cli.ensure_daemon_ready()
            .await
            .map_err(|error| BotBackendError::Io(error.to_string()))?;
        let name = resolve_bot_container_name_with_cli(&cli, &bot_id)
            .await
            .map_err(|error| BotBackendError::Io(error.to_string()))?
            .unwrap_or_else(|| {
                let backend = match self.flavor {
                    BotFlavor::SnowLuma => BackendType::SnowLuma,
                    BotFlavor::NapCat => BackendType::NapCat,
                };
                bot_docker_container_name(backend, bot_id.as_str().parse().unwrap_or(0))
            });
        let logs = cli
            .logs(&name, opts.lines as u32)
            .await
            .map_err(|err| BotBackendError::Io(err.to_string()))?;
        let lines: Vec<String> = match self.flavor {
            BotFlavor::NapCat => ncd_deploy::filter_napcat_console_lines(logs.lines()),
            BotFlavor::SnowLuma => {
                ncd_backend_snowluma::filter_snowluma_console_lines(logs.lines())
            }
        };
        let total = lines.len();
        Ok(LogSnapshot {
            lines,
            total_lines: total,
        })
    }
}
