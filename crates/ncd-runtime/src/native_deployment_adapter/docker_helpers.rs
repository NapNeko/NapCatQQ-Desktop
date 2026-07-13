//! Docker 项目目录与配置渲染

use std::collections::HashMap;

use ncd_deploy::DockerDeployment;
use ncd_domain::ids::BotId;
use ncd_domain::{BackendType, BotConfig};
use ncd_host::{Host, HostError, HostPath};
use ncd_traits::runtime_backend::BotBackendError;
use serde_json::Value;

use crate::backend_config_renderer::{
    render_napcat_docker_config_payloads, render_snowluma_docker_config_payloads,
};

fn docker_container_name(config: &BotConfig) -> String {
    DockerDeployment::container_name(config)
}

pub(crate) async fn docker_project_dir(
    host: &dyn Host,
    name: &str,
) -> Result<String, BotBackendError> {
    let home = probe_home(host).await?;
    Ok(format!("{home}/.napcat-bots/{name}"))
}

async fn probe_home(host: &dyn Host) -> Result<String, BotBackendError> {
    let cmd = ncd_host::HostCommand::new("sh").arg("-c").arg("echo $HOME");
    match host.run_to_string(cmd).await {
        Ok(out) if out.success() => {
            let home = out.stdout.trim().to_string();
            if home.is_empty() {
                Err(BotBackendError::InvalidConfig(
                    "Docker host HOME is empty; cannot determine deployment project directory"
                        .into(),
                ))
            } else {
                Ok(home)
            }
        }
        Ok(out) => Err(BotBackendError::Io(format!(
            "探测 Docker 主机 HOME 失败: exit={:?}, stderr={}",
            out.exit_code,
            out.stderr.trim()
        ))),
        Err(error) => Err(BotBackendError::Io(format!(
            "探测 Docker 主机 HOME 失败: {error}"
        ))),
    }
}

fn docker_config_file_names(bot_id: &BotId) -> [String; 2] {
    [
        format!("onebot11_{}.json", bot_id.as_str()),
        format!("napcat_{}.json", bot_id.as_str()),
    ]
}

pub(crate) async fn render_docker_config_on_host(
    host: &dyn Host,
    bot_id: &BotId,
    config: &BotConfig,
) -> Result<(), BotBackendError> {
    let name = docker_container_name(config);
    let project_dir = docker_project_dir(host, &name).await?;
    match config.bot.backend_type {
        BackendType::NapCat => {
            let config_dir = format!("{project_dir}/napcat/config");
            let config_dir_path = HostPath::from_posix(&config_dir);
            host.create_dir_all(&config_dir_path)
                .await
                .map_err(|error| {
                    BotBackendError::Io(format!("创建 Docker 配置目录失败: {error}"))
                })?;

            let existing = read_existing_docker_napcat_config(host, bot_id, &config_dir).await?;
            for item in render_napcat_docker_config_payloads(bot_id, config, &existing) {
                let bytes = serde_json::to_vec_pretty(&item.payload)
                    .map_err(|error| BotBackendError::Json(error.to_string()))?;
                let path = HostPath::from_posix(format!("{config_dir}/{}", item.file_name));
                host.write_file(&path, &bytes).await.map_err(|error| {
                    BotBackendError::Io(format!("写 Docker 配置文件失败: {error}"))
                })?;
            }
        }
        BackendType::SnowLuma => {
            let config_dir = format!("{project_dir}/snowluma-data/config");
            let config_dir_path = HostPath::from_posix(&config_dir);
            host.create_dir_all(&config_dir_path)
                .await
                .map_err(|error| {
                    BotBackendError::Io(format!("创建 Docker 配置目录失败: {error}"))
                })?;

            let existing = read_existing_docker_snowluma_config(host, bot_id, &config_dir).await?;
            for item in render_snowluma_docker_config_payloads(bot_id, config, &existing) {
                let bytes = serde_json::to_vec_pretty(&item.payload)
                    .map_err(|error| BotBackendError::Json(error.to_string()))?;
                let path = HostPath::from_posix(format!("{config_dir}/{}", item.file_name));
                host.write_file(&path, &bytes).await.map_err(|error| {
                    BotBackendError::Io(format!("写 Docker 配置文件失败: {error}"))
                })?;
            }
        }
    }
    Ok(())
}

async fn read_existing_docker_napcat_config(
    host: &dyn Host,
    bot_id: &BotId,
    config_dir: &str,
) -> Result<HashMap<String, Value>, BotBackendError> {
    let mut existing = HashMap::new();
    for file_name in docker_config_file_names(bot_id) {
        let path = HostPath::from_posix(format!("{config_dir}/{file_name}"));
        match host.read_file(&path).await {
            Ok(bytes) => {
                if let Ok(value) = serde_json::from_slice::<Value>(&bytes) {
                    existing.insert(file_name, value);
                }
            }
            Err(HostError::PathNotFound { .. }) => {}
            Err(error) => return Err(BotBackendError::Io(error.to_string())),
        }
    }
    Ok(existing)
}

async fn read_existing_docker_snowluma_config(
    host: &dyn Host,
    bot_id: &BotId,
    config_dir: &str,
) -> Result<HashMap<String, Value>, BotBackendError> {
    let mut existing = HashMap::new();
    let file_name = format!("onebot_{}.json", bot_id.as_str());
    let path = HostPath::from_posix(format!("{config_dir}/{file_name}"));
    match host.read_file(&path).await {
        Ok(bytes) => {
            if let Ok(value) = serde_json::from_slice::<Value>(&bytes) {
                existing.insert(file_name, value);
            }
        }
        Err(HostError::PathNotFound { .. }) => {}
        Err(error) => return Err(BotBackendError::Io(error.to_string())),
    }
    Ok(existing)
}
