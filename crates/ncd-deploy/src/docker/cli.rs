//! DockerCli:docker 命令封装,跑在任意 Host 上(本地 Windows / 远端 Linux)。
//!
//! 设计:只持有 &dyn Host,每个方法拼一条 docker 命令交给 host.run_to_string。
//! 命令参数全部走 HostCommand::arg 分开传,由 shell 层做转义,杜绝把用户输入
//! (容器名 / 端口)拼进命令字符串导致注入。
//!
//! 解析策略:用 `--format '{{json .}}'` 让 docker 自己吐 JSON,逐行 serde 解析,
//! 不靠脆弱的列宽切分。

use std::collections::HashMap;

use ncd_domain::{ContainerInfo, ContainerState, DockerPullLayerSnapshot, DockerStatus, ImageInfo};
use ncd_host::{Host, HostCommand, HostError, StreamSource};
use tracing::{info, warn};

/// DockerCli 操作错误。
#[derive(Debug, thiserror::Error)]
pub enum DockerCliError {
    /// host 层调用失败(SSH 中断 / 进程起不来)。
    #[error("host error: {0}")]
    Host(#[from] HostError),

    /// docker 命令跑了但退出码非 0。stderr 给上层拼错误文案。
    #[error("docker command failed: {command}: exit={exit_code:?}: {stderr}")]
    CommandFailed {
        command: String,
        exit_code: Option<i32>,
        stderr: String,
    },

    /// Docker runtime 未达到部署要求。
    #[error("docker runtime unavailable: {status:?}")]
    RuntimeUnavailable { status: DockerStatus },

    /// 解析 docker 输出失败(JSON 格式不对等)。
    #[error("failed to parse docker output: {0}")]
    ParseFailed(String),
}

/// docker CLI 封装。轻量,每个操作拼一条命令交给 host。
///
/// 提权:远端用户常不在 docker 组(装完没重登 / usermod 没生效),裸 docker
/// 命令会 permission denied 连不上 /var/run/docker.sock。probe() 会探一次
/// 「裸 docker 行不行,不行但 sudo 行」,把结果记在 elevated 里;之后所有命令
/// 按它决定要不要 .elevated()(提权密码由 Host 层注入)。本机 Windows 用不到,
/// elevated 恒 false。用 AtomicBool 是因为操作方法都是 &self,且要跨 await 保持 Send。
pub struct DockerCli<'h> {
    host: &'h dyn Host,
    elevated: std::sync::atomic::AtomicBool,
}

