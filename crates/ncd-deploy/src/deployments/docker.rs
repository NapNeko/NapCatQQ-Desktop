//! Docker 部署：用 docker compose 在某台 Host（本机 Docker Desktop / 远端 SSH）
//! 上把 bot 跑成容器。
//!
//! 与组件页「拉镜像」(commands/docker.rs docker_deploy)的关系:组件页只预拉
//! NapCat/SnowLuma 官方镜像,不创建 napcat/snowluma 演示容器;这里是 bot 生命周期
//! 部署形态(deployment_type=docker),由 BotManager 在 start_bot 时按 bot 配置驱动,
//! 容器纳入 bot 状态机。两者共用底层 DockerCli + compose 渲染,但入口和归属不同。
//!
//! 容器命名:`ncbot-<qq>` / `slbot-<qq>`（按口味），区别于组件页 `napcat`/`snowluma` 演示容器。
//! compose 项目目录:远端 $HOME/.napcat-bots/<name>。探不到 HOME 直接失败,
//! 避免把生产数据静默落到 /tmp。
//!
//! 镜像须在「组件」页预拉；Bot 启动 install 只检查本地是否存在官方镜像,不现场 pull。
//!
//! NapCat / SnowLuma 均走官方 compose 语义(见 docker/compose.rs 与 SnowLuma.Docker.Framework)。
//! NapCat 可预写 onebot/napcat 到 bind 目录; SnowLuma 写 onebot_<qq>.json 到 named volume。
//! 容器命名：NapCat `ncbot-<qq>`，SnowLuma `slbot-<qq>`。
//! compose 项目目录:远端 $HOME/.napcat-bots/<name>。

use async_trait::async_trait;
use ncd_domain::{BackendType, BotConfig, BotFlavor, BotId, DockerDeploySpec, DockerFlavor, StopMode};
use ncd_host::{Host, HostCommand, HostPath};
use tracing::{error, info};

use crate::deployment::{
    Deployment, DeploymentError, DeploymentHandle, DeploymentProgressSink, DeploymentState,
};
use crate::docker::{
    DockerCli,
    compose::{render_compose_with_env, render_snowluma_compose_with_env},
};

/// Docker 部署实装。
pub struct DockerDeployment {
    id: &'static str,
    flavors: &'static [BotFlavor],
    /// NapCat: WEBUI_TOKEN; SnowLuma: VNC_PASSWD(compose 环境变量)。
    compose_secret: Option<String>,
    /// SnowLuma: 写入 SNOWLUMA_WEBUI_BOOTSTRAP_PASSWORD（WebUI 登录，非 VNC）。
    sl_webui_bootstrap: Option<String>,
    allow_test_default_token: bool,
}

impl DockerDeployment {
    pub fn new() -> Self {
        Self {
            id: "docker",
            flavors: &[BotFlavor::NapCat, BotFlavor::SnowLuma],
            compose_secret: None,
            sl_webui_bootstrap: None,
            allow_test_default_token: false,
        }
    }

    /// NapCat Docker: WEBUI_TOKEN。ncd-deploy 不从 QQ 号派生凭据。
    pub fn with_webui_token(token: impl Into<String>) -> Self {
        Self {
            compose_secret: Some(token.into()),
            ..Self::new()
        }
    }

    /// SnowLuma Docker: VNC_PASSWD + SNOWLUMA_WEBUI_BOOTSTRAP_PASSWORD。
    pub fn with_sl_secrets(vnc_passwd: impl Into<String>, webui_bootstrap: impl Into<String>) -> Self {
        Self {
            compose_secret: Some(vnc_passwd.into()),
            sl_webui_bootstrap: Some(webui_bootstrap.into()),
            ..Self::new()
        }
    }

    /// SnowLuma Docker: VNC_PASSWD(noVNC 入口密码)。
    pub fn with_vnc_passwd(passwd: impl Into<String>) -> Self {
        Self {
            compose_secret: Some(passwd.into()),
            ..Self::new()
        }
    }

    #[cfg(test)]
    fn with_test_default_token() -> Self {
        Self {
            allow_test_default_token: true,
            ..Self::new()
        }
    }

    /// bot 容器名：NapCat `ncbot-<qq>`，SnowLuma `slbot-<qq>`。
    pub fn container_name(config: &BotConfig) -> String {
        bot_docker_container_name(config.bot.backend_type, config.bot.qq_id)
    }

