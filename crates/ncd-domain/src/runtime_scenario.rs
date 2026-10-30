use crate::bot_config::{BackendType, BotConfig, BotConfigError, DeploymentType};
use crate::kinds::{BotFlavor, RuntimeTarget};

/// Bot runtime 的后端唯一场景模型。
///
/// `BackendType`, `DeploymentType`, `RuntimeTarget` 仍然保持磁盘兼容;运行时只消费
/// 场景只描述部署位置；实际主机平台由运行时校验（Docker 仅支持 Linux）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeScenario {
    LocalNative {
        backend: BackendType,
    },
    LocalDocker {
        backend: BackendType,
    },
    RemoteNative {
        server_id: String,
        backend: BackendType,
    },
    RemoteDocker {
        server_id: String,
        backend: BackendType,
    },
}

impl RuntimeScenario {
    pub fn from_config(config: &BotConfig) -> Result<Self, BotConfigError> {
        Self::from_parts(
            config.bot.backend_type,
            config.bot.deployment_type,
            &config.bot.runtime_target,
        )
    }

    pub fn from_parts(
        backend: BackendType,
        deployment: DeploymentType,
        target: &RuntimeTarget,
    ) -> Result<Self, BotConfigError> {
        match (target, deployment) {
            (RuntimeTarget::Local, DeploymentType::Native) => Ok(Self::LocalNative { backend }),
            (RuntimeTarget::Local, DeploymentType::Docker) => Ok(Self::LocalDocker { backend }),
            (RuntimeTarget::Server(server_id), DeploymentType::Native) => Ok(Self::RemoteNative {
                server_id: normalize_server_id(server_id)?,
                backend,
            }),
            (RuntimeTarget::Server(server_id), DeploymentType::Docker) => Ok(Self::RemoteDocker {
                server_id: normalize_server_id(server_id)?,
                backend,
            }),
        }
    }

    pub const fn backend_type(&self) -> BackendType {
        match self {
            Self::LocalNative { backend }
            | Self::LocalDocker { backend }
            | Self::RemoteNative { backend, .. }
            | Self::RemoteDocker { backend, .. } => *backend,
        }
    }

    pub fn flavor(&self) -> BotFlavor {
        BotFlavor::from(self.backend_type())
    }

    pub fn server_id(&self) -> Option<&str> {
        match self {
            Self::LocalNative { .. } | Self::LocalDocker { .. } => None,
            Self::RemoteNative { server_id, .. } | Self::RemoteDocker { server_id, .. } => {
                Some(server_id.as_str())
            }
        }
    }

    pub const fn is_remote_docker(&self) -> bool {
        matches!(self, Self::RemoteDocker { .. })
    }

    pub const fn is_docker(&self) -> bool {
        matches!(self, Self::LocalDocker { .. } | Self::RemoteDocker { .. })
    }

    pub const fn is_remote_native_napcat(&self) -> bool {
        matches!(
            self,
            Self::RemoteNative {
                backend: BackendType::NapCat,
                ..
            }
        )
    }

    pub const fn is_remote_native_snowluma(&self) -> bool {
        matches!(
            self,
            Self::RemoteNative {
                backend: BackendType::SnowLuma,
                ..
            }
        )
    }
}

fn normalize_server_id(server_id: &str) -> Result<String, BotConfigError> {
    let trimmed = server_id.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("remote") {
        return Err(BotConfigError::UnsupportedRuntimeMatrix(
            "Remote runtime requires a concrete SSH server id".to_string(),
        ));
    }
    Ok(trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bot_config::{
        AdvancedConfig, AutoRestartSchedule, BotBasicConfig, ConnectConfig, StatusCommandConfig,
    };

    fn cfg(backend: BackendType, deployment: DeploymentType, target: RuntimeTarget) -> BotConfig {
        BotConfig {
            bot: BotBasicConfig {
                name: "t".to_string(),
                qq_id: 10001,
                music_sign_url: String::new(),
                auto_restart_schedule: AutoRestartSchedule::default(),
                offline_auto_restart: false,
                runtime_target: target,
                backend_type: backend,
                deployment_type: deployment,
                snowluma_start_mode: None,
                webui_password_takeover: false,
            },
            connect: ConnectConfig::default(),
            advanced: AdvancedConfig::default(),
            docker: None,
            status_command: None::<StatusCommandConfig>,
        }
    }

    #[test]
    fn maps_supported_matrix_to_three_scenarios() {
        assert_eq!(
            RuntimeScenario::from_config(&cfg(
                BackendType::NapCat,
                DeploymentType::Native,
                RuntimeTarget::Local
            ))
            .unwrap(),
            RuntimeScenario::LocalNative {
                backend: BackendType::NapCat
            }
        );
        assert_eq!(
            RuntimeScenario::from_config(&cfg(
                BackendType::SnowLuma,
                DeploymentType::Native,
                RuntimeTarget::Local
            ))
            .unwrap(),
            RuntimeScenario::LocalNative {
                backend: BackendType::SnowLuma
            }
        );
        assert_eq!(
            RuntimeScenario::from_config(&cfg(
                BackendType::NapCat,
                DeploymentType::Native,
                RuntimeTarget::server("srv-a")
            ))
            .unwrap(),
            RuntimeScenario::RemoteNative {
                server_id: "srv-a".to_string(),
                backend: BackendType::NapCat
            }
        );
        assert_eq!(
            RuntimeScenario::from_config(&cfg(
                BackendType::SnowLuma,
                DeploymentType::Native,
                RuntimeTarget::server("srv-a")
            ))
            .unwrap(),
            RuntimeScenario::RemoteNative {
                server_id: "srv-a".to_string(),
                backend: BackendType::SnowLuma
            }
        );
        assert_eq!(
            RuntimeScenario::from_config(&cfg(
                BackendType::NapCat,
                DeploymentType::Docker,
                RuntimeTarget::server("srv-a")
            ))
            .unwrap(),
            RuntimeScenario::RemoteDocker {
                server_id: "srv-a".to_string(),
                backend: BackendType::NapCat
            }
        );
        assert_eq!(
            RuntimeScenario::from_config(&cfg(
                BackendType::SnowLuma,
                DeploymentType::Docker,
                RuntimeTarget::server("srv-a")
            ))
            .unwrap(),
            RuntimeScenario::RemoteDocker {
                server_id: "srv-a".to_string(),
                backend: BackendType::SnowLuma
            }
        );
    }

    #[test]
    fn local_docker_has_no_server_and_remote_placeholder_is_rejected() {
        assert!(matches!(
            RuntimeScenario::from_config(&cfg(
                BackendType::NapCat,
                DeploymentType::Docker,
                RuntimeTarget::Local
            )),
            Ok(RuntimeScenario::LocalDocker {
                backend: BackendType::NapCat
            })
        ));
        assert!(matches!(
            RuntimeScenario::from_config(&cfg(
                BackendType::SnowLuma,
                DeploymentType::Native,
                RuntimeTarget::server("remote")
            )),
            Err(BotConfigError::UnsupportedRuntimeMatrix(_))
        ));
    }
}
