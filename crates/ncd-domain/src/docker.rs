//! Docker 部署与容器管理的数据契约。
//!
//! 这一层只放强类型 serde 结构和纯枚举,给 ncd-deploy 的 DockerCli 和前端
//! 共用。容器不进 bot 列表,所以这里的类型独立于 BotConfig / BotActor 那套,
//! 自成一个"Docker 管理面"的数据模型。
//!
//! 端口 / 卷 / 环境变量都用强类型表达,避免在命令层拼裸 dict。WebUI 地址
//! 等回读结果走 DeployedContainer,前端拿来直接渲染可点链接。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// 目标主机上 Docker 的探测结果。
///
/// installed=false 时其余字段无意义(version 为空,两个 bool 为 false),前端
/// 据此显示"安装 Docker"按钮。compose_available 单独拎出来是因为老系统可能
/// 有 docker 但没有 compose v2 插件。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct DockerStatus {
    /// docker 二进制是否存在且能跑 `docker version`。
    pub installed: bool,
    /// docker 客户端版本号(如 "27.3.1");探测不到留空。
    pub version: String,
    /// `docker compose version` 是否可用(compose v2 插件)。
    pub compose_available: bool,
    /// docker daemon 是否在跑(`docker info` 成功)。装了但没起 daemon 时为 false。
    pub daemon_running: bool,
}

impl DockerStatus {
    /// 一个"什么都没有"的状态,探测彻底失败时返回。
    pub fn absent() -> Self {
        Self {
            installed: false,
            version: String::new(),
            compose_available: false,
            daemon_running: false,
        }
    }

    /// 是否可以直接部署:装了 + daemon 在跑 + compose 可用。
    pub fn ready_to_deploy(&self) -> bool {
        self.installed && self.daemon_running && self.compose_available
    }
}

/// 容器运行状态。对齐 `docker ps` 的 State 字段语义。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum ContainerState {
    /// 正在运行。
    Running,
    /// 已创建未启动。
    Created,
    /// 重启中。
    Restarting,
    /// 已暂停。
    Paused,
    /// 已退出(停止)。
    Exited,
    /// dead / removing 等其它状态,统一归到这里。
    Other,
}

impl ContainerState {
    /// 从 `docker ps` 的 State 字符串解析。未知值落到 Other。
    pub fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "running" => Self::Running,
            "created" => Self::Created,
            "restarting" => Self::Restarting,
            "paused" => Self::Paused,
            "exited" => Self::Exited,
            _ => Self::Other,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Created => "created",
            Self::Restarting => "restarting",
            Self::Paused => "paused",
            Self::Exited => "exited",
            Self::Other => "other",
        }
    }
}

/// 一个已存在容器的概要信息。来自 `docker ps -a` 逐行 JSON 解析。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct ContainerInfo {
    /// 容器短 id(12 位)。
    pub id: String,
    /// 容器名。
    pub name: String,
    /// 镜像名(含 tag)。
    pub image: String,
    /// 解析后的运行状态。
    pub state: ContainerState,
    /// docker 原始 status 文案(如 "Up 3 hours" / "Exited (0) 2 minutes ago")。
    pub status: String,
    /// 端口映射文案(如 "0.0.0.0:6099->6099/tcp"),逐条拆好给 UI。
    pub ports: Vec<String>,
}

/// Desktop 认识的 Docker 部署口味。只有 NapCat / SnowLuma 两种有官方镜像。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum DockerFlavor {
    NapCat,
    SnowLuma,
}

