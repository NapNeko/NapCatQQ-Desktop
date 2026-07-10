//! 适配层:把 ncd-deploy 的 NativeDeployment 接入当前 BotManager 体系
//!
//! 三个适配器:
//! - RuntimeLaunchPlannerAdapter:实装 NativeLaunchTranslator trait,包装现有
//!   FileSystemRuntimeLaunchPlanner
//! - EventBusSink:实装 NativeRuntimeEventSink trait,桥接 BroadcastEventBus
//!   (已下沉到 ncd-deploy,此处 re-export)
//! - NativeDeploymentBackend:把 NativeDeployment 包成 BotBackend trait object,
//!   让 BotManager 无需修改结构体即可切到新实装后续删 BotBackend 时一起删

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use ncd_deploy::docker::DockerCli;
use ncd_deploy::{
    Deployment, DeploymentError, NativeDeployment, NativeLaunchCommand, NativeLaunchTranslator,
};
use ncd_deploy::{DockerDeployment, bot_docker_container_name, resolve_bot_container_name};
use ncd_domain::bot_status::BotStatus;
use ncd_domain::ids::BotId;
use ncd_domain::kinds::{BackendKind, StopMode};
use ncd_domain::{BackendType, BotConfig, BotFlavor};
use ncd_host::{Host, HostError, HostPath};
use serde_json::{Map, Value, json};

use crate::backend_config_renderer::{
    render_napcat_docker_config_payloads, render_snowluma_docker_config_payloads,
};
use crate::bot_actor::BotActorState;
use crate::runtime_launch_plan::RuntimeLaunchPlanner;
use ncd_backend_napcat::remote_native_launch::{
    RemoteNapcatLayout, napcat_remote_log_path, probe_remote_napcat_layout,
    remote_napcat_running_pid, stop_remote_napcat_on_host,
};
use ncd_traits::runtime_backend::{
    BotBackend, BotBackendError, BotRuntimeConfig, BotStartCtx, LogSnapshot, TailOpts,
};

// RuntimeLaunchPlannerAdapter

/// 把 FileSystemRuntimeLaunchPlanner 包装成 NativeLaunchTranslator
///
/// NativeLaunchTranslator::translate 收 &BotConfig,输出 NativeLaunchCommand
/// 内部先调 build_plan 拿到 RuntimeLaunchPlan,再取出 NapCat 分支的
/// program / args / working_dir / environmentSnowLuma 分支的 launch_command
/// 为空(由 SnowLumaDaemon 另走),适配器对空命令返回错误
pub struct RuntimeLaunchPlannerAdapter {
    planner: Arc<dyn RuntimeLaunchPlanner>,
}

impl RuntimeLaunchPlannerAdapter {
    pub fn new(planner: Arc<dyn RuntimeLaunchPlanner>) -> Self {
        Self { planner }
    }
}

#[async_trait]
impl NativeLaunchTranslator for RuntimeLaunchPlannerAdapter {
    async fn translate(&self, config: &BotConfig) -> Result<NativeLaunchCommand, DeploymentError> {
        let bot_id = BotId::new(config.bot.qq_id.to_string());
        let plan = self
            .planner
            .build_plan(&bot_id, config)
            .await
            .map_err(|err| DeploymentError::LaunchFailed(err.to_string()))?;

        // 把 RuntimeLaunchPlan 转成 BotRuntimeConfig 再抽出 launch 字段
        let cfg = BotRuntimeConfig::default_path("/tmp", bot_id);
        let cfg = plan.into_runtime_config(cfg);

        let Some((program, args)) = cfg.launch_command.split_first() else {
            return Err(DeploymentError::LaunchFailed(
                "launch plan produced empty command (SnowLuma backend uses daemon, not direct spawn)".into(),
            ));
        };
        Ok(NativeLaunchCommand {
            program: program.clone(),
            args: args.to_vec(),
            working_dir: cfg.working_dir,
            environment: cfg.environment,
        })
    }
}

// re-export EventBusSink from ncd-deploy for backward compatibility
pub use ncd_deploy::EventBusSink;

// NativeDeploymentBackend:过渡壳
//
// 让 BotManager 在不改结构体的情况下就能用 NativeDeployment
// BotBackend 要求 start/stop/status/tail_log/read_config/write_config,
// 这里把前三个转发给 NativeDeployment,后三个保留原来的文件 IO 逻辑
// 后续删 BotBackend trait 时整个文件一起扬掉

