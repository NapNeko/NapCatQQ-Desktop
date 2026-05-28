use std::collections::{BTreeMap, HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::Mutex;

use crate::bot_actor::BotActorState;
use crate::bot_config::{BackendType, BotConfig};
use crate::events::{BroadcastEventBus, DomainEvent, EventBus};
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
    processes: Arc<Mutex<HashMap<BotId, ManagedProcess>>>,
    logs: Arc<Mutex<HashMap<BotId, RuntimeLogBuffer>>>,
    event_bus: Option<Arc<BroadcastEventBus>>,
}

#[derive(Debug)]
struct ManagedProcess {
    pid: u32,
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
            processes: Arc::new(Mutex::new(HashMap::new())),
            logs: Arc::new(Mutex::new(HashMap::new())),
            event_bus: None,
        }
    }

    /// 注入事件总线，用于发布 BotLogAppended / NapCatWebuiAvailable / BotProcessExited 事件。
    pub fn with_event_bus(mut self, bus: Arc<BroadcastEventBus>) -> Self {
        self.event_bus = Some(bus);
        self
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
        let mut status = BotStatus::running(bot_id, record.pid, record.started_at);
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
        // 只看内存 buffer。bot 退出 / 重启时 spawn_exit_watcher 会清掉，
        // 这样 BotLogPage 拿到的历史快照永远是当前实例的输出，不会混到上轮
        // crash 前的归档。磁盘 .log 文件继续按 append 模式写，留给用户事后
        // 从 runtime/log/bots/ 自行查阅，不再回填到 UI。
        let logs = self.logs.lock().await;
        if let Some(buffer) = logs.get(bot_id) {
            return Ok(buffer.snapshot(limit));
        }
        Ok(LogSnapshot {
            lines: Vec::new(),
            total_lines: 0,
        })
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
        // 必须捕获 stdout/stderr：legacy 从中解析 NapCat WebUI URL/token，
        // 也用作运行时日志来源。
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());

        // Windows 上避免弹出额外控制台窗口。
        #[cfg(windows)]
        {
            // CREATE_NO_WINDOW = 0x08000000
            command.creation_flags(0x0800_0000);
        }

        let mut child = command
            .spawn()
            .map_err(|error| BotBackendError::Io(error.to_string()))?;
        let pid = child.id().unwrap_or(0);
        let started_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // 取出 stdout/stderr，转交给后台读取任务。
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let bot_id = cfg.bot_id.clone();

        // 启动新进程前清掉这个 bot 旧的内存日志，让 BotLogPage 一开页拿到的是
        // 当前实例的输出，不会混入上一轮 crash 的历史行。
        {
            let mut guard = self.logs.lock().await;
            guard.remove(&bot_id);
        }

        if let Some(stream) = stdout {
            self.spawn_log_reader(bot_id.clone(), stream, "stdout");
        }
        if let Some(stream) = stderr {
            self.spawn_log_reader(bot_id.clone(), stream, "stderr");
        }
        self.spawn_exit_watcher(bot_id.clone(), child);

        let mut processes = self.processes.lock().await;
        processes.insert(
            bot_id.clone(),
            ManagedProcess {
                pid,
                started_at,
                config: cfg.clone(),
            },
        );

        Ok(BotStatus::running(bot_id, pid, started_at))
    }

    async fn stop(&self, bot_id: BotId, _mode: StopMode) -> Result<(), BotBackendError> {
        let record = {
            let mut processes = self.processes.lock().await;
            processes.remove(&bot_id)
        };
        // 没有运行中的记录视为已停止（exit watcher 可能已先一步清理），按幂等返回。
        let Some(record) = record else {
            return Ok(());
        };

        // NapCat 是注入器：NapCatWinBootMain.exe → QQ.exe → renderer 子进程。
        // 必须递归 kill 整个进程树，否则 QQ.exe 会残留。
        kill_process_tree(record.pid)
            .await
            .map_err(BotBackendError::Io)?;
        Ok(())
    }

    async fn status(&self, bot_id: BotId) -> Result<BotStatus, BotBackendError> {
        let processes = self.processes.lock().await;
        if let Some(record) = processes.get(&bot_id) {
            Ok(self.build_status(bot_id, record))
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

    /// 异步读取子进程的 stdout 或 stderr，按行解码后写入日志缓冲、磁盘日志，
    /// 并通过事件总线广播 `BotLogAppended` / `NapCatWebuiAvailable` 事件。
    fn spawn_log_reader<R>(&self, bot_id: BotId, stream: R, channel: &'static str)
    where
        R: tokio::io::AsyncRead + Send + Unpin + 'static,
    {
        let logs = Arc::clone(&self.logs);
        let log_path = self.log_path_for(&bot_id);
        let event_bus = self.event_bus.clone();

        tokio::spawn(async move {
            let mut reader = BufReader::new(stream);
            let mut buf: Vec<u8> = Vec::with_capacity(1024);
            loop {
                buf.clear();
                match reader.read_until(b'\n', &mut buf).await {
                    Ok(0) => break,
                    Ok(_) => {}
                    Err(_) => break,
                }
                // 去除尾部换行符。
                while matches!(buf.last(), Some(b'\n' | b'\r')) {
                    buf.pop();
                }
                if buf.is_empty() {
                    continue;
                }
                let line = decode_log_line(&buf);
                if line.is_empty() {
                    continue;
                }

                {
                    let mut guard = logs.lock().await;
                    guard.entry(bot_id.clone()).or_default().push_text(&line);
                }

                // 写文件失败不致命，仅打日志。
                if let Some(parent) = log_path.parent() {
                    let _ = tokio::fs::create_dir_all(parent).await;
                }
                if let Ok(mut file) = tokio::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&log_path)
                    .await
                {
                    let _ = file.write_all(line.as_bytes()).await;
                    let _ = file.write_all(b"\n").await;
                }

                if let Some(bus) = event_bus.as_ref() {
                    bus.publish(DomainEvent::BotLogAppended {
                        bot_id: bot_id.clone(),
                        line: line.clone(),
                        channel: Some(channel.to_string()),
                    });

                    if let Some((port, token)) = parse_napcat_webui_line(&line) {
                        bus.publish(DomainEvent::napcat_webui_available(
                            bot_id.clone(),
                            port,
                            token,
                        ));
                    }
                }
            }
        });
    }

    /// 监听子进程退出。退出后从 processes 中移除记录，并广播 `BotProcessExited`。
    fn spawn_exit_watcher(&self, bot_id: BotId, mut child: tokio::process::Child) {
        let processes = Arc::clone(&self.processes);
        let logs = Arc::clone(&self.logs);
        let event_bus = self.event_bus.clone();

        tokio::spawn(async move {
            let result = child.wait().await;
            {
                let mut guard = processes.lock().await;
                guard.remove(&bot_id);
            }
            // 进程退出后清空内存日志缓冲，避免 bot 重启后老日志残留在 UI 上。
            // 写盘的 .log 文件不动 —— 那是用户手动追溯崩溃时用的归档，需要保留。
            // BotLogPage 走 tail_log 拉历史时，如果内存里没有就从磁盘文件加载，
            // 重启后第一次拉到的会是上一轮残留的尾部，但上面 spawn_log_reader
            // 同时会在新进程启动后立刻写新行，UI 上就是旧尾部 + 新行，既保留崩前
            // 上下文又不会重复增量。
            {
                let mut guard = logs.lock().await;
                guard.remove(&bot_id);
            }
            if let Some(bus) = event_bus.as_ref() {
                let (exit_code, reason) = match result {
                    Ok(status) => (status.code(), None),
                    Err(err) => (None, Some(format!("wait failed: {err}"))),
                };
                bus.publish(DomainEvent::bot_process_exited(bot_id, exit_code, reason));
            }
        });
    }
}

/// 按 GBK / UTF-8 解码 NapCat 子进程的一行原始字节，并清洗 ANSI 转义。
///
/// NapCatWinBootMain.exe 在中文 Windows 上输出 GBK；其它平台默认 UTF-8。
/// 优先尝试 UTF-8，失败再退回 GBK；都失败则使用 lossy UTF-8。
///
/// Linux 上 NapCat 通过 logger 输出会带 `\x1b[32m` 之类颜色转义，直接渲染会
/// 在 UI 上残留 tofu 字符并干扰 LogHighlighter 的级别匹配（参考 legacy
/// `_sanitize_log_text` 的处理）。这里在解码后做一次 ANSI 清洗。
fn decode_log_line(raw: &[u8]) -> String {
    let decoded = if let Ok(s) = std::str::from_utf8(raw) {
        s.to_string()
    } else {
        #[cfg(windows)]
        {
            let (cow, _, had_errors) = encoding_rs::GBK.decode(raw);
            if !had_errors {
                cow.into_owned()
            } else {
                String::from_utf8_lossy(raw).into_owned()
            }
        }
        #[cfg(not(windows))]
        {
            String::from_utf8_lossy(raw).into_owned()
        }
    };
    strip_ansi_escapes(&decoded)
}

/// 移除字符串中的 ANSI 转义序列（CSI、OSC、单字符 ESC 命令）。
///
/// 对齐 legacy 正则 `\x1B(?:[@-Z\\-_]|\[[0-?]*[ -/]*[@-~])` 的覆盖范围，
/// 但用 byte 级状态机实现，不引入 regex 依赖：
///
/// - `ESC [` 进入 CSI：吞掉参数字节(`0x30..=0x3F`) 和 intermediate (`0x20..=0x2F`)，
///   到 final 字节 (`0x40..=0x7E`) 为止。
/// - `ESC ]` / `ESC P` 等开 string-context：吞到 BEL (`\x07`) 或 `ESC \` (ST)。
/// - 其它 `ESC X`：直接丢弃 ESC 和 X 两字节。
///
/// 不破坏多字节 UTF-8：ESC = 0x1B，UTF-8 continuation 字节均 ≥ 0x80，不冲突。
fn strip_ansi_escapes(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b != 0x1B {
            out.push(b);
            i += 1;
            continue;
        }
        // 进入 ESC 处理。
        i += 1;
        if i >= bytes.len() {
            // 末尾孤立 ESC，丢弃。
            break;
        }
        match bytes[i] {
            b'[' => {
                // CSI: 跳过参数 + intermediate + final。
                i += 1;
                while i < bytes.len() && (0x30..=0x3F).contains(&bytes[i]) {
                    i += 1;
                }
                while i < bytes.len() && (0x20..=0x2F).contains(&bytes[i]) {
                    i += 1;
                }
                if i < bytes.len() && (0x40..=0x7E).contains(&bytes[i]) {
                    i += 1;
                }
            }
            b']' | b'P' | b'X' | b'^' | b'_' => {
                // string-context: OSC/DCS/SOS/PM/APC，吞到 BEL 或 ESC \\。
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == 0x07 {
                        i += 1;
                        break;
                    }
                    if bytes[i] == 0x1B && i + 1 < bytes.len() && bytes[i + 1] == b'\\' {
                        i += 2;
                        break;
                    }
                    i += 1;
                }
            }
            _ => {
                // 其它两字节命令（包括 `ESC @-Z`, `ESC \\`, `ESC -_`），整体丢弃。
                i += 1;
            }
        }
    }
    // bytes 路径下 out 是有效 UTF-8（ESC 是单字节 ASCII，且所有跳过的字节都 ≤ 0x7E
    // 都是 ASCII；不会拆裂多字节字符）。
    String::from_utf8(out)
        .unwrap_or_else(|err| String::from_utf8_lossy(err.as_bytes()).into_owned())
}