impl DockerFlavor {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NapCat => "napcat",
            Self::SnowLuma => "snowluma",
        }
    }

    /// 官方镜像引用(含 tag)。compose.yml 永远写这个标准名;拉取时若走了镜像站
    /// 前缀,会 retag 回这个名字让 compose 命中本地缓存。
    pub const fn default_image(self) -> &'static str {
        match self {
            Self::NapCat => "mlikiowa/napcat-docker:latest",
            Self::SnowLuma => "motricseven7/snowluma:latest",
        }
    }

    /// 拉镜像时按优先级尝试的镜像引用列表:先逐个走国内反代镜像站
    /// ([`DOCKER_HUB_MIRRORS`]),最后回退官方直连([`Self::default_image`])。
    /// 调用方逐个尝试,第一个拉成功的即采用,随后 retag 回 default_image。
    pub fn pull_candidates(self) -> Vec<String> {
        let official = self.default_image();
        let mut refs: Vec<String> = DOCKER_HUB_MIRRORS
            .iter()
            .map(|m| format!("{m}/{official}"))
            .collect();
        refs.push(official.to_string());
        refs
    }
}

/// Docker Hub 国内反代镜像站前缀(按优先级)。Docker Hub 官方 registry 在国内
/// 基本不可直连,这些是社区维护的反代,拉取时拼成 `<mirror>/<image>`。这类公共
/// 站点存活期不稳定(常因流量/备案关停),所以做成多站点 + 官方直连兜底的
/// fallback 链,任一可达即可。换站只改这里,不动其它逻辑。
pub const DOCKER_HUB_MIRRORS: &[&str] = &[
    "docker.1ms.run",
    "docker.m.daocloud.io",
    "docker.xuanyuan.me",
];

/// 一条端口映射:宿主机端口 → 容器端口。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct PortMapping {
    /// 宿主机监听端口。
    pub host: u16,
    /// 容器内端口。
    pub container: u16,
}

impl PortMapping {
    pub const fn new(host: u16, container: u16) -> Self {
        Self { host, container }
    }
}

/// 一键部署 NapCat / SnowLuma 容器的输入参数。
///
/// 前端填好端口(给默认值即可)和容器名提交,后端据此渲染 compose.yml 并起容器。
/// 凭据(WebUI token / VNC 密码)不在这里——由后端部署时随机生成,避免前端硬编码
/// 或明文回传。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct DockerDeploySpec {
    /// 部署口味。
    pub flavor: DockerFlavor,
    /// 容器名(也用作 compose project 名 + 子目录名)。必须是合法的 docker
    /// 名字符集 [a-zA-Z0-9][a-zA-Z0-9_.-]*。
    pub container_name: String,
    /// 端口映射列表。前端给默认值,高级用户可改宿主机端口避免冲突。
    pub ports: Vec<PortMapping>,
    /// 可选:绑定登录的 QQ 号(NapCat 的 ACCOUNT env)。0 / None 表示不预绑。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub qq_id: Option<u64>,
}

impl DockerDeploySpec {
    /// NapCat 默认 spec:端口 3000/3001/6099,容器名 napcat。
    pub fn napcat_default() -> Self {
        Self {
            flavor: DockerFlavor::NapCat,
            container_name: "napcat".to_string(),
            ports: vec![
                PortMapping::new(3000, 3000),
                PortMapping::new(3001, 3001),
                PortMapping::new(6099, 6099),
            ],
            qq_id: None,
        }
    }

    /// SnowLuma 默认 spec:端口 5900/6081/5099/3000/3001,容器名 snowluma。
    pub fn snowluma_default() -> Self {
        Self {
            flavor: DockerFlavor::SnowLuma,
            container_name: "snowluma".to_string(),
            ports: vec![
                PortMapping::new(5900, 5900),
                PortMapping::new(6081, 6081),
                PortMapping::new(5099, 5099),
                PortMapping::new(3000, 3000),
                PortMapping::new(3001, 3001),
            ],
            qq_id: None,
        }
    }

    /// 容器名合法性校验。docker 要求首字符是字母数字,其余可含 _.- 。
    pub fn validate(&self) -> Result<(), DockerSpecError> {
        let name = self.container_name.trim();
        if name.is_empty() {
            return Err(DockerSpecError::EmptyName);
        }
        let mut chars = name.chars();
        let first_ok = chars
            .next()
            .map(|c| c.is_ascii_alphanumeric())
            .unwrap_or(false);
        if !first_ok {
            return Err(DockerSpecError::InvalidName(name.to_string()));
        }
        let rest_ok = name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'));
        if !rest_ok {
            return Err(DockerSpecError::InvalidName(name.to_string()));
        }
        if self.ports.is_empty() {
            return Err(DockerSpecError::NoPorts);
        }
        Ok(())
    }
}

