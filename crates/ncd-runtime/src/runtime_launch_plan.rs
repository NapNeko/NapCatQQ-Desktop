use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use async_trait::async_trait;

use crate::bot_config::{BackendType, BotConfig};
use crate::ids::BotId;
use crate::kinds::RuntimeTarget;
use crate::runtime_backend::BotRuntimeConfig;
use crate::snowluma::SnowLumaStartMode;

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
    snowluma_data_root: Option<PathBuf>,
    /// SnowLuma daemon 安装根（含 `node.exe` 与 entry 脚本）。
    /// `None` 时回落到 `<runtime_root>/snowluma`。注意：这与 NapCat 的
    /// `runtime_root` 不同——NapCat 直接装在 runtime 根，SnowLuma 装在子目录。
    snowluma_runtime_root: Option<PathBuf>,
}

impl FileSystemRuntimeLaunchPlanner {
    /// 仅 NapCat 路径所需的最小构造（保留向后兼容）。SnowLuma 路径未注入
    /// `snowluma_data_root` / `snowluma_runtime_root`，调用 SnowLuma 分支会
    /// 回落到 `<runtime_root>/snowluma`。
    pub fn new(runtime_root: impl Into<PathBuf>) -> Self {
        Self {
            runtime_root: runtime_root.into(),
            snowluma_data_root: None,
            snowluma_runtime_root: None,
        }
    }

    /// 注入 SnowLuma 持久化数据根目录（对齐红线 4.1：由 `bootstrap::resolve_data_root`
    /// 单一权威派生，本结构体只是消费者）。
    pub fn with_snowluma_data_root(mut self, data_root: impl Into<PathBuf>) -> Self {
        self.snowluma_data_root = Some(data_root.into());
        self
    }

    /// 注入 SnowLuma daemon 安装根（含 `node.exe`）。
    /// 与 NapCat `runtime_root` 严格分离。
    pub fn with_snowluma_runtime_root(mut self, runtime_root: impl Into<PathBuf>) -> Self {
        self.snowluma_runtime_root = Some(runtime_root.into());
        self
    }
}

#[async_trait]
impl RuntimeLaunchPlanner for FileSystemRuntimeLaunchPlanner {
    async fn build_plan(
        &self,
        bot_id: &BotId,
        config: &BotConfig,
    ) -> Result<RuntimeLaunchPlan, RuntimeLaunchPlanError> {
        let snowluma_data_root = self
            .snowluma_data_root
            .clone()
            .unwrap_or_else(|| self.runtime_root.join("snowluma"));
        let snowluma_runtime_root = self
            .snowluma_runtime_root
            .clone()
            .unwrap_or_else(|| self.runtime_root.join("snowluma"));
        build_runtime_launch_plan(
            bot_id,
            config,
            &self.runtime_root,
            &snowluma_runtime_root,
            &snowluma_data_root,
        )
        .await
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

/// SnowLuma 启动计划。
/// 路径来源：所有路径都必须由调用方（最终来自
/// `bootstrap::resolve_data_root` 与 PathProbe）传入；本 struct 不在
/// 业务模块内硬编码 `%ProgramData%` / `%LocalAppData%`。
/// 字段语义：
/// - `runtime_root`：SnowLuma 安装根，含 `node.exe` 与 daemon entry 脚本。
/// - `snowluma_data_root`：SnowLuma 持久化数据根（`<data_root>/snowluma`）
/// 存放 `app-config.json` / `session.json` / per-Bot `onebot_<uin>.json` 等。
/// - `start_mode`：本次启动是 ColdStart（backend spawn QQ.exe）还是 HotStart
/// （attach 到用户已开的 QQ.exe）。
/// - `qq_install_path`：仅 ColdStart 路径需要；HotStart 留 `None`。
/// - `bot_qq_id`：渲染 per-Bot OneBot 配置文件名时使用。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnowLumaLaunchPlan {
    pub runtime_root: PathBuf,
    pub snowluma_data_root: PathBuf,
    pub start_mode: SnowLumaStartMode,
    pub qq_install_path: Option<PathBuf>,
    pub bot_qq_id: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum RuntimeLaunchPlanError {
    #[error("unsupported runtime target: {0:?}")]
    UnsupportedTarget(RuntimeTarget),
    #[error("snowluma node.exe missing at {}", .0.display())]
    SnowLumaNodeMissing(PathBuf),
    #[error("snowluma start mode invalid: {0}")]
    SnowLumaInvalidStartMode(String),
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
                // SnowLuma 启动语义由 SnowLumaRuntimeBackend 自己处理（spawn QQ.exe +
                // daemon load_process），不通过 BotRuntimeConfig.launch_command 触发外部
                // 进程。working_dir 设为 snowluma_data_root，便于运行时调试与日志关联。
                cfg.launch_command = Vec::new();
                cfg.working_dir = Some(plan.snowluma_data_root);
                // SnowLumaRuntimeBackend.start 通过 environment 渠道拿 QQ.exe 路径
                // / start_mode（避免给 BotRuntimeConfig 加新字段）。
                // HotStart 不再透传 attach_pid，由 backend 在 Phase A 自动按 qq_id
                // 扫一遍系统进程 + tencent:// 探测匹配出真实 PID。
                if let Some(qq_install) = plan.qq_install_path {
                    cfg.environment.insert(
                        "SNOWLUMA_QQ_EXE".to_string(),
                        qq_install.join("QQ.exe").to_string_lossy().into_owned(),
                    );
                }
                // 把 qq_id 也注入环境变量，让 backend 在 HotStart 时自动匹配；
                // ColdStart 也用得到（落盘配置文件名等）。
                cfg.environment
                    .insert("SNOWLUMA_QQ_ID".to_string(), plan.bot_qq_id.to_string());
                let mode_str = match plan.start_mode {
                    SnowLumaStartMode::ColdStart => "cold_start",
                    SnowLumaStartMode::HotStart => "hot_start",
                };
                cfg.environment
                    .insert("SNOWLUMA_START_MODE".to_string(), mode_str.to_string());
            }
        }
        cfg
    }
}