impl<'h> DockerCli<'h> {
    pub fn new(host: &'h dyn Host) -> Self {
        Self {
            host,
            elevated: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// 按当前提权标志构造一条 docker 命令。elevated=true 时打 .elevated() 标,
    /// Host 层用注入的 sudo 密码走 sudo -S/-n;否则裸 docker。所有 docker 操作
    /// 都经这里,保证「探测判定要 sudo」后续操作就一致地 sudo,不会探测过了部署却挂。
    fn docker_cmd(&self) -> HostCommand {
        let cmd = HostCommand::new("docker");
        if self.elevated.load(std::sync::atomic::Ordering::Relaxed) {
            cmd.elevated()
        } else {
            cmd
        }
    }

    /// 探测 docker 是否可用。任何一步失败都退化成"未装/未就绪",不报错——
    /// 探测本身不该把"没装 docker"当异常。
    pub async fn probe(&self) -> DockerStatus {
        // docker 客户端版本(不连 daemon,普通用户就能跑)。拿不到直接判定未装。
        let version = match self.docker_client_version().await {
            Some(v) => v,
            None => return DockerStatus::absent(),
        };

        // 探 daemon。裸 docker info 能过最好(用户已在 docker 组);permission denied
        // 但 sudo 能过 → daemon 其实在跑,只是当前会话没 socket 权限(没重登/没进组),
        // 记下 elevated 让后续命令都走 sudo。两者都不过才是 daemon 真没起。
        let daemon_running = self.probe_daemon_with_elevation().await;

        // compose v2 插件(按已定的 elevated 标志跑)。
        let compose_available = self.docker_compose_ok().await;

        DockerStatus {
            installed: true,
            version,
            compose_available,
            daemon_running,
        }
    }

    /// 确保当前 Host 上 docker + daemon + compose 都可用,并刷新提权决策。
    ///
    /// Deployment 的 install / launch / observe / stop 各自会新建 DockerCli。每个
    /// operation 入口先调用这里,就算上一次 probe 的 elevated 标志没有跨对象保存,
    /// 本次命令也会重新定夺 sudo 路径。
    pub async fn ensure_ready(&self) -> Result<DockerStatus, DockerCliError> {
        let status = self.probe().await;
        if status.ready_to_deploy() {
            Ok(status)
        } else {
            Err(DockerCliError::RuntimeUnavailable { status })
        }
    }

    /// 确保 docker daemon 可用并刷新提权决策,但**不要求 compose 插件**。
    ///
    /// 容器管理面(list / start / stop / restart / remove / logs)只需要 daemon,
    /// 不需要 compose;它们必须先走这里 probe 一次,否则后续裸 docker 命令在"装了
    /// docker 但当前会话没 socket 权限(没重登 / 没进 docker 组)"的远端会 permission
    /// denied 而不会自动 sudo。部署 / compose down 仍用 ensure_ready(额外要 compose)。
    pub async fn ensure_daemon_ready(&self) -> Result<DockerStatus, DockerCliError> {
        let status = self.probe().await;
        if status.daemon_running {
            Ok(status)
        } else {
            Err(DockerCliError::RuntimeUnavailable { status })
        }
    }

    /// `docker version --format '{{.Client.Version}}'`,失败返回 None。
    /// 客户端版本不连 daemon,无需提权,固定裸跑——它是"装没装 docker"的判据。
    async fn docker_client_version(&self) -> Option<String> {
        // 先尝试普通命令
        let cmd = HostCommand::new("docker")
            .arg("version")
            .arg("--format")
            .arg("{{.Client.Version}}");
        if let Ok(out) = self.host.run_to_string(cmd).await {
            if out.success() {
                let v = out.stdout.trim();
                if !v.is_empty() {
                    return Some(v.to_string());
                }
            }
        }

        // fallback: 尝试 sudo（刚安装完 PATH 可能未刷新）
        let cmd_sudo = HostCommand::new("docker")
            .arg("version")
            .arg("--format")
            .arg("{{.Client.Version}}")
            .elevated();
        let out = self.host.run_to_string(cmd_sudo).await.ok()?;
        if !out.success() {
            return None;
        }
        let v = out.stdout.trim();
        if v.is_empty() {
            None
        } else {
            Some(v.to_string())
        }
    }

    /// 探 daemon 是否在跑,顺带定夺后续是否需要提权:先裸 docker info,过了说明
    /// 用户有 socket 权限(elevated 保持 false);不过就用 sudo 再探一次,sudo 能过
    /// 说明 daemon 就绪只是缺组权限,置 elevated=true 让后续命令都走 sudo。
    async fn probe_daemon_with_elevation(&self) -> bool {
        use std::sync::atomic::Ordering;

        // 优先尝试 sudo（针对刚安装完的情况：用户已加入 docker 组，
        // 但当前 SSH 会话的组信息未刷新，需要重登才生效）
        if self.docker_info_once(true).await {
            self.elevated.store(true, Ordering::Relaxed);
            return true;
        }

        // fallback 到无 sudo（用户已在 docker 组且会话已刷新）
        if self.docker_info_once(false).await {
            self.elevated.store(false, Ordering::Relaxed);
            return true;
        }

        false
    }

    /// 跑一次 `docker info --format '{{.ServerVersion}}'`,elevated 决定要不要 sudo。
    async fn docker_info_once(&self, elevated: bool) -> bool {
        let mut cmd = HostCommand::new("docker")
            .arg("info")
            .arg("--format")
            .arg("{{.ServerVersion}}");
        if elevated {
            cmd = cmd.elevated();
        }
        matches!(self.host.run_to_string(cmd).await, Ok(out) if out.success())
    }

    /// `docker compose version`,compose v2 插件存在时退出码 0。按 elevated 标志跑。
    async fn docker_compose_ok(&self) -> bool {
        let cmd = self.docker_cmd().arg("compose").arg("version");
        matches!(self.host.run_to_string(cmd).await, Ok(out) if out.success())
    }
}

impl<'h> DockerCli<'h> {
    /// 列所有容器(含已停止)。`docker ps -a --format '{{json .}}'` 逐行 JSON。
    pub async fn list_containers(&self) -> Result<Vec<ContainerInfo>, DockerCliError> {
        let cmd = self
            .docker_cmd()
            .arg("ps")
            .arg("-a")
            .arg("--format")
            .arg("{{json .}}");
        let out = self.host.run_to_string(cmd).await?;
        if !out.success() {
            return Err(DockerCliError::CommandFailed {
                command: "docker ps -a".to_string(),
                exit_code: out.exit_code,
                stderr: out.stderr.trim().to_string(),
            });
        }
        parse_ps_json(&out.stdout)
    }

    /// 列本地镜像(含悬空)。`docker images --format '{{json .}}'` 逐行 JSON。
    pub async fn list_images(&self) -> Result<Vec<ImageInfo>, DockerCliError> {
        let cmd = self
            .docker_cmd()
            .arg("images")
            .arg("--format")
            .arg("{{json .}}");
        let out = self.host.run_to_string(cmd).await?;
        if !out.success() {
            return Err(DockerCliError::CommandFailed {
                command: "docker images".to_string(),
                exit_code: out.exit_code,
                stderr: out.stderr.trim().to_string(),
            });
        }
        parse_images_json(&out.stdout)
    }

    /// 删除本地镜像。`image_ref` 可为 repo:tag 或 id;`force` 时加 `-f`。
    pub async fn remove_image(&self, image_ref: &str, force: bool) -> Result<(), DockerCliError> {
        let mut cmd = self.docker_cmd().arg("rmi");
        if force {
            cmd = cmd.arg("-f");
        }
        let cmd = cmd.arg(image_ref);
        let out = self.host.run_to_string(cmd).await?;
        if !out.success() {
            return Err(DockerCliError::CommandFailed {
                command: format!("docker rmi{} {image_ref}", if force { " -f" } else { "" }),
                exit_code: out.exit_code,
                stderr: out.stderr.trim().to_string(),
            });
        }
        Ok(())
    }

    /// 对单个容器执行 start / stop / restart。命令名固定,容器名走 arg 转义。
    pub async fn lifecycle(&self, action: &str, container: &str) -> Result<(), DockerCliError> {
        let cmd = self.docker_cmd().arg(action).arg(container);
        let out = self.host.run_to_string(cmd).await?;
        if !out.success() {
            return Err(DockerCliError::CommandFailed {
                command: format!("docker {action} {container}"),
                exit_code: out.exit_code,
                stderr: out.stderr.trim().to_string(),
            });
        }
        Ok(())
    }

    /// 删除容器。默认带 -f 强制删(运行中也删),避免用户先 stop 再 remove 两步。
    pub async fn remove(&self, container: &str) -> Result<(), DockerCliError> {
        let cmd = self.docker_cmd().arg("rm").arg("-f").arg(container);
        let out = self.host.run_to_string(cmd).await?;
        if !out.success() {
            return Err(DockerCliError::CommandFailed {
                command: format!("docker rm -f {container}"),
                exit_code: out.exit_code,
                stderr: out.stderr.trim().to_string(),
            });
        }
        Ok(())
    }

    /// 取容器最近 tail 行日志。stdout + stderr 合并返回(docker logs 两路都吐)。
    pub async fn logs(&self, container: &str, tail: u32) -> Result<String, DockerCliError> {
        let cmd = self
            .docker_cmd()
            .arg("logs")
            .arg("--tail")
            .arg(tail.to_string())
            .arg(container);
        let out = self.host.run_to_string(cmd).await?;
        if !out.success() {
            return Err(DockerCliError::CommandFailed {
                command: format!("docker logs --tail {tail} {container}"),
                exit_code: out.exit_code,
                stderr: out.stderr.trim().to_string(),
            });
        }
        // docker logs 把容器 stdout 走 stdout、stderr 走 stderr;合并保留时序近似。
        let mut combined = out.stdout;
        if !out.stderr.trim().is_empty() {
            if !combined.is_empty() && !combined.ends_with('\n') {
                combined.push('\n');
            }
            combined.push_str(&out.stderr);
        }
        Ok(combined)
    }
}

impl<'h> DockerCli<'h> {
    /// `docker compose up -d`,在 project_dir 下跑(compose 会读那里的
    /// docker-compose.yml)。pull 由 compose 自己按需做;这里加 --pull missing
    /// 让首次部署自动拉镜像。
    pub async fn compose_up(&self, project_dir: &str) -> Result<(), DockerCliError> {
        let cmd = self
            .docker_cmd()
            .arg("compose")
            .arg("up")
            .arg("-d")
            .arg("--pull")
            .arg("missing")
            .working_dir(ncd_host::HostPath::from_posix(project_dir))
            .timeout(std::time::Duration::from_secs(900));
        let out = self.host.run_to_string(cmd).await?;
        if !out.success() {
            return Err(DockerCliError::CommandFailed {
                command: format!("docker compose up -d (in {project_dir})"),
                exit_code: out.exit_code,
                stderr: out.stderr.trim().to_string(),
            });
        }
        Ok(())
    }

    /// `docker compose down`,可选 -v 连卷一起删(彻底清理时用)。
    pub async fn compose_down(
        &self,
        project_dir: &str,
        remove_volumes: bool,
    ) -> Result<(), DockerCliError> {
        let mut cmd = self
            .docker_cmd()
            .arg("compose")
            .arg("down")
            .working_dir(ncd_host::HostPath::from_posix(project_dir))
            .timeout(std::time::Duration::from_secs(300));
        if remove_volumes {
            cmd = cmd.arg("-v");
        }
        let out = self.host.run_to_string(cmd).await?;
        if !out.success() {
            return Err(DockerCliError::CommandFailed {
                command: format!("docker compose down (in {project_dir})"),
                exit_code: out.exit_code,
                stderr: out.stderr.trim().to_string(),
            });
        }
        Ok(())
    }

    /// `docker pull <image>` 流式版本。`on_line` 每收到一行就被调用，调用方
    /// 可在回调里更新 `PullProgress` 并推进度事件。命令结束后返回 CommandOutput。
    /// 失败（exit code 非 0）时返回 `DockerCliError::CommandFailed`。
    ///
    /// 不加 `--progress=plain`：部分远端（apt 装的老版 docker/cli）会直接 exit 125
    /// unknown flag。非 TTY 下 pull 仍会按行输出 layer 进度，PullProgress 可解析。
    pub async fn pull_streaming(
        &self,
        image: &str,
        on_line: impl FnMut(StreamSource, String) + Send + 'static,
    ) -> Result<(), DockerCliError> {
        let cmd = self
            .docker_cmd()
            .arg("pull")
            .arg(image)
            .timeout(std::time::Duration::from_secs(900));
        let out = self.host.run_streaming(cmd, Box::new(on_line)).await?;
        if !out.success() {
            return Err(DockerCliError::CommandFailed {
                command: format!("docker pull {image}"),
                exit_code: out.exit_code,
                stderr: out.stderr.trim().to_string(),
            });
        }
        Ok(())
    }

    /// 带镜像站 fallback 的拉取:按 `candidates` 顺序逐个 `docker pull`,第一个
    /// 成功的即采用;若它不是 `official_image`(走了镜像站前缀),再 `docker tag`
    /// 回官方名,这样后续 `docker compose up`(compose.yml 写的是官方名)能命中
    /// 本地缓存,不会再去 Docker Hub 直连。全部候选都失败才返回最后一次的错误。
    ///
    /// `new_line_cb` 是回调工厂:每开始尝试一个候选就调一次,参数是候选的
    /// 0-based 序号和镜像引用,返回该次拉取专用的逐行回调。每个候选独立计数,
    /// 避免上一个站失败的 layer 状态串进下一个站。
    pub async fn pull_with_fallback<F, L, M>(
        &self,
        candidates: &[String],
        official_image: &str,
        mut new_line_cb: F,
        mut on_mirror_fail: Option<M>,
    ) -> Result<String, DockerCliError>
    where
        F: FnMut(usize, &str) -> L,
        L: FnMut(StreamSource, String) + Send + 'static,
        M: FnMut(usize, &str, &DockerCliError),
    {
        let mut last_err: Option<DockerCliError> = None;
        for (idx, image) in candidates.iter().enumerate() {
            let on_line = new_line_cb(idx, image);
            match self.pull_streaming(image, on_line).await {
                Ok(()) => {
                    if image != official_image {
                        let _ = self.retag(image, official_image).await;
                    }
                    info!(
                        target: "ncd_deploy::docker",
                        index = idx,
                        total = candidates.len(),
                        pulled = %image,
                        official = %official_image,
                        "docker pull 成功"
                    );
                    return Ok(image.clone());
                }
                Err(e) => {
                    warn!(
                        target: "ncd_deploy::docker",
                        index = idx,
                        total = candidates.len(),
                        image = %image,
                        err = %e,
                        "docker pull 候选失败，尝试下一个"
                    );
                    if let Some(ref mut cb) = on_mirror_fail {
                        cb(idx, image, &e);
                    }
                    last_err = Some(e);
                }
            }
        }
        Err(last_err.unwrap_or_else(|| DockerCliError::CommandFailed {
            command: "docker pull".to_string(),
            exit_code: None,
            stderr: "没有可用的镜像候选".to_string(),
        }))
    }

    /// 本地是否已有指定镜像引用(`docker image inspect` 成功即视为存在)。
    pub async fn image_exists(&self, image_ref: &str) -> Result<bool, DockerCliError> {
        let cmd = self
            .docker_cmd()
            .arg("image")
            .arg("inspect")
            .arg(image_ref);
        let out = self.host.run_to_string(cmd).await?;
        Ok(out.success())
    }

    /// `docker tag <src> <dst>`。给镜像打一个别名引用,不重新拉取。
    async fn retag(&self, src: &str, dst: &str) -> Result<(), DockerCliError> {
        let cmd = self.docker_cmd().arg("tag").arg(src).arg(dst);
        let out = self.host.run_to_string(cmd).await?;
        if !out.success() {
            return Err(DockerCliError::CommandFailed {
                command: format!("docker tag {src} {dst}"),
                exit_code: out.exit_code,
                stderr: out.stderr.trim().to_string(),
            });
        }
        Ok(())
    }
}

/// `docker pull` 输出里单个 layer 的阶段。
/// docker 在非 TTY 下每行格式：`<layerId>: <phase text>`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayerPhase {
    PullingFsLayer,
    Waiting,
    Downloading,
    VerifyingChecksum,
    DownloadComplete,
    Extracting,
    PullComplete,
    /// 见过但不认识的 phase 文本，保留原始字符串。
    Unknown(String),
}

impl LayerPhase {
    fn from_text(s: &str) -> Self {
        let t = s.trim();
        if t.starts_with("Downloading") {
            return Self::Downloading;
        }
        if t.starts_with("Extracting") {
            return Self::Extracting;
        }
        if t.starts_with("Verifying Checksum") {
            return Self::VerifyingChecksum;
        }
        match t {
            "Pulling fs layer" => Self::PullingFsLayer,
            "Waiting" => Self::Waiting,
            "Download complete" => Self::DownloadComplete,
            "Pull complete" => Self::PullComplete,
            other => Self::Unknown(other.to_string()),
        }
    }

