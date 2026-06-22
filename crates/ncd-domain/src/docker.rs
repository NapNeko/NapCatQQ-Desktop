//! Docker 部署与容器管理的数据契约
//!
//! 只放强类型 serde 结构和纯枚举, 给 ncd-deploy 的 DockerCli 和前端共用
//! 容器不进 bot 列表, 类型独立于 BotConfig / BotActor, 自成 Docker 管理面数据模型
//!
//! 端口 / 卷 / 环境变量都用强类型表达, 避免在命令层拼裸 dict
//! WebUI 地址等回读结果走 DeployedContainer, 前端拿来直接渲染可点链接

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// 目标主机上 Docker 的探测结果
///
/// installed=false 时其余字段无意义(version 为空, 两个 bool 为 false),
/// 前端据此显示"安装 Docker"按钮
/// compose_available 单独拎出来是因为老系统可能有 docker 但没有 compose v2 插件
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct DockerStatus {
    /// docker 二进制是否存在且能跑 docker version
    pub installed: bool,
    /// docker 客户端版本号(如 "27.3.1");探测不到留空
    pub version: String,
    /// docker compose version 是否可用(compose v2 插件)
    pub compose_available: bool,
    /// docker daemon 是否在跑(docker info 成功)装了但没起 daemon 时为 false
    pub daemon_running: bool,
}

impl DockerStatus {
    /// 一个"什么都没有"的状态,探测彻底失败时返回
    pub fn absent() -> Self {
        Self {
            installed: false,
            version: String::new(),
            compose_available: false,
            daemon_running: false,
        }
    }

    /// 是否可以直接部署:装了 + daemon 在跑 + compose 可用
    pub fn ready_to_deploy(&self) -> bool {
        self.installed && self.daemon_running && self.compose_available
    }
}

/// 容器运行状态对齐 docker ps 的 State 字段语义
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum ContainerState {
    /// 正在运行
    Running,
    /// 已创建未启动
    Created,
    /// 重启中
    Restarting,
    /// 已暂停
    Paused,
    /// 已退出(停止)
    Exited,
    /// dead / removing 等其它状态,统一归到这里
    Other,
}

impl ContainerState {
    /// 从 docker ps 的 State 字符串解析未知值落到 Other
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

/// 一个已存在容器的概要信息, 来自 docker ps -a 逐行 JSON 解析
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct ContainerInfo {
    /// 容器短 id(12 位)
    pub id: String,
    /// 容器名
    pub name: String,
    /// 镜像名(含 tag)
    pub image: String,
    /// 解析后的运行状态
    pub state: ContainerState,
    /// docker 原始 status 文案(如 "Up 3 hours" / "Exited (0) 2 minutes ago")
    pub status: String,
    /// 端口映射文案(如 "0.0.0.0:6099->6099/tcp"),逐条拆好给 UI
    pub ports: Vec<String>,
}

/// 本地镜像概要来自 docker images --format '{{json .}}' 逐行解析
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct ImageInfo {
    /// 镜像 id 短码(12 位)
    pub id: String,
    /// 仓库名;<none> 表示悬空层
    pub repository: String,
    /// 标签;<none> 常见于悬空镜像
    pub tag: String,
    /// 人类可读大小(如 1.2GB),与 docker CLI 一致
    pub size: String,
    /// 创建时间文案(如 2 weeks ago)
    pub created_since: String,
}

/// 删除本地镜像时的可选参数
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct ImageRemoveOptions {
    /// 为 true 时加 docker rmi -f,用于仍有容器引用时强制删
    #[serde(default)]
    pub force: bool,
}

/// Desktop 认识的 Docker 部署口味只有 NapCat / SnowLuma 两种有官方镜像
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

    /// 官方镜像引用(含 tag), compose.yml 永远写这个标准名
    /// 拉取时若走了镜像站前缀, 会 retag 回这个名字让 compose 命中本地缓存
    pub const fn default_image(self) -> &'static str {
        match self {
            Self::NapCat => "mlikiowa/napcat-docker:latest",
            Self::SnowLuma => "motricseven7/snowluma:latest",
        }
    }

    /// SnowLuma 镜像在 Hub 上的压缩体积约值(MB), 用于任务提示, 与 Hub tags 页一致
    pub const SNOWLUMA_COMPRESSED_MB_APPROX: u32 = 933;

    /// 拉镜像时按优先级尝试的镜像引用列表
    ///
    /// Hub 官方名优先(走 daemon registry-mirrors), SnowLuma 在 Hub 500 时尽早试
    /// docker.1ms.run(用户实测可拉满 ~933MB), 再 GHCR, 其余 Hub 反代
    /// 不试 GHCR 加速前缀(not found/403 居多)
    pub fn pull_candidates(self) -> Vec<String> {
        let official = self.default_image();
        let mut refs = vec![official.to_string()];
        match self {
            Self::NapCat => {
                refs.extend(DOCKER_HUB_MIRRORS.iter().map(|m| format!("{m}/{official}")));
            }
            Self::SnowLuma => {
                refs.push(format!("docker.1ms.run/{official}"));
                // GHCR 官方源 + 国内加速前缀
                refs.push("ghcr.io/snowluma/snowluma:latest".to_string());
                refs.extend(
                    GHCR_MIRROR_PREFIXES
                        .iter()
                        .map(|prefix| format!("{prefix}/snowluma/snowluma:latest")),
                );
                refs.extend(
                    DOCKER_HUB_MIRRORS[1..]
                        .iter()
                        .map(|m| format!("{m}/{official}")),
                );
            }
        }
        refs
    }
}