/// DockerDeploySpec 校验错误。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DockerSpecError {
    #[error("container name cannot be empty")]
    EmptyName,
    #[error("invalid container name: {0}")]
    InvalidName(String),
    #[error("at least one port mapping is required")]
    NoPorts,
}

/// 部署完成后回读的结果,给前端展示"去哪登录 + 凭据是什么"。
///
/// webui_url / novnc_url 是拼好的可点链接(已带宿主机 ip / 端口)。token / vnc
/// 密码是后端生成的明文,只在这一次部署响应里返回给前端展示一次,不落
/// servers.json。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct DeployedContainer {
    /// 容器名。
    pub name: String,
    /// 部署口味。
    pub flavor: DockerFlavor,
    /// WebUI 完整地址(NapCat: http://host:6099/webui;SnowLuma: http://host:5099/)。
    pub webui_url: String,
    /// noVNC 地址(仅 SnowLuma 有;NapCat 为 None)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub novnc_url: Option<String>,
    /// WebUI 登录凭据(NapCat 是 token,SnowLuma 是首启随机密码)。拿不到时 None。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub webui_secret: Option<String>,
}

/// 容器生命周期操作。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum ContainerAction {
    Start,
    Stop,
    Restart,
    Remove,
}

impl ContainerAction {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Stop => "stop",
            Self::Restart => "restart",
            Self::Remove => "remove",
        }
    }
}

/// docker_install 命令回给前端的结构化结果。
///
/// 不再裸返回一句 String:前端要靠 status 区分"装好了弹绿条""需要 sudo 密码弹
/// 输入框""彻底装不了弹红条",光凭文案没法可靠分流。message 是给用户看的人话,
/// download_url 仅 Windows/macOS 引导手动装时给下载入口。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct DockerInstallReport {
    /// 本次安装尝试的结果分类。
    pub status: DockerInstallStatus,
    /// 给用户展示的人话文案(成功提示 / 失败原因 / 需要密码的说明)。
    pub message: String,
    /// 可选下载入口(Windows/macOS 不能静默装时给 Docker Desktop 链接)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub download_url: Option<String>,
    /// 安装流程结束时的探测快照，供前端立刻刷新 Docker 行，无需等下一轮 probe 或重启应用。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub probed_status: Option<DockerStatus>,
}

/// docker_install 的结果分类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum DockerInstallStatus {
    /// 早就装好且 daemon 在跑,这次没动手。
    AlreadyInstalled,
    /// 这次成功装上了。
    Installed,
    /// 远端是密钥登录、keyring 里也没缓存密码,sudo 又要密码:需要前端弹框
    /// 向用户要 sudo 密码后带着重试。这是唯一一个前端要弹输入框的分支。
    NeedSudoPassword,
    /// 装不了,需要用户去远端手动处理(非 Linux 平台、脚本跑完仍探不到等)。
    ManualRequired,
}

impl DockerInstallReport {
    pub fn already_installed(version: &str) -> Self {
        Self {
            status: DockerInstallStatus::AlreadyInstalled,
            message: format!("Docker 已就绪（{version}）"),
            download_url: None,
            probed_status: None,
        }
    }

    pub fn installed() -> Self {
        Self {
            status: DockerInstallStatus::Installed,
            message: "Docker 安装完成，现在可以部署容器了".to_string(),
            download_url: None,
            probed_status: None,
        }
    }

    pub fn need_sudo_password(message: impl Into<String>) -> Self {
        Self {
            status: DockerInstallStatus::NeedSudoPassword,
            message: message.into(),
            download_url: None,
            probed_status: None,
        }
    }