    /// 用于进度条：下载完或整层拉完都算「完成」，避免 UI 长期 0/N。
    fn counts_toward_progress_complete(&self) -> bool {
        matches!(
            self,
            Self::DownloadComplete | Self::PullComplete | Self::Extracting
        )
    }

    fn user_label(&self) -> String {
        match self {
            Self::PullingFsLayer => "准备层".into(),
            Self::Waiting => "等待".into(),
            Self::Downloading => "下载中".into(),
            Self::VerifyingChecksum => "校验".into(),
            Self::DownloadComplete => "下载完成".into(),
            Self::Extracting => "解压中".into(),
            Self::PullComplete => "完成".into(),
            Self::Unknown(s) => {
                let t = s.trim();
                if t.is_empty() {
                    "处理中".into()
                } else {
                    t.chars().take(48).collect()
                }
            }
        }
    }

    /// 下载/解压行上的进度后缀,供 UI 展示 `[====] 12MB/50MB`。
    fn detail_from_line(phase: &Self, phase_text: &str) -> Option<String> {
        match phase {
            Self::Downloading => phase_text
                .strip_prefix("Downloading")
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from),
            Self::Extracting => phase_text
                .strip_prefix("Extracting")
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from),
            _ => None,
        }
    }

    /// 加权进度 0–100,下载中也会前进,不会长期停在 0%。
    fn weight_percent(&self) -> u8 {
        match self {
            Self::PullComplete => 100,
            Self::DownloadComplete => 92,
            Self::Extracting => 88,
            Self::VerifyingChecksum => 75,
            Self::Downloading => 45,
            Self::PullingFsLayer => 8,
            Self::Waiting => 4,
            Self::Unknown(_) => 15,
        }
    }
}