/// 过渡壳:让 NativeDeployment 穿上 BotBackend trait 的外套
pub struct NativeDeploymentBackend {
    deployment: Arc<NativeDeployment>,
    host: Arc<dyn Host>,
    backend_id: BotId,
    flavor: BotFlavor,
}

impl NativeDeploymentBackend {
    pub fn new(
        deployment: Arc<NativeDeployment>,
        host: Arc<dyn Host>,
        backend_id: impl Into<BotId>,
        flavor: BotFlavor,
    ) -> Self {
        Self {
            deployment,
            host,
            backend_id: backend_id.into(),
            flavor,
        }
    }
}

#[async_trait]
impl BotBackend for NativeDeploymentBackend {
    fn id(&self) -> &BotId {
        &self.backend_id
    }

    fn kind(&self) -> BackendKind {
        BackendKind::Local
    }

    fn flavor(&self) -> BotFlavor {
        self.flavor
    }

    async fn start(&self, ctx: &BotStartCtx) -> Result<BotStatus, BotBackendError> {
        let bot_config = bot_config_for_start(ctx, self.flavor, false)?;

        let handle = self
            .deployment
            .launch(self.host.as_ref(), &bot_config)
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
        match state {
            ncd_deploy::DeploymentState::Running => {
                // 从 deployment 拿不到精确 pid/started_at,返回 running 用 0 占位
                // BotManager 只看 state 字段做决策,pid 在 start 时已经拿到了
                Ok(BotStatus::running(bot_id, 0, 0))
            }
            _ => Ok(BotStatus::stopped(bot_id)),
        }
    }

    async fn read_config(&self, bot_id: BotId) -> Result<BotRuntimeConfig, BotBackendError> {
        // 过渡期不支持,BotManager 不从 backend 读 config(用 repo 读)
        Err(BotBackendError::ConfigNotFound(bot_id))
    }

    async fn write_config(
        &self,
        _bot_id: BotId,
        _cfg: &BotRuntimeConfig,
    ) -> Result<(), BotBackendError> {
        // 过渡期:config 落盘已由 BotManager 自己做,backend 不需要管
        Ok(())
    }

    async fn tail_log(
        &self,
        bot_id: BotId,
        opts: TailOpts,
    ) -> Result<LogSnapshot, BotBackendError> {
        let snap = self.deployment.tail_log(&bot_id, opts.lines).await;
        Ok(LogSnapshot {
            lines: snap.lines,
            total_lines: snap.total_lines,
        })
    }
}

// RemoteNativeDeploymentBackend:远端 SSH + NativeDeployment

