//! DockerDeployment -> BotBackend 过渡壳

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use ncd_deploy::docker::DockerCli;
use ncd_deploy::{
    Deployment, DeploymentHandle, DockerDeployment, NullProgressSink, bot_docker_container_name,
    resolve_bot_container_name,
};
use ncd_domain::bot_status::BotStatus;
use ncd_domain::ids::BotId;
use ncd_domain::kinds::{BackendKind, StopMode};
use ncd_domain::{BackendType, BotFlavor};
use ncd_host::Host;
use ncd_traits::runtime_backend::{
    BotBackend, BotBackendError, BotRuntimeConfig, BotStartCtx, LogSnapshot, TailOpts,
};
use tracing::warn;

use crate::metrics::{BotRuntimeMetricsPrefs, prepare_docker_metrics_overlay, probe_remote_home};

use super::config::{bot_config_for_start, status_for_deployment_state};
use super::docker_helpers::{docker_project_dir, render_docker_config_on_host};

/// 过渡壳:让 DockerDeployment 穿上 BotBackend trait 外套
pub struct DockerDeploymentBackend {
    deployment: Arc<DockerDeployment>,
    host: Arc<dyn Host>,
    backend_id: BotId,
    flavor: BotFlavor,
    /// 本机 data_root：写出探针再 upload；未设则不注入 metrics
    local_data_root: Option<PathBuf>,
    metrics_prefs: BotRuntimeMetricsPrefs,
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

        let home = match probe_remote_home(self.host.as_ref()).await {
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
        let project_dir = match docker_project_dir(self.host.as_ref(), &name).await {
            Ok(p) => p,
            Err(err) => {
                warn!(
                    target: "ncd_runtime::docker_metrics",
                    err = %err,
                    "Docker metrics: project_dir failed"
                );
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
        // docker 容器跑在 host 上;host 是本机还是远端由注入的 host 决定
        BackendKind::Local
    }

    fn flavor(&self) -> BotFlavor {
        self.flavor
    }

    async fn start(&self, ctx: &BotStartCtx) -> Result<BotStatus, BotBackendError> {
        let bot_config = bot_config_for_start(ctx, self.flavor, true)?;
        render_docker_config_on_host(self.host.as_ref(), &ctx.config.bot_id, &bot_config).await?;

        // 指标：上传探针 + 写 NC load 覆盖；clone deployment 挂 overlay 再 install
        // 失败只 warn，不阻断启动（与远端原生一致）
        let deployment = self.deployment_with_metrics(&bot_config).await;

        let sink = NullProgressSink;
        deployment
            .install(self.host.as_ref(), &bot_config, &sink)
            .await
            .map_err(|err| BotBackendError::Io(err.to_string()))?;

        let handle = deployment
            .launch(self.host.as_ref(), &bot_config)
            .await
            .map_err(|err| BotBackendError::Io(err.to_string()))?;

        match handle {
            DeploymentHandle::Docker { started_at, .. } => {
                Ok(BotStatus::running(ctx.config.bot_id.clone(), 0, started_at))
            }
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
        let name = resolve_bot_container_name(self.host.as_ref(), &bot_id)
            .await
            .ok()
            .flatten()
            .unwrap_or_else(|| {
                let backend = match self.flavor {
                    BotFlavor::SnowLuma => BackendType::SnowLuma,
                    BotFlavor::NapCat => BackendType::NapCat,
                };
                bot_docker_container_name(backend, bot_id.as_str().parse().unwrap_or(0))
            });
        let cli = DockerCli::new(self.host.as_ref());
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