/// `docker pull` 进度状态。维护一张 layerId → phase 表，提供 (completed, total)
/// 计数供调用方换算百分比。
///
/// 用法：每收到一行就调 `update(line)`，然后读 `summary()` / `layer_snapshots()`。
pub struct PullProgress {
    layers: HashMap<String, (LayerPhase, Option<String>)>,
}

impl PullProgress {
    pub fn new() -> Self {
        Self {
            layers: HashMap::new(),
        }
    }

    /// 解析一行 docker pull 输出，更新内部状态。
    /// 支持短 id / sha256: 前缀、以及带进度后缀的 Downloading 行。
    pub fn update(&mut self, line: &str) {
        let line = line.trim();
        if line.is_empty() {
            return;
        }
        let Some(colon_pos) = line.find(": ") else {
            return;
        };
        let id_raw = line[..colon_pos].trim();
        if id_raw.eq_ignore_ascii_case("status")
            || id_raw.eq_ignore_ascii_case("digest")
            || id_raw.eq_ignore_ascii_case("latest")
        {
            return;
        }
        let id = normalize_pull_layer_id(id_raw);
        if id.is_empty() {
            return;
        }
        let phase_text = line[colon_pos + 2..].trim();
        let phase = LayerPhase::from_text(phase_text);
        let detail = LayerPhase::detail_from_line(&phase, phase_text);
        self.layers.insert(id, (phase, detail));
    }

