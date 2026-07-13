//! Bot 配置加载与部署状态映射

use std::path::{Path, PathBuf};

use ncd_deploy::DeploymentState;
use ncd_domain::bot_status::BotStatus;
use ncd_domain::ids::BotId;
use ncd_domain::{BackendType, BotConfig, BotFlavor, RuntimeTarget};
use ncd_traits::runtime_backend::{BotBackendError, BotStartCtx};
use serde_json::{Map, Value, json};

use crate::bot_actor::BotActorState;

pub(crate) fn minimal_bot_config(qq_id: u64, flavor: BotFlavor) -> BotConfig {
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

pub(crate) fn bot_config_for_start(
    ctx: &BotStartCtx,
    flavor: BotFlavor,
    require_real: bool,
) -> Result<BotConfig, BotBackendError> {
    if let Some(ref cfg) = ctx.bot_config {
        return Ok(cfg.clone());
    }
    real_bot_config_from_ctx(ctx, flavor, require_real)
}

pub(crate) fn real_bot_config_from_ctx(
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

pub(crate) fn data_root_from_config_path(
    runtime_config_path: &Path,
    bot_id: &BotId,
) -> Option<PathBuf> {
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

pub(crate) fn status_for_deployment_state(bot_id: BotId, state: DeploymentState) -> BotStatus {
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