/// 远端「直接运行」:每 Bot 绑定一台 Host + 独立 NativeDeployment(translator 写远端路径)
///
/// 不再常驻固定 host 引用,而是持有 resolver + target;在 start/status/stop/tail_log
/// 边界按需取当前应活 host,传输层断连时 refresh 后重试一次。持久失败不在此层标 Crashed
/// (由 BotManager 区分 transport_error 后决定是否只发 bot_error)
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
            .map_err(BotBackendError::RemoteHostTransport)
    }

    /// 通过 resolver 取得一个“新鲜”host(会触发底层刷新/重连)
    async fn refreshed_host(&self) -> Result<Arc<dyn Host>, BotBackendError> {
        self.resolver
            .refresh(&self.target)
            .await
            .map_err(BotBackendError::RemoteHostTransport)
    }

    /// 在操作边界使用:先拿 host 执行 op;失败则 refresh 后再试一次。
    /// 当前对任意错误都重试一次(远端 Native 失败多半与 SSH 有关);
    /// 若错误类型以后带上 transport 分类,可再收紧为仅 transport 才刷新。
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
                // 刷新后重试一次
                let host2 = self.refreshed_host().await?;
                op(host2)
                    .await
                    .map_err(|e2| BotBackendError::RemoteHostTransport(format!("{:?}", e2)))
            }
        }
    }

    async fn napcat_install_base(&self) -> Result<HostPath, BotBackendError> {
        // 拿一次当前 host 做探测(install base 通常在启动前调用,不长期持有)
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
        // 用刷新包装:传输断连时会尝试 refresh 后重试一次
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
            // stop_remote_napcat_on_host 直接用 host 做 pgrep/kill,也需要刷新保护
            let host = self.current_host().await?;
            stop_remote_napcat_on_host(host.as_ref(), qq_id).await?;
        }
        // deployment.stop 也走刷新包装
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
            // remote_napcat_running_pid 做 pgrep,也需要活连接
            let host = self.current_host().await?;
            if let Some(pid) = remote_napcat_running_pid(host.as_ref(), qq_id).await? {
                return Ok(BotStatus::running(bot_id, pid, 0));
            }
            return Ok(BotStatus::stopped(bot_id));
        }
        // 通用 observe 走刷新包装;持久失败时上层可根据错误形状决定不推 Crashed
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
            // 非 NapCat:deployment.tail_log 不直接依赖远端 host 句柄,保持原直连调用
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
        let path = HostPath::from_posix(&log_path);

        // 读日志文件走刷新包装:失败(含 transport 类)时刷新 host 后重试一次
        let bytes = self
            .with_host_refresh(|h| {
                let p = path.clone();
                async move {
                    match h.read_file(&p).await {
                        Ok(b) => Ok(b),
                        Err(HostError::PathNotFound { .. }) => {
                            Err(HostError::PathNotFound { path: p })
                        }
                        Err(e) => Err(e),
                    }
                }
            })
            .await;

        let bytes = match bytes {
            Ok(b) => b,
            Err(BotBackendError::Io(msg)) if msg.contains("PathNotFound") => {
                return Ok(LogSnapshot {
                    lines: Vec::new(),
                    total_lines: 0,
                });
            }
            Err(e) => return Err(e),
        };

        let text = String::from_utf8_lossy(&bytes);
        let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
        let total_lines = lines.len();
        if opts.lines > 0 && lines.len() > opts.lines {
            lines = lines.split_off(lines.len() - opts.lines);
        }
        Ok(LogSnapshot { lines, total_lines })
    }
}

use ncd_domain::RuntimeTarget;

fn minimal_bot_config(qq_id: u64, flavor: BotFlavor) -> BotConfig {
    use ncd_domain::{
        AdvancedConfig, AutoRestartSchedule, BotBasicConfig, ConnectConfig, DeploymentType,
    };
    BotConfig {
        bot: BotBasicConfig {
            name: String::new(),
            qq_id,
            music_sign_url: String::new(),
            auto_restart_schedule: AutoRestartSchedule::default(),
            offline_auto_restart: false,
            runtime_target: RuntimeTarget::Local,
            backend_type: match flavor {
                BotFlavor::NapCat => BackendType::NapCat,
                BotFlavor::SnowLuma => BackendType::SnowLuma,
            },
            deployment_type: DeploymentType::Native,
            snowluma_start_mode: None,
        },
        connect: ConnectConfig::default(),
        advanced: AdvancedConfig::default(),
        status_command: None,
    }
}

fn bot_config_for_start(
    ctx: &BotStartCtx,
    flavor: BotFlavor,
    require_real: bool,
) -> Result<BotConfig, BotBackendError> {
    if let Some(ref cfg) = ctx.bot_config {
        return Ok(cfg.clone());
    }
    real_bot_config_from_ctx(ctx, flavor, require_real)
}

fn real_bot_config_from_ctx(
    ctx: &BotStartCtx,
    flavor: BotFlavor,
    require_real: bool,
) -> Result<BotConfig, BotBackendError> {
    match load_bot_config_from_runtime_path(&ctx.config.config_path, &ctx.config.bot_id)? {
        Some(config) => Ok(config),
        None if require_real => Err(BotBackendError::ConfigNotFound(ctx.config.bot_id.clone())),
        None => {
            let qq_id: u64 = ctx.config.bot_id.as_str().parse().unwrap_or(0);
            Ok(minimal_bot_config(qq_id, flavor))
        }
    }
}

