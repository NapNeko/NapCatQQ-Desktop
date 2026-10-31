//! Docker 部署:用 docker compose 在 Linux Host(本机 / 远端 SSH)
//! 上把 bot 跑成容器
//!
//! 与组件页「拉镜像」(commands/docker.rs docker_deploy)的关系:组件页只预拉
//! NapCat/SnowLuma 官方镜像,不创建 napcat/snowluma 演示容器;这里是 bot 生命周期
//! 部署形态(deployment_type=docker),由 BotManager 在 start_bot 时按 bot 配置驱动,
//! 容器纳入 bot 状态机,两者共用底层 DockerCli + compose 渲染,但入口和归属不同
//!
//! 容器命名:ncbot-<qq> / slbot-<qq>(按口味),区别于组件页 napcat/snowluma 演示容器
//! compose 项目目录:远端 $HOME/.napcat-bots/<name>,探不到 HOME 直接失败,
//! 避免把生产数据静默落到 /tmp
//!
//! 镜像须在「组件」页预拉;Bot 启动 install 只检查本地是否存在官方镜像,不现场 pull
//!
//! NapCat / SnowLuma 均走官方 compose 语义(见 docker/compose.rs 与 SnowLuma.Docker.Framework)
//! NapCat 可预写 onebot/napcat 到 bind 目录; SnowLuma 写 onebot_<qq>.json 到 named volume
//! 容器命名:NapCat ncbot-<qq>,SnowLuma slbot-<qq>
//! compose 项目目录:远端 $HOME/.napcat-bots/<name>

use async_trait::async_trait;
use ncd_domain::{
    BackendType, BotConfig, BotFlavor, BotId, DockerDeploySpec, DockerFlavor, StopMode,
};
use ncd_host::{Host, HostCommand, HostPath};
use tracing::{error, info};

use crate::deployment::{
    Deployment, DeploymentError, DeploymentHandle, DeploymentProgressSink, DeploymentState,
};
use crate::docker::{
    DockerCli, DockerLoginOverlay, DockerMetricsOverlay,
    compose::{
        apply_bot_docker_settings, render_compose_with_metrics,
        render_snowluma_compose_with_overlays,
    },
};

/// Docker 部署实装
#[derive(Clone)]
pub struct DockerDeployment {
    id: &'static str,
    flavors: &'static [BotFlavor],
    /// NapCat: WEBUI_TOKEN; SnowLuma: VNC_PASSWD(compose 环境变量)
    compose_secret: Option<String>,
    /// SnowLuma: 写入 SNOWLUMA_WEBUI_BOOTSTRAP_PASSWORD(WebUI 登录,非 VNC)
    sl_webui_bootstrap: Option<String>,
    allow_test_default_token: bool,
    /// 可选实例指标:compose 注入 NCD_METRICS_* + bind 卷
    metrics_overlay: Option<DockerMetricsOverlay>,
    login_overlay: Option<DockerLoginOverlay>,
}

impl DockerDeployment {
    pub fn new() -> Self {
        Self {
            id: "docker",
            flavors: &[BotFlavor::NapCat, BotFlavor::SnowLuma],
            compose_secret: None,
            sl_webui_bootstrap: None,
            allow_test_default_token: false,
            metrics_overlay: None,
            login_overlay: None,
        }
    }

    /// NapCat Docker: WEBUI_TOKEN,ncd-deploy 不从 QQ 号派生凭据
    pub fn with_webui_token(token: impl Into<String>) -> Self {
        Self {
            compose_secret: Some(token.into()),
            ..Self::new()
        }
    }

    /// SnowLuma Docker: VNC_PASSWD + SNOWLUMA_WEBUI_BOOTSTRAP_PASSWORD
    pub fn with_sl_secrets(
        vnc_passwd: impl Into<String>,
        webui_bootstrap: impl Into<String>,
    ) -> Self {
        Self {
            compose_secret: Some(vnc_passwd.into()),
            sl_webui_bootstrap: Some(webui_bootstrap.into()),
            ..Self::new()
        }
    }

    /// 附加实例指标 compose overlay(None = 关,与现网 compose 一致)
    pub fn with_metrics_overlay(mut self, overlay: Option<DockerMetricsOverlay>) -> Self {
        self.metrics_overlay = overlay;
        self
    }

    pub fn with_login_overlay(mut self, overlay: Option<DockerLoginOverlay>) -> Self {
        self.login_overlay = overlay;
        self
    }

    #[cfg(test)]
    fn with_test_default_token() -> Self {
        Self {
            allow_test_default_token: true,
            ..Self::new()
        }
    }

    /// bot 容器名:NapCat ncbot-<qq>,SnowLuma slbot-<qq>
    pub fn container_name(config: &BotConfig) -> String {
        bot_docker_container_name(config.bot.backend_type, config.bot.qq_id)
    }

    /// compose 项目目录(host 侧 POSIX 路径),远端 HOME 探测失败时 hard fail
    async fn project_dir(host: &dyn Host, name: &str) -> Result<String, DeploymentError> {
        docker_project_dir(host, name).await.map_err(|reason| {
            DeploymentError::ConfigInvalid(format!("{reason}，拒绝回退到临时目录部署 Docker bot"))
        })
    }

    fn compose_secret_for(&self, backend: BackendType) -> Result<String, DeploymentError> {
        if let Some(secret) = self
            .compose_secret
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            return Ok(secret.to_string());
        }
        if self.allow_test_default_token {
            return Ok(match backend {
                BackendType::SnowLuma => "test-vnc-passwd".to_string(),
                BackendType::NapCat => "test-webui-token".to_string(),
            });
        }
        Err(DeploymentError::ConfigInvalid(match backend {
            BackendType::SnowLuma => "DockerDeployment 需要上层显式传入 VNC 密码 (SnowLuma)".into(),
            BackendType::NapCat => "DockerDeployment 需要上层显式传入 WebUI token".into(),
        }))
    }

    fn compose_env_var(backend: BackendType) -> &'static str {
        match backend {
            BackendType::SnowLuma => "VNC_PASSWD",
            BackendType::NapCat => "WEBUI_TOKEN",
        }
    }

    /// 从 BotConfig 构造 DockerDeploySpec:容器名按口味,端口默认 + per-qq 宿主机偏移
    pub fn build_spec(config: &BotConfig) -> DockerDeploySpec {
        let qq = config.bot.qq_id;
        let mut spec = match config.bot.backend_type {
            BackendType::SnowLuma => DockerDeploySpec::snowluma_default(),
            BackendType::NapCat => DockerDeploySpec::napcat_default(),
        }
        .with_host_port_offset(qq);
        spec.container_name = Self::container_name(config);
        spec.ports = config.effective_docker_ports();
        if config.bot.backend_type == BackendType::NapCat && qq != 0 {
            spec.qq_id = Some(qq);
        }
        spec
    }

    pub async fn container_uid_gid(host: &dyn Host) -> Result<(u32, u32), DeploymentError> {
        if host.locality() == ncd_host::Locality::Remote {
            return Ok((1000, 1000));
        }
        let mut ids = [0; 2];
        for (index, flag) in ["-u", "-g"].into_iter().enumerate() {
            let output = host.run_to_string(HostCommand::new("id").arg(flag)).await?;
            if !output.success() {
                return Err(DeploymentError::ConfigInvalid(format!(
                    "读取本机容器运行用户失败: {}",
                    output.stderr.trim()
                )));
            }
            ids[index] = output.stdout.trim().parse::<u32>().map_err(|error| {
                DeploymentError::ConfigInvalid(format!("解析本机用户ID失败: {error}"))
            })?;
        }
        Ok((ids[0], ids[1]))
    }
}

