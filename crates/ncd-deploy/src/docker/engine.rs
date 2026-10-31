//! 按主机选择容器引擎；Podman 始终使用当前用户的容器存储，不能自动切换到 sudo。
pub use ncd_domain::docker::ContainerEngine;
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};

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
    preference_for_host_id(host.id())
}

pub fn preference_for_host_id(host_id: &str) -> ContainerEngine {
    let host_id = host_id.strip_prefix("remote:").unwrap_or(host_id);
    ENGINES
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(host_id)
        .copied()
        .unwrap_or_default()
}

pub fn engine_for_bot(
    host: &dyn ncd_host::Host,
    config: &ncd_domain::BotConfig,
) -> ContainerEngine {
    config
        .docker
        .as_ref()
        .and_then(|settings| settings.engine)
        .unwrap_or_else(|| engine_for(host))
}

/// 补齐历史配置时只改持久化归属，不重建、停止或迁移容器。
pub fn pin_bot_engine(config: &mut ncd_domain::BotConfig) -> bool {
    if config.bot.deployment_type != ncd_domain::DeploymentType::Docker {
        return false;
    }
    let settings = config.docker.get_or_insert_with(Default::default);
    if settings.engine.is_some() {
        return false;
    }
    let host_id = match &config.bot.runtime_target {
        ncd_domain::RuntimeTarget::Local => "local".to_owned(),
        ncd_domain::RuntimeTarget::Server(id) => format!("remote:{id}"),
    };
    settings.engine = Some(preference_for_host_id(&host_id));
    true
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