fn load_bot_config_from_runtime_path(
    runtime_config_path: &Path,
    bot_id: &BotId,
) -> Result<Option<BotConfig>, BotBackendError> {
    let Some(root) = data_root_from_config_path(runtime_config_path, bot_id) else {
        return Ok(None);
    };
    let paths = crate::data_paths::DataPaths::new(&root);
    let bot_path = if paths.bot_config_path().is_file() {
        paths.bot_config_path()
    } else {
        paths.legacy_bot_config_path()
    };
    let text = match std::fs::read_to_string(&bot_path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(BotBackendError::Io(error.to_string())),
    };
    let payload: Value =
        serde_json::from_str(&text).map_err(|error| BotBackendError::Json(error.to_string()))?;
    let qq_id: u64 = bot_id
        .as_str()
        .parse()
        .map_err(|_| BotBackendError::InvalidConfig(format!("invalid bot id: {bot_id}")))?;
    let bots = payload
        .get("bots")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            BotBackendError::InvalidConfig("config/bot.json missing bots array".into())
        })?;
    for value in bots {
        let config: BotConfig = serde_json::from_value(value.clone())
            .map_err(|error| BotBackendError::Json(error.to_string()))?;
        if config.bot.qq_id == qq_id {
            config
                .validate()
                .map_err(|error| BotBackendError::InvalidConfig(error.to_string()))?;
            return Ok(Some(config));
        }
    }
    Ok(None)
}

fn data_root_from_config_path(runtime_config_path: &Path, bot_id: &BotId) -> Option<PathBuf> {
    let file_name = runtime_config_path.file_name()?.to_string_lossy();
    if file_name != format!("{}.json", bot_id.as_str()) {
        return None;
    }
    let bots_dir = runtime_config_path.parent()?;
    if bots_dir.file_name()?.to_string_lossy() != "bots" {
        return None;
    }
    let config_dir = bots_dir.parent()?;
    if config_dir.file_name()?.to_string_lossy() != "config" {
        return None;
    }
    let parent = config_dir.parent()?;
    // 旧布局:<data_root>/runtime/config/bots/x.json
    if parent.file_name()?.to_string_lossy() == "runtime" {
        return parent.parent().map(Path::to_path_buf);
    }
    // 布局 v1:<data_root>/config/bots/x.json
    Some(parent.to_path_buf())
}

fn docker_container_name(config: &BotConfig) -> String {
    DockerDeployment::container_name(config)
}

async fn docker_project_dir(host: &dyn Host, name: &str) -> Result<String, BotBackendError> {
    let home = probe_home(host).await?;
    Ok(format!("{home}/.napcat-bots/{name}"))
}

async fn probe_home(host: &dyn Host) -> Result<String, BotBackendError> {
    let cmd = ncd_host::HostCommand::new("sh").arg("-c").arg("echo $HOME");
    match host.run_to_string(cmd).await {
        Ok(out) if out.success() => {
            let home = out.stdout.trim().to_string();
            if home.is_empty() {
                Err(BotBackendError::InvalidConfig(
                    "Docker host HOME is empty; cannot determine deployment project directory"
                        .into(),
                ))
            } else {
                Ok(home)
            }
        }
        Ok(out) => Err(BotBackendError::Io(format!(
            "探测 Docker 主机 HOME 失败: exit={:?}, stderr={}",
            out.exit_code,
            out.stderr.trim()
        ))),
        Err(error) => Err(BotBackendError::Io(format!(
            "探测 Docker 主机 HOME 失败: {error}"
        ))),
    }
}

fn docker_config_file_names(bot_id: &BotId) -> [String; 2] {
    [
        format!("onebot11_{}.json", bot_id.as_str()),
        format!("napcat_{}.json", bot_id.as_str()),
    ]
}