impl Default for DockerDeployment {
    fn default() -> Self {
        Self::new()
    }
}

/// docker compose .env 行:含空格,#,: 等时加双引号并转义
fn dotenv_value(raw: &str) -> String {
    let needs_quote = raw.is_empty()
        || raw.bytes().any(|b| b.is_ascii_whitespace())
        || raw.contains('#')
        || raw.contains('"')
        || raw.contains('\'');
    if !needs_quote {
        return raw.to_string();
    }
    let mut out = String::from('"');
    for c in raw.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// bot 的 compose 项目目录(host 侧 POSIX 路径)
///
/// 部署和 runtime 渲染 Docker 配置都要落到同一个目录,探 HOME 只放这一份;
/// 探不到时给出原因,由调用方包成各自的错误类型
pub async fn docker_project_dir(host: &dyn Host, name: &str) -> Result<String, String> {
    if host.locality() == ncd_host::Locality::Local {
        return host
            .target_managed_root()
            .map(|root| root.join("docker").join(name).as_posix().to_owned())
            .ok_or_else(|| "本机 Desktop 数据目录未配置，无法部署 Docker Bot".into());
    }
    let home = probe_home(host).await?;
    Ok(ncd_domain::remote_paths::docker_bot_project_dir(
        &home, name,
    ))
}

/// 自动重建前核对 Compose 所属目录，避免接管同名的外部容器。
/// 两种历史命名都检查，因为部署清理会访问这两个候选名。
pub async fn ensure_managed_bot_container(
    host: &dyn Host,
    config: &BotConfig,
) -> Result<(), DeploymentError> {
    let cli = DockerCli::new(host);
    cli.ensure_daemon_ready()
        .await
        .map_err(|e| DeploymentError::ConfigInvalid(format!("检查容器归属失败: {e}")))?;
    let containers = cli
        .list_containers()
        .await
        .map_err(|e| DeploymentError::ConfigInvalid(format!("检查容器归属失败: {e}")))?;
    for name in bot_docker_container_candidates(config.bot.backend_type, config.bot.qq_id) {
        if !containers.iter().any(|c| c.name == name) {
            continue;
        }
        let expected = docker_project_dir(host, &name)
            .await
            .map_err(DeploymentError::ConfigInvalid)?;
        let output = host
            .run_to_string(
                cli.command()
                    .arg("inspect")
                    .arg("--type")
                    .arg("container")
                    .arg("--format")
                    .arg("{{json .Config.Labels}}")
                    .arg("--")
                    .arg(&name)
                    .timeout(std::time::Duration::from_secs(30)),
            )
            .await
            .map_err(|e| {
                DeploymentError::ConfigInvalid(format!("检查容器 {name} 的 Compose 归属失败: {e}"))
            })?;
        if !output.success() {
            return Err(DeploymentError::ConfigInvalid(format!(
                "检查容器 {name} 的 Compose 归属失败: {}",
                output.stderr.trim()
            )));
        }
        let labels: Option<std::collections::BTreeMap<String, String>> =
            serde_json::from_str(output.stdout.trim()).map_err(|e| {
                DeploymentError::ConfigInvalid(format!("读取容器 {name} 的 Compose 标签失败: {e}"))
            })?;
        let actual = labels
            .as_ref()
            .and_then(|labels| labels.get("com.docker.compose.project.working_dir"));
        let owned = actual.is_some_and(|dir| {
            ncd_domain::remote_paths::normalize_posix(dir)
                == ncd_domain::remote_paths::normalize_posix(&expected)
        });
        if !owned {
            let actual = actual.map(String::as_str).unwrap_or("未记录");
            return Err(DeploymentError::ConfigInvalid(format!(
                "容器 {name} 的实际 Compose 目录为「{actual}」，当前托管目录为「{expected}」，可能来自旧数据目录、外部 Compose 或其他方式创建，不能自动重建。若刚迁移本机数据目录，请先停止旧容器，在 Docker 页删除该已停止容器（保留数据卷），再启动 Bot 重建；确认新位置正常前请保留旧目录。未迁移的容器请在原部署中调整，或先迁入 Desktop 托管。"
            )));
        }
    }
    Ok(())
}

async fn probe_home(host: &dyn Host) -> Result<String, String> {
    let cmd = HostCommand::new("sh").arg("-c").arg("echo $HOME");
    match host.run_to_string(cmd).await {
        Ok(out) if out.success() => {
            let home = out.stdout.trim().to_string();
            if home.is_empty() {
                Err("Docker 主机 HOME 为空，无法确定部署项目目录".into())
            } else {
                Ok(home)
            }
        }
        Ok(out) => Err(format!(
            "探测 Docker 主机 HOME 失败: exit={:?}, stderr={}",
            out.exit_code,
            out.stderr.trim()
        )),
        Err(error) => Err(format!("探测 Docker 主机 HOME 失败: {error}")),
    }
}

#[async_trait]
impl Deployment for DockerDeployment {
    fn id(&self) -> &str {
        self.id
    }

    fn supported_flavors(&self) -> &[BotFlavor] {
        self.flavors
    }

    fn supports(&self, host: &dyn Host) -> bool {
        // 本机与远端 Linux 共用部署链，daemon 状态在 install / launch 时探测。
        use ncd_host::Os;
        matches!(host.os(), Os::Linux)
    }

    async fn install(
        &self,
        host: &dyn Host,
        config: &BotConfig,
        progress: &dyn DeploymentProgressSink,
    ) -> Result<(), DeploymentError> {
        if !self.supports(host) {
            return Err(DeploymentError::Unsupported("Docker Bot 仅支持 Linux 主机"));
        }
        config
            .validate_docker()
            .map_err(DeploymentError::ConfigInvalid)?;
        let backend = config.bot.backend_type;
        if !matches!(backend, BackendType::NapCat | BackendType::SnowLuma) {
            return Err(DeploymentError::UnsupportedFlavor {
                flavor: format!("{:?} docker 部署暂未实装", backend),
            });
        }

        let cli = DockerCli::new(host);
        progress.report("docker", "探测 Docker 状态", 5);
        cli.ensure_ready().await.map_err(|e| {
            error!(
                target: "ncd_deploy::docker_bot",
                qq_id = config.bot.qq_id,
                err = %e,
                "Bot Docker 部署: Docker 未就绪"
            );
            DeploymentError::InstallFailed(format!("Docker 未就绪: {e}"))
        })?;

        let docker_flavor = match backend {
            BackendType::SnowLuma => DockerFlavor::SnowLuma,
            BackendType::NapCat => DockerFlavor::NapCat,
        };
        let official = docker_flavor.default_image();
        let image_present = cli
            .image_exists(official)
            .await
            .map_err(|e| DeploymentError::InstallFailed(format!("探测镜像失败: {e}")))?;
        if !image_present {
            let label = match backend {
                BackendType::SnowLuma => "SnowLuma",
                BackendType::NapCat => "NapCat",
            };
            return Err(DeploymentError::InstallFailed(format!(
                "{label} 镜像未拉取，请到「组件」页拉取镜像后再启动"
            )));
        }
        cli.ensure_image_compatible(official)
            .await
            .map_err(|error| DeploymentError::InstallFailed(error.to_string()))?;

        let spec = Self::build_spec(config);
        let name = Self::container_name(config);
        let project_dir = Self::project_dir(host, &name).await?;

        progress.report("compose", "准备部署目录", 15);
        host.create_dir_all(&HostPath::from_posix(&project_dir))
            .await
            .map_err(|e| DeploymentError::InstallFailed(format!("创建部署目录失败: {e}")))?;

        let secret = self.compose_secret_for(backend)?;
        let env_var = Self::compose_env_var(backend);
        let env_path = HostPath::from_posix(format!("{project_dir}/.env"));
        let mut env_body = format!("{env_var}={}\n", dotenv_value(&secret));
        if backend == BackendType::SnowLuma {
            let webui = self
                .sl_webui_bootstrap
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| {
                    DeploymentError::ConfigInvalid(
                        "SnowLuma Docker 需要 SNOWLUMA_WEBUI_BOOTSTRAP_PASSWORD".into(),
                    )
                })?;
            env_body.push_str(&format!(
                "SNOWLUMA_WEBUI_BOOTSTRAP_PASSWORD={}\n",
                dotenv_value(webui)
            ));
        }
        host.write_file(&env_path, env_body.as_bytes())
            .await
            .map_err(|e| DeploymentError::InstallFailed(format!("写 Docker .env 失败: {e}")))?;

        let (uid, gid) = Self::container_uid_gid(host).await?;
        let metrics = self.metrics_overlay.as_ref();
        let yaml = if backend == BackendType::SnowLuma {
            render_snowluma_compose_with_overlays(
                &spec,
                "VNC_PASSWD",
                "SNOWLUMA_WEBUI_BOOTSTRAP_PASSWORD",
                uid,
                gid,
                metrics,
                self.login_overlay.as_ref(),
            )
        } else {
            render_compose_with_metrics(&spec, env_var, uid, gid, metrics)
        };
        let yaml = apply_bot_docker_settings(&yaml, config.docker.as_ref())
            .map_err(DeploymentError::ConfigInvalid)?;
        let yaml =
            crate::docker::engine::prepare_compose(crate::docker::engine::engine_for(host), &yaml)
                .map_err(DeploymentError::ConfigInvalid)?;
        let compose_path = HostPath::from_posix(format!("{project_dir}/docker-compose.yml"));
        host.write_file(&compose_path, yaml.as_bytes())
            .await
            .map_err(|e| DeploymentError::InstallFailed(format!("写 compose 文件失败: {e}")))?;

        progress.report("pull", "镜像就绪", 90);
        info!(
            target: "ncd_deploy::docker_bot",
            qq_id = config.bot.qq_id,
            container = %name,
            image = %official,
            "Bot Docker 使用本地已拉取镜像"
        );
        Ok(())
    }

    async fn launch(
        &self,
        host: &dyn Host,
        config: &BotConfig,
    ) -> Result<DeploymentHandle, DeploymentError> {
        if !self.supports(host) {
            return Err(DeploymentError::Unsupported("Docker Bot 仅支持 Linux 主机"));
        }
        let cli = DockerCli::new(host);
        cli.ensure_ready().await.map_err(|e| {
            error!(
                target: "ncd_deploy::docker_bot",
                qq_id = config.bot.qq_id,
                err = %e,
                "Bot Docker 部署: Docker 未就绪"
            );
            DeploymentError::LaunchFailed(format!("Docker 未就绪: {e}"))
        })?;
        let name = Self::container_name(config);
        let bot_id = BotId::new(config.bot.qq_id.to_string());

        if matches!(self.observe(host, &bot_id).await?, DeploymentState::Running) {
            if host.locality() == ncd_host::Locality::Local {
                ensure_managed_bot_container(host, config).await?;
            }
            let started_at = now_secs();
            let container_id = running_container_id(&cli, &name).await?;
            info!(
                target: "ncd_deploy::docker_bot",
                qq_id = config.bot.qq_id,
                container = %name,
                "Bot Docker 容器已在运行，跳过 compose up"
            );
            return Ok(DeploymentHandle::Docker {
                container_id,
                started_at,
            });
        }

        let project_dir = Self::project_dir(host, &name).await?;

        // 只重建 Desktop 托管容器；同名外部容器不得被清理。
        ensure_managed_bot_container(host, config).await?;
        let containers = cli
            .list_containers()
            .await
            .map_err(|error| DeploymentError::LaunchFailed(error.to_string()))?;
        for stale in bot_docker_container_candidates(config.bot.backend_type, config.bot.qq_id) {
            if containers.iter().any(|container| container.name == stale) {
                cli.remove(&stale).await.map_err(|error| {
                    DeploymentError::LaunchFailed(format!("清理旧容器失败: {error}"))
                })?;
            }
        }

        cli.compose_up(&project_dir).await.map_err(|e| {
            error!(
                target: "ncd_deploy::docker_bot",
                qq_id = config.bot.qq_id,
                container = %name,
                err = %e,
                "Bot Docker 部署: 启动容器失败"
            );
            DeploymentError::LaunchFailed(format!("启动容器失败: {e}"))
        })?;

        // compose 成功不代表服务仍在运行，必须回读实际容器状态。
        let started_at = now_secs();
        let container_id = running_container_id(&cli, &name).await?;
        info!(
            target: "ncd_deploy::docker_bot",
            qq_id = config.bot.qq_id,
            container = %name,
            "Bot Docker 容器已启动"
        );
        Ok(DeploymentHandle::Docker {
            container_id,
            started_at,
        })
    }

    async fn observe(
        &self,
        host: &dyn Host,
        bot_id: &BotId,
    ) -> Result<DeploymentState, DeploymentError> {
        let cli = DockerCli::new(host);
        if !self.supports(host) {
            return Err(DeploymentError::Unsupported("Docker Bot 仅支持 Linux 主机"));
        }
        if let Err(e) = cli.ensure_daemon_ready().await {
            return Ok(DeploymentState::Failed {
                reason: format!("Docker 未就绪: {e}"),
            });
        }
        let name = match resolve_bot_container_name_with_cli(&cli, bot_id).await {
            Ok(Some(name)) => name,
            Ok(None) => return Ok(DeploymentState::Stopped),
            Err(error) => {
                return Ok(DeploymentState::Failed {
                    reason: error.to_string(),
                });
            }
        };
        // 归到 Failed 状态(带原因)让上层显示,而不是抛错中断轮询
        match cli.list_containers().await {
            Ok(containers) => Ok(containers
                .iter()
                .find(|c| c.name == name)
                .map(|c| map_state(&c.state))
                .unwrap_or(DeploymentState::Stopped)),
            Err(e) => Ok(DeploymentState::Failed {
                reason: format!("查询容器状态失败: {e}"),
            }),
        }
    }

    async fn stop(
        &self,
        host: &dyn Host,
        bot_id: &BotId,
        _mode: StopMode,
    ) -> Result<(), DeploymentError> {
        if !self.supports(host) {
            return Err(DeploymentError::Unsupported("Docker Bot 仅支持 Linux 主机"));
        }
        let cli = DockerCli::new(host);
        cli.ensure_daemon_ready()
            .await
            .map_err(|e| DeploymentError::StopFailed(format!("Docker 未就绪: {e}")))?;
        let name = match resolve_bot_container_name_with_cli(&cli, bot_id).await {
            Ok(Some(n)) => n,
            Ok(None) => return Ok(()),
            Err(e) => {
                return Err(DeploymentError::StopFailed(format!("查询容器失败: {e}")));
            }
        };
        let containers = cli
            .list_containers()
            .await
            .map_err(|e| DeploymentError::StopFailed(format!("查询容器失败: {e}")))?;
        if !containers.iter().any(|c| c.name == name) {
            return Ok(());
        }
        cli.lifecycle("stop", &name).await.map_err(|e| {
            error!(
                target: "ncd_deploy::docker_bot",
                bot_id = %bot_id,
                container = %name,
                err = %e,
                "Bot Docker 停止容器失败"
            );
            DeploymentError::StopFailed(format!("停止容器失败: {e}"))
        })?;
        info!(
            target: "ncd_deploy::docker_bot",
            bot_id = %bot_id,
            container = %name,
            "Bot Docker 容器已停止"
        );
        Ok(())
    }

    async fn uninstall(&self, host: &dyn Host, config: &BotConfig) -> Result<(), DeploymentError> {
        if !self.supports(host) {
            return Err(DeploymentError::Unsupported("Docker Bot 仅支持 Linux 主机"));
        }
        let cli = DockerCli::new(host);
        let name = Self::container_name(config);
        let project_dir = Self::project_dir(host, &name)
            .await
            .map_err(|e| DeploymentError::UninstallFailed(e.to_string()))?;
        let compose_path = HostPath::from_posix(format!("{project_dir}/docker-compose.yml"));
        if !host.exists(&compose_path).await? {
            return Ok(());
        }
        cli.ensure_ready()
            .await
            .map_err(|error| DeploymentError::UninstallFailed(error.to_string()))?;
        ensure_managed_bot_container(host, config).await?;
        cli.compose_down(&project_dir, true)
            .await
            .map_err(|error| DeploymentError::UninstallFailed(error.to_string()))?;
        host.remove_dir_all(&HostPath::from_posix(&project_dir))
            .await?;
        Ok(())
    }
}

