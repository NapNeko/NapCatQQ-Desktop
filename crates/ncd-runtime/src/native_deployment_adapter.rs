//! 适配层：把 ncd-deploy 的 NativeDeployment 接入当前 BotManager 体系。
//!
//! 三个适配器：
//! - RuntimeLaunchPlannerAdapter：实装 NativeLaunchTranslator trait，包装现有
//!   FileSystemRuntimeLaunchPlanner。
//! - EventBusSink：实装 NativeRuntimeEventSink trait，桥接 BroadcastEventBus。
//! - NativeDeploymentBackend：把 NativeDeployment 包成 BotBackend trait object，
//!   让 BotManager 无需修改结构体即可切到新实装。后续删 BotBackend 时一起删。

use std::sync::Arc;

use async_trait::async_trait;
use ncd_deploy::{
    Deployment, DeploymentError, NativeDeployment, NativeLaunchCommand, NativeLaunchTranslator,
    NativeRuntimeEventSink,
};
use ncd_domain::{BotConfig, BotFlavor, BotId, StopMode};

use crate::events::{BroadcastEventBus, DomainEvent, EventBus};
use crate::runtime_backend::{
    BotBackend, BotBackendError, BotRuntimeConfig, BotStartCtx, BotStatus, LogSnapshot, TailOpts,
};
use crate::runtime_launch_plan::RuntimeLaunchPlanner;
use crate::kinds::BackendKind;

// ============================================================
// RuntimeLaunchPlannerAdapter
// ============================================================

/// 把 FileSystemRuntimeLaunchPlanner 包装成 NativeLaunchTranslator。
///
/// NativeLaunchTranslator::translate 收 &BotConfig，输出 NativeLaunchCommand。
/// 内部先调 build_plan 拿到 RuntimeLaunchPlan，再取出 NapCat 分支的
/// program / args / working_dir / environment。SnowLuma 分支的 launch_command
/// 为空（由 SnowLumaDaemon 另走），适配器对空命令返回错误。
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
    async fn translate(
        &self,
        config: &BotConfig,
    ) -> Result<NativeLaunchCommand, DeploymentError> {
        let bot_id = BotId::new(config.bot.qq_id.to_string());
        let plan = self
            .planner
            .build_plan(&bot_id, config)
            .await
            .map_err(|err| DeploymentError::LaunchFailed(err.to_string()))?;

        // 把 RuntimeLaunchPlan 转成 BotRuntimeConfig 再抽出 launch 字段。
        let cfg = BotRuntimeConfig::default_path("/tmp", bot_id);
        let cfg = plan.into_runtime_config(cfg);

        if cfg.launch_command.is_empty() {
            return Err(DeploymentError::LaunchFailed(
                "launch plan produced empty command (SnowLuma backend uses daemon, not direct spawn)".into(),
            ));
        }

        let (program, args) = cfg.launch_command.split_first().unwrap();
        Ok(NativeLaunchCommand {
            program: program.clone(),
            args: args.to_vec(),
            working_dir: cfg.working_dir,
            environment: cfg.environment,
        })
    }
}

// ============================================================
// EventBusSink
// ============================================================

/// 把 NativeDeployment 的运行时事件桥接到 BroadcastEventBus。
pub struct EventBusSink {
    bus: Arc<BroadcastEventBus>,
}

impl EventBusSink {
    pub fn new(bus: Arc<BroadcastEventBus>) -> Self {
        Self { bus }
    }
}

impl NativeRuntimeEventSink for EventBusSink {
    fn publish_log_line(&self, bot_id: &BotId, line: &str, channel: &str) {
        self.bus.publish(DomainEvent::BotLogAppended {
            bot_id: bot_id.clone(),
            line: line.to_string(),
            channel: Some(channel.to_string()),
        });
    }

    fn publish_napcat_webui_available(&self, bot_id: &BotId, port: u16, token: String) {
        self.bus
            .publish(DomainEvent::napcat_webui_available(bot_id.clone(), port, token));
    }

    fn publish_bot_process_exited(
        &self,
        bot_id: &BotId,
        exit_code: Option<i32>,
        reason: Option<String>,
    ) {
        self.bus
            .publish(DomainEvent::bot_process_exited(bot_id.clone(), exit_code, reason));
    }
}

// ============================================================
// NativeDeploymentBackend：过渡壳
//
// 让 BotManager 在不改结构体的情况下就能用 NativeDeployment。
// BotBackend 要求 start/stop/status/tail_log/read_config/write_config，
// 这里把前三个转发给 NativeDeployment，后三个保留原来的文件 IO 逻辑。
// 后续删 BotBackend trait 时整个文件一起扬掉。
// ============================================================

use ncd_host::Host;

/// 过渡壳：让 NativeDeployment 穿上 BotBackend trait 的外套。
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
        if ctx.config.launch_command.is_empty() {
            return Err(BotBackendError::EmptyLaunchCommand);
        }

        // 构造一个最小 BotConfig 给 deployment.launch 用（BotConfig 的 qq_id 就是 bot_id）。
        let qq_id: u64 = ctx.config.bot_id.as_str().parse().unwrap_or(0);
        let bot_config = minimal_bot_config(qq_id, self.flavor);

        let handle = self
            .deployment
            .launch(self.host.as_ref(), &bot_config)
            .await
            .map_err(|err| BotBackendError::Io(err.to_string()))?;

        match handle {
            ncd_deploy::DeploymentHandle::Native { pid, started_at } => {
                Ok(BotStatus::running(ctx.config.bot_id.clone(), pid, started_at))
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
        match state {
            ncd_deploy::DeploymentState::Running => {
                // 从 deployment 拿不到精确 pid/started_at，返回 running 用 0 占位。
                // BotManager 只看 state 字段做决策，pid 在 start 时已经拿到了。
                Ok(BotStatus::running(bot_id, 0, 0))
            }
            _ => Ok(BotStatus::stopped(bot_id)),
        }
    }

    async fn read_config(&self, bot_id: BotId) -> Result<BotRuntimeConfig, BotBackendError> {
        // 过渡期不支持，BotManager 不从 backend 读 config（用 repo 读）。
        Err(BotBackendError::ConfigNotFound(bot_id))
    }

    async fn write_config(
        &self,
        _bot_id: BotId,
        _cfg: &BotRuntimeConfig,
    ) -> Result<(), BotBackendError> {
        // 过渡期：config 落盘已由 BotManager 自己做，backend 不需要管。
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

use ncd_domain::{BackendType, DeploymentType, RuntimeTarget};

fn minimal_bot_config(qq_id: u64, flavor: BotFlavor) -> BotConfig {
    use ncd_domain::{AdvancedConfig, AutoRestartSchedule, BotBasicConfig, ConnectConfig};
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
    }
}
