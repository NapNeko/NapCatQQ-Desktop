//! 用户会话中的 NapCat 运行器，复用现有进程树和日志实现。
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use ncd_deploy::{
    Deployment, DeploymentError, DeploymentHandle, EventBusSink, NativeDeployment,
    NativeLaunchCommand, NativeLaunchTranslator,
};
use ncd_domain::windows_bot::WindowsBotSpec;
use ncd_domain::{BackendKind, BotConfig, BotFlavor, BotId, BotStatus, LogSnapshot, StopMode};
use ncd_host::local::LocalWindowsHost;
use ncd_traits::events::BroadcastEventBus;
use ncd_traits::runtime_backend::{
    BotBackend, BotBackendError, BotRuntimeConfig, BotStartCtx, TailOpts,
};

struct Translator(WindowsBotSpec);

#[async_trait]
impl NativeLaunchTranslator for Translator {
    async fn translate(&self, _: &BotConfig) -> Result<NativeLaunchCommand, DeploymentError> {
        let directory = PathBuf::from(&self.0.napcat_dir);
        for name in [
            "NapCatWinBootMain.exe",
            "NapCatWinBootHook.dll",
            "napcat.mjs",
            "qqnt.json",
        ] {
            if !directory.join(name).is_file() {
                return Err(DeploymentError::LaunchFailed(format!(
                    "NapCat 文件缺失：{name}"
                )));
            }
        }
        let entry = url::Url::from_file_path(directory.join("napcat.mjs"))
            .map_err(|_| DeploymentError::LaunchFailed("NapCat 路径无法转换为文件 URL".into()))?;
        let load = directory.join("loadNapCat.js");
        let mut script = String::new();
        for (key, value) in &self.0.metrics_environment {
            if key.starts_with("NCD_METRICS_") {
                script.push_str(&format!(
                    "process.env[{}]={};\n",
                    serde_json::to_string(key).unwrap(),
                    serde_json::to_string(value).unwrap()
                ));
            }
        }
        if let Some(probe) = self.0.metrics_environment.get("NCD_METRICS_PROBE_PATH") {
            script.push_str(&format!(
                "require({});\n",
                serde_json::to_string(probe).unwrap()
            ));
        }
        script.push_str(&format!(
            "import({});",
            serde_json::to_string(entry.as_str())
                .map_err(|error| DeploymentError::LaunchFailed(error.to_string()))?
        ));
        tokio::fs::write(&load, script)
            .await
            .map_err(|error| DeploymentError::LaunchFailed(error.to_string()))?;
        let hook = directory
            .join("NapCatWinBootHook.dll")
            .to_string_lossy()
            .into_owned();
        let launcher = directory
            .join("NapCatWinBootMain.exe")
            .to_string_lossy()
            .into_owned();
        let mut environment = self.0.metrics_environment.clone();
        environment.extend(BTreeMap::from([
            (
                "NAPCAT_PATCH_PACKAGE".into(),
                directory.join("qqnt.json").to_string_lossy().into_owned(),
            ),
            (
                "NAPCAT_LOAD_PATH".into(),
                load.to_string_lossy().into_owned(),
            ),
            ("NAPCAT_INJECT_PATH".into(), hook.clone()),
            ("NAPCAT_LAUNCHER_PATH".into(), launcher.clone()),
            (
                "NAPCAT_MAIN_PATH".into(),
                directory.join("napcat.mjs").to_string_lossy().into_owned(),
            ),
        ]));
        Ok(NativeLaunchCommand {
            program: launcher,
            args: vec![self.0.qq_executable.clone(), hook, self.0.bot_id.clone()],
            working_dir: Some(directory),
            environment,
        })
    }
}

pub(super) struct NapCatBackend {
    id: BotId,
    deployment: NativeDeployment,
    host: LocalWindowsHost,
}

impl NapCatBackend {
    pub fn new(spec: WindowsBotSpec, bus: Arc<BroadcastEventBus>) -> Result<Self, String> {
        Ok(Self {
            id: BotId::new(&spec.bot_id),
            host: LocalWindowsHost::new(),
            deployment: NativeDeployment::new(
                Arc::new(Translator(spec.clone())),
                Arc::new(EventBusSink::new(bus)),
                Some(PathBuf::from(&spec.data_root).join("logs")),
            ),
        })
    }
}

#[async_trait]
impl BotBackend for NapCatBackend {
    fn id(&self) -> &BotId {
        &self.id
    }
    fn kind(&self) -> BackendKind {
        BackendKind::Local
    }
    fn flavor(&self) -> BotFlavor {
        BotFlavor::NapCat
    }
    async fn start(&self, ctx: &BotStartCtx) -> Result<BotStatus, BotBackendError> {
        let config = ctx
            .bot_config
            .as_ref()
            .ok_or_else(|| BotBackendError::InvalidConfig("缺少 Bot 配置".into()))?;
        match self
            .deployment
            .launch(&self.host, config)
            .await
            .map_err(|error| BotBackendError::Io(error.to_string()))?
        {
            DeploymentHandle::Native { pid, started_at } => Ok(BotStatus::running(
                ctx.config.bot_id.clone(),
                pid,
                started_at,
            )),
            _ => Err(BotBackendError::Io("NapCat 返回了非进程运行句柄".into())),
        }
    }
    async fn stop(&self, id: BotId, mode: StopMode) -> Result<(), BotBackendError> {
        self.deployment
            .stop(&self.host, &id, mode)
            .await
            .map_err(|error| BotBackendError::Io(error.to_string()))
    }
    async fn status(&self, id: BotId) -> Result<BotStatus, BotBackendError> {
        let state = self
            .deployment
            .observe(&self.host, &id)
            .await
            .map_err(|error| BotBackendError::Io(error.to_string()))?;
        Ok(if state == ncd_deploy::DeploymentState::Running {
            BotStatus::running(id, 0, 0)
        } else {
            BotStatus::stopped(id)
        })
    }
    async fn tail_log(&self, id: BotId, options: TailOpts) -> Result<LogSnapshot, BotBackendError> {
        Ok(self.deployment.tail_log(&id, options.lines).await)
    }
    async fn read_config(&self, id: BotId) -> Result<BotRuntimeConfig, BotBackendError> {
        Err(BotBackendError::ConfigNotFound(id))
    }
    async fn write_config(&self, _: BotId, _: &BotRuntimeConfig) -> Result<(), BotBackendError> {
        Ok(())
    }
}