fn container_name_for(backend: BackendType, qq: impl std::fmt::Display) -> String {
    match backend {
        BackendType::SnowLuma => format!("slbot-{qq}"),
        BackendType::NapCat => format!("ncbot-{qq}"),
    }
}

/// NapCat ncbot-<qq>,SnowLuma slbot-<qq>
pub fn bot_docker_container_name(backend: BackendType, qq_id: u64) -> String {
    container_name_for(backend, qq_id)
}

/// 这个 QQ 的 bot 容器可能叫的名字,按口味当前的名字排前面
///
/// SnowLuma 旧版本也起过 ncbot-<qq>,改过口味的 bot 也会留下另一个名字的容器,
/// 按名字找容器、清旧容器的地方都得两个都认
pub fn bot_docker_container_candidates(
    backend: BackendType,
    qq: impl std::fmt::Display,
) -> [String; 2] {
    let other = match backend {
        BackendType::NapCat => BackendType::SnowLuma,
        BackendType::SnowLuma => BackendType::NapCat,
    };
    [
        container_name_for(backend, &qq),
        container_name_for(other, &qq),
    ]
}

/// 按 qq 在远端查找实际 bot 容器名。
///
/// SnowLuma Docker 容器当前命名为 `slbot-<qq>`,但旧版本曾有 `ncbot-<qq>`
/// 残留。日志、状态和停止路径都应先解析远端真实容器名,避免只按当前 flavor 拼名。
/// 查不到容器时返回 None,由调用方按自身 flavor 决定 fallback 名称。
pub async fn resolve_bot_container_name(
    host: &dyn Host,
    bot_id: &BotId,
) -> Result<Option<String>, DeploymentError> {
    let cli = DockerCli::new(host);
    cli.ensure_daemon_ready()
        .await
        .map_err(|error| DeploymentError::LaunchFailed(error.to_string()))?;
    resolve_bot_container_name_with_cli(&cli, bot_id).await
}