    /// 返回 (completed_layers, total_layers, last_message, percent_0_100)。
    pub fn summary(&self) -> (usize, usize, String, u8) {
        let total = self.layers.len();
        let completed = self
            .layers
            .values()
            .filter(|(p, _)| p.counts_toward_progress_complete())
            .count();
        let percent = self.weighted_percent();
        let msg = if total == 0 {
            "拉取中…".to_string()
        } else {
            format!("镜像层 {completed}/{total} · {percent}%")
        };
        (completed, total, msg, percent)
    }

    fn weighted_percent(&self) -> u8 {
        let total = self.layers.len();
        if total == 0 {
            return 0;
        }
        let sum: u32 = self
            .layers
            .values()
            .map(|(p, _)| u32::from(p.weight_percent()))
            .sum();
        ((sum / total as u32).min(100)) as u8
    }

    /// 按层 id 排序的稳定快照,供 IPC 推到前端。
    pub fn layer_snapshots(&self) -> Vec<DockerPullLayerSnapshot> {
        let mut ids: Vec<&String> = self.layers.keys().collect();
        ids.sort();
        ids.into_iter()
            .map(|id| {
                let (phase, detail) = self.layers.get(id).expect("layer key");
                DockerPullLayerSnapshot {
                    id: id.clone(),
                    phase: phase.user_label(),
                    detail: detail.clone(),
                    done: phase.counts_toward_progress_complete(),
                }
            })
            .collect()
    }