pub async fn build_runtime_launch_plan(
    bot_id: &BotId,
    config: &BotConfig,
    runtime_root: impl AsRef<Path>,
    snowluma_runtime_root: impl AsRef<Path>,
    snowluma_data_root: impl AsRef<Path>,
) -> Result<RuntimeLaunchPlan, RuntimeLaunchPlanError> {
    match config.bot.backend_type {
        BackendType::NapCat => {
            build_napcat_launch_plan(bot_id, config, runtime_root.as_ref()).await
        }
        BackendType::SnowLuma => {
            build_snowluma_launch_plan(
                config,
                snowluma_runtime_root.as_ref(),
                snowluma_data_root.as_ref(),
            )
            .await
        }
    }
}

/// 构造 SnowLuma 启动计划（COLD / HOT 两路）。
/// 决策流程：
/// 1. 从 `BotConfig.bot.snowluma_start_mode` 读出 start_mode；缺省（`None`）
/// 回退到 `SnowLumaStartMode::ColdStart`（设计文档约定的默认行为）。
/// 2. 任何模式都校验 `<runtime_root>/node.exe` 是常规文件存在。
/// 3. ColdStart 解析 QQ install path（Windows 注册表）；HotStart 跳过。
async fn build_snowluma_launch_plan(
    config: &BotConfig,
    snowluma_runtime_root: &Path,
    snowluma_data_root: &Path,
) -> Result<RuntimeLaunchPlan, RuntimeLaunchPlanError> {
    let start_mode = config
        .bot
        .snowluma_start_mode
        .unwrap_or(SnowLumaStartMode::ColdStart);

    // node.exe 是 daemon 的二进制入口，任何模式下都必须存在。
    // 注意：这里读的是 SnowLuma 自己的安装根（`<data_root>/runtime/snowluma`）
    // 与 NapCat 的 `runtime_root` 严格分离。
    let node_path = snowluma_runtime_root.join("node.exe");
    if !is_regular_file(&node_path).await {
        return Err(RuntimeLaunchPlanError::SnowLumaNodeMissing(node_path));
    }

    let qq_install_path = match start_mode {
        SnowLumaStartMode::ColdStart => Some(resolve_qq_install_path()?),
        SnowLumaStartMode::HotStart => {
            // HotStart 自带 PID 自动匹配语义（backend Phase A 按 qq_id 扫进程），
            // 这里不需要 QQ install path：用户手动启动了 QQ，QQ 路径已经定了。
            None
        }
    };

    Ok(RuntimeLaunchPlan::SnowLuma(SnowLumaLaunchPlan {
        runtime_root: snowluma_runtime_root.to_path_buf(),
        snowluma_data_root: snowluma_data_root.to_path_buf(),
        start_mode,
        qq_install_path,
        bot_qq_id: config.bot.qq_id,
    }))
}