    /// compose 项目目录(host 侧 POSIX 路径)。远端 HOME 探测失败时 hard fail。
    async fn project_dir(host: &dyn Host, name: &str) -> Result<String, DeploymentError> {
        let home = probe_home(host).await.ok_or_else(|| {
            DeploymentError::ConfigInvalid(
                "无法探测远端 HOME，拒绝回退到临时目录部署 Docker bot".into(),
            )
        })?;
        Ok(format!("{home}/.napcat-bots/{name}"))
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
            BackendType::SnowLuma => {
                "DockerDeployment 需要上层显式传入 VNC 密码 (SnowLuma)".into()
            }
            BackendType::NapCat => "DockerDeployment 需要上层显式传入 WebUI token".into(),
        }))
    }

    fn compose_env_var(backend: BackendType) -> &'static str {
        match backend {
            BackendType::SnowLuma => "VNC_PASSWD",
            BackendType::NapCat => "WEBUI_TOKEN",
        }
    }

    /// 从 BotConfig 构造 DockerDeploySpec:容器名按口味,端口默认 + per-qq 宿主机偏移。
    pub fn build_spec(config: &BotConfig) -> DockerDeploySpec {
        let qq = config.bot.qq_id;
        let mut spec = match config.bot.backend_type {
            BackendType::SnowLuma => DockerDeploySpec::snowluma_default(),
            BackendType::NapCat => DockerDeploySpec::napcat_default(),
        }
        .with_host_port_offset(qq);
        spec.container_name = Self::container_name(config);
        if config.bot.backend_type == BackendType::NapCat && qq != 0 {
            spec.qq_id = Some(qq);
        }
        spec
    }
}

impl Default for DockerDeployment {
    fn default() -> Self {
        Self::new()
    }
}

/// docker compose `.env` 行：含空格、`#`、`:` 等时加双引号并转义。
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