    /// 兼容旧调用: (completed, total, message)。
    pub fn summary_legacy(&self) -> (usize, usize, String) {
        let (c, t, m, _) = self.summary();
        (c, t, m)
    }
}

fn normalize_pull_layer_id(id_raw: &str) -> String {
    let s = id_raw.trim();
    if let Some(rest) = s.strip_prefix("sha256:") {
        let hex: String = rest.chars().take(64).filter(|c| c.is_ascii_hexdigit()).collect();
        if hex.len() >= 6 {
            return hex.chars().take(12).collect();
        }
        return hex;
    }
    if s.chars().all(|c| c.is_ascii_hexdigit()) && !s.is_empty() && s.len() <= 64 {
        return s.chars().take(12).collect();
    }
    String::new()
}

impl Default for PullProgress {
    fn default() -> Self {
        Self::new()
    }
}

/// 解析 `docker ps --format '{{json .}}'` 的多行 JSON 输出。
/// 每行一个容器对象;空行跳过;单行解析失败时整体报 ParseFailed。
fn parse_ps_json(stdout: &str) -> Result<Vec<ContainerInfo>, DockerCliError> {
    let mut out = Vec::new();
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let raw: PsLine = serde_json::from_str(line)
            .map_err(|e| DockerCliError::ParseFailed(format!("{e}: {line}")))?;
        out.push(raw.into_info());
    }
    Ok(out)
}

/// `docker ps --format '{{json .}}'` 单行的字段子集。docker 这个格式的字段名
/// 是固定的(ID / Names / Image / State / Status / Ports)。
#[derive(serde::Deserialize)]
struct PsLine {
    #[serde(rename = "ID")]
    id: String,
    #[serde(rename = "Names")]
    names: String,
    #[serde(rename = "Image")]
    image: String,
    #[serde(rename = "State")]
    state: String,
    #[serde(rename = "Status")]
    status: String,
    #[serde(rename = "Ports", default)]
    ports: String,
}