async fn render_docker_config_on_host(
    host: &dyn Host,
    bot_id: &BotId,
    config: &BotConfig,
) -> Result<(), BotBackendError> {
    let name = docker_container_name(config);
    let project_dir = docker_project_dir(host, &name).await?;
    match config.bot.backend_type {
        BackendType::NapCat => {
            let config_dir = format!("{project_dir}/napcat/config");
            let config_dir_path = HostPath::from_posix(&config_dir);
            host.create_dir_all(&config_dir_path)
                .await
                .map_err(|error| {
                    BotBackendError::Io(format!("创建 Docker 配置目录失败: {error}"))
                })?;

            let existing = read_existing_docker_napcat_config(host, bot_id, &config_dir).await?;
            for item in render_napcat_docker_config_payloads(bot_id, config, &existing) {
                let bytes = serde_json::to_vec_pretty(&item.payload)
                    .map_err(|error| BotBackendError::Json(error.to_string()))?;
                let path = HostPath::from_posix(format!("{config_dir}/{}", item.file_name));
                host.write_file(&path, &bytes).await.map_err(|error| {
                    BotBackendError::Io(format!("写 Docker 配置文件失败: {error}"))
                })?;
            }
        }
        BackendType::SnowLuma => {
            let config_dir = format!("{project_dir}/snowluma-data/config");
            let config_dir_path = HostPath::from_posix(&config_dir);
            host.create_dir_all(&config_dir_path)
                .await
                .map_err(|error| {
                    BotBackendError::Io(format!("创建 Docker 配置目录失败: {error}"))
                })?;

            let existing = read_existing_docker_snowluma_config(host, bot_id, &config_dir).await?;
            for item in render_snowluma_docker_config_payloads(bot_id, config, &existing) {
                let bytes = serde_json::to_vec_pretty(&item.payload)
                    .map_err(|error| BotBackendError::Json(error.to_string()))?;
                let path = HostPath::from_posix(format!("{config_dir}/{}", item.file_name));
                host.write_file(&path, &bytes).await.map_err(|error| {
                    BotBackendError::Io(format!("写 Docker 配置文件失败: {error}"))
                })?;
            }
        }
    }
    Ok(())
}

async fn read_existing_docker_napcat_config(
    host: &dyn Host,
    bot_id: &BotId,
    config_dir: &str,
) -> Result<HashMap<String, Value>, BotBackendError> {
    let mut existing = HashMap::new();
    for file_name in docker_config_file_names(bot_id) {
        let path = HostPath::from_posix(format!("{config_dir}/{file_name}"));
        match host.read_file(&path).await {
            Ok(bytes) => {
                if let Ok(value) = serde_json::from_slice::<Value>(&bytes) {
                    existing.insert(file_name, value);
                }
            }
            Err(HostError::PathNotFound { .. }) => {}
            Err(error) => return Err(BotBackendError::Io(error.to_string())),
        }
    }
    Ok(existing)
}

async fn read_existing_docker_snowluma_config(
    host: &dyn Host,
    bot_id: &BotId,
    config_dir: &str,
) -> Result<HashMap<String, Value>, BotBackendError> {
    let mut existing = HashMap::new();
    let file_name = format!("onebot_{}.json", bot_id.as_str());
    let path = HostPath::from_posix(format!("{config_dir}/{file_name}"));
    match host.read_file(&path).await {
        Ok(bytes) => {
            if let Ok(value) = serde_json::from_slice::<Value>(&bytes) {
                existing.insert(file_name, value);
            }
        }
        Err(HostError::PathNotFound { .. }) => {}
        Err(error) => return Err(BotBackendError::Io(error.to_string())),
    }
    Ok(existing)
}

fn status_for_deployment_state(bot_id: BotId, state: DeploymentState) -> BotStatus {
    match state {
        DeploymentState::Running => BotStatus::running(bot_id, 0, 0),
        DeploymentState::Stopped => BotStatus::stopped(bot_id),
        DeploymentState::Starting => {
            deployment_status(bot_id, BotActorState::Starting, "starting", None)
        }
        DeploymentState::Stopping => {
            deployment_status(bot_id, BotActorState::Stopping, "stopping", None)
        }
        DeploymentState::Failed { reason } => {
            deployment_status(bot_id, BotActorState::Crashed, "failed", Some(reason))
        }
    }
}

fn deployment_status(
    bot_id: BotId,
    state: BotActorState,
    deployment_state: &'static str,
    reason: Option<String>,
) -> BotStatus {
    let mut extra = Map::new();
    extra.insert("deployment_state".into(), json!(deployment_state));
    if let Some(reason) = reason {
        extra.insert("reason".into(), json!(reason));
    }
    BotStatus {
        bot_id,
        state,
        transport_error: None,
        pid: None,
        started_at: None,
        memory_rss_bytes: None,
        server_total_memory_bytes: None,
        extra,
    }
}