/// 从一行 NapCat stdout 解析出 WebUI 登录入口的 (port, token)。
///
/// 对齐 legacy 正则：
/// `\[info\] \[NapCat\] \[WebUi\] WebUi User Panel Url: http://127\.0\.0\.1:(\d+)/webui\?token=(\S+)`
fn parse_napcat_webui_line(line: &str) -> Option<(u16, String)> {
    let needle = "[info] [NapCat] [WebUi] WebUi User Panel Url: http://127.0.0.1:";
    let idx = line.find(needle)?;
    let rest = &line[idx + needle.len()..];
    let (port_str, after_port) = rest.split_once("/webui?token=")?;
    let port: u16 = port_str.trim().parse().ok()?;
    let token: String = after_port
        .chars()
        .take_while(|c| !c.is_whitespace())
        .collect();
    if token.is_empty() {
        return None;
    }
    Some((port, token))
}

/// 递归 kill 进程树（NapCatWinBootMain.exe → QQ.exe → renderer 链）。
///
/// - Windows 用 `taskkill /F /T /PID <pid>`，由系统按进程树递归终止。
/// - Unix 用 `kill -KILL -<pid>`（进程组）；失败则退回 `kill -KILL <pid>`。
async fn kill_process_tree(pid: u32) -> Result<(), String> {
    if pid == 0 {
        return Ok(());
    }
    #[cfg(windows)]
    {
        let mut command = tokio::process::Command::new("taskkill");
        command.arg("/F").arg("/T").arg("/PID").arg(pid.to_string());
        // taskkill 自身也用 CREATE_NO_WINDOW，避免闪窗。
        command.creation_flags(0x0800_0000);
        let output = command
            .output()
            .await
            .map_err(|err| format!("taskkill spawn failed: {err}"))?;
        if !output.status.success() {
            // 进程已退出（128）通常不是错误。
            let code = output.status.code().unwrap_or(-1);
            if code == 128 {
                return Ok(());
            }
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!(
                "taskkill exited with code {code}: {}",
                stderr.trim()
            ));
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let mut command = tokio::process::Command::new("kill");
        command.arg("-KILL").arg(format!("-{pid}"));
        let output = command
            .output()
            .await
            .map_err(|err| format!("kill spawn failed: {err}"))?;
        if !output.status.success() {
            let mut fallback = tokio::process::Command::new("kill");
            fallback.arg("-KILL").arg(pid.to_string());
            let fb = fallback
                .output()
                .await
                .map_err(|err| format!("kill fallback spawn failed: {err}"))?;
            if !fb.status.success() {
                let stderr = String::from_utf8_lossy(&fb.stderr);
                return Err(format!("kill failed: {}", stderr.trim()));
            }
        }
        Ok(())
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
        let root = PathBuf::from("test-data").join("NapCatQQ Desktop");
        let backend = LocalRuntimeBackend::new(root.clone(), "backend-1");
        let path = backend.default_config_path(&BotId::new("10001"));
        assert!(path.ends_with("runtime/config/bots/10001.json"));
        assert!(path.starts_with(&root));
    }

    #[test]
    fn parse_napcat_webui_line_extracts_port_and_token() {
        let line = "[2026-05-23 10:00:00.000] [info] [NapCat] [WebUi] WebUi User Panel Url: http://127.0.0.1:6099/webui?token=abc123XYZ";
        let parsed = parse_napcat_webui_line(line).expect("expected to parse");
        assert_eq!(parsed.0, 6099);
        assert_eq!(parsed.1, "abc123XYZ");
    }

    #[test]
    fn parse_napcat_webui_line_stops_at_whitespace_in_token() {
        let line = "[info] [NapCat] [WebUi] WebUi User Panel Url: http://127.0.0.1:6099/webui?token=tok123 trailing comment";
        let parsed = parse_napcat_webui_line(line).expect("expected to parse");
        assert_eq!(parsed.0, 6099);
        assert_eq!(parsed.1, "tok123");
    }

    #[test]
    fn parse_napcat_webui_line_returns_none_for_unrelated_line() {
        assert!(parse_napcat_webui_line("[info] [Core] starting").is_none());
        // 缺 token 部分
        assert!(
            parse_napcat_webui_line(
                "[info] [NapCat] [WebUi] WebUi User Panel Url: http://127.0.0.1:6099/webui?token="
            )
            .is_none()
        );
    }

    #[test]
    fn decode_log_line_handles_utf8_passthrough() {
        let bytes = "你好 NapCat".as_bytes();
        assert_eq!(decode_log_line(bytes), "你好 NapCat");
    }

    #[test]
    fn strip_ansi_escapes_removes_color_csi_sequences() {
        // legacy NapCat Linux 端常见的彩色级别标签
        let input = "\x1b[32m[info]\x1b[0m hello";
        assert_eq!(strip_ansi_escapes(input), "[info] hello");
    }

    #[test]
    fn strip_ansi_escapes_removes_cursor_csi_with_params() {
        // 带参数 + intermediate 的 CSI
        let input = "before\x1b[1;31mred\x1b[0;39mafter\x1b[2J";
        assert_eq!(strip_ansi_escapes(input), "beforeredafter");
    }

    #[test]
    fn strip_ansi_escapes_removes_osc_with_bel_and_st() {
        // OSC 通常用于设置窗口标题，分别用 BEL 和 ESC\\ 终止
        let bel = "pre\x1b]0;title\x07post";
        assert_eq!(strip_ansi_escapes(bel), "prepost");
        let st = "pre\x1b]0;title\x1b\\post";
        assert_eq!(strip_ansi_escapes(st), "prepost");
    }

    #[test]
    fn strip_ansi_escapes_drops_two_byte_esc_commands() {
        // ESC = 选字符集；ESC c 重置
        let input = "a\x1b=b\x1bcc";
        assert_eq!(strip_ansi_escapes(input), "abc");
    }

    #[test]
    fn strip_ansi_escapes_preserves_multibyte_utf8() {
        // 多字节字符的 continuation byte ≥ 0x80，不会被状态机吞掉
        let input = "\x1b[32m你好\x1b[0m";
        assert_eq!(strip_ansi_escapes(input), "你好");
    }

    #[test]
    fn decode_log_line_strips_ansi_after_decoding() {
        // 模拟 Linux 上 NapCat logger 输出的彩色 [info] 标签
        let raw = b"\x1b[32m[info]\x1b[0m [NapCat] starting";
        assert_eq!(decode_log_line(raw), "[info] [NapCat] starting");
    }

    #[cfg(windows)]
    #[test]
    fn decode_log_line_falls_back_to_gbk_on_windows() {
        // GBK encoded "你好" = 0xC4 0xE3 0xBA 0xC3
        let bytes: &[u8] = &[0xC4, 0xE3, 0xBA, 0xC3];
        let decoded = decode_log_line(bytes);
        assert_eq!(decoded, "你好");
    }

    #[tokio::test]
    async fn runtime_backend_stop_is_idempotent_when_process_already_gone() {
        let root = tempdir().unwrap();
        let backend = LocalRuntimeBackend::new(root.path(), "backend-1");
        // 不存在的 bot_id 调用 stop 应当幂等成功，不再返回 ProcessNotFound。
        backend
            .stop(BotId::new("ghost"), StopMode::Force)
            .await
            .expect("stop should be idempotent");
    }

    #[tokio::test]
    async fn runtime_backend_publishes_log_and_exit_events() {
        use crate::events::{BroadcastEventBus, DomainEventKind, EventFilter};

        let root = tempdir().unwrap();
        let bus = BroadcastEventBus::default();
        let backend = LocalRuntimeBackend::new(root.path(), "backend-1")
            .with_event_bus(Arc::new(bus.clone()));

        // 用一个能立刻退出且打印一行 NapCat-like log 的命令。
        // Windows 下用 cmd /C echo，其他平台用 sh -c。
        #[cfg(windows)]
        let (program, args) = (
            "cmd".to_string(),
            vec![
                "/C".to_string(),
                "echo [info] [NapCat] [WebUi] WebUi User Panel Url: http://127.0.0.1:6099/webui?token=t"
                    .to_string(),
            ],
        );
        #[cfg(not(windows))]
        let (program, args) = (
            "sh".to_string(),
            vec![
                "-c".to_string(),
                "echo '[info] [NapCat] [WebUi] WebUi User Panel Url: http://127.0.0.1:6099/webui?token=t'"
                    .to_string(),
            ],
        );

        let cfg = BotRuntimeConfig {
            bot_id: BotId::new("10100"),
            config_path: root.path().join("runtime/config/bots/10100.json"),
            backend_kind: BackendKind::Local,
            flavor: BotFlavor::NapCat,
            runtime_target: RuntimeTarget::Local,
            launch_command: std::iter::once(program).chain(args).collect(),
            working_dir: None,
            log_path: None,
            environment: BTreeMap::new(),
        };

        let mut log_sub = bus.subscribe(EventFilter::kind(DomainEventKind::BotLogAppended));
        let mut webui_sub = bus.subscribe(EventFilter::kind(DomainEventKind::NapCatWebuiAvailable));
        let mut exit_sub = bus.subscribe(EventFilter::kind(DomainEventKind::BotProcessExited));

        backend.start(&BotStartCtx { config: cfg }).await.unwrap();

        // 给 reader/exit watcher 留点时间。
        let log_event = tokio::time::timeout(std::time::Duration::from_secs(5), log_sub.next())
            .await
            .expect("log event timeout")
            .expect("log event closed");
        match log_event {
            DomainEvent::BotLogAppended { bot_id, line, .. } => {
                assert_eq!(bot_id.as_str(), "10100");
                assert!(line.contains("WebUi User Panel Url"));
            }
            other => panic!("unexpected event: {other:?}"),
        }

        let webui_event = tokio::time::timeout(std::time::Duration::from_secs(5), webui_sub.next())
            .await
            .expect("webui event timeout")
            .expect("webui event closed");
        match webui_event {
            DomainEvent::NapCatWebuiAvailable {
                bot_id,
                port,
                token,
            } => {
                assert_eq!(bot_id.as_str(), "10100");
                assert_eq!(port, 6099);
                assert_eq!(token, "t");
            }
            other => panic!("unexpected event: {other:?}"),
        }

        let exit_event = tokio::time::timeout(std::time::Duration::from_secs(5), exit_sub.next())
            .await
            .expect("exit event timeout")
            .expect("exit event closed");
        match exit_event {
            DomainEvent::BotProcessExited { bot_id, .. } => {
                assert_eq!(bot_id.as_str(), "10100");
            }
            other => panic!("unexpected event: {other:?}"),
        }

        // 进程退出后 status 应自动转回 Stopped（exit watcher 已移除记录）。
        let status = backend.status(BotId::new("10100")).await.unwrap();
        assert_eq!(status.state, BotActorState::Stopped);
    }
}
