use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use async_trait::async_trait;

use crate::bot_config::{BackendType, BotConfig};
use crate::ids::BotId;
use crate::kinds::RuntimeTarget;
use crate::runtime_backend::BotRuntimeConfig;

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
    #[error("required runtime file missing: {message}; checked path: {}", checked_path.display())]
    MissingFile {
        message: String,
        checked_path: PathBuf,
    },
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
        Err(RuntimeLaunchPlanError::MissingFile {
            message: message.to_string(),
            checked_path: path.to_path_buf(),
        })
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