impl PsLine {
    fn into_info(self) -> ContainerInfo {
        // Ports 形如 "0.0.0.0:6099->6099/tcp, :::6099->6099/tcp";按逗号拆开,
        // 去重空白。Names 多名时 docker 用逗号分隔,取第一个作主名。
        let ports: Vec<String> = self
            .ports
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let name = self
            .names
            .split(',')
            .next()
            .unwrap_or(&self.names)
            .trim()
            .to_string();
        ContainerInfo {
            id: self.id,
            name,
            image: self.image,
            state: ContainerState::parse(&self.state),
            status: self.status,
            ports,
        }
    }
}

/// 解析 `docker images --format '{{json .}}'` 的多行 JSON 输出。
fn parse_images_json(stdout: &str) -> Result<Vec<ImageInfo>, DockerCliError> {
    let mut out = Vec::new();
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let raw: ImagesLine = serde_json::from_str(line)
            .map_err(|e| DockerCliError::ParseFailed(format!("{e}: {line}")))?;
        out.push(raw.into_info());
    }
    Ok(out)
}

#[derive(serde::Deserialize)]
struct ImagesLine {
    #[serde(rename = "ID")]
    id: String,
    #[serde(rename = "Repository")]
    repository: String,
    #[serde(rename = "Tag")]
    tag: String,
    #[serde(rename = "Size")]
    size: String,
    #[serde(rename = "CreatedSince")]
    created_since: String,
}

impl ImagesLine {
    fn into_info(self) -> ImageInfo {
        ImageInfo {
            id: self.id,
            repository: self.repository,
            tag: self.tag,
            size: self.size,
            created_since: self.created_since,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_images_json_single_line() {
        let line = r#"{"ID":"abc123","Repository":"nginx","Tag":"latest","Size":"192MB","CreatedSince":"2 weeks ago"}"#;
        let parsed = parse_images_json(line).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].repository, "nginx");
        assert_eq!(parsed[0].tag, "latest");
    }

    #[test]
    fn parse_ps_json_single_running_container() {
        let line = r#"{"ID":"abc123def456","Names":"napcat","Image":"mlikiowa/napcat-docker:latest","State":"running","Status":"Up 3 hours","Ports":"0.0.0.0:6099->6099/tcp, :::6099->6099/tcp"}"#;
        let parsed = parse_ps_json(line).unwrap();
        assert_eq!(parsed.len(), 1);
        let c = &parsed[0];
        assert_eq!(c.id, "abc123def456");
        assert_eq!(c.name, "napcat");
        assert_eq!(c.state, ContainerState::Running);
        assert_eq!(c.ports.len(), 2);
    }

