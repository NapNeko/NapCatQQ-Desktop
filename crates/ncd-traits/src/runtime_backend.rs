// BotBackend trait + 相关运行时类型定义
//
// 从 ncd-runtime/src/runtime_backend.rs 下沉到此处（Layer 2 接口契约）。
// 具体实装（NativeDeploymentBackend, RemoteSnowLumaBackend 等）留在 ncd-runtime。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

pub use ncd_domain::StopMode;
pub use ncd_domain::bot_status::{BotStatus, ProcessHandle};
use ncd_domain::bot_config::{BackendType, BotConfig};
use ncd_domain::ids::BotId;
use ncd_domain::kinds::{BackendKind, BotFlavor, RuntimeTarget};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BotStartCtx {
    pub config: BotRuntimeConfig,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bot_config: Option<BotConfig>,
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
    #[error("remote host transport error: {0}")]
    RemoteHostTransport(String),
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
