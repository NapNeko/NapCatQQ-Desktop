//! Docker 项目目录与配置渲染

use std::collections::HashMap;

use ncd_deploy::{DockerCli, DockerCliError, DockerDeployment};
use ncd_domain::ids::BotId;
use ncd_domain::{BackendType, BotConfig, ImportedNetworkConfig};
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

/// 导入迁移：从桌面 compose 项目目录或容器内读取网络配置。
/// NapCat 先读 host bind，没有再 `docker cp`；SnowLuma 以容器 named volume 为权威。
pub(crate) async fn read_docker_imported_network(
    host: &dyn Host,
    home: &str,
    docker_name: &str,
    backend: BackendType,
    qq_id: &str,
) -> Result<Option<ImportedNetworkConfig>, String> {
    let project_dir = format!(
        "{}/.napcat-bots/{}",
        home.trim_end_matches('/'),
        docker_name
    );
    match backend {
        BackendType::NapCat => {
            let host_path = format!("{project_dir}/napcat/config/onebot11_{qq_id}.json");
            if let Some(value) = read_json_if_exists(host, &host_path).await? {
                return parse_napcat_file(&host_path, &value);
            }
            match copy_container_json(
                host,
                docker_name,
                &format!("/app/napcat/config/onebot11_{qq_id}.json"),
                qq_id,
            )
            .await?
            {
                Some(value) => parse_napcat_file(
                    &format!("docker://{docker_name}/app/napcat/config/onebot11_{qq_id}.json"),
                    &value,
                ),
                None => Ok(None),
            }
        }
        BackendType::SnowLuma => {
            match copy_container_json(
                host,
                docker_name,
                &format!("/app/snowluma-data/config/onebot_{qq_id}.json"),
                qq_id,
            )
            .await?
            {
                Some(value) => {
                    return parse_snowluma_file(
                        &format!(
                            "docker://{docker_name}/app/snowluma-data/config/onebot_{qq_id}.json"
                        ),
                        &value,
                    );
                }
                None => {
                    let host_path =
                        format!("{project_dir}/snowluma-data/config/onebot_{qq_id}.json");
                    if let Some(value) = read_json_if_exists(host, &host_path).await? {
                        return parse_snowluma_file(&host_path, &value);
                    }
                    Ok(None)
                }
            }
        }
    }
}

async fn read_json_if_exists(host: &dyn Host, path: &str) -> Result<Option<Value>, String> {
    match host.read_file(&HostPath::from_posix(path)).await {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|e| format!("解析 {path} 失败: {e}")),
        Err(HostError::PathNotFound { .. }) => Ok(None),
        Err(err) => {
            tracing::info!(
                target: "ncd_runtime::remote",
                %path,
                %err,
                "远端 Docker 配置不存在或不可读，跳过网络配置迁移"
            );
            Ok(None)
        }
    }
}

fn parse_napcat_file(path: &str, value: &Value) -> Result<Option<ImportedNetworkConfig>, String> {
    ncd_deploy::backend_config_renderer::parse_napcat_onebot_connect(value)
        .map(Some)
        .ok_or_else(|| format!("{path} 缺少 network 字段，无法迁移网络配置"))
}

fn parse_snowluma_file(path: &str, value: &Value) -> Result<Option<ImportedNetworkConfig>, String> {
    ncd_deploy::backend_config_renderer::parse_snowluma_onebot_connect(value)
        .map(Some)
        .ok_or_else(|| format!("{path} 缺少 networks 字段，无法迁移网络配置"))
}

async fn copy_container_json(
    host: &dyn Host,
    docker_name: &str,
    src: &str,
    qq_id: &str,
) -> Result<Option<Value>, String> {
    let cli = DockerCli::new(host);
    cli.ensure_daemon_ready()
        .await
        .map_err(|e| format!("Docker 未就绪，无法从容器读取网络配置: {e}"))?;
    let dest = import_tmp_path(docker_name, qq_id);
    match cli.copy_from_container(docker_name, src, &dest).await {
        Ok(()) => {
            let result = match host.read_file(&HostPath::from_posix(&dest)).await {
                Ok(bytes) => serde_json::from_slice(&bytes)
                    .map(Some)
                    .map_err(|e| format!("解析 {dest} 失败: {e}")),
                Err(HostError::PathNotFound { .. }) => Ok(None),
                Err(err) => Err(format!("读取 {dest} 失败: {err}")),
            };
            cli.remove_copied_file(&dest).await;
            result
        }
        Err(DockerCliError::CommandFailed { stderr, .. }) => {
            tracing::info!(
                target: "ncd_runtime::remote",
                container = %docker_name,
                %src,
                %stderr,
                "容器内没有可迁移的网络配置文件"
            );
            Ok(None)
        }
        Err(error) => Err(error.to_string()),
    }
}

fn import_tmp_path(docker_name: &str, qq_id: &str) -> String {
    let safe_name: String = docker_name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let safe_qq: String = qq_id.chars().filter(|c| c.is_ascii_digit()).collect();
    format!("/tmp/ncd-import-{safe_name}-{safe_qq}.json")
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
