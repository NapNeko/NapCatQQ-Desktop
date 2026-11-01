//! Docker 协议读取不依赖 WebUI：未同意时启动编排会停止容器，弹窗与同意落盘
//! 必须仍可工作。协议正文始终现读，同意记录写入容器，并交接给下次重建。

use ncd_deploy::{DockerCli, DockerCliError, bot_docker_container_candidates};
use ncd_domain::{BackendType, BotConfig, ContainerInfo, ContainerState};
use ncd_host::{Host, HostCommand, HostPath};

use super::AgreementsPayload;
use super::consent_files::{
    consent_record_json, parse_consent_version_json, payload_from_agreement_texts,
};
use crate::bot_manager::BotManagerError;

pub(super) async fn load_docker_agreements(
    host: &dyn Host,
    config: &BotConfig,
) -> Result<Option<AgreementsPayload>, BotManagerError> {
    let cli = DockerCli::for_bot(host, config);
    let Some(container) = find_container(&cli, config).await? else {
        // 首次创建尚无容器：compose up 后再查一次。
        return Ok(None);
    };
    let paths = container_paths(host, &cli, &container.id).await?;
    load_payload(host, &cli, &container.id, &paths)
        .await
        .map(Some)
}

/// 返回 false 时由调用方走运行中 WebUI，通知进程内的协议 gate。
pub(super) async fn record_docker_consent(
    host: &dyn Host,
    config: &BotConfig,
    version: &str,
) -> Result<bool, BotManagerError> {
    let cli = DockerCli::for_bot(host, config);
    let container = find_container(&cli, config)
        .await?
        .ok_or_else(|| failure("SnowLuma 容器不存在，请先启动 Bot 读取协议"))?;
    if !matches!(
        container.state,
        ContainerState::Created | ContainerState::Exited
    ) {
        return Ok(false);
    }
    let paths = container_paths(host, &cli, &container.id).await?;
    let payload = load_payload(host, &cli, &container.id, &paths).await?;
    if payload.version != version {
        return Err(failure("SnowLuma 协议版本已变化，请重新阅读并同意"));
    }
    let temp = temporary_path();
    let result = async {
        let config_dir = format!("{temp}/config");
        let file = format!("{config_dir}/consent.json");
        host.create_dir_all(&HostPath::from_posix(&config_dir))
            .await
            .map_err(failure)?;
        let bytes = consent_record_json(version).map_err(failure)?;
        host.write_file(&HostPath::from_posix(&file), &bytes)
            .await
            .map_err(failure)?;
        // SFTP 暂存默认可能为 0600；docker cp 写出的 root 文件必须允许协议用户读。
        // 官方镜像下次启动时会把持久化数据的属主改回 SNOWLUMA_UID/GID。
        let chmod = host
            .run_to_string(HostCommand::new("chmod").args(["-R", "u=rwX,go=rX", "--", &config_dir]))
            .await
            .map_err(failure)?;
        if !chmod.success() {
            return Err(failure(format!(
                "设置协议同意文件权限失败：{}",
                chmod.stderr.trim()
            )));
        }
        // 拷贝只有 consent.json 的 config 目录，兼容尚未首次启动的 Created 容器；
        // cp 合并已有目录，保留 WebUI 与 OneBot 配置。
        cli.copy_to_container(&container.id, &config_dir, &paths.data)
            .await
            .map_err(failure)?;
        let saved = read_container_text(host, &cli, &container.id, &paths.consent).await?;
        if saved
            .as_deref()
            .and_then(parse_consent_version_json)
            .as_deref()
            != Some(version)
        {
            return Err(failure("SnowLuma 协议同意记录写入后校验失败，请重试"));
        }
        // 旧模板可能把新版镜像的数据留在 /app/data 匿名卷；启动会重建容器，
        // 暂存本次明确同意的版本，交给配置同步写入托管 named volume 后删除。
        let project = ncd_deploy::docker_project_dir(
            host,
            &ncd_deploy::DockerDeployment::container_name(config),
        )
        .await
        .map_err(failure)?;
        let staging = HostPath::from_posix(format!("{project}/snowluma-data/config"));
        host.create_dir_all(&staging).await.map_err(failure)?;
        host.write_file(&staging.join("consent.json"), &bytes)
            .await
            .map_err(failure)?;
        Ok(true)
    }
    .await;
    let _ = host.remove_dir_all(&HostPath::from_posix(&temp)).await;
    result
}

async fn find_container(
    cli: &DockerCli<'_>,
    config: &BotConfig,
) -> Result<Option<ContainerInfo>, BotManagerError> {
    cli.ensure_daemon_ready().await.map_err(failure)?;
    let containers = cli.list_containers().await.map_err(failure)?;
    for name in bot_docker_container_candidates(BackendType::SnowLuma, config.bot.qq_id) {
        if let Some(container) = containers.iter().find(|container| container.name == name) {
            // 后续操作绑定本次读到的 ID，不能写到并发重建后的同名容器。
            return Ok(Some(container.clone()));
        }
    }
    Ok(None)
}

