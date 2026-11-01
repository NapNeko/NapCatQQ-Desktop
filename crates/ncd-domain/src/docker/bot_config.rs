//! Bot 容器设置与有效端口推导。持久化用户覆盖值，监听端口始终从连接配置推导。

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{DockerDeploySpec, PortMapping};
use crate::{BackendType, BotConfig};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum DockerRestartPolicy {
    #[default]
    UnlessStopped,
    Always,
    OnFailure,
    No,
}

impl DockerRestartPolicy {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UnlessStopped => "unless-stopped",
            Self::Always => "always",
            Self::OnFailure => "on-failure",
            Self::No => "no",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct BotDockerConfig {
    /// 缺失表示旧配置，由编排层按原主机设置补齐后持久化。
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub engine: Option<super::ContainerEngine>,
    /// 默认仅发布到宿主机回环；开启后监听所有 IPv4 网卡。
    pub publish_on_all_interfaces: bool,
    /// 按容器端口覆盖自动映射，也可追加额外 TCP 端口。
    pub port_mappings: Vec<PortMapping>,
    pub environment: BTreeMap<String, String>,
    pub restart_policy: DockerRestartPolicy,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub memory_mb: Option<u32>,
}

fn managed_environment(key: &str) -> bool {
    matches!(
        key,
        "ACCOUNT" | "WEBUI_TOKEN" | "VNC_PASSWD" | "NODE_OPTIONS"
    ) || key.starts_with("NAPCAT_")
        || key.starts_with("SNOWLUMA_")
        || key.starts_with("NCD_")
}

impl BotDockerConfig {
    pub fn validate(&self) -> Result<(), String> {
        let mut host_ports = BTreeSet::new();
        let mut container_ports = BTreeSet::new();
        for port in &self.port_mappings {
            if port.host == 0 || port.container == 0 {
                return Err("端口必须在 1–65535 之间".into());
            }
            if !host_ports.insert(port.host) {
                return Err(format!("宿主机端口 {} 重复，请调整映射", port.host));
            }
            if !container_ports.insert(port.container) {
                return Err(format!("容器端口 {} 重复，请调整映射", port.container));
            }
        }
        for (key, value) in &self.environment {
            let mut chars = key.chars();
            if !chars
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                || !chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
            {
                return Err(format!("环境变量名称无效: {key}"));
            }
            if managed_environment(key) {
                return Err(format!("环境变量 {key} 由 Desktop 管理，不能覆盖"));
            }
            if value.contains('\0') {
                return Err(format!("环境变量 {key} 的值不能包含 NUL"));
            }
        }
        if self.memory_mb == Some(0) {
            return Err("内存限制必须大于 0 MiB，留空表示不限制".into());
        }
        Ok(())
    }
}

impl BotConfig {
    fn docker_base_spec(&self) -> DockerDeploySpec {
        match self.bot.backend_type {
            BackendType::NapCat => DockerDeploySpec::napcat_default(),
            BackendType::SnowLuma => DockerDeploySpec::snowluma_default(),
        }
    }

    fn docker_listener_ports(&self) -> impl Iterator<Item = u16> + '_ {
        self.connect
            .http_servers
            .iter()
            .filter(|c| c.base.enable)
            .map(|c| c.port)
            .chain(
                self.connect
                    .http_sse_servers
                    .iter()
                    .filter(|c| c.base.enable)
                    .map(|c| c.port),
            )
            .chain(
                self.connect
                    .websocket_servers
                    .iter()
                    .filter(|c| c.base.enable)
                    .map(|c| c.port),
            )
    }

    /// 用于界面预览和部署。实际写入前必须先 validate_docker，拒绝溢出及冲突。
    pub fn effective_docker_ports(&self) -> Vec<PortMapping> {
        let offset = DockerDeploySpec::host_port_offset_for_qq(self.bot.qq_id);
        let mut ports = self
            .docker_base_spec()
            .with_host_port_offset(self.bot.qq_id)
            .ports;
        for port in self.docker_listener_ports() {
            if !ports.iter().any(|p| p.container == port) {
                ports.push(PortMapping::new(port.saturating_add(offset), port));
            }
        }
        if let Some(settings) = &self.docker {
            for custom in &settings.port_mappings {
                if let Some(existing) = ports.iter_mut().find(|p| p.container == custom.container) {
                    *existing = *custom;
                } else {
                    ports.push(*custom);
                }
            }
        }
        ports
    }

    pub fn validate_docker(&self) -> Result<(), String> {
        if let Some(settings) = &self.docker {
            settings.validate()?;
        }
        let offset = DockerDeploySpec::host_port_offset_for_qq(self.bot.qq_id);
        for port in self.docker_listener_ports() {
            let explicit = self
                .docker
                .as_ref()
                .is_some_and(|s| s.port_mappings.iter().any(|p| p.container == port));
            if !explicit && port.checked_add(offset).is_none() {
                return Err(format!(
                    "容器端口 {port} 的自动宿主机端口超出 65535，请手动设置映射"
                ));
            }
        }
        let mut host_ports = BTreeSet::new();
        for port in self.effective_docker_ports() {
            if port.host == 0 || port.container == 0 {
                return Err("端口必须在 1–65535 之间".into());
            }
            if !host_ports.insert(port.host) {
                return Err(format!(
                    "宿主机端口 {} 重复，请在容器设置中调整映射",
                    port.host
                ));
            }
        }
        Ok(())
    }