// DockerDeploymentBackend:把 DockerDeployment 包成 BotBackend
//
// 与 NativeDeploymentBackend 平行:让 BotManager 用统一的 BotBackend 接口起
// docker 容器形态的 botstart 走 install(拉镜像/写 compose)+ launch(compose up);
// status 走 observe(docker ps);stop 走 docker stop;tail_log 走 docker logs
// host 在构造时注入(由 BotManager 按 runtime_target 解析后传入)

use ncd_deploy::{DeploymentHandle, DeploymentState, NullProgressSink};

/// 过渡壳:让 DockerDeployment 穿上 BotBackend trait 外套
pub struct DockerDeploymentBackend {
    deployment: Arc<DockerDeployment>,
    host: Arc<dyn Host>,
    backend_id: BotId,
    flavor: BotFlavor,
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
        }
    }
}

#[async_trait]
impl BotBackend for DockerDeploymentBackend {
    fn id(&self) -> &BotId {
        &self.backend_id
    }

    fn kind(&self) -> BackendKind {
        // docker 容器跑在 host 上;host 是本机还是远端由注入的 host 决定,
        // 这里 kind 表达"部署形态来源",docker 统一归 Local 语义(非 SSH backend 抽象)
        BackendKind::Local
    }

    fn flavor(&self) -> BotFlavor {
        self.flavor
    }