    #[test]
    fn parse_ps_json_multiple_lines_and_blanks() {
        let stdout = "\n\
{\"ID\":\"a1\",\"Names\":\"napcat\",\"Image\":\"img:1\",\"State\":\"running\",\"Status\":\"Up\",\"Ports\":\"\"}\n\
\n\
{\"ID\":\"b2\",\"Names\":\"snowluma\",\"Image\":\"img:2\",\"State\":\"exited\",\"Status\":\"Exited (0)\",\"Ports\":\"\"}\n";
        let parsed = parse_ps_json(stdout).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].name, "napcat");
        assert_eq!(parsed[1].state, ContainerState::Exited);
        // 空 Ports 字段解析成空 vec,不是 [""]。
        assert!(parsed[0].ports.is_empty());
    }

    #[test]
    fn parse_ps_json_empty_output_is_empty_vec() {
        assert!(parse_ps_json("").unwrap().is_empty());
        assert!(parse_ps_json("\n\n").unwrap().is_empty());
    }

    #[test]
    fn parse_ps_json_bad_line_errors() {
        let err = parse_ps_json("not json at all").unwrap_err();
        assert!(matches!(err, DockerCliError::ParseFailed(_)));
    }

    #[test]
    fn parse_ps_json_takes_first_name_when_multiple() {
        let line = r#"{"ID":"x","Names":"primary,alias","Image":"i","State":"created","Status":"Created","Ports":""}"#;
        let parsed = parse_ps_json(line).unwrap();
        assert_eq!(parsed[0].name, "primary");
        assert_eq!(parsed[0].state, ContainerState::Created);
    }

    // ---- PullProgress 解析器测试 ----

    #[test]
    fn pull_progress_empty_has_zero_counts() {
        let p = PullProgress::new();
        let (completed, total, _, _) = p.summary();
        assert_eq!(completed, 0);
        assert_eq!(total, 0);
    }

    #[test]
    fn pull_progress_single_layer_lifecycle() {
        let mut p = PullProgress::new();
        p.update("a1b2c3d4e5f6: Pulling fs layer");
        let (c, t, _, _) = p.summary();
        assert_eq!(t, 1);
        assert_eq!(c, 0);

        p.update("a1b2c3d4e5f6: Downloading");
        let (c, t, _, _) = p.summary();
        assert_eq!(t, 1);
        assert_eq!(c, 0);

        p.update("a1b2c3d4e5f6: Pull complete");
        let (c, t, _, _) = p.summary();
        assert_eq!(t, 1);
        assert_eq!(c, 1);
    }

    #[test]
    fn pull_progress_multiple_layers_out_of_order() {
        let mut p = PullProgress::new();
        // 3 个 layer，乱序到达
        p.update("aabbccdd1122: Pulling fs layer");
        p.update("112233445566: Waiting");
        p.update("deadbeef0000: Pulling fs layer");
        let (c, t, _, _) = p.summary();
        assert_eq!(t, 3);
        assert_eq!(c, 0);

        p.update("aabbccdd1122: Pull complete");
        p.update("deadbeef0000: Pull complete");
        let (c, t, _, _) = p.summary();
        assert_eq!(t, 3);
        assert_eq!(c, 2);

        p.update("112233445566: Pull complete");
        let (c, t, _, _) = p.summary();
        assert_eq!(t, 3);
        assert_eq!(c, 3);
    }

    #[test]
    fn pull_progress_ignores_digest_and_status_lines() {
        let mut p = PullProgress::new();
        p.update("Digest: sha256:abc123");
        p.update("Status: Downloaded newer image for napcat:latest");
        p.update("");
        p.update("  ");
        let (_, t, _, _) = p.summary();
        assert_eq!(t, 0);
    }

    #[test]
    fn pull_progress_ignores_non_hex_id_lines() {
        let mut p = PullProgress::new();
        // "latest: Pulling from ..." 这类行 id 含非 hex 字符，应忽略
        p.update("latest: Pulling from mlikiowa/napcat-docker");
        p.update("Status: Image is up to date for napcat:latest");
        let (_, t, _, _) = p.summary();
        assert_eq!(t, 0);
    }

    #[test]
    fn pull_progress_download_complete_counts_as_done() {
        let mut p = PullProgress::new();
        p.update("a1b2c3d4e5f6: Pulling fs layer");
        p.update("a1b2c3d4e5f6: Download complete");
        let (c, t, _, _) = p.summary();
        assert_eq!(t, 1);
        assert_eq!(c, 1);
    }

    #[test]
    fn pull_progress_downloading_with_suffix_parses_layer() {
        let mut p = PullProgress::new();
        p.update("deadbeefcafe: Downloading [====>    ] 12.5MB/50MB");
        let (c, t, _, _) = p.summary();
        assert_eq!(t, 1);
        assert_eq!(c, 0);
        p.update("deadbeefcafe: Download complete");
        let (c, t, _, _) = p.summary();
        assert_eq!(c, 1);
    }

    #[test]
    fn pull_progress_sha256_prefix_id() {
        let mut p = PullProgress::new();
        p.update("sha256:abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789: Download complete");
        let (c, t, _, _) = p.summary();
        assert_eq!(t, 1);
        assert_eq!(c, 1);
    }

    #[test]
    fn pull_progress_summary_message_format() {
        let mut p = PullProgress::new();
        p.update("aabbccdd1122: Pull complete");
        p.update("112233445566: Downloading");
        let (_, _, msg, _) = p.summary();
        assert!(msg.contains("1/2"), "expected '1/2' in '{msg}'");
    }

    #[test]
    fn pull_progress_unknown_phase_still_counts_as_layer() {
        let mut p = PullProgress::new();
        p.update("aabbccdd1122: Some Future Phase");
        let (c, t, _, _) = p.summary();
        assert_eq!(t, 1);
        assert_eq!(c, 0);
    }
}