    pub fn manual_required(message: impl Into<String>, download_url: Option<String>) -> Self {
        Self {
            status: DockerInstallStatus::ManualRequired,
            message: message.into(),
            download_url,
            probed_status: None,
        }
    }

    pub fn with_probed_status(mut self, status: DockerStatus) -> Self {
        self.probed_status = Some(status);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docker_status_ready_requires_all_three() {
        let mut s = DockerStatus {
            installed: true,
            version: "27.0".into(),
            compose_available: true,
            daemon_running: true,
        };
        assert!(s.ready_to_deploy());
        s.daemon_running = false;
        assert!(!s.ready_to_deploy());
    }

    #[test]
    fn container_state_parse_known_and_unknown() {
        assert_eq!(ContainerState::parse("running"), ContainerState::Running);
        assert_eq!(ContainerState::parse("Exited"), ContainerState::Exited);
        assert_eq!(ContainerState::parse("dead"), ContainerState::Other);
    }

    #[test]
    fn docker_flavor_image_refs_are_official() {
        assert_eq!(
            DockerFlavor::NapCat.default_image(),
            "mlikiowa/napcat-docker:latest"
        );
        assert_eq!(
            DockerFlavor::SnowLuma.default_image(),
            "motricseven7/snowluma:latest"
        );
    }

    #[test]
    fn pull_candidates_mirrors_first_official_last() {
        let cands = DockerFlavor::NapCat.pull_candidates();
        // 至少有「每个镜像站一条 + 官方一条」。
        assert_eq!(cands.len(), DOCKER_HUB_MIRRORS.len() + 1);
        // 镜像站候选拼成 <mirror>/<official>。
        for (i, mirror) in DOCKER_HUB_MIRRORS.iter().enumerate() {
            assert_eq!(cands[i], format!("{mirror}/mlikiowa/napcat-docker:latest"));
        }
        // 最后一条是官方裸名,作直连兜底。
        assert_eq!(cands.last().unwrap(), "mlikiowa/napcat-docker:latest");
    }

    #[test]
    fn docker_flavor_wire_format_is_lowercase() {
        // 与 BotFlavor 一致用 lowercase（napcat / snowluma），不走 snake_case
        // （否则会变成 nap_cat / snow_luma，跟前端 BotFlavor 漂移）。
        assert_eq!(
            serde_json::to_string(&DockerFlavor::NapCat).unwrap(),
            "\"napcat\""
        );
        assert_eq!(
            serde_json::to_string(&DockerFlavor::SnowLuma).unwrap(),
            "\"snowluma\""
        );
    }

    #[test]
    fn napcat_default_spec_validates() {
        let spec = DockerDeploySpec::napcat_default();
        assert!(spec.validate().is_ok());
        assert_eq!(spec.ports.len(), 3);
    }

    #[test]
    fn snowluma_default_spec_validates() {
        let spec = DockerDeploySpec::snowluma_default();
        assert!(spec.validate().is_ok());
        assert_eq!(spec.flavor, DockerFlavor::SnowLuma);
    }

    #[test]
    fn spec_rejects_bad_container_names() {
        let mut spec = DockerDeploySpec::napcat_default();
        spec.container_name = String::new();
        assert_eq!(spec.validate(), Err(DockerSpecError::EmptyName));
        spec.container_name = "-bad".into();
        assert!(matches!(
            spec.validate(),
            Err(DockerSpecError::InvalidName(_))
        ));
        spec.container_name = "good_name.1".into();
        assert!(spec.validate().is_ok());
        spec.container_name = "has space".into();
        assert!(matches!(
            spec.validate(),
            Err(DockerSpecError::InvalidName(_))
        ));
    }

    #[test]
    fn spec_rejects_empty_ports() {
        let mut spec = DockerDeploySpec::napcat_default();
        spec.ports.clear();
        assert_eq!(spec.validate(), Err(DockerSpecError::NoPorts));
    }
}
