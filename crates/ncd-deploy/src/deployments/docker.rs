//! Docker 部署：用 docker compose 在某台 Host（本机 Docker Desktop / 远端 SSH）
//! 上把 bot 跑成容器。
//!
//! 与"Docker 管理面一键部署"(commands/docker.rs)的关系:那条链路是用户在组件页
//! 手动起一个 napcat/snowluma 容器,对象是容器本身;这里是 bot 生命周期的一种
//! 部署形态(deployment_type=docker),由 BotManager 在 start_bot 时按 bot 配置驱动,
//! 容器纳入 bot 状态机。两者共用底层 DockerCli + compose 渲染,但入口和归属不同。
//!
//! 容器命名:`ncbot-<qq_id>`,前缀区别于手动部署的 `napcat`/`snowluma`,避免撞名。
//! compose 项目目录:远端 $HOME/.napcat-bots/<name>,探不到 $HOME 回退 /tmp。
//!
//! 当前实装范围:NapCat flavor。SnowLuma 容器化涉及 noVNC/daemon 差异,留待后续。
//! 容器内 OneBot 网络配置由用户在 NapCat WebUI 中配置(与手动部署一致),本层只
//! 负责把容器拉起来 + 映射端口 + 注入 WebUI token。

use async_trait::async_trait;
use ncd_domain::{BotConfig, BotFlavor, BotId, DockerDeploySpec, DockerFlavor, StopMode};
use ncd_host::{Host, HostCommand, HostPath, StreamSource};

use crate::deployment::{
    Deployment, DeploymentError, DeploymentHandle, DeploymentProgressSink, DeploymentState,
};
use crate::docker::{render_compose, DockerCli};

/// Docker 部署实装。
pub struct DockerDeployment {
    id: &'static str,
    flavors: &'static [BotFlavor],
}

impl DockerDeployment {
    pub fn new() -> Self {
        Self {
            id: "docker",
            // 当前只做 NapCat 容器化;SnowLuma 容器涉及 noVNC/daemon 差异,后续接入。
            flavors: &[BotFlavor::NapCat],
        }
    }

    /// bot 容器名:ncbot-<qq_id>。
    fn container_name(config: &BotConfig) -> String {
        format!("ncbot-{}", config.bot.qq_id)
    }

    /// compose 项目目录(host 侧 POSIX 路径)。远端探 $HOME,本机/探不到回退 /tmp。
    async fn project_dir(host: &dyn Host, name: &str) -> String {
        let home = probe_home(host).await.unwrap_or_else(|| "/tmp".to_string());
        format!("{home}/.napcat-bots/{name}")
    }

    /// 从 BotConfig 构造 NapCat DockerDeploySpec:容器名 ncbot-<qq>,端口走默认,
    /// qq_id 预绑。
    fn build_spec(config: &BotConfig) -> DockerDeploySpec {
        let mut spec = DockerDeploySpec::napcat_default();
        spec.container_name = Self::container_name(config);
        if config.bot.qq_id != 0 {
            spec.qq_id = Some(config.bot.qq_id);
        }
        spec
    }
}

impl Default for DockerDeployment {
    fn default() -> Self {
        Self::new()
    }
}