/// 远端探 $HOME。失败返回 None。
async fn probe_home(host: &dyn Host) -> Option<String> {
    let cmd = HostCommand::new("sh").arg("-c").arg("echo $HOME");
    match host.run_to_string(cmd).await {
        Ok(out) if out.success() => {
            let h = out.stdout.trim().to_string();
            if h.is_empty() { None } else { Some(h) }
        }
        _ => None,
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
        // 仅 Linux 主机(一般是远端 SSH)。本机 Windows 不支持 Docker bot 部署:
        // Docker Desktop 安装链路太麻烦,产品上不在本机做容器化。daemon 是否真在
        // 跑留给 install 阶段动态探测。
        use ncd_host::Os;
        matches!(host.os(), Os::Linux)
    }

    async fn install(
        &self,
        host: &dyn Host,
        config: &BotConfig,
        progress: &dyn DeploymentProgressSink,
    ) -> Result<(), DeploymentError> {
        let backend = config.bot.backend_type;
        if !matches!(backend, BackendType::NapCat | BackendType::SnowLuma) {
            return Err(DeploymentError::UnsupportedFlavor {
                flavor: format!("{:?} docker 部署暂未实装", backend),
            });
        }

        let cli = DockerCli::new(host);
        progress.report("docker", "探测 Docker 状态", 5);
        cli.ensure_ready()
            .await
            .map_err(|e| {
                error!(
                    target: "ncd_deploy::docker_bot",
                    qq_id = config.bot.qq_id,
                    err = %e,
                    "Bot Docker 部署: Docker 未就绪"
                );
                DeploymentError::RuntimeUnavailable { kind: "docker" }
            })?;

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

        let (uid, gid) = default_uid_gid(host);
        let yaml = if backend == BackendType::SnowLuma {
            render_snowluma_compose_with_env(
                &spec,
                "VNC_PASSWD",
                "SNOWLUMA_WEBUI_BOOTSTRAP_PASSWORD",
                uid,
                gid,
            )
        } else {
            render_compose_with_env(&spec, env_var, uid, gid)
        };
        let compose_path = HostPath::from_posix(format!("{project_dir}/docker-compose.yml"));
        host.write_file(&compose_path, yaml.as_bytes())
            .await
            .map_err(|e| DeploymentError::InstallFailed(format!("写 compose 文件失败: {e}")))?;

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
        let cli = DockerCli::new(host);
        cli.ensure_ready()
            .await
            .map_err(|e| {
                error!(
                    target: "ncd_deploy::docker_bot",
                    qq_id = config.bot.qq_id,
                    err = %e,
                    "Bot Docker 部署: Docker 未就绪"
                );
                DeploymentError::RuntimeUnavailable { kind: "docker" }
            })?;
        let name = Self::container_name(config);
        let bot_id = BotId::new(config.bot.qq_id.to_string());

        if matches!(
            self.observe(host, &bot_id).await?,
            DeploymentState::Running
        ) {
            let started_at = now_secs();
            let container_id = find_container_id(&cli, &name).await.unwrap_or_default();
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

        // 上次失败/改口味可能留下同名容器；compose orphan 也可能是旧服务名。
        let _ = cli.remove(&name).await;
        let legacy_nc = format!("ncbot-{}", config.bot.qq_id);
        let legacy_sl = format!("slbot-{}", config.bot.qq_id);
        if legacy_nc != name {
            let _ = cli.remove(&legacy_nc).await;
        }
        if legacy_sl != name {
            let _ = cli.remove(&legacy_sl).await;
        }

        cli.compose_up(&project_dir)
            .await
            .map_err(|e| {
                error!(
                    target: "ncd_deploy::docker_bot",
                    qq_id = config.bot.qq_id,
                    container = %name,
                    err = %e,
                    "Bot Docker 部署: 启动容器失败"
                );
                DeploymentError::LaunchFailed(format!("启动容器失败: {e}"))
            })?;

        // 回读容器 id + 启动时间。找不到也不致命:容器已起,observe 后续能纠正。
        let started_at = now_secs();
        let container_id = find_container_id(&cli, &name).await.unwrap_or_default();
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
        if let Err(e) = cli.ensure_ready().await {
            return Ok(DeploymentState::Failed {
                reason: format!("Docker 未就绪: {e}"),
            });
        }
        let name = resolve_bot_container_name(&cli, bot_id).await.unwrap_or_else(|_| {
            format!("ncbot-{}", bot_id.as_str())
        });
        // 归到 Failed 状态(带原因)让上层显示,而不是抛错中断轮询。
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
        let cli = DockerCli::new(host);
        cli.ensure_ready()
            .await
            .map_err(|e| DeploymentError::StopFailed(format!("Docker 未就绪: {e}")))?;
        let name = match resolve_bot_container_name(&cli, bot_id).await {
            Ok(n) => n,
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
        cli.lifecycle("stop", &name)
            .await
            .map_err(|e| {
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
        let cli = DockerCli::new(host);
        let name = Self::container_name(config);
        let project_dir = Self::project_dir(host, &name)
            .await
            .map_err(|e| DeploymentError::UninstallFailed(e.to_string()))?;
        // compose down -v 清容器 + 卷。目录不存在时 down 会报错,忽略(幂等)。
        let _ = cli.compose_down(&project_dir, true).await;
        let _ = host
            .remove_dir_all(&HostPath::from_posix(&project_dir))
            .await;
        Ok(())
    }
}

/// NapCat `ncbot-<qq>`，SnowLuma `slbot-<qq>`。
pub fn bot_docker_container_name(backend: BackendType, qq_id: u64) -> String {
    match backend {
        BackendType::SnowLuma => format!("slbot-{qq_id}"),
        BackendType::NapCat => format!("ncbot-{qq_id}"),
    }
}

/// 按 qq 在远端查找实际在跑的 bot 容器名（兼容历史 ncbot 命名跑 SL 的残留）。
async fn resolve_bot_container_name(
    cli: &DockerCli<'_>,
    bot_id: &BotId,
) -> Result<String, DeploymentError> {
    let qq = bot_id.as_str();
    let candidates = [format!("slbot-{qq}"), format!("ncbot-{qq}")];
    let containers = cli
        .list_containers()
        .await
        .map_err(|e| DeploymentError::LaunchFailed(format!("查询容器失败: {e}")))?;
    for name in &candidates {
        if containers.iter().any(|c| c.name == *name) {
            return Ok(name.clone());
        }
    }
    Ok(candidates[0].clone())
}

/// 容器 state 字符串 -> DeploymentState。
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

/// 找指定名字容器的短 id。
async fn find_container_id(cli: &DockerCli<'_>, name: &str) -> Option<String> {
    let containers = cli.list_containers().await.ok()?;
    containers
        .into_iter()
        .find(|c| c.name == name)
        .map(|c| c.id)
}

/// 默认文件属主。远端 Linux 普通用户一般 1000;本机 Windows 不在意给 0。
fn default_uid_gid(host: &dyn Host) -> (u32, u32) {
    match host.os() {
        ncd_host::Os::Linux => (1000, 1000),
        _ => (0, 0),
    }
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
        PackageManager,
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
        /// launch 幂等测试：`docker ps` 在 compose up 之前返回空，之后返回 running 容器。
        compose_up_done: Arc<Mutex<bool>>,
        commands: Arc<Mutex<Vec<HostCommand>>>,
        writes: Arc<Mutex<Vec<RecordedWrite>>>,
        created_dirs: Arc<Mutex<Vec<String>>>,
    }

    impl MockHost {
        fn new() -> Self {
            Self {
                home: Some("/home/napcat".into()),
                require_elevated: false,
                compose_up_done: Arc::new(Mutex::new(false)),
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
        fn pkg_manager(&self) -> Option<&dyn PackageManager> {
            None
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
                return Ok(output(1, "", "permission denied"));
            }
            match cmd.args.first().map(String::as_str) {
                Some("version") => Ok(output(0, "27.3.1\n", "")),
                Some("info") => Ok(output(0, "27.3.1\n", "")),
                Some("compose") => {
                    if cmd.args.get(1).map(String::as_str) == Some("up") {
                        *self.compose_up_done.lock().unwrap() = true;
                    }
                    Ok(output(0, "ok\n", ""))
                }
                Some("pull") => Ok(output(0, "layer: Pull complete\n", "")),
                Some("tag") => Ok(output(0, "", "")),
                Some("ps") => {
                    let show = *self.compose_up_done.lock().unwrap();
                    if show {
                        Ok(output(0, ps_json(), ""))
                    } else {
                        Ok(output(0, "", ""))
                    }
                }
                Some("rm") => Ok(output(0, "", "")),
                Some("stop") => Ok(output(0, "ncbot-10001\n", "")),
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

    fn ps_json() -> String {
        r#"{"ID":"abc123","Names":"ncbot-10001","Image":"mlikiowa/napcat-docker:latest","State":"running","Status":"Up","Ports":"0.0.0.0:6099->6099/tcp"}"#.to_string()
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
            },
            connect: ConnectConfig::default(),
            advanced: AdvancedConfig::default(),
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
        assert!(
            compose
                .text
                .contains("WEBUI_TOKEN: \"${WEBUI_TOKEN:?WEBUI_TOKEN is required}\"")
        );
        assert!(compose.text.contains("\"6099:6099\""));
        assert!(compose.text.contains("./napcat/config:/app/napcat/config"));
        assert!(compose.text.contains("./ntqq:/app/.config/QQ"));

        let docker_args: Vec<Vec<String>> =
            host.docker_commands().into_iter().map(|c| c.args).collect();
        assert!(
            docker_args
                .iter()
                .any(|a| a == &["pull", "docker.1ms.run/mlikiowa/napcat-docker:latest"])
        );
        assert!(
            docker_args
                .iter()
                .any(|a| a == &["compose", "up", "-d", "--pull", "missing"])
        );
        assert!(
            docker_args
                .iter()
                .any(|a| a == &["ps", "-a", "--format", "{{json .}}"])
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
            !docker_args.iter().any(|a| a.first().map(String::as_str) == Some("rm")),
            "Running 时 launch 不应 docker rm: {docker_args:?}"
        );
        assert!(
            !docker_args
                .iter()
                .any(|a| a == &["compose", "up", "-d", "--pull", "missing"]),
            "Running 时 launch 不应 compose up: {docker_args:?}"
        );
        assert!(
            docker_args
                .iter()
                .any(|a| a == &["ps", "-a", "--format", "{{json .}}"]),
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
                .any(|c| c.args.first().map(String::as_str) == Some("pull") && c.elevated)
        );
        assert!(
            docker
                .iter()
                .any(|c| c.args == ["compose", "up", "-d", "--pull", "missing"] && c.elevated)
        );
        assert!(
            docker
                .iter()
                .any(|c| c.args == ["stop", "ncbot-10001"] && c.elevated)
        );
    }

    #[test]
    fn test_only_default_secret_is_not_production_default() {
        assert!(DockerDeployment::new().compose_secret_for(BackendType::NapCat).is_err());
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