    pub fn docker_deploy_spec(&self) -> Result<DockerDeploySpec, String> {
        self.validate_docker()?;
        let mut spec = self.docker_base_spec();
        let prefix = match self.bot.backend_type {
            BackendType::NapCat => "ncbot",
            BackendType::SnowLuma => "slbot",
        };
        spec.container_name = format!("{prefix}-{}", self.bot.qq_id);
        spec.ports = self.effective_docker_ports();
        if self.bot.backend_type == BackendType::NapCat && self.bot.qq_id != 0 {
            spec.qq_id = Some(self.bot.qq_id);
        }
        Ok(spec)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn config(backend: &str) -> BotConfig {
        serde_json::from_value(json!({
            "bot": {"name": "ports", "QQID": 10001, "backend_type": backend, "deploymentType": "docker"},
            "connect": {
                "httpServers": [{"name": "api", "host": "127.0.0.1", "port": 4000}],
                "httpSseServers": [{"name": "sse", "host": "0.0.0.0", "port": 4002}],
                "websocketServers": [{"name": "ws", "host": "", "port": 4001}, {"name": "off", "host": "", "port": 4500, "enable": false}],
                "websocketClients": [{"name": "client", "url": "ws://example.test:4600"}]
            },
            "advanced": {}
        })).unwrap()
    }

    #[test]
    fn docker_ports_include_enabled_listeners_for_both_backends() {
        for backend in ["napcat", "snowluma"] {
            let config = config(backend);
            let spec = config.docker_deploy_spec().unwrap();
            for port in [4000, 4001, 4002] {
                assert_eq!(spec.host_port_for_container(port), Some(port + 1));
            }
            assert_eq!(spec.host_port_for_container(4500), None);
            assert_eq!(spec.host_port_for_container(4600), None);
            assert_eq!(spec.host_port_for_container(3000), Some(3001));
        }
    }

    #[test]
    fn docker_explicit_mapping_overrides_auto_and_can_add_ports() {
        let mut config = config("napcat");
        config.docker = Some(BotDockerConfig {
            port_mappings: vec![PortMapping::new(8000, 4000), PortMapping::new(8001, 9000)],
            ..Default::default()
        });
        let spec = config.docker_deploy_spec().unwrap();
        assert_eq!(spec.host_port_for_container(4000), Some(8000));
        assert_eq!(spec.host_port_for_container(9000), Some(8001));
        assert_eq!(spec.ports.iter().filter(|p| p.container == 4000).count(), 1);
    }

    #[test]
    fn docker_port_collisions_and_overflow_require_explicit_fix() {
        let mut config = config("snowluma");
        config.docker = Some(BotDockerConfig {
            port_mappings: vec![PortMapping::new(3001, 4000)],
            ..Default::default()
        });
        assert!(config.validate_docker().unwrap_err().contains("重复"));
        config.docker = None;
        config.connect.http_servers[0].port = 65535;
        assert!(config.validate_docker().unwrap_err().contains("超出"));
        config.docker = Some(BotDockerConfig {
            port_mappings: vec![PortMapping::new(8000, 65535)],
            ..Default::default()
        });
        assert!(config.validate_docker().is_ok());
    }

    #[test]
    fn docker_settings_reject_invalid_and_managed_environment() {
        for key in [
            "WEBUI_TOKEN",
            "ACCOUNT",
            "NAPCAT_UID",
            "SNOWLUMA_WEBUI_PORT",
            "NCD_METRICS_OUT",
            "NODE_OPTIONS",
            "bad-name",
            "1BAD",
        ] {
            let settings = BotDockerConfig {
                environment: [(key.into(), "x".into())].into(),
                ..Default::default()
            };
            assert!(settings.validate().is_err(), "{key}");
        }
        let mut settings = BotDockerConfig {
            memory_mb: Some(0),
            ..Default::default()
        };
        assert!(settings.validate().is_err());
        settings.memory_mb = Some(64);
        assert!(settings.validate().is_ok());
        settings.port_mappings = vec![PortMapping::new(0, 80)];
        assert!(settings.validate().is_err());
    }

    #[test]
    fn docker_old_config_stays_compatible_and_settings_round_trip() {
        let mut config = config("napcat");
        assert!(config.docker.is_none());
        assert!(
            serde_json::to_value(&config)
                .unwrap()
                .get("docker")
                .is_none()
        );
        config.docker = Some(BotDockerConfig::default());
        let encoded = serde_json::to_value(&config).unwrap();
        assert_eq!(encoded["docker"]["restartPolicy"], "unless-stopped");
        assert_eq!(
            serde_json::from_value::<BotConfig>(encoded).unwrap(),
            config
        );
    }
}
