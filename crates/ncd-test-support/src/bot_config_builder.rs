use ncd_domain::{
    AdvancedConfig, AutoRestartSchedule, BackendType, BotBasicConfig, BotConfig, ConnectConfig,
    DeploymentType, RuntimeTarget, SnowLumaStartMode, StatusCommandConfig,
};

#[derive(Debug, Clone)]
pub struct BotConfigBuilder {
    name: String,
    qq_id: u64,
    music_sign_url: String,
    auto_restart_schedule: AutoRestartSchedule,
    offline_auto_restart: bool,
    runtime_target: RuntimeTarget,
    backend_type: BackendType,
    deployment_type: DeploymentType,
    snowluma_start_mode: Option<SnowLumaStartMode>,
    connect: ConnectConfig,
    advanced: AdvancedConfig,
    status_command: Option<StatusCommandConfig>,
}

impl BotConfigBuilder {
    pub fn new() -> Self {
        Self {
            name: "test-bot".to_string(),
            qq_id: 10001,
            music_sign_url: String::new(),
            auto_restart_schedule: AutoRestartSchedule::default(),
            offline_auto_restart: false,
            runtime_target: RuntimeTarget::Local,
            backend_type: BackendType::NapCat,
            deployment_type: DeploymentType::Native,
            snowluma_start_mode: None,
            connect: ConnectConfig::default(),
            advanced: AdvancedConfig::default(),
            status_command: None,
        }
    }

    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    pub fn qq_id(mut self, qq_id: u64) -> Self {
        self.qq_id = qq_id;
        self
    }

    pub fn music_sign_url(mut self, music_sign_url: impl Into<String>) -> Self {
        self.music_sign_url = music_sign_url.into();
        self
    }

    pub fn auto_restart_schedule(mut self, schedule: AutoRestartSchedule) -> Self {
        self.auto_restart_schedule = schedule;
        self
    }

    pub fn offline_auto_restart(mut self, enabled: bool) -> Self {
        self.offline_auto_restart = enabled;
        self
    }

    pub fn runtime_target(mut self, runtime_target: RuntimeTarget) -> Self {
        self.runtime_target = runtime_target;
        self
    }

    pub fn backend_type(mut self, backend_type: BackendType) -> Self {
        self.backend_type = backend_type;
        self
    }

    pub fn deployment_type(mut self, deployment_type: DeploymentType) -> Self {
        self.deployment_type = deployment_type;
        self
    }

    pub fn snowluma_start_mode(mut self, start_mode: Option<SnowLumaStartMode>) -> Self {
        self.snowluma_start_mode = start_mode;
        self
    }

    pub fn connect(mut self, connect: ConnectConfig) -> Self {
        self.connect = connect;
        self
    }

    pub fn advanced(mut self, advanced: AdvancedConfig) -> Self {
        self.advanced = advanced;
        self
    }

    pub fn status_command(mut self, status_command: Option<StatusCommandConfig>) -> Self {
        self.status_command = status_command;
        self
    }

    pub fn with_status_command(mut self, status_command: StatusCommandConfig) -> Self {
        self.status_command = Some(status_command);
        self
    }

    pub fn without_status_command(mut self) -> Self {
        self.status_command = None;
        self
    }

    pub fn build(self) -> BotConfig {
        BotConfig {
            bot: BotBasicConfig {
                name: self.name,
                qq_id: self.qq_id,
                music_sign_url: self.music_sign_url,
                auto_restart_schedule: self.auto_restart_schedule,
                offline_auto_restart: self.offline_auto_restart,
                runtime_target: self.runtime_target,
                backend_type: self.backend_type,
                deployment_type: self.deployment_type,
                snowluma_start_mode: self.snowluma_start_mode,
            },
            connect: self.connect,
            advanced: self.advanced,
            status_command: self.status_command,
        }
    }
}

impl Default for BotConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}
