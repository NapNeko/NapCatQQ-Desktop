use std::collections::{BTreeMap, HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::sync::Mutex;

use crate::bot_actor::BotActorState;
use crate::bot_config::{BackendType, BotConfig};
use crate::ids::BotId;
use crate::kinds::{BackendKind, BotFlavor, RuntimeTarget};
use crate::remote_host::{RemoteHost, RemoteHostError, ShellCmd};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopMode {
    Graceful,
    Force,
}

impl Default for StopMode {
    fn default() -> Self {
        Self::Graceful
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessHandle {
    pub bot_id: BotId,
    pub pid: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BotStartCtx {
    pub config: BotRuntimeConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TailOpts {
    #[serde(default = "default_tail_lines")]
    pub lines: usize,
}

fn default_tail_lines() -> usize {
    200
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BotStatus {
    pub bot_id: BotId,
    pub state: BotActorState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_rss_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_total_memory_bytes: Option<u64>,
    #[serde(default)]
    pub extra: Map<String, Value>,
}

impl BotStatus {
    pub fn stopped(bot_id: impl Into<BotId>) -> Self {
        Self {
            bot_id: bot_id.into(),
            state: BotActorState::Stopped,
            pid: None,
            started_at: None,
            memory_rss_bytes: None,
            server_total_memory_bytes: None,
            extra: Map::new(),
        }
    }

    pub fn running(bot_id: impl Into<BotId>, pid: u32, started_at: u64) -> Self {
        Self {
            bot_id: bot_id.into(),
            state: BotActorState::Running,
            pid: Some(pid),
            started_at: Some(started_at),
            memory_rss_bytes: None,
            server_total_memory_bytes: None,
            extra: Map::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogSnapshot {
    pub lines: Vec<String>,
    pub total_lines: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BotRuntimeConfig {
    pub bot_id: BotId,
    pub config_path: PathBuf,
    #[serde(default = "default_backend_kind")]
    pub backend_kind: BackendKind,
    #[serde(default = "default_bot_flavor")]
    pub flavor: BotFlavor,
    #[serde(default = "default_runtime_target")]
    pub runtime_target: RuntimeTarget,
    #[serde(default)]
    pub launch_command: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub log_path: Option<PathBuf>,
    #[serde(default)]
    pub environment: BTreeMap<String, String>,
}

impl BotRuntimeConfig {
    pub fn default_path(root: impl Into<PathBuf>, bot_id: impl Into<BotId>) -> Self {
        let bot_id = bot_id.into();
        let root = root.into();
        Self {
            bot_id: bot_id.clone(),
            config_path: root
                .join("runtime")
                .join("config")
                .join("bots")
                .join(format!("{}.json", bot_id.as_str())),
            backend_kind: BackendKind::Local,
            flavor: BotFlavor::NapCat,
            runtime_target: RuntimeTarget::Local,
            launch_command: Vec::new(),
            working_dir: None,
            log_path: None,
            environment: BTreeMap::new(),
        }
    }

    pub fn with_runtime_defaults(mut self, root: impl AsRef<Path>) -> Self {
        if self.log_path.is_none() {
            self.log_path = Some(
                root.as_ref()
                    .join("runtime")
                    .join("log")
                    .join("bots")
                    .join(format!("{}.log", self.bot_id.as_str())),
            );
        }
        self
    }

    pub fn with_bot_config(mut self, config: &BotConfig) -> Self {
        self.flavor = match config.bot.backend_type {
            BackendType::NapCat => BotFlavor::NapCat,
            BackendType::SnowLuma => BotFlavor::SnowLuma,
        };
        self.runtime_target = config.bot.runtime_target.clone();
        self.backend_kind = if self.runtime_target.is_local() {
            BackendKind::Local
        } else {
            BackendKind::RemoteSsh
        };
        self
    }
}

#[async_trait]
pub trait RuntimeLaunchPlanner: Send + Sync {
    async fn build_plan(
        &self,
        bot_id: &BotId,
        config: &BotConfig,
    ) -> Result<RuntimeLaunchPlan, RuntimeLaunchPlanError>;
}

#[derive(Debug, Clone)]
pub struct FileSystemRuntimeLaunchPlanner {
    runtime_root: PathBuf,
}

impl FileSystemRuntimeLaunchPlanner {
    pub fn new(runtime_root: impl Into<PathBuf>) -> Self {
        Self {
            runtime_root: runtime_root.into(),
        }
    }
}

#[async_trait]
impl RuntimeLaunchPlanner for FileSystemRuntimeLaunchPlanner {
    async fn build_plan(
        &self,
        bot_id: &BotId,
        config: &BotConfig,
    ) -> Result<RuntimeLaunchPlan, RuntimeLaunchPlanError> {
        build_runtime_launch_plan(bot_id, config, &self.runtime_root).await
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeLaunchPlan {
    NapCat(NapCatLaunchPlan),
    SnowLuma(SnowLumaLaunchPlan),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NapCatLaunchPlan {
    pub runtime_root: PathBuf,
    pub napcat_dir: PathBuf,
    pub program: PathBuf,
    pub args: Vec<String>,
    pub environment: BTreeMap<String, String>,
    pub working_dir: PathBuf,
    pub load_script_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnowLumaLaunchPlan {
    pub runtime_root: PathBuf,
}

#[derive(Debug, thiserror::Error)]
pub enum RuntimeLaunchPlanError {
    #[error("unsupported runtime target: {0:?}")]
    UnsupportedTarget(RuntimeTarget),
    #[error("SnowLuma 启动链路尚未接入：需要 daemon + WebUI load_process 支持")]
    SnowLumaNotImplemented,
    #[error("unsupported platform for QQ registry lookup: {0}")]
    UnsupportedPlatform(String),
    #[error("required runtime file missing: {0}")]
    MissingFile(String),
    #[error("failed to write loadNapCat.js: {0}")]
    LoadScript(String),
}

impl RuntimeLaunchPlan {
    pub fn into_runtime_config(self, mut cfg: BotRuntimeConfig) -> BotRuntimeConfig {
        match self {
            RuntimeLaunchPlan::NapCat(plan) => {
                cfg.launch_command = std::iter::once(plan.program.to_string_lossy().to_string())
                    .chain(plan.args.into_iter())
                    .collect();
                cfg.working_dir = Some(plan.working_dir);
                cfg.environment = plan.environment;
            }
            RuntimeLaunchPlan::SnowLuma(plan) => {
                cfg.launch_command = Vec::new();
                cfg.working_dir = Some(plan.runtime_root);
            }
        }
        cfg
    }
}

pub async fn build_runtime_launch_plan(
    bot_id: &BotId,
    config: &BotConfig,
    runtime_root: impl AsRef<Path>,
) -> Result<RuntimeLaunchPlan, RuntimeLaunchPlanError> {
    match config.bot.backend_type {
        BackendType::NapCat => {
            build_napcat_launch_plan(bot_id, config, runtime_root.as_ref()).await
        }
        BackendType::SnowLuma => Err(RuntimeLaunchPlanError::SnowLumaNotImplemented),
    }
}

pub async fn build_napcat_launch_plan_with_qq_install_path(
    bot_id: &BotId,
    _config: &BotConfig,
    runtime_root: impl AsRef<Path>,
    qq_install: impl AsRef<Path>,
) -> Result<RuntimeLaunchPlan, RuntimeLaunchPlanError> {
    build_napcat_launch_plan_inner(bot_id, runtime_root.as_ref(), qq_install.as_ref()).await
}

pub async fn build_napcat_launch_plan(
    bot_id: &BotId,
    _config: &BotConfig,
    runtime_root: impl AsRef<Path>,
) -> Result<RuntimeLaunchPlan, RuntimeLaunchPlanError> {
    let qq_install = resolve_qq_install_path()?;
    build_napcat_launch_plan_inner(bot_id, runtime_root.as_ref(), &qq_install).await
}

async fn build_napcat_launch_plan_inner(
    bot_id: &BotId,
    runtime_root: &Path,
    qq_install: &Path,
) -> Result<RuntimeLaunchPlan, RuntimeLaunchPlanError> {
    let napcat_dir = runtime_root.join("NapCatQQ");
    ensure_runtime_file(
        &napcat_dir.join("NapCatWinBootMain.exe"),
        "未检测到 NapCatWinBootMain.exe，请先安装 NapCat 运行时组件",
    )?;
    ensure_runtime_file(
        &napcat_dir.join("NapCatWinBootHook.dll"),
        "未检测到 NapCatWinBootHook.dll，请先安装 NapCat 运行时组件",
    )?;
    ensure_runtime_file(
        &napcat_dir.join("napcat.mjs"),
        "未检测到 napcat.mjs，请先安装 NapCat 运行时组件",
    )?;
    ensure_runtime_file(
        &napcat_dir.join("qqnt.json"),
        "未检测到 qqnt.json，请先安装 NapCat 运行时组件",
    )?;

    let qq_exe = qq_install.join("QQ.exe");
    ensure_runtime_file(&qq_exe, "未检测到 QQ.exe，请确认已安装 QQ NT")?;

    let load_script_path = napcat_dir.join("loadNapCat.js");
    let napcat_mjs_uri = path_to_file_uri(&napcat_dir.join("napcat.mjs"));
    let load_script = format!("(async () => {{await import('{}')}})()", napcat_mjs_uri);
    tokio::fs::write(&load_script_path, load_script)
        .await
        .map_err(|error| RuntimeLaunchPlanError::LoadScript(error.to_string()))?;

    let mut environment = BTreeMap::new();
    environment.insert(
        "NAPCAT_PATCH_PACKAGE".to_string(),
        napcat_dir.join("qqnt.json").to_string_lossy().to_string(),
    );
    environment.insert(
        "NAPCAT_LOAD_PATH".to_string(),
        load_script_path.to_string_lossy().to_string(),
    );
    environment.insert(
        "NAPCAT_INJECT_PATH".to_string(),
        napcat_dir
            .join("NapCatWinBootHook.dll")
            .to_string_lossy()
            .to_string(),
    );
    environment.insert(
        "NAPCAT_LAUNCHER_PATH".to_string(),
        napcat_dir
            .join("NapCatWinBootMain.exe")
            .to_string_lossy()
            .to_string(),
    );
    environment.insert(
        "NAPCAT_MAIN_PATH".to_string(),
        napcat_dir.join("napcat.mjs").to_string_lossy().to_string(),
    );

    Ok(RuntimeLaunchPlan::NapCat(NapCatLaunchPlan {
        runtime_root: runtime_root.to_path_buf(),
        napcat_dir: napcat_dir.clone(),
        program: napcat_dir.join("NapCatWinBootMain.exe"),
        args: vec![
            qq_exe.to_string_lossy().to_string(),
            napcat_dir
                .join("NapCatWinBootHook.dll")
                .to_string_lossy()
                .to_string(),
            bot_id.as_str().to_string(),
        ],
        environment,
        working_dir: napcat_dir,
        load_script_path,
    }))
}

fn path_to_file_uri(path: &Path) -> String {
    let normalized = path.to_string_lossy().replace('\\', "/");
    if normalized.contains(":/") {
        format!("file:///{}", normalized)
    } else if normalized.starts_with('/') {
        format!("file://{}", normalized)
    } else {
        format!("file:///{}", normalized)
    }
}

fn ensure_runtime_file(path: &Path, message: &str) -> Result<(), RuntimeLaunchPlanError> {
    if path.exists() {
        Ok(())
    } else {
        Err(RuntimeLaunchPlanError::MissingFile(message.to_string()))
    }
}

#[cfg(windows)]
fn resolve_qq_install_path() -> Result<PathBuf, RuntimeLaunchPlanError> {
    use winreg::RegKey;
    use winreg::enums::HKEY_LOCAL_MACHINE;

    let hkml = RegKey::predef(HKEY_LOCAL_MACHINE);
    let key = hkml
        .open_subkey(r"SOFTWARE\WOW6432Node\Tencent\QQNT")
        .map_err(|error| RuntimeLaunchPlanError::UnsupportedPlatform(error.to_string()))?;
    let install: String = key
        .get_value("Install")
        .map_err(|error| RuntimeLaunchPlanError::UnsupportedPlatform(error.to_string()))?;
    Ok(PathBuf::from(install))
}

#[cfg(not(windows))]
fn resolve_qq_install_path() -> Result<PathBuf, RuntimeLaunchPlanError> {
    Err(RuntimeLaunchPlanError::UnsupportedPlatform(
        "non-windows platform does not support QQ registry lookup".to_string(),
    ))
}

fn default_backend_kind() -> BackendKind {
    BackendKind::Local
}

fn default_bot_flavor() -> BotFlavor {
    BotFlavor::NapCat
}

fn default_runtime_target() -> RuntimeTarget {
    RuntimeTarget::Local
}

#[derive(Debug, thiserror::Error)]
pub enum BotBackendError {
    #[error("launch command is empty")]
    EmptyLaunchCommand,
    #[error("bot backend configuration not found for {0}")]
    ConfigNotFound(BotId),
    #[error("bot process not found for {0}")]
    ProcessNotFound(BotId),
    #[error("invalid runtime configuration: {0}")]
    InvalidConfig(String),
    #[error("io error: {0}")]
    Io(String),
    #[error("json error: {0}")]
    Json(String),
    #[error("remote host error: {0}")]
    RemoteHost(String),
}

impl From<RemoteHostError> for BotBackendError {
    fn from(error: RemoteHostError) -> Self {
        Self::RemoteHost(error.to_string())
    }
}

#[async_trait]
pub trait BotBackend: Send + Sync {
    fn id(&self) -> &BotId;
    fn kind(&self) -> BackendKind;
    fn flavor(&self) -> BotFlavor;

    async fn start(&self, ctx: &BotStartCtx) -> Result<BotStatus, BotBackendError>;
    async fn stop(&self, bot_id: BotId, mode: StopMode) -> Result<(), BotBackendError>;
    async fn status(&self, bot_id: BotId) -> Result<BotStatus, BotBackendError>;
    async fn read_config(&self, bot_id: BotId) -> Result<BotRuntimeConfig, BotBackendError>;
    async fn write_config(
        &self,
        bot_id: BotId,
        cfg: &BotRuntimeConfig,
    ) -> Result<(), BotBackendError>;
    async fn tail_log(&self, bot_id: BotId, opts: TailOpts)
    -> Result<LogSnapshot, BotBackendError>;
}

pub struct RemoteRuntimeBackend<H: RemoteHost> {
    backend_id: BotId,
    host: H,
}

impl<H: RemoteHost> RemoteRuntimeBackend<H> {
    pub fn new(backend_id: impl Into<BotId>, host: H) -> Self {
        Self {
            backend_id: backend_id.into(),
            host,
        }
    }

    pub fn host(&self) -> &H {
        &self.host
    }
}

fn remote_config_path(bot_id: &BotId) -> String {
    format!("runtime/config/bots/{}.json", bot_id.as_str())
}

fn remote_log_path(bot_id: &BotId) -> String {
    format!("runtime/log/bots/{}.log", bot_id.as_str())
}

#[async_trait]
impl<H: RemoteHost> BotBackend for RemoteRuntimeBackend<H> {
    fn id(&self) -> &BotId {
        &self.backend_id
    }

    fn kind(&self) -> BackendKind {
        BackendKind::RemoteSsh
    }

    fn flavor(&self) -> BotFlavor {
        BotFlavor::NapCat
    }

    async fn start(&self, ctx: &BotStartCtx) -> Result<BotStatus, BotBackendError> {
        if ctx.config.launch_command.is_empty() {
            return Err(BotBackendError::EmptyLaunchCommand);
        }
        self.write_config(ctx.config.bot_id.clone(), &ctx.config)
            .await?;
        let (program, args) = ctx
            .config
            .launch_command
            .split_first()
            .ok_or(BotBackendError::EmptyLaunchCommand)?;
        let result = self
            .host
            .exec(ShellCmd {
                program: program.clone(),
                args: args.to_vec(),
                working_dir: ctx
                    .config
                    .working_dir
                    .as_ref()
                    .map(|path| path.to_string_lossy().to_string()),
                environment: ctx.config.environment.clone(),
            })
            .await?;
        if result.exit_code != 0 {
            return Err(BotBackendError::RemoteHost(result.stderr));
        }
        self.status(ctx.config.bot_id.clone()).await
    }

    async fn stop(&self, bot_id: BotId, _mode: StopMode) -> Result<(), BotBackendError> {
        let result = self
            .host
            .exec(ShellCmd::new("napcat-stop").arg(bot_id.as_str()))
            .await?;
        if result.exit_code == 0 {
            Ok(())
        } else {
            Err(BotBackendError::RemoteHost(result.stderr))
        }
    }

    async fn status(&self, bot_id: BotId) -> Result<BotStatus, BotBackendError> {
        let tree = match self.host.process_tree(bot_id.clone()).await {
            Ok(tree) => tree,
            Err(RemoteHostError::ProcessTreeFailed(_)) | Err(RemoteHostError::NotFound(_)) => {
                return Ok(BotStatus::stopped(bot_id));
            }
            Err(error) => return Err(error.into()),
        };
        let mut status = BotStatus::running(bot_id, tree.root.pid, 0);
        status.extra.insert(
            "backend_kind".to_string(),
            Value::String(BackendKind::RemoteSsh.as_str().to_string()),
        );
        status
            .extra
            .insert("process_name".to_string(), Value::String(tree.root.name));
        Ok(status)
    }

    async fn read_config(&self, bot_id: BotId) -> Result<BotRuntimeConfig, BotBackendError> {
        let path = remote_config_path(&bot_id);
        let bytes = match self.host.read_file(&path).await {
            Ok(bytes) => bytes,
            Err(RemoteHostError::NotFound(_)) => {
                return Err(BotBackendError::ConfigNotFound(bot_id));
            }
            Err(error) => return Err(error.into()),
        };
        serde_json::from_slice(&bytes).map_err(|error| BotBackendError::Json(error.to_string()))
    }

    async fn write_config(
        &self,
        bot_id: BotId,
        cfg: &BotRuntimeConfig,
    ) -> Result<(), BotBackendError> {
        let mut payload =
            serde_json::to_value(cfg).map_err(|error| BotBackendError::Json(error.to_string()))?;
        if let Value::Object(map) = &mut payload {
            map.insert(
                "bot_id".to_string(),
                Value::String(bot_id.as_str().to_string()),
            );
            map.insert(
                "backend_kind".to_string(),
                Value::String(BackendKind::RemoteSsh.as_str().to_string()),
            );
        }
        let bytes = serde_json::to_vec_pretty(&payload)
            .map_err(|error| BotBackendError::Json(error.to_string()))?;
        self.host
            .write_file(&remote_config_path(&bot_id), &bytes, 0o600)
            .await?;
        Ok(())
    }

    async fn tail_log(
        &self,
        bot_id: BotId,
        opts: TailOpts,
    ) -> Result<LogSnapshot, BotBackendError> {
        let bytes = match self.host.read_file(&remote_log_path(&bot_id)).await {
            Ok(bytes) => bytes,
            Err(RemoteHostError::NotFound(_)) => {
                return Ok(LogSnapshot {
                    lines: Vec::new(),
                    total_lines: 0,
                });
            }
            Err(error) => return Err(error.into()),
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

#[derive(Debug)]
pub struct LocalRuntimeBackend {
    root: PathBuf,
    backend_id: BotId,
    flavor: BotFlavor,
    processes: Mutex<HashMap<BotId, ManagedProcess>>,
    logs: Mutex<HashMap<BotId, RuntimeLogBuffer>>,
}

#[derive(Debug)]
struct ManagedProcess {
    child: tokio::process::Child,
    started_at: u64,
    config: BotRuntimeConfig,
}

#[derive(Debug, Default)]
struct RuntimeLogBuffer {
    lines: VecDeque<String>,
    total_lines: usize,
}

impl RuntimeLogBuffer {
    const MAX_LINES: usize = 10_000;

    fn push_text(&mut self, text: &str) {
        let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
        for line in normalized.lines() {
            if line.is_empty() {
                continue;
            }
            if self.lines.len() == Self::MAX_LINES {
                self.lines.pop_front();
            }
            self.lines.push_back(line.to_string());
            self.total_lines = self.total_lines.saturating_add(1);
        }
    }

    fn snapshot(&self, limit: usize) -> LogSnapshot {
        let mut lines: Vec<String> = self.lines.iter().cloned().collect();
        if limit > 0 && lines.len() > limit {
            lines = lines.split_off(lines.len() - limit);
        }
        LogSnapshot {
            lines,
            total_lines: self.total_lines,
        }
    }
}

impl LocalRuntimeBackend {
    pub fn new(root: impl Into<PathBuf>, backend_id: impl Into<BotId>) -> Self {
        Self::new_with_flavor(root, backend_id, BotFlavor::NapCat)
    }

    pub fn new_with_flavor(
        root: impl Into<PathBuf>,
        backend_id: impl Into<BotId>,
        flavor: BotFlavor,
    ) -> Self {
        Self {
            root: root.into(),
            backend_id: backend_id.into(),
            flavor,
            processes: Mutex::new(HashMap::new()),
            logs: Mutex::new(HashMap::new()),
        }
    }

    fn default_config_path(&self, bot_id: &BotId) -> PathBuf {
        BotRuntimeConfig::default_path(&self.root, bot_id.clone()).config_path
    }

    fn config_path_for(&self, bot_id: &BotId) -> PathBuf {
        self.default_config_path(bot_id)
    }

    fn log_path_for(&self, bot_id: &BotId) -> PathBuf {
        self.root
            .join("runtime")
            .join("log")
            .join("bots")
            .join(format!("{}.log", bot_id.as_str()))
    }

    async fn ensure_parent_dir(path: &Path) -> Result<(), BotBackendError> {
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|error| BotBackendError::Io(error.to_string()))?;
        }
        Ok(())
    }

    async fn write_log_line(&self, bot_id: &BotId, line: &str) -> Result<(), BotBackendError> {
        let path = self.log_path_for(bot_id);
        Self::ensure_parent_dir(&path).await?;
        tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .await
            .map_err(|error| BotBackendError::Io(error.to_string()))?
            .write_all(format!("{line}\n").as_bytes())
            .await
            .map_err(|error| BotBackendError::Io(error.to_string()))
    }

    fn build_status(&self, bot_id: BotId, record: &ManagedProcess) -> BotStatus {
        let pid = record.child.id();
        let mut status = if let Some(pid) = pid {
            BotStatus::running(bot_id, pid, record.started_at)
        } else {
            BotStatus::stopped(bot_id)
        };
        status.state = BotActorState::Running;
        status.extra.insert(
            "backend_kind".to_string(),
            Value::String(record.config.backend_kind.as_str().to_string()),
        );
        status.extra.insert(
            "flavor".to_string(),
            Value::String(record.config.flavor.as_str().to_string()),
        );
        status.extra.insert(
            "runtime_target".to_string(),
            Value::String(match &record.config.runtime_target {
                RuntimeTarget::Local => "local".to_string(),
                RuntimeTarget::Server(id) => id.clone(),
            }),
        );
        status
    }

    pub async fn sync_runtime_config(
        &self,
        bot_id: BotId,
        cfg: &BotRuntimeConfig,
    ) -> Result<BotRuntimeConfig, BotBackendError> {
        let normalized = cfg.clone().with_runtime_defaults(&self.root);
        self.write_config(bot_id, &normalized).await?;
        Ok(normalized)
    }

    pub async fn append_log_line(
        &self,
        bot_id: &BotId,
        line: impl AsRef<str>,
    ) -> Result<(), BotBackendError> {
        let line = line.as_ref();
        {
            let mut logs = self.logs.lock().await;
            logs.entry(bot_id.clone()).or_default().push_text(line);
        }
        self.write_log_line(bot_id, line).await
    }

    pub async fn log_snapshot(
        &self,
        bot_id: &BotId,
        limit: usize,
    ) -> Result<LogSnapshot, BotBackendError> {
        {
            let logs = self.logs.lock().await;
            if let Some(buffer) = logs.get(bot_id) {
                return Ok(buffer.snapshot(limit));
            }
        }

        let path = self.log_path_for(bot_id);
        if !tokio::fs::try_exists(&path)
            .await
            .map_err(|error| BotBackendError::Io(error.to_string()))?
        {
            return Ok(LogSnapshot {
                lines: Vec::new(),
                total_lines: 0,
            });
        }

        let text = tokio::fs::read_to_string(&path)
            .await
            .map_err(|error| BotBackendError::Io(error.to_string()))?;
        let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
        let total_lines = lines.len();
        if limit > 0 && lines.len() > limit {
            lines = lines.split_off(lines.len() - limit);
        }
        Ok(LogSnapshot { lines, total_lines })
    }
}

#[async_trait]
impl BotBackend for LocalRuntimeBackend {
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
        let cfg = self
            .sync_runtime_config(ctx.config.bot_id.clone(), &ctx.config)
            .await?;
        if cfg.launch_command.is_empty() {
            return Err(BotBackendError::EmptyLaunchCommand);
        }

        let (program, args) = cfg
            .launch_command
            .split_first()
            .ok_or(BotBackendError::EmptyLaunchCommand)?;
        let mut command = Command::new(program);
        command.args(args);
        if let Some(dir) = &cfg.working_dir {
            command.current_dir(dir);
        }
        for (key, value) in &cfg.environment {
            command.env(key, value);
        }
        command.stdout(std::process::Stdio::null());
        command.stderr(std::process::Stdio::null());

        let child = command
            .spawn()
            .map_err(|error| BotBackendError::Io(error.to_string()))?;
        let pid = child.id().unwrap_or(0);
        let started_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut processes = self.processes.lock().await;
        processes.insert(
            cfg.bot_id.clone(),
            ManagedProcess {
                child,
                started_at,
                config: cfg.clone(),
            },
        );

        Ok(BotStatus::running(cfg.bot_id, pid, started_at))
    }

    async fn stop(&self, bot_id: BotId, _mode: StopMode) -> Result<(), BotBackendError> {
        let record = {
            let mut processes = self.processes.lock().await;
            processes.remove(&bot_id)
        }
        .ok_or_else(|| BotBackendError::ProcessNotFound(bot_id.clone()))?;

        let mut child = record.child;
        child
            .kill()
            .await
            .map_err(|error| BotBackendError::Io(error.to_string()))?;
        Ok(())
    }

    async fn status(&self, bot_id: BotId) -> Result<BotStatus, BotBackendError> {
        let mut processes = self.processes.lock().await;
        if let Some(record) = processes.get_mut(&bot_id) {
            match record.child.try_wait() {
                Ok(Some(_)) => {
                    processes.remove(&bot_id);
                    Ok(BotStatus::stopped(bot_id))
                }
                Ok(None) => Ok(self.build_status(bot_id, record)),
                Err(error) => Err(BotBackendError::Io(error.to_string())),
            }
        } else {
            Ok(BotStatus::stopped(bot_id))
        }
    }

    async fn read_config(&self, bot_id: BotId) -> Result<BotRuntimeConfig, BotBackendError> {
        let path = self.config_path_for(&bot_id);
        if !tokio::fs::try_exists(&path)
            .await
            .map_err(|error| BotBackendError::Io(error.to_string()))?
        {
            return Err(BotBackendError::ConfigNotFound(bot_id));
        }

        let text = tokio::fs::read_to_string(&path)
            .await
            .map_err(|error| BotBackendError::Io(error.to_string()))?;
        serde_json::from_str(&text).map_err(|error| BotBackendError::Json(error.to_string()))
    }

    async fn write_config(
        &self,
        bot_id: BotId,
        cfg: &BotRuntimeConfig,
    ) -> Result<(), BotBackendError> {
        let mut payload =
            serde_json::to_value(cfg).map_err(|error| BotBackendError::Json(error.to_string()))?;
        if let Value::Object(map) = &mut payload {
            map.insert(
                "bot_id".to_string(),
                Value::String(bot_id.as_str().to_string()),
            );
        }

        Self::ensure_parent_dir(&cfg.config_path).await?;
        let bytes = serde_json::to_vec_pretty(&payload)
            .map_err(|error| BotBackendError::Json(error.to_string()))?;
        tokio::fs::write(&cfg.config_path, bytes)
            .await
            .map_err(|error| BotBackendError::Io(error.to_string()))
    }

    async fn tail_log(
        &self,
        bot_id: BotId,
        opts: TailOpts,
    ) -> Result<LogSnapshot, BotBackendError> {
        self.log_snapshot(&bot_id, opts.lines).await
    }
}

impl LocalRuntimeBackend {
    pub async fn list_running(&self) -> Vec<BotStatus> {
        let processes = self.processes.lock().await;
        processes
            .iter()
            .map(|(bot_id, record)| self.build_status(bot_id.clone(), record))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn runtime_backend_writes_and_reads_config() {
        let root = tempdir().unwrap();
        let backend = LocalRuntimeBackend::new(root.path(), "backend-1");
        let cfg = BotRuntimeConfig {
            bot_id: BotId::new("10001"),
            config_path: root.path().join("runtime/config/bots/10001.json"),
            backend_kind: BackendKind::Local,
            flavor: BotFlavor::NapCat,
            runtime_target: RuntimeTarget::Local,
            launch_command: vec!["rustc".to_string(), "--version".to_string()],
            working_dir: None,
            log_path: None,
            environment: BTreeMap::new(),
        };

        backend
            .write_config(BotId::new("10001"), &cfg)
            .await
            .unwrap();
        let loaded = backend.read_config(BotId::new("10001")).await.unwrap();
        assert_eq!(loaded.bot_id.as_str(), "10001");
    }

    #[tokio::test]
    async fn runtime_backend_log_buffer_tails_latest_lines() {
        let root = tempdir().unwrap();
        let backend = LocalRuntimeBackend::new(root.path(), "backend-1");
        let bot_id = BotId::new("10002");

        backend.append_log_line(&bot_id, "first").await.unwrap();
        backend.append_log_line(&bot_id, "second").await.unwrap();

        let snapshot = backend.log_snapshot(&bot_id, 1).await.unwrap();
        assert_eq!(snapshot.lines, vec!["second".to_string()]);
        assert_eq!(snapshot.total_lines, 2);
    }

    #[test]
    fn runtime_backend_config_path_is_stable() {
        let root = PathBuf::from("C:/ProgramData/NapCatQQ Desktop");
        let backend = LocalRuntimeBackend::new(root.clone(), "backend-1");
        let path = backend.default_config_path(&BotId::new("10001"));
        assert!(path.ends_with("runtime/config/bots/10001.json"));
        assert!(path.starts_with(&root));
    }
}