/// Docker Hub 国内反代镜像站主机名(按优先级), 拉取时拼成 <host>/<官方镜像路径>,
/// 无需改远端 daemon.json
/// 公共站存活期不稳定, 故多站 + 官方直连兜底, 换站只改这里
///
/// 含社区常用站与用户提供的 2026 可用源(毫秒/轩辕/渡渡鸟等)
/// 1ms.run 与 docker.1ms.run 同属毫秒镜像, 文档写法不一, 两条都试
pub const DOCKER_HUB_MIRRORS: &[&str] = &[
    "docker.1ms.run",
    "1ms.run",
    "xuanyuan.cloud",
    "docker.xuanyuan.me",
    "docker.aityp.com",
    "docker.m.daocloud.io",
    "dockerproxy.net",
];

/// GHCR 国内加速前缀(拼在 ghcr.io/... 路径前,见各站 ghcr 文档)
/// 仅 SnowLuma 拉取候选使用;NapCat 镜像在 Docker Hub
pub const GHCR_MIRROR_PREFIXES: &[&str] = &["ghcr.1ms.run", "ghcr.m.daocloud.io"];

/// 一条端口映射:宿主机端口 → 容器端口
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct PortMapping {
    /// 宿主机监听端口
    pub host: u16,
    /// 容器内端口
    pub container: u16,
}

impl PortMapping {
    pub const fn new(host: u16, container: u16) -> Self {
        Self { host, container }
    }
}

/// 组件页「拉镜像」请求只选框架口味;不创建容器,端口与容器名由 Bot 启动时决定
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct DockerPullSpec {
    pub flavor: DockerFlavor,
}

/// Bot 启动 / compose 渲染用的完整部署参数(容器名,端口,可选 QQ)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct DockerDeploySpec {
    /// 部署口味
    pub flavor: DockerFlavor,
    /// 容器名(也用作 compose project 名 + 子目录名), 必须是合法的 docker 名字符集
    /// [a-zA-Z0-9][a-zA-Z0-9_.-]*
    pub container_name: String,
    /// 端口映射列表, 前端给默认值, 高级用户可改宿主机端口避免冲突
    pub ports: Vec<PortMapping>,
    /// 可选:绑定登录的 QQ 号(NapCat 的 ACCOUNT env), 0 / None 表示不预绑
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub qq_id: Option<u64>,
}

impl DockerDeploySpec {
    /// NapCat 默认 spec:端口 3000/3001/6099,容器名 napcat
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

    /// 同一远端多 Bot 时给宿主机端口加偏移,避免 compose 绑定冲突
    /// 用 qq_id 取模,偏移量落在 [0, 499]
    pub fn host_port_offset_for_qq(qq_id: u64) -> u16 {
        (qq_id % 500) as u16
    }

    /// 把默认 spec 的宿主机侧端口整体加上 per-bot 偏移;容器内端口不变
    /// 端口加偏移后超过 65535 时 wrap 到高位区间避免饱和碰撞
    pub fn with_host_port_offset(mut self, qq_id: u64) -> Self {
        let off = Self::host_port_offset_for_qq(qq_id);
        if off == 0 {
            return self;
        }
        for p in &mut self.ports {
            if let Some(next) = p.host.checked_add(off) {
                p.host = next;
            } else {
                // overflow: wrap into [1024, 65535] range
                p.host = 1024 + (off % (65535 - 1024));
            }
        }
        self
    }

    /// SnowLuma 默认 spec:端口 5900/6081/5099/3000/3001,容器名 snowluma
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

    /// 查 compose 里某容器端口对应的宿主机绑定端口
    pub fn host_port_for_container(&self, container_port: u16) -> Option<u16> {
        self.ports
            .iter()
            .find(|p| p.container == container_port)
            .map(|p| p.host)
    }

