use ncd_domain::{
    AdvancedConfig, AutoRestartSchedule, BackendType, BotBasicConfig, BotConfig, ConnectConfig,
    DeploymentType, RuntimeTarget,
};

#[derive(Debug, Clone)]
pub struct BotConfigBuilder(BotConfig);

impl BotConfigBuilder {
    pub fn new() -> Self {
        Self(BotConfig {
            bot: BotBasicConfig {
                name: "test-bot".to_string(),
                qq_id: 10001,
                music_sign_url: String::new(),
                auto_restart_schedule: AutoRestartSchedule::default(),
                offline_auto_restart: false,
                runtime_target: RuntimeTarget::Local,
                backend_type: BackendType::NapCat,
                deployment_type: DeploymentType::Native,
                snowluma_start_mode: None,
                webui_password_takeover: false,
            },
            connect: ConnectConfig::default(),
            advanced: AdvancedConfig::default(),
            status_command: None,
            docker: None,
        })
    }

    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.0.bot.name = name.into();
        self
    }

    pub fn qq_id(mut self, qq_id: u64) -> Self {
        self.0.bot.qq_id = qq_id;
        self
    }

    pub fn runtime_target(mut self, runtime_target: RuntimeTarget) -> Self {
        self.0.bot.runtime_target = runtime_target;
        self
    }

    pub fn backend_type(mut self, backend_type: BackendType) -> Self {
        self.0.bot.backend_type = backend_type;
        self
    }

    pub fn deployment_type(mut self, deployment_type: DeploymentType) -> Self {
        self.0.bot.deployment_type = deployment_type;
        self
    }

    pub fn build(self) -> BotConfig {
        self.0
    }
}

impl Default for BotConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}