/// 按 qq 在远端查找实际在跑的 bot 容器名(兼容历史 ncbot 命名跑 SL 的残留)
async fn resolve_bot_container_name_with_cli(
    cli: &DockerCli<'_>,
    bot_id: &BotId,
) -> Result<Option<String>, DeploymentError> {
    // 这里不知道口味,slbot 在前:两个都在多半是 SnowLuma 留着旧名字的容器,新名字才是在用的
    let candidates = bot_docker_container_candidates(BackendType::SnowLuma, bot_id.as_str());
    let containers = cli
        .list_containers()
        .await
        .map_err(|e| DeploymentError::LaunchFailed(format!("查询容器失败: {e}")))?;
    for name in &candidates {
        if containers.iter().any(|c| c.name == *name) {
            return Ok(Some(name.clone()));
        }
    }
    Ok(None)
}

/// 容器 state 字符串 -> DeploymentState
fn map_state(state: &ncd_domain::ContainerState) -> DeploymentState {
    use ncd_domain::ContainerState;
    match state {
        ContainerState::Running => DeploymentState::Running,
        ContainerState::Created | ContainerState::Restarting => DeploymentState::Starting,
        ContainerState::Paused | ContainerState::Exited => DeploymentState::Stopped,
        ContainerState::Other => DeploymentState::Failed {
            reason: "容器处于异常状态".to_string(),
        },
    }
}