struct ContainerPaths {
    runtime: String,
    data: String,
    consent: String,
}

async fn container_paths(
    host: &dyn Host,
    cli: &DockerCli<'_>,
    container: &str,
) -> Result<ContainerPaths, BotManagerError> {
    let output = host
        .run_to_string(cli.command().args([
            "inspect",
            "--type",
            "container",
            "--format",
            "{{json .Config.Env}}",
            "--",
            container,
        ]))
        .await
        .map_err(failure)?;
    if !output.success() {
        return Err(failure(format!(
            "读取 SnowLuma 容器目录失败：{}",
            output.stderr.trim()
        )));
    }
    // 只使用两个目录变量，不记录或向 UI 返回其余容器环境变量（含 WebUI 凭据）。
    let environment: Option<Vec<String>> = serde_json::from_str(&output.stdout).map_err(failure)?;
    let environment = environment.unwrap_or_default();
    let directory = |key: &str, default: &str| -> Result<String, BotManagerError> {
        let value = environment
            .iter()
            .rev()
            .filter_map(|entry| entry.split_once('='))
            .find(|(name, _)| *name == key)
            .map(|(_, value)| value)
            .filter(|value| !value.is_empty())
            .unwrap_or(default);
        if !value.starts_with('/') || value.contains(['\0', '\n', '\r']) {
            return Err(failure(format!("SnowLuma 容器的 {key} 不是有效绝对目录")));
        }
        Ok(value.trim_end_matches('/').to_owned())
    };
    let data = directory("SNOWLUMA_DATA", "/app/snowluma-data")?;
    Ok(ContainerPaths {
        runtime: directory("SNOWLUMA_HOME", "/app/snowluma")?,
        consent: format!("{data}/config/consent.json"),
        data,
    })
}

async fn load_payload(
    host: &dyn Host,
    cli: &DockerCli<'_>,
    container: &str,
    paths: &ContainerPaths,
) -> Result<AgreementsPayload, BotManagerError> {
    // 等三条读取都完成并清理暂存文件，再传播其中任一错误。
    let (eula, privacy, consent) = tokio::join!(
        read_document(host, cli, container, &paths.runtime, "EULA.md"),
        read_document(host, cli, container, &paths.runtime, "PRIVACY.md"),
        read_container_text(host, cli, container, &paths.consent),
    );
    let (eula, privacy, consent) = (eula?, privacy?, consent?);
    if eula.trim().is_empty() || privacy.trim().is_empty() {
        return Err(failure(
            "SnowLuma 镜像缺少用户协议或隐私政策，请更新镜像后重试",
        ));
    }
    let version = consent.as_deref().and_then(parse_consent_version_json);
    payload_from_agreement_texts(&eula, &privacy, version.as_deref())
        .ok_or_else(|| failure("SnowLuma 协议正文为空"))
}

async fn read_document(
    host: &dyn Host,
    cli: &DockerCli<'_>,
    container: &str,
    runtime: &str,
    name: &str,
) -> Result<String, BotManagerError> {
    if let Some(text) =
        read_container_text(host, cli, container, &format!("{runtime}/{name}")).await?
        && !text.trim().is_empty()
    {
        return Ok(text);
    }
    Ok(
        read_container_text(host, cli, container, &format!("{runtime}/dist/{name}"))
            .await?
            .unwrap_or_default(),
    )
}

async fn read_container_text(
    host: &dyn Host,
    cli: &DockerCli<'_>,
    container: &str,
    path: &str,
) -> Result<Option<String>, BotManagerError> {
    let temp = temporary_path();
    let result = match cli.copy_from_container(container, path, &temp).await {
        Ok(()) => host
            .read_file(&HostPath::from_posix(&temp))
            .await
            .map_err(failure)
            .and_then(|bytes| String::from_utf8(bytes.to_vec()).map(Some).map_err(failure)),
        Err(DockerCliError::CommandFailed { ref stderr, .. }) if missing_file(stderr) => Ok(None),
        Err(error) => Err(failure(error)),
    };
    cli.remove_copied_file(&temp).await;
    result
}

fn missing_file(stderr: &str) -> bool {
    let stderr = stderr.to_ascii_lowercase();
    stderr.contains("could not find the file") || stderr.contains("no such file or directory")
}

fn temporary_path() -> String {
    format!("/tmp/ncd-snowluma-agreement-{}", uuid::Uuid::new_v4())
}

fn failure(error: impl std::fmt::Display) -> BotManagerError {
    BotManagerError::RemoteRead(error.to_string())
}

#[cfg(test)]
#[path = "docker_agreements_tests.rs"]
mod tests;