/// 远端探 $HOME。失败返回 None。
async fn probe_home(host: &dyn Host) -> Option<String> {
    let cmd = HostCommand::new("sh").arg("-c").arg("echo $HOME");
    match host.run_to_string(cmd).await {
        Ok(out) if out.success() => {
            let h = out.stdout.trim().to_string();
            if h.is_empty() {
                None
            } else {
                Some(h)
            }
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
        if config.bot.backend_type != ncd_domain::BackendType::NapCat {
            return Err(DeploymentError::UnsupportedFlavor {
                flavor: format!("{:?} docker 部署暂未实装", config.bot.backend_type),
            });
        }

        let cli = DockerCli::new(host);
        // 探 docker 就绪。
        progress.report("docker", "探测 Docker 状态", 5);
        let status = cli.probe().await;
        if !status.ready_to_deploy() {
            return Err(DeploymentError::RuntimeUnavailable { kind: "docker" });
        }

        let spec = Self::build_spec(config);
        let name = Self::container_name(config);
        let project_dir = Self::project_dir(host, &name).await;

        // 准备目录 + 写 compose。
        progress.report("compose", "准备部署目录", 15);
        host.create_dir_all(&HostPath::from_posix(&project_dir))
            .await
            .map_err(|e| DeploymentError::InstallFailed(format!("创建部署目录失败: {e}")))?;

        // WebUI token:用 qq_id 派生一个稳定 token(每个 bot 固定,便于重装幂等)。
        // 不持久化明文,容器 env 注入。
        let token = format!("ncbot{}", config.bot.qq_id);
        let (uid, gid) = default_uid_gid(host);
        let yaml = render_compose(&spec, &token, uid, gid);
        let compose_path = HostPath::from_posix(format!("{project_dir}/docker-compose.yml"));
        host.write_file(&compose_path, yaml.as_bytes())
            .await
            .map_err(|e| DeploymentError::InstallFailed(format!("写 compose 文件失败: {e}")))?;

        // 拉镜像(走镜像站 fallback)。逐行日志透传到 sink。
        progress.report("pull", "拉取镜像", 30);
        let candidates = DockerFlavor::NapCat.pull_candidates();
        let official = DockerFlavor::NapCat.default_image();
        // 回调工厂:每个候选给一份独立逐行回调。progress 是 &dyn 借用,不能进
        // 'static 回调,所以这里只做空回调(install 阶段日志细节非关键),百分比
        // 由外层粗粒度报。后续要逐行日志可改走事件总线注入。
        let new_line_cb = |_idx: usize, _img: &str| {
            move |_src: StreamSource, _line: String| {}
        };
        cli.pull_with_fallback(&candidates, official, new_line_cb)
            .await
            .map_err(|e| DeploymentError::InstallFailed(format!("拉取镜像失败: {e}")))?;
        progress.report("pull", "镜像就绪", 90);
        Ok(())
    }

    async fn launch(
        &self,
        host: &dyn Host,
        config: &BotConfig,
    ) -> Result<DeploymentHandle, DeploymentError> {
        let cli = DockerCli::new(host);
        let name = Self::container_name(config);
        let project_dir = Self::project_dir(host, &name).await;

        // compose up -d。镜像在 install 阶段已拉好(--pull missing 命中本地缓存)。
        cli.compose_up(&project_dir)
            .await
            .map_err(|e| DeploymentError::LaunchFailed(format!("启动容器失败: {e}")))?;

        // 回读容器 id + 启动时间。找不到也不致命:容器已起,observe 后续能纠正。
        let started_at = now_secs();
        let container_id = find_container_id(&cli, &name).await.unwrap_or_default();
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
        let name = format!("ncbot-{}", bot_id.as_str());
        // observe 高频轮询,不该因一次 docker 命令失败就 hard error。失败时
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
        let name = format!("ncbot-{}", bot_id.as_str());
        // 容器不存在时 stop 报错,先查一遍,幂等。
        let containers = cli
            .list_containers()
            .await
            .map_err(|e| DeploymentError::StopFailed(format!("查询容器失败: {e}")))?;
        if !containers.iter().any(|c| c.name == name) {
            return Ok(());
        }
        cli.lifecycle("stop", &name)
            .await
            .map_err(|e| DeploymentError::StopFailed(format!("停止容器失败: {e}")))
    }

    async fn uninstall(
        &self,
        host: &dyn Host,
        config: &BotConfig,
    ) -> Result<(), DeploymentError> {
        let cli = DockerCli::new(host);
        let name = Self::container_name(config);
        let project_dir = Self::project_dir(host, &name).await;
        // compose down -v 清容器 + 卷。目录不存在时 down 会报错,忽略(幂等)。
        let _ = cli.compose_down(&project_dir, true).await;
        let _ = host
            .remove_dir_all(&HostPath::from_posix(&project_dir))
            .await;
        Ok(())
    }
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

    #[test]
    fn docker_deployment_id_is_stable() {
        assert_eq!(DockerDeployment::new().id(), "docker");
    }

    #[test]
    fn only_supports_napcat_flavor() {
        // supports 的 OS 判定(仅 Linux)用 MockHost 在集成测试覆盖;这里断言
        // 只支持 NapCat 底座。
        let dep = DockerDeployment::new();
        assert_eq!(dep.supported_flavors(), &[BotFlavor::NapCat]);
    }

    #[test]
    fn container_name_uses_qq_prefix() {
        use ncd_domain::{AdvancedConfig, AutoRestartSchedule, BackendType, BotBasicConfig,
            ConnectConfig, DeploymentType, RuntimeTarget};
        let config = BotConfig {
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
        };
        assert_eq!(DockerDeployment::container_name(&config), "ncbot-10001");
        let spec = DockerDeployment::build_spec(&config);
        assert_eq!(spec.container_name, "ncbot-10001");
        assert_eq!(spec.qq_id, Some(10001));
    }

    #[test]
    fn map_state_running_to_running() {
        use ncd_domain::ContainerState;
        assert_eq!(map_state(&ContainerState::Running), DeploymentState::Running);
        assert_eq!(map_state(&ContainerState::Exited), DeploymentState::Stopped);
        assert_eq!(
            map_state(&ContainerState::Created),
            DeploymentState::Starting
        );
    }
}
