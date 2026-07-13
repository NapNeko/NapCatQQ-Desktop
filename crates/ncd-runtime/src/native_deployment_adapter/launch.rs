//! RuntimeLaunchPlanner -> NativeLaunchTranslator 适配

use std::sync::Arc;

use async_trait::async_trait;
use ncd_deploy::{DeploymentError, NativeLaunchCommand, NativeLaunchTranslator};
use ncd_domain::ids::BotId;
use ncd_domain::BotConfig;
use ncd_traits::runtime_backend::BotRuntimeConfig;

use crate::runtime_launch_plan::RuntimeLaunchPlanner;

/// 把 FileSystemRuntimeLaunchPlanner 包装成 NativeLaunchTranslator
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