async fn is_regular_file(path: &Path) -> bool {
    match tokio::fs::metadata(path).await {
        Ok(meta) => meta.is_file(),
        Err(_) => false,
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

#[cfg(test)]
mod snowluma_plan_tests {
    //! `SnowLumaLaunchPlan` 字段扩展锁定测试。
    //! 覆盖：
    //! 1. `node.exe` 缺失立即返回 `SnowLumaNodeMissing`。
    //! 2. ColdStart（含 `None` 默认）携带 `qq_install_path = Some(_)`（仅
    //! Windows 平台能解析 QQ install path；非 Windows 退化为 UnsupportedPlatform）。
    //! 3. HotStart 跳过 QQ install 解析，`qq_install_path = None`，PID 由
    //! backend Phase A 自动按 qq_id 匹配，落盘配置不再持久化 PID。
    //! 4. `into_runtime_config` 把 SnowLuma working_dir 设到 snowluma_data_root
    //! 且 launch_command 为空。

    use super::*;
    use crate::bot_config::{
        AdvancedConfig, AutoRestartSchedule, BotBasicConfig, BotConfig, ConnectConfig,
        DeploymentType,
    };
    use crate::ids::BotId;
    use crate::kinds::RuntimeTarget;
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn make_config(start_mode: Option<SnowLumaStartMode>) -> BotConfig {
        BotConfig {
            bot: BotBasicConfig {
                name: "snowluma-bot".to_string(),
                qq_id: 100200,
                music_sign_url: String::new(),
                auto_restart_schedule: AutoRestartSchedule::default(),
                offline_auto_restart: false,
                runtime_target: RuntimeTarget::Local,
                backend_type: BackendType::SnowLuma,
                deployment_type: DeploymentType::Native,
                snowluma_start_mode: start_mode,
            },
            connect: ConnectConfig::default(),
            advanced: AdvancedConfig::default(),
        }
    }

    /// `node.exe` 缺失（runtime_root 是空目录）：必须立即返回 `SnowLumaNodeMissing`
    /// 携带的路径精确指向缺失的 `<runtime_root>/node.exe`。
    #[tokio::test]
    async fn snowluma_plan_rejects_missing_node_exe() {
        let runtime_root_dir = tempdir().unwrap();
        let data_root_dir = tempdir().unwrap();
        let runtime_root = runtime_root_dir.path();
        let data_root = data_root_dir.path();
        let bot_id = BotId::from("bot-1");
        let config = make_config(Some(SnowLumaStartMode::ColdStart));

        let result =
            build_runtime_launch_plan(&bot_id, &config, runtime_root, runtime_root, data_root)
                .await;

        match result {
            Err(RuntimeLaunchPlanError::SnowLumaNodeMissing(path)) => {
                assert_eq!(path, runtime_root.join("node.exe"));
            }
            other => panic!("expected SnowLumaNodeMissing, got {other:?}"),
        }
    }

    /// HotStart 路径：跳过 QQ install path 解析（即便 Windows 注册表查询失败也应该 OK）。
    /// `qq_install_path = None`，`bot_qq_id` 与 `snowluma_data_root` 透传。
    /// PID 不再持久化，由 backend 在 Phase A 按 qq_id 自动匹配。
    #[tokio::test]
    async fn snowluma_plan_hot_start_skips_qq_install_resolution() {
        let runtime_root_dir = tempdir().unwrap();
        let data_root_dir = tempdir().unwrap();
        let runtime_root = runtime_root_dir.path();
        let data_root = data_root_dir.path();
        tokio::fs::write(runtime_root.join("node.exe"), b"stub")
            .await
            .unwrap();

        let bot_id = BotId::from("bot-1");
        let config = make_config(Some(SnowLumaStartMode::HotStart));

        let plan =
            build_runtime_launch_plan(&bot_id, &config, runtime_root, runtime_root, data_root)
                .await
                .expect("hot start plan");

        let RuntimeLaunchPlan::SnowLuma(plan) = plan else {
            panic!("expected SnowLuma plan");
        };
        assert_eq!(plan.runtime_root, runtime_root);
        assert_eq!(plan.snowluma_data_root, data_root);
        assert_eq!(plan.start_mode, SnowLumaStartMode::HotStart);
        assert!(plan.qq_install_path.is_none());
        assert_eq!(plan.bot_qq_id, 100200);
    }

    /// 在非 Windows 平台 ColdStart 路径会因 QQ install 注册表查询失败返回
    /// `UnsupportedPlatform`；Windows 平台才走真实分支。本测试仅断言"非 Windows
    /// 上确实在 ColdStart 时调用了注册表解析（因此返回 UnsupportedPlatform）"
    /// 用以验证 ColdStart 与 HotStart 的分支差异。
    #[cfg(not(windows))]
    #[tokio::test]
    async fn snowluma_plan_cold_start_attempts_qq_install_lookup() {
        let runtime_root_dir = tempdir().unwrap();
        let data_root_dir = tempdir().unwrap();
        let runtime_root = runtime_root_dir.path();
        let data_root = data_root_dir.path();
        tokio::fs::write(runtime_root.join("node.exe"), b"stub")
            .await
            .unwrap();

        let bot_id = BotId::from("bot-1");
        let config = make_config(Some(SnowLumaStartMode::ColdStart));

        let result =
            build_runtime_launch_plan(&bot_id, &config, runtime_root, runtime_root, data_root)
                .await;

        match result {
            Err(RuntimeLaunchPlanError::UnsupportedPlatform(_)) => {}
            other => panic!("expected UnsupportedPlatform on non-windows, got {other:?}"),
        }
    }

    /// `None` start_mode 走 ColdStart 默认行为：与显式 ColdStart 走同一分支。
    /// 本测试只验证默认 fallback 触发了 ColdStart 的 QQ install 解析（在非 Windows
    /// 上会失败为 UnsupportedPlatform，与显式 ColdStart 行为一致）。
    #[cfg(not(windows))]
    #[tokio::test]
    async fn snowluma_plan_defaults_to_cold_start_when_unset() {
        let runtime_root_dir = tempdir().unwrap();
        let data_root_dir = tempdir().unwrap();
        let runtime_root = runtime_root_dir.path();
        let data_root = data_root_dir.path();
        tokio::fs::write(runtime_root.join("node.exe"), b"stub")
            .await
            .unwrap();

        let bot_id = BotId::from("bot-1");
        let config = make_config(None);

        let result =
            build_runtime_launch_plan(&bot_id, &config, runtime_root, runtime_root, data_root)
                .await;

        match result {
            Err(RuntimeLaunchPlanError::UnsupportedPlatform(_)) => {}
            other => panic!(
                "expected UnsupportedPlatform from ColdStart fallback on non-windows, got {other:?}"
            ),
        }
    }

    /// `into_runtime_config` SnowLuma 分支：launch_command 必须清空
    /// working_dir 必须设为 snowluma_data_root（不是 runtime_root）。
    #[test]
    fn snowluma_into_runtime_config_uses_data_root_as_working_dir() {
        let runtime_root = PathBuf::from("C:/SnowLumaRuntime");
        let data_root = PathBuf::from("C:/ProgramData/NapCatQQ Desktop/snowluma");
        let plan = RuntimeLaunchPlan::SnowLuma(SnowLumaLaunchPlan {
            runtime_root: runtime_root.clone(),
            snowluma_data_root: data_root.clone(),
            start_mode: SnowLumaStartMode::ColdStart,
            qq_install_path: Some(PathBuf::from("C:/QQ")),
            bot_qq_id: 100200,
        });

        let cfg = BotRuntimeConfig::default_path(PathBuf::from("/tmp"), BotId::from("bot-1"));
        let result = plan.into_runtime_config(cfg);

        assert!(
            result.launch_command.is_empty(),
            "launch_command must be empty"
        );
        assert_eq!(result.working_dir, Some(data_root));
    }
}