/// 找指定名字容器的短 id
async fn running_container_id(cli: &DockerCli<'_>, name: &str) -> Result<String, DeploymentError> {
    let containers = cli
        .list_containers()
        .await
        .map_err(|error| DeploymentError::LaunchFailed(error.to_string()))?;
    let container = containers
        .into_iter()
        .find(|c| c.name == name)
        .ok_or_else(|| DeploymentError::LaunchFailed(format!("启动后未找到容器 {name}")))?;
    if container.state != ncd_domain::ContainerState::Running {
        return Err(DeploymentError::LaunchFailed(format!(
            "容器 {name} 启动后状态为 {:?}，请检查容器日志",
            container.state
        )));
    }
    Ok(container.id)
}

fn now_secs() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use bytes::Bytes;
    use ncd_domain::{
        AdvancedConfig, AutoRestartSchedule, BackendType, BotBasicConfig, ConnectConfig,
        ContainerState, DeploymentType, RuntimeTarget,
    };
    use ncd_host::{
        Arch, ArchiveKind, CommandOutput, DirEntry, HostError, HostShell, Locality, Os,
    };
    use std::path::Path;
    use std::sync::{Arc, Mutex};

    struct NoopShell;

    impl HostShell for NoopShell {
        fn kind(&self) -> ncd_host::ShellKind {
            ncd_host::ShellKind::Bash
        }

        fn escape(&self, arg: &str) -> String {
            arg.to_string()
        }

        fn line_separator(&self) -> &'static str {
            "\n"
        }
    }

    #[derive(Clone)]
    struct RecordedWrite {
        path: String,
        text: String,
    }

    struct MockHost {
        home: Option<String>,
        require_elevated: bool,
        inspect_labels: String,
        retag_failure: bool,
        image_platform: &'static str,
        /// launch 幂等测试:docker ps 在 compose up 之前返回空,之后返回 running 容器
        compose_up_done: Arc<Mutex<bool>>,
        ps_json: Arc<Mutex<String>>,
        commands: Arc<Mutex<Vec<HostCommand>>>,
        writes: Arc<Mutex<Vec<RecordedWrite>>>,
        created_dirs: Arc<Mutex<Vec<String>>>,
    }

    impl MockHost {
        fn new() -> Self {
            Self {
                home: Some("/home/napcat".into()),
                require_elevated: false,
                inspect_labels: r#"{"com.docker.compose.project.working_dir":"/home/napcat/.napcat-bots/ncbot-10001"}"#.into(),
                retag_failure: false,
                image_platform: "linux/amd64",
                compose_up_done: Arc::new(Mutex::new(false)),
                ps_json: Arc::new(Mutex::new(napcat_ps_json())),
                commands: Arc::new(Mutex::new(Vec::new())),
                writes: Arc::new(Mutex::new(Vec::new())),
                created_dirs: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn with_running_container() -> Self {
            let h = Self::new();
            *h.compose_up_done.lock().unwrap() = true;
            h
        }

        fn with_running_snowluma_container() -> Self {
            let h = Self::new();
            *h.compose_up_done.lock().unwrap() = true;
            *h.ps_json.lock().unwrap() = snowluma_ps_json();
            h
        }

        fn without_home() -> Self {
            Self {
                home: None,
                ..Self::new()
            }
        }

        fn elevated_docker() -> Self {
            Self {
                require_elevated: true,
                ..Self::new()
            }
        }

        fn commands(&self) -> Vec<HostCommand> {
            self.commands.lock().unwrap().clone()
        }

        fn writes(&self) -> Vec<RecordedWrite> {
            self.writes.lock().unwrap().clone()
        }

        fn docker_commands(&self) -> Vec<HostCommand> {
            self.commands()
                .into_iter()
                .filter(|c| c.program == "docker")
                .collect()
        }
    }

    #[async_trait]
    impl Host for MockHost {
        fn os(&self) -> Os {
            Os::Linux
        }
        fn arch(&self) -> Arch {
            Arch::X86_64
        }
        fn locality(&self) -> Locality {
            Locality::Remote
        }
        fn id(&self) -> &str {
            "mock-linux"
        }
        fn shell(&self) -> &dyn HostShell {
            &NoopShell
        }
        async fn read_file(&self, _: &HostPath) -> Result<Bytes, HostError> {
            Err(HostError::Unsupported { operation: "mock" })
        }
        async fn write_file(&self, path: &HostPath, data: &[u8]) -> Result<(), HostError> {
            self.writes.lock().unwrap().push(RecordedWrite {
                path: path.as_posix().to_string(),
                text: String::from_utf8_lossy(data).into_owned(),
            });
            Ok(())
        }
        async fn list_dir(&self, _: &HostPath) -> Result<Vec<DirEntry>, HostError> {
            Err(HostError::Unsupported { operation: "mock" })
        }
        async fn create_dir_all(&self, path: &HostPath) -> Result<(), HostError> {
            self.created_dirs
                .lock()
                .unwrap()
                .push(path.as_posix().to_string());
            Ok(())
        }
        async fn remove_file(&self, _: &HostPath) -> Result<(), HostError> {
            Ok(())
        }
        async fn remove_dir_all(&self, _: &HostPath) -> Result<(), HostError> {
            Ok(())
        }
        async fn exists(&self, _: &HostPath) -> Result<bool, HostError> {
            Ok(false)
        }
        async fn upload(&self, _: &Path, _: &HostPath) -> Result<(), HostError> {
            Err(HostError::Unsupported { operation: "mock" })
        }
        async fn download(&self, _: &HostPath, _: &Path) -> Result<(), HostError> {
            Err(HostError::Unsupported { operation: "mock" })
        }
        async fn extract_archive(
            &self,
            _: &HostPath,
            _: &HostPath,
            _: ArchiveKind,
        ) -> Result<(), HostError> {
            Err(HostError::Unsupported { operation: "mock" })
        }
        async fn spawn(&self, _: HostCommand) -> Result<Box<dyn ncd_host::HostProcess>, HostError> {
            Err(HostError::Unsupported { operation: "mock" })
        }
        async fn run_to_string(&self, cmd: HostCommand) -> Result<CommandOutput, HostError> {
            self.commands.lock().unwrap().push(cmd.clone());
            if cmd.program == "sh" && cmd.args == ["-c", "echo $HOME"] {
                return Ok(output(0, self.home.clone().unwrap_or_default(), ""));
            }
            if cmd.program != "docker" {
                return Ok(output(0, "", ""));
            }
            if self.require_elevated
                && !cmd.elevated
                && cmd.args.first().map(String::as_str) != Some("version")
            {
                return Ok(output(
                    1,
                    "",
                    "permission denied connecting to /var/run/docker.sock",
                ));
            }
            match cmd.args.first().map(String::as_str) {
                Some("version") => Ok(output(0, "27.3.1\n", "")),
                Some("info") if cmd.args.iter().any(|arg| arg.contains("OSType")) => {
                    Ok(output(0, "linux/x86_64", ""))
                }
                Some("info") => Ok(output(0, "27.3.1\n", "")),
                Some("image") => Ok(output(0, self.image_platform, "")),
                Some("compose") => {
                    if cmd.args.get(1).map(String::as_str) == Some("up") {
                        *self.compose_up_done.lock().unwrap() = true;
                    }
                    Ok(output(0, "ok\n", ""))
                }
                Some("pull") => Ok(output(0, "layer: Pull complete\n", "")),
                Some("tag") if self.retag_failure => Ok(output(1, "", "retag denied")),
                Some("tag") => Ok(output(0, "", "")),
                Some("ps") => {
                    let show = *self.compose_up_done.lock().unwrap();
                    if show {
                        Ok(output(0, self.ps_json.lock().unwrap().clone(), ""))
                    } else {
                        Ok(output(0, "", ""))
                    }
                }
                Some("rm") => Ok(output(0, "", "")),
                Some("stop") => Ok(output(0, "ncbot-10001\n", "")),
                Some("exec") => Ok(output(0, r#"{"token":"from-container"}"#, "")),
                Some("inspect") => Ok(output(0, &self.inspect_labels, "")),
                _ => Ok(output(0, "", "")),
            }
        }
    }

    fn output(
        exit_code: i32,
        stdout: impl Into<String>,
        stderr: impl Into<String>,
    ) -> CommandOutput {
        CommandOutput {
            exit_code: Some(exit_code),
            stdout: stdout.into(),
            stderr: stderr.into(),
        }
    }

    fn napcat_ps_json() -> String {
        r#"{"ID":"abc123","Names":"ncbot-10001","Image":"mlikiowa/napcat-docker:latest","State":"running","Status":"Up","Ports":"0.0.0.0:6099->6099/tcp"}"#.to_string()
    }

    fn snowluma_ps_json() -> String {
        r#"{"ID":"def456","Names":"slbot-10001","Image":"motricseven7/snowluma:latest","State":"running","Status":"Up","Ports":"0.0.0.0:5099->5099/tcp"}"#.to_string()
    }

    fn bot_config() -> BotConfig {
        BotConfig {
            bot: BotBasicConfig {
                name: "t".into(),
                qq_id: 10001,
                music_sign_url: String::new(),
                auto_restart_schedule: AutoRestartSchedule::default(),
                offline_auto_restart: false,
                runtime_target: RuntimeTarget::Local,
                backend_type: BackendType::NapCat,
                deployment_type: DeploymentType::Docker,
                snowluma_start_mode: None,
                webui_password_takeover: false,
            },
            connect: ConnectConfig::default(),
            advanced: AdvancedConfig::default(),
            docker: None,
            status_command: None,
        }
    }

    #[test]
    fn docker_deployment_id_is_stable() {
        assert_eq!(DockerDeployment::new().id(), "docker");
    }

    #[test]
    fn supports_napcat_and_snowluma_flavors() {
        let dep = DockerDeployment::new();
        assert_eq!(
            dep.supported_flavors(),
            &[BotFlavor::NapCat, BotFlavor::SnowLuma]
        );
    }

    #[test]
    fn container_name_uses_qq_prefix() {
        let config = bot_config();
        assert_eq!(DockerDeployment::container_name(&config), "ncbot-10001");
        let spec = DockerDeployment::build_spec(&config);
        assert_eq!(spec.container_name, "ncbot-10001");
        assert_eq!(spec.qq_id, Some(10001));

        let mut sl_config = bot_config();
        sl_config.bot.backend_type = BackendType::SnowLuma;
        assert_eq!(DockerDeployment::container_name(&sl_config), "slbot-10001");
        assert_eq!(
            DockerDeployment::build_spec(&sl_config).container_name,
            "slbot-10001"
        );
    }

    #[test]
    fn container_candidates_put_current_flavor_first() {
        assert_eq!(
            bot_docker_container_candidates(BackendType::NapCat, 10001u64),
            ["ncbot-10001", "slbot-10001"]
        );
        assert_eq!(
            bot_docker_container_candidates(BackendType::SnowLuma, "10001"),
            ["slbot-10001", "ncbot-10001"]
        );
    }

    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn local_project_directory_follows_managed_data_root() {
        let host = ncd_host::local::LocalLinuxHost::new()
            .with_managed_root(Path::new("/tmp/NCD relocated data"));
        assert_eq!(
            docker_project_dir(&host, "slbot-10001").await.unwrap(),
            "/tmp/NCD relocated data/docker/slbot-10001"
        );
    }

    #[tokio::test]
    async fn mirror_pull_cannot_report_success_when_official_tag_fails() {
        let host = MockHost {
            retag_failure: true,
            ..MockHost::new()
        };
        let cli = DockerCli::new(&host);
        let result = cli
            .pull_with_fallback_cancel(
                &["mirror.example/napcat:latest".to_string()],
                "mlikiowa/napcat-docker:latest",
                None,
                |_, _| |_, _| {},
                None::<fn(usize, &str, &crate::docker::DockerCliError)>,
            )
            .await;
        assert!(result.unwrap_err().to_string().contains("retag denied"));
    }

    #[tokio::test]
    async fn install_rejects_image_from_different_daemon_architecture() {
        let host = MockHost {
            image_platform: "linux/arm64",
            ..MockHost::new()
        };
        let result = DockerDeployment::with_webui_token("test")
            .install(&host, &bot_config(), &crate::deployment::NullProgressSink)
            .await;
        assert!(result.unwrap_err().to_string().contains("不兼容"));
        assert!(
            !host
                .commands()
                .iter()
                .any(|cmd| cmd.args.get(1).map(String::as_str) == Some("up"))
        );
    }

    #[tokio::test]
    async fn compose_success_does_not_hide_container_immediate_exit() {
        let host = MockHost::new();
        *host.ps_json.lock().unwrap() = napcat_ps_json().replace("\"running\"", "\"exited\"");
        let result = DockerDeployment::new().launch(&host, &bot_config()).await;
        assert!(result.unwrap_err().to_string().contains("Exited"));
    }

    #[tokio::test]
    async fn managed_container_check_allows_absent_and_owned_containers() {
        ensure_managed_bot_container(&MockHost::new(), &bot_config())
            .await
            .unwrap();
        ensure_managed_bot_container(&MockHost::with_running_container(), &bot_config())
            .await
            .unwrap();
        let mut host = MockHost::with_running_snowluma_container();
        host.inspect_labels =
            r#"{"com.docker.compose.project.working_dir":"/home/napcat/.napcat-bots/slbot-10001"}"#
                .into();
        let mut config = bot_config();
        config.bot.backend_type = BackendType::SnowLuma;
        ensure_managed_bot_container(&host, &config).await.unwrap();
    }

    #[tokio::test]
    async fn managed_container_check_rejects_external_compose_or_missing_labels() {
        for labels in [
            "null",
            "{}",
            r#"{"com.docker.compose.project.working_dir":"/srv/external"}"#,
        ] {
            let mut host = MockHost::with_running_container();
            host.inspect_labels = labels.into();
            let error = ensure_managed_bot_container(&host, &bot_config())
                .await
                .unwrap_err();
            assert!(error.to_string().contains("外部 Compose"));
            assert!(host.writes().is_empty());
            assert!(
                !host
                    .docker_commands()
                    .iter()
                    .any(|cmd| matches!(cmd.args.first().map(String::as_str), Some("stop" | "rm")))
            );
        }
    }

    #[tokio::test]
    async fn copy_from_container_uses_arg_separated_docker_cp() {
        let host = MockHost::new();
        let cli = DockerCli::new(&host);
        cli.copy_from_container(
            "slbot-10001",
            "/app/snowluma-data/config/onebot_10001.json",
            "/tmp/ncd-import-slbot-10001-10001.json",
        )
        .await
        .unwrap();
        let cp = host
            .docker_commands()
            .into_iter()
            .find(|c| c.args.first().map(String::as_str) == Some("cp"))
            .expect("docker cp");
        assert_eq!(
            cp.args,
            [
                "cp",
                "slbot-10001:/app/snowluma-data/config/onebot_10001.json",
                "/tmp/ncd-import-slbot-10001-10001.json",
            ]
        );
        assert!(
            !host.commands().iter().any(|c| c.program == "chmod"),
            "未提权时不必 chmod"
        );
    }

    #[tokio::test]
    async fn exec_read_file_uses_arg_separated_docker_exec() {
        let host = MockHost::new();
        let cli = DockerCli::new(&host);
        let text = cli
            .exec_read_file("ncbot-10001", "/app/napcat/config/webui.json")
            .await
            .unwrap();
        assert_eq!(text, r#"{"token":"from-container"}"#);
        let exec = host
            .docker_commands()
            .into_iter()
            .find(|c| c.args.first().map(String::as_str) == Some("exec"))
            .expect("docker exec");
        assert_eq!(
            exec.args,
            [
                "exec",
                "ncbot-10001",
                "cat",
                "/app/napcat/config/webui.json"
            ]
        );
        assert!(!exec.elevated);
    }

    #[tokio::test]
    async fn exec_read_file_is_elevated_when_docker_needs_sudo() {
        let host = MockHost::elevated_docker();
        let cli = DockerCli::new(&host);
        cli.ensure_daemon_ready().await.unwrap();
        let text = cli
            .exec_read_file("ncbot-10001", "/app/napcat/config/webui.json")
            .await
            .unwrap();
        assert!(text.contains("from-container"));
        let exec = host
            .docker_commands()
            .into_iter()
            .find(|c| c.args.first().map(String::as_str) == Some("exec"))
            .expect("docker exec");
        assert!(exec.elevated, "探测判定要 sudo 后 exec 也要 sudo");
    }

    #[tokio::test]
    async fn copy_from_container_chmods_when_docker_needs_sudo() {
        let host = MockHost::elevated_docker();
        let cli = DockerCli::new(&host);
        cli.ensure_daemon_ready().await.unwrap();
        cli.copy_from_container(
            "slbot-10001",
            "/app/snowluma-data/config/onebot_10001.json",
            "/tmp/ncd-import-slbot-10001-10001.json",
        )
        .await
        .unwrap();
        let chmod = host
            .commands()
            .into_iter()
            .find(|c| c.program == "chmod")
            .expect("sudo docker cp 之后要 chmod 644，否则 SFTP 读不到 root 文件");
        assert!(chmod.elevated);
        assert_eq!(
            chmod.args,
            ["644", "/tmp/ncd-import-slbot-10001-10001.json"]
        );
    }

    #[tokio::test]
    async fn resolve_container_name_prefers_current_snowluma_name() {
        let host = MockHost::with_running_snowluma_container();
        let name = resolve_bot_container_name(&host, &BotId::from("10001".to_string()))
            .await
            .unwrap();
        assert_eq!(name.as_deref(), Some("slbot-10001"));
    }

    #[tokio::test]
    async fn resolve_container_name_keeps_legacy_ncbot_when_that_is_running() {
        let host = MockHost::with_running_container();
        let name = resolve_bot_container_name(&host, &BotId::from("10001".to_string()))
            .await
            .unwrap();
        assert_eq!(name.as_deref(), Some("ncbot-10001"));
    }

    #[tokio::test]
    async fn resolve_container_name_returns_none_when_container_is_absent() {
        let host = MockHost::new();
        let name = resolve_bot_container_name(&host, &BotId::from("10001".to_string()))
            .await
            .unwrap();
        assert_eq!(name, None);
    }

    #[test]
    fn map_state_keeps_running_starting_stopped_failed() {
        assert_eq!(
            map_state(&ContainerState::Running),
            DeploymentState::Running
        );
        assert_eq!(map_state(&ContainerState::Exited), DeploymentState::Stopped);
        assert_eq!(
            map_state(&ContainerState::Created),
            DeploymentState::Starting
        );
        assert_eq!(
            map_state(&ContainerState::Restarting),
            DeploymentState::Starting
        );
        assert!(matches!(
            map_state(&ContainerState::Other),
            DeploymentState::Failed { .. }
        ));
    }

    #[tokio::test]
    async fn install_applies_custom_container_settings_for_both_backends() {
        for backend in [BackendType::NapCat, BackendType::SnowLuma] {
            let host = MockHost::new();
            let dep = match backend {
                BackendType::NapCat => DockerDeployment::with_webui_token("test-token"),
                BackendType::SnowLuma => {
                    DockerDeployment::with_sl_secrets("test-vnc", "test-webui")
                }
            };
            let mut config = bot_config();
            config.bot.backend_type = backend;
            config.docker = Some(ncd_domain::BotDockerConfig {
                port_mappings: vec![ncd_domain::PortMapping::new(8500, 4500)],
                environment: [("TZ".into(), "Asia/Shanghai".into())].into(),
                memory_mb: Some(256),
                ..Default::default()
            });
            dep.install(&host, &config, &crate::deployment::NullProgressSink)
                .await
                .unwrap();
            let writes = host.writes();
            let compose = writes
                .iter()
                .find(|w| w.path.ends_with("/docker-compose.yml"))
                .unwrap();
            let yaml: serde_yaml::Value = serde_yaml::from_str(&compose.text).unwrap();
            let key = match backend {
                BackendType::NapCat => "napcat",
                BackendType::SnowLuma => "snowluma",
            };
            let service = &yaml["services"][key];
            assert!(
                service["ports"]
                    .as_sequence()
                    .unwrap()
                    .iter()
                    .any(|p| p.as_str() == Some("127.0.0.1:8500:4500"))
            );
            assert_eq!(service["environment"]["TZ"], "Asia/Shanghai");
            assert_eq!(service["mem_limit"], "256m");
            assert_eq!(service["restart"], "unless-stopped");
        }
    }

    #[tokio::test]
    async fn install_rejects_conflicting_ports_before_writing_any_files() {
        let host = MockHost::new();
        let mut config = bot_config();
        config.docker = Some(ncd_domain::BotDockerConfig {
            port_mappings: vec![ncd_domain::PortMapping::new(3001, 4500)],
            ..Default::default()
        });
        let result = DockerDeployment::with_webui_token("test-token")
            .install(&host, &config, &crate::deployment::NullProgressSink)
            .await;
        assert!(matches!(result, Err(DeploymentError::ConfigInvalid(_))));
        assert!(host.writes().is_empty());
        assert!(host.commands().is_empty());
    }

    #[tokio::test]
    async fn install_launch_observe_records_commands_compose_ports_volumes_and_token() {
        let host = MockHost::new();
        let dep = DockerDeployment::with_webui_token("formal-token-9x");
        let config = bot_config();
        dep.install(&host, &config, &crate::deployment::NullProgressSink)
            .await
            .unwrap();
        let handle = dep.launch(&host, &config).await.unwrap();
        assert!(matches!(handle, DeploymentHandle::Docker { .. }));
        assert_eq!(
            dep.observe(&host, &BotId::from("10001".to_string()))
                .await
                .unwrap(),
            DeploymentState::Running
        );

        let writes = host.writes();
        let env = writes.iter().find(|w| w.path.ends_with("/.env")).unwrap();
        assert!(env.text.contains("WEBUI_TOKEN=formal-token-9x"));
        assert!(!env.text.contains("ncbot10001"));
        let compose = writes
            .iter()
            .find(|w| w.path.ends_with("/docker-compose.yml"))
            .unwrap();
        let compose_yaml: serde_yaml::Value = serde_yaml::from_str(&compose.text).unwrap();
        assert_eq!(
            compose_yaml["services"]["napcat"]["environment"]["WEBUI_TOKEN"],
            "${WEBUI_TOKEN:?WEBUI_TOKEN is required}"
        );
        // qq=10001 经 host_port_offset_for_qq 偏移 1,host 端口 6099→6100
        assert!(
            compose_yaml["services"]["napcat"]["ports"]
                .as_sequence()
                .unwrap()
                .iter()
                .any(|p| p.as_str() == Some("127.0.0.1:6100:6099"))
        );
        assert!(compose.text.contains("./napcat/config:/app/napcat/config"));
        assert!(compose.text.contains("./ntqq:/app/.config/QQ"));

        let docker_args: Vec<Vec<String>> =
            host.docker_commands().into_iter().map(|c| c.args).collect();
        assert!(
            docker_args
                .iter()
                .any(|a| a == &["image", "inspect", "mlikiowa/napcat-docker:latest"])
        );
        assert!(
            docker_args
                .iter()
                .any(|a| a == &["compose", "up", "-d", "--remove-orphans", "--pull", "never"])
        );
        assert!(
            docker_args
                .iter()
                .any(|a| a == &["ps", "-a", "--no-trunc", "--format", "{{json .}}"])
        );
    }

    #[tokio::test]
    async fn install_hard_fails_when_home_probe_fails() {
        let host = MockHost::without_home();
        let dep = DockerDeployment::with_webui_token("formal-token-9x");
        let err = dep
            .install(&host, &bot_config(), &crate::deployment::NullProgressSink)
            .await
            .unwrap_err();
        assert!(matches!(err, DeploymentError::ConfigInvalid(_)));
        assert!(host.writes().is_empty());
    }

    #[tokio::test]
    async fn production_install_requires_explicit_secret() {
        let host = MockHost::new();
        let err = DockerDeployment::new()
            .install(&host, &bot_config(), &crate::deployment::NullProgressSink)
            .await
            .unwrap_err();
        assert!(matches!(err, DeploymentError::ConfigInvalid(_)));
        assert!(host.writes().is_empty());
    }

    #[tokio::test]
    async fn launch_when_container_already_running_skips_compose_up_and_remove() {
        let host = MockHost::with_running_container();
        let dep = DockerDeployment::with_test_default_token();
        let config = bot_config();
        dep.install(&host, &config, &crate::deployment::NullProgressSink)
            .await
            .unwrap();
        dep.launch(&host, &config).await.unwrap();

        let docker_args: Vec<Vec<String>> =
            host.docker_commands().into_iter().map(|c| c.args).collect();
        assert!(
            !docker_args
                .iter()
                .any(|a| a.first().map(String::as_str) == Some("rm")),
            "Running 时 launch 不应 docker rm: {docker_args:?}"
        );
        assert!(
            !docker_args
                .iter()
                .any(|a| a == &["compose", "up", "-d", "--pull", "never"]),
            "Running 时 launch 不应 compose up: {docker_args:?}"
        );
        assert!(
            docker_args
                .iter()
                .any(|a| a == &["ps", "-a", "--no-trunc", "--format", "{{json .}}"]),
            "observe 应至少一次 docker ps: {docker_args:?}"
        );
    }

    #[tokio::test]
    async fn elevated_docker_decision_is_recomputed_for_each_operation() {
        let host = MockHost::elevated_docker();
        let dep = DockerDeployment::with_webui_token("formal-token-9x");
        let config = bot_config();
        dep.install(&host, &config, &crate::deployment::NullProgressSink)
            .await
            .unwrap();
        dep.launch(&host, &config).await.unwrap();
        dep.stop(&host, &BotId::from("10001".to_string()), StopMode::Graceful)
            .await
            .unwrap();

        let docker = host.docker_commands();
        // 普通用户的 socket 权限失败后才提权。
        assert!(
            docker
                .iter()
                .any(|c| c.args.first().map(String::as_str) == Some("info") && !c.elevated)
        );
        assert!(
            docker
                .iter()
                .any(|c| c.args.first().map(String::as_str) == Some("info") && c.elevated)
        );
        assert!(
            docker
                .iter()
                .any(|c| c.args.first().map(String::as_str) == Some("image")
                    && c.args.get(1).map(String::as_str) == Some("inspect")
                    && c.elevated)
        );
        assert!(docker.iter().any(|c| c.args
            == ["compose", "up", "-d", "--remove-orphans", "--pull", "never"]
            && c.elevated));
        assert!(
            docker
                .iter()
                .any(|c| c.args == ["stop", "--", "ncbot-10001"] && c.elevated)
        );
    }

    #[test]
    fn test_only_default_secret_is_not_production_default() {
        assert!(
            DockerDeployment::new()
                .compose_secret_for(BackendType::NapCat)
                .is_err()
        );
        assert_eq!(
            DockerDeployment::with_test_default_token()
                .compose_secret_for(BackendType::NapCat)
                .unwrap(),
            "test-webui-token"
        );
        assert_eq!(
            DockerDeployment::with_test_default_token()
                .compose_secret_for(BackendType::SnowLuma)
                .unwrap(),
            "test-vnc-passwd"
        );
    }
}
