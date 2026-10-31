//! 按主机选择容器引擎；Podman 始终使用当前用户的容器存储，不能自动切换到 sudo。
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum ContainerEngine {
    #[default]
    Docker,
    Podman,
}
impl ContainerEngine {
    pub fn program(self) -> &'static str {
        match self {
            Self::Docker => "docker",
            Self::Podman => "podman",
        }
    }
}
static ENGINES: OnceLock<Mutex<HashMap<String, ContainerEngine>>> = OnceLock::new();
pub fn preferences() -> HashMap<String, ContainerEngine> {
    ENGINES
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}
pub fn replace_preferences(value: HashMap<String, ContainerEngine>) {
    *ENGINES
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = value;
}
pub fn engine_for(host: &dyn ncd_host::Host) -> ContainerEngine {
    if host.os() != ncd_host::Os::Linux {
        return ContainerEngine::Docker;
    }
    ENGINES
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(host.id())
        .copied()
        .unwrap_or_default()
}

pub fn image_reference(engine: ContainerEngine, image: &str) -> String {
    let has_registry = image.contains('/')
        && image
            .split('/')
            .next()
            .is_some_and(|host| host.contains('.') || host.contains(':') || host == "localhost");
    if engine == ContainerEngine::Podman && !has_registry {
        if image.contains('/') {
            format!("docker.io/{image}")
        } else {
            format!("docker.io/library/{image}")
        }
    } else {
        image.to_owned()
    }
}

/// Podman 使用完整 registry 名称，并给 Desktop 的 bind mounts 添加共享 SELinux 标签。
pub fn prepare_compose(engine: ContainerEngine, yaml: &str) -> Result<String, String> {
    if engine == ContainerEngine::Docker {
        return Ok(yaml.to_owned());
    }
    let mut root: serde_yaml::Value = serde_yaml::from_str(yaml).map_err(|e| e.to_string())?;
    if let Some(services) = root
        .get_mut("services")
        .and_then(serde_yaml::Value::as_mapping_mut)
    {
        for (_, service) in services {
            service["pull_policy"] = serde_yaml::Value::String("never".into());
            if let Some(image) = service.get_mut("image") {
                if let Some(value) = image.as_str() {
                    *image = serde_yaml::Value::String(image_reference(engine, value));
                }
            }
            if let Some(volumes) = service
                .get_mut("volumes")
                .and_then(serde_yaml::Value::as_sequence_mut)
            {
                for volume in volumes {
                    let Some(value) = volume.as_str() else {
                        continue;
                    };
                    let mut parts = value.splitn(3, ':');
                    let Some(source) = parts.next() else { continue };
                    if !source.starts_with('/')
                        && !source.starts_with("./")
                        && !source.starts_with("../")
                    {
                        continue;
                    }
                    let Some(target) = parts.next() else { continue };
                    let mode = parts.next().unwrap_or("");
                    if mode.split(',').any(|m| matches!(m, "z" | "Z")) {
                        continue;
                    }
                    *volume = serde_yaml::Value::String(format!(
                        "{source}:{target}:{}z",
                        if mode.is_empty() {
                            String::new()
                        } else {
                            format!("{mode},")
                        }
                    ));
                }
            }
        }
    }
    serde_yaml::to_string(&root).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn podman_compose_keeps_named_volumes_and_relabels_only_bind_mounts() {
        let input = "services:\n  napcat:\n    image: mlikiowa/napcat-docker:latest\n    volumes:\n      - ./napcat/config:/app/config\n      - /data/metrics:/app/metrics:ro\n      - account-data:/app/data\n";
        let prepared = prepare_compose(ContainerEngine::Podman, input).unwrap();
        assert!(prepared.contains("docker.io/mlikiowa/napcat-docker:latest"));
        assert!(prepared.contains("pull_policy: never"));
        assert!(prepared.contains("./napcat/config:/app/config:z"));
        assert!(prepared.contains("/data/metrics:/app/metrics:ro,z"));
        assert!(prepared.contains("account-data:/app/data"));
        assert!(!prepared.contains("account-data:/app/data:z"));
        assert_eq!(
            prepare_compose(ContainerEngine::Podman, &prepared).unwrap(),
            prepared
        );
        assert_eq!(
            prepare_compose(ContainerEngine::Docker, input).unwrap(),
            input
        );
    }
}