    /// 容器名合法性校验, docker 要求首字符是字母数字, 其余可含 _.-
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

/// DockerDeploySpec 校验错误
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DockerSpecError {
    #[error("container name cannot be empty")]
    EmptyName,
    #[error("invalid container name: {0}")]
    InvalidName(String),
    #[error("at least one port mapping is required")]
    NoPorts,
}

/// 组件页「拉镜像」完成后的回读结果, 不创建容器, Bot 启动时再按配置起 ncbot-<qq>
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct DockerImageReady {
    /// 部署口味
    pub flavor: DockerFlavor,
    /// 已就绪的镜像引用(与 compose 使用的官方名一致,如 mlikiowa/napcat-docker:latest)
    pub image: String,
}

/// 历史 IPC 名保留别名, 语义同 DockerImageReady
pub type DeployedContainer = DockerImageReady;

/// docker pull 单层进度快照, 随 StepProgress 推到前端任务队列
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct DockerPullLayerSnapshot {
    /// 层 id 短码(与 docker 输出一致,通常 12 位 hex)
    pub id: String,
    /// 阶段文案:等待 / 下载中 / 校验 / 解压中 / 完成 等
    pub phase: String,
    /// 下载进度后缀,如 [====>    ] 12.5MB/50MB;无则 None
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// 该层是否已计入整体 completed 计数
    pub done: bool,
}

/// 容器生命周期操作
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

/// docker_install 命令回给前端的结构化结果
///
/// 不再裸返回 String: 前端要靠 status 区分"装好了弹绿条" / "需要 sudo 密码弹输入框" /
/// "彻底装不了弹红条", 光凭文案没法可靠分流
/// message 是给用户看的人话, download_url 仅 Windows/macOS 引导手动装时给下载入口
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct DockerInstallReport {
    /// 本次安装尝试的结果分类
    pub status: DockerInstallStatus,
    /// 给用户展示的人话文案(成功提示 / 失败原因 / 需要密码的说明)
    pub message: String,
    /// 可选下载入口(Windows/macOS 不能静默装时给 Docker Desktop 链接)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub download_url: Option<String>,
    /// 安装流程结束时的探测快照, 供前端立刻刷新 Docker 行, 无需等下一轮 probe 或重启应用
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub probed_status: Option<DockerStatus>,
}

/// docker_install 的结果分类
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum DockerInstallStatus {
    /// 早就装好且 daemon 在跑,这次没动手
    AlreadyInstalled,
    /// 这次成功装上了
    Installed,
    /// 远端是密钥登录, keyring 里也没缓存密码, sudo 又要密码
    /// 前端弹框向用户要 sudo 密码后带着重试, 唯一需要弹输入框的分支
    NeedSudoPassword,
    /// 装不了, 需要用户去远端手动处理(非 Linux 平台, 脚本跑完仍探不到等)
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
    fn pull_candidates_official_first_then_mirrors() {
        let cands = DockerFlavor::NapCat.pull_candidates();
        assert_eq!(cands.len(), DOCKER_HUB_MIRRORS.len() + 1);
        assert_eq!(cands[0], "mlikiowa/napcat-docker:latest");
        for (i, mirror) in DOCKER_HUB_MIRRORS.iter().enumerate() {
            assert_eq!(
                cands[i + 1],
                format!("{mirror}/mlikiowa/napcat-docker:latest")
            );
        }
    }

    #[test]
    fn snowluma_pull_candidates_hub_mirrors_before_ghcr() {
        let cands = DockerFlavor::SnowLuma.pull_candidates();
        assert_eq!(cands[0], "motricseven7/snowluma:latest");
        assert_eq!(cands[1], "docker.1ms.run/motricseven7/snowluma:latest");
        // GHCR official + mirror prefixes
        assert_eq!(cands[2], "ghcr.io/snowluma/snowluma:latest");
        assert_eq!(cands[3], "ghcr.1ms.run/snowluma/snowluma:latest");
        assert_eq!(cands[4], "ghcr.m.daocloud.io/snowluma/snowluma:latest");
        // Hub mirrors (skipping first, which is docker.1ms.run already at index 1)
        assert_eq!(cands[5], "1ms.run/motricseven7/snowluma:latest");
        assert_eq!(
            cands.len(),
            1 + 1 + 1 + GHCR_MIRROR_PREFIXES.len() + (DOCKER_HUB_MIRRORS.len() - 1)
        );
    }

    #[test]
    fn docker_flavor_wire_format_is_lowercase() {
        // 与 BotFlavor 一致用 lowercase(napcat / snowluma),不走 snake_case
        // (否则会变成 nap_cat / snow_luma,跟前端 BotFlavor 漂移)
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