    async fn start(&self, ctx: &BotStartCtx) -> Result<BotStatus, BotBackendError> {
        let bot_config = bot_config_for_start(ctx, self.flavor, true)?;
        render_docker_config_on_host(self.host.as_ref(), &ctx.config.bot_id, &bot_config).await?;

        // install:探 docker + 写 compose + 拉镜像
        let sink = NullProgressSink;
        self.deployment
            .install(self.host.as_ref(), &bot_config, &sink)
            .await
            .map_err(|err| BotBackendError::Io(err.to_string()))?;

        // launch:compose up
        let handle = self
            .deployment
            .launch(self.host.as_ref(), &bot_config)
            .await
            .map_err(|err| BotBackendError::Io(err.to_string()))?;

        match handle {
            DeploymentHandle::Docker { started_at, .. } => {
                // 容器没有宿主机 pid,用 0 占位;BotManager 只看 state 做决策
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
        let lines: Vec<String> = logs.lines().map(|l| l.to_string()).collect();
        let total = lines.len();
        Ok(LogSnapshot {
            lines,
            total_lines: total,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    use async_trait::async_trait;
    use bytes::Bytes;
    use ncd_domain::{DeploymentType, RuntimeTarget};
    use ncd_host::remote::{ConnectionConfig, HostKeyPolicy, RemoteWindowsHost, SshCredentials};
    use ncd_host::{
        Arch, ArchiveKind, CommandOutput, DirEntry, HostCommand, HostProcess, HostShell, Locality,
        Os, PackageManager, ShellKind,
    };
    use ncd_test_support::BotConfigBuilder;
    use serde_json::json;
    use tempfile::tempdir;

    struct NoopShell;

    impl HostShell for NoopShell {
        fn kind(&self) -> ShellKind {
            ShellKind::Bash
        }

        fn escape(&self, arg: &str) -> String {
            arg.to_string()
        }

        fn line_separator(&self) -> &'static str {
            "\n"
        }
    }

    static NOOP_SHELL: NoopShell = NoopShell;

    #[derive(Clone)]
    struct RecordedWrite {
        path: String,
    }

    struct DockerRuntimeMockHost {
        ps_json: String,
        commands: Mutex<Vec<HostCommand>>,
        writes: Mutex<Vec<RecordedWrite>>,
        created_dirs: Mutex<Vec<String>>,
    }

    impl DockerRuntimeMockHost {
        fn new(ps_json: impl Into<String>) -> Self {
            Self {
                ps_json: ps_json.into(),
                commands: Mutex::new(Vec::new()),
                writes: Mutex::new(Vec::new()),
                created_dirs: Mutex::new(Vec::new()),
            }
        }

        fn with_legacy_snowluma_container() -> Self {
            Self::new(
                r#"{"ID":"abc123","Names":"ncbot-10001","Image":"motricseven7/snowluma:latest","State":"running","Status":"Up","Ports":"0.0.0.0:5099->5099/tcp"}"#,
            )
        }

        fn commands(&self) -> Vec<HostCommand> {
            self.commands.lock().unwrap().clone()
        }

        fn writes(&self) -> Vec<RecordedWrite> {
            self.writes.lock().unwrap().clone()
        }

        fn created_dirs(&self) -> Vec<String> {
            self.created_dirs.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl Host for DockerRuntimeMockHost {
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
            &NOOP_SHELL
        }

        fn pkg_manager(&self) -> Option<&dyn PackageManager> {
            None
        }

        async fn read_file(&self, path: &HostPath) -> Result<Bytes, HostError> {
            Err(HostError::PathNotFound { path: path.clone() })
        }

        async fn write_file(&self, path: &HostPath, bytes: &[u8]) -> Result<(), HostError> {
            let _ = bytes;
            self.writes.lock().unwrap().push(RecordedWrite {
                path: path.as_posix().to_string(),
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

        async fn spawn(&self, _: HostCommand) -> Result<Box<dyn HostProcess>, HostError> {
            Err(HostError::Unsupported { operation: "mock" })
        }

        async fn run_to_string(&self, cmd: HostCommand) -> Result<CommandOutput, HostError> {
            self.commands.lock().unwrap().push(cmd.clone());
            if cmd.program == "sh" && cmd.args == ["-c", "echo $HOME"] {
                return Ok(command_output(0, "/home/napcat\n", ""));
            }
            if cmd.program != "docker" {
                return Ok(command_output(0, "", ""));
            }
            match cmd.args.first().map(String::as_str) {
                Some("ps") => Ok(command_output(0, self.ps_json.clone(), "")),
                Some("logs") => Ok(command_output(0, "legacy-line-a\nlegacy-line-b\n", "")),
                _ => Ok(command_output(0, "", "")),
            }
        }
    }

    fn command_output(
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

    fn docker_config(backend: BackendType) -> BotConfig {
        BotConfigBuilder::new()
            .qq_id(10001)
            .runtime_target(RuntimeTarget::server("remote-a"))
            .backend_type(backend)
            .deployment_type(DeploymentType::Docker)
            .build()
    }

    #[test]
    fn runtime_root_is_derived_from_real_runtime_bot_config_path() {
        let bot_id = BotId::new("10001");
        let path = BotRuntimeConfig::default_path("/data", bot_id.clone()).config_path;
        assert_eq!(
            data_root_from_config_path(&path, &bot_id),
            Some(PathBuf::from("/data"))
        );
    }

    #[test]
    fn data_root_rejects_unexpected_config_path_shape() {
        let bot_id = BotId::new("10001");
        let wrong_file = PathBuf::from("/data/config/bots/10002.json");
        let wrong_dir = PathBuf::from("/data/other/bots/10001.json");

        assert_eq!(data_root_from_config_path(&wrong_file, &bot_id), None);
        assert_eq!(data_root_from_config_path(&wrong_dir, &bot_id), None);
    }

    #[test]
    fn docker_requires_real_config_when_bot_json_is_missing() {
        let root = tempdir().unwrap();
        let bot_id = BotId::new("10001");
        let ctx = BotStartCtx {
            config: BotRuntimeConfig::default_path(root.path(), bot_id.clone()),
            bot_config: None,
        };

        let err = real_bot_config_from_ctx(&ctx, BotFlavor::NapCat, true).unwrap_err();

        assert!(matches!(err, BotBackendError::ConfigNotFound(id) if id == bot_id));
    }

    #[test]
    fn docker_loads_real_config_from_default_runtime_path() {
        let root = tempdir().unwrap();
        let bot_id = BotId::new("10001");
        let config_dir = root.path().join("config");
        std::fs::create_dir_all(&config_dir).unwrap();
        std::fs::write(
            config_dir.join("bot.json"),
            serde_json::to_vec(&json!({
                "bots": [{
                    "bot": {
                        "name": "real-bot",
                        "QQID": 10001,
                        "musicSignUrl": "https://sign.example.com",
                        "autoRestartSchedule": {"enabled": false, "time": "04:00", "unit": "daily"},
                        "offlineAutoRestart": false,
                        "runtime_target": "remote_linux",
                        "backendType": "NapCat",
                        "deploymentType": "docker"
                    },
                    "connect": {},
                    "advanced": {}
                }]
            }))
            .unwrap(),
        )
        .unwrap();
        let ctx = BotStartCtx {
            config: BotRuntimeConfig::default_path(root.path(), bot_id),
            bot_config: None,
        };

        let config = real_bot_config_from_ctx(&ctx, BotFlavor::NapCat, true).unwrap();

        assert_eq!(config.bot.name, "real-bot");
        assert_eq!(config.bot.music_sign_url, "https://sign.example.com");
    }

    #[tokio::test]
    async fn docker_project_dir_home_probe_failure_is_hard_error() {
        let host = RemoteWindowsHost::new_stub(
            "stub",
            ConnectionConfig::new(
                "example.com",
                22,
                SshCredentials::password("u", "p"),
                HostKeyPolicy::Insecure,
            ),
        );

        let err = docker_project_dir(&host, "ncbot-10001").await.unwrap_err();

        assert!(matches!(err, BotBackendError::Io(message) if message.contains("HOME")));
    }

    #[tokio::test]
    async fn docker_snowluma_config_uses_slbot_project_dir() {
        let host = DockerRuntimeMockHost::new("");
        let bot_id = BotId::new("10001");
        let config = docker_config(BackendType::SnowLuma);

        render_docker_config_on_host(&host, &bot_id, &config)
            .await
            .unwrap();

        let expected_dir = "/home/napcat/.napcat-bots/slbot-10001/snowluma-data/config";
        assert!(host.created_dirs().iter().any(|path| path == expected_dir));
        let writes = host.writes();
        assert!(
            writes
                .iter()
                .any(|write| write.path == format!("{expected_dir}/onebot_10001.json"))
        );
        assert!(
            host.created_dirs()
                .iter()
                .chain(writes.iter().map(|write| &write.path))
                .all(|path| !path.contains("ncbot-10001"))
        );
    }

    #[tokio::test]
    async fn docker_tail_log_uses_resolved_running_container_name() {
        let host = Arc::new(DockerRuntimeMockHost::with_legacy_snowluma_container());
        let backend = DockerDeploymentBackend::new(
            Arc::new(DockerDeployment::new()),
            host.clone(),
            BotId::new("docker"),
            BotFlavor::SnowLuma,
        );

        let snap = backend
            .tail_log(BotId::new("10001"), TailOpts { lines: 20 })
            .await
            .unwrap();

        assert_eq!(snap.lines, vec!["legacy-line-a", "legacy-line-b"]);
        let logs_cmd = host
            .commands()
            .into_iter()
            .find(|cmd| {
                cmd.program == "docker" && cmd.args.first().map(String::as_str) == Some("logs")
            })
            .unwrap();
        assert_eq!(
            logs_cmd.args.last().map(String::as_str),
            Some("ncbot-10001")
        );
    }

    #[tokio::test]
    async fn docker_tail_log_falls_back_to_flavor_name_when_container_is_absent() {
        let host = Arc::new(DockerRuntimeMockHost::new(""));
        let backend = DockerDeploymentBackend::new(
            Arc::new(DockerDeployment::new()),
            host.clone(),
            BotId::new("docker"),
            BotFlavor::NapCat,
        );

        backend
            .tail_log(BotId::new("10001"), TailOpts { lines: 20 })
            .await
            .unwrap();

        let logs_cmd = host
            .commands()
            .into_iter()
            .find(|cmd| {
                cmd.program == "docker" && cmd.args.first().map(String::as_str) == Some("logs")
            })
            .unwrap();
        assert_eq!(
            logs_cmd.args.last().map(String::as_str),
            Some("ncbot-10001")
        );
    }

    #[test]
    fn docker_starting_status_is_not_stopped() {
        let status = status_for_deployment_state(BotId::new("10001"), DeploymentState::Starting);

        assert_eq!(status.state, BotActorState::Starting);
        assert_eq!(status.extra["deployment_state"], "starting");
    }

    #[test]
    fn docker_failed_status_keeps_reason() {
        let status = status_for_deployment_state(
            BotId::new("10001"),
            DeploymentState::Failed {
                reason: "docker ps failed".to_string(),
            },
        );

        assert_eq!(status.state, BotActorState::Crashed);
        assert_eq!(status.extra["deployment_state"], "failed");
        assert_eq!(status.extra["reason"], "docker ps failed");
    }
}
