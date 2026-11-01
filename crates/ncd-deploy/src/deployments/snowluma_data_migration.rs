//! 在删除旧 SnowLuma 容器之前保全整个数据目录，并预填 Compose 的持久卷。
//! 快照按不可变容器 ID 保存；失败时旧容器仍在，完整快照也不会被重试覆盖。

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ncd_domain::{ContainerInfo, ContainerState};
use ncd_host::{Host, HostCommand, HostPath};

use crate::{DeploymentError, DockerCli};

const TARGET: &str = "/app/snowluma-data";
const MERGE: &str = include_str!("../../resources/snowluma-data-migrate.cjs");
const TIMEOUT: Duration = Duration::from_secs(900);

fn failure(error: impl std::fmt::Display) -> DeploymentError {
    DeploymentError::LaunchFailed(format!(
        "迁移 SnowLuma 旧容器数据失败，保留旧容器和备份：{error}"
    ))
}

async fn checked(host: &dyn Host, command: HostCommand) -> Result<(), DeploymentError> {
    let output = host
        .run_to_string(command.timeout(TIMEOUT))
        .await
        .map_err(failure)?;
    if !output.success() {
        return Err(failure(output.stderr.trim()));
    }
    Ok(())
}

fn data_dir(environment: &str) -> Result<String, DeploymentError> {
    // 不在错误或日志中回传完整环境变量，其中可能包含 WebUI 密码。
    let environment: Option<Vec<String>> =
        serde_json::from_str(environment).map_err(|_| failure("无法解析旧容器目录配置"))?;
    let directory = environment
        .as_deref()
        .unwrap_or_default()
        .iter()
        .rev()
        .find_map(|entry| entry.strip_prefix("SNOWLUMA_DATA="))
        .filter(|value| !value.is_empty())
        .unwrap_or("/app/data")
        .trim_end_matches('/');
    if !directory.starts_with('/')
        || directory.len() < 2
        || directory.contains(['\0', '\n', '\r'])
        || directory.split('/').any(|part| matches!(part, "." | ".."))
    {
        return Err(failure("旧容器 SNOWLUMA_DATA 不是有效数据目录"));
    }
    Ok(directory.to_string())
}

pub(super) async fn migrate(
    host: &dyn Host,
    cli: &DockerCli<'_>,
    project: &str,
    container: &ContainerInfo,
) -> Result<(), DeploymentError> {
    if !matches!(
        container.state,
        ContainerState::Created | ContainerState::Exited
    ) {
        return Err(failure("旧容器尚未停止，请先停止后重试"));
    }
    if container.id.is_empty() || !container.id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(failure("旧容器 ID 无效"));
    }
    let output = host
        .run_to_string(
            cli.command()
                .args([
                    "inspect",
                    "--type",
                    "container",
                    "--format",
                    "{{json .Config.Env}}",
                    "--",
                    &container.id,
                ])
                .timeout(Duration::from_secs(30)),
        )
        .await
        .map_err(failure)?;
    if !output.success() {
        return Err(failure(output.stderr.trim()));
    }
    // 升级后旧镜像可能只剩 digest/ID，不能仅凭镜像标签跳过迁移。
    let environment: Option<Vec<String>> =
        serde_json::from_str(&output.stdout).map_err(|_| failure("无法解析旧容器目录配置"))?;
    let snowluma = environment
        .as_deref()
        .unwrap_or_default()
        .iter()
        .any(|entry| entry.starts_with("SNOWLUMA_"));
    let image = container.image.to_ascii_lowercase();
    if !snowluma && !image.contains("snowluma") {
        // 只跳过已确认的 NapCat；无法确认类型的容器必须保留供用户核对。
        if image.contains("napcat") {
            return Ok(());
        }
        return Err(failure("无法确认旧容器的协议端类型，请核对镜像和目录配置"));
    }
    let source = data_dir(&output.stdout)?;
    if source == TARGET {
        return Ok(());
    }

    let root = format!("{project}/snowluma-legacy");
    host.create_dir_all(&HostPath::from_posix(&root))
        .await
        .map_err(failure)?;
    checked(host, HostCommand::new("chmod").args(["700", "--", &root])).await?;
    let backup = format!("{root}/{}", container.id);
    if !host
        .exists(&HostPath::from_posix(&backup))
        .await
        .map_err(failure)?
    {
        let partial = format!("{backup}.partial");
        // 提权 cp 的半成品可能归 root；只清理本次快照的未完成目录。
        let cleanup = HostCommand::new("rm").args(["-rf", "--", &partial]);
        checked(
            host,
            if cli.is_elevated() {
                cleanup.elevated()
            } else {
                cleanup
            },
        )
        .await?;
        host.create_dir_all(&HostPath::from_posix(&partial))
            .await
            .map_err(failure)?;
        let output = host
            .run_to_string(
                cli.command()
                    .args(["cp", &format!("{}:{source}/.", container.id), &partial])
                    .timeout(TIMEOUT),
            )
            .await
            .map_err(failure)?;
        if !output.success() {
            // Created 容器可能从未生成数据目录；其他拷贝错误一律中止重建。
            let error = output.stderr.to_ascii_lowercase();
            if container.state == ContainerState::Created
                && error.contains(&source.to_ascii_lowercase())
                && (error.contains("could not find the file")
                    || error.contains("no such file or directory"))
            {
                return Ok(());
            }
            return Err(failure(output.stderr.trim()));
        }
        host.rename(
            &HostPath::from_posix(&partial),
            &HostPath::from_posix(&backup),
        )
        .await
        .map_err(failure)?;
    }

    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(failure)?
        .as_nanos();
    let helper = format!("ncd-sl-migrate-{}-{nonce}", container.id);
    let result = async {
        // Compose 决定实际卷名，兼容旧项目名与 Podman provider。没有服务端口、
        // 没有依赖，覆盖入口后绝不启动 QQ / WebUI / 协议 gate。
        let mut command = cli.compose_command().await.map_err(failure)?.args([
            "run",
            "--rm",
            "--no-deps",
            "-d",
            "--name",
            &helper,
            "--user",
            "0",
            "--entrypoint",
            "node",
            "-e",
            "NODE_OPTIONS=",
        ]);
        if cli.engine() == crate::docker::engine::ContainerEngine::Docker {
            command = command.args(["--pull", "never"]);
        }
        checked(
            host,
            command
                .args(["snowluma", "-e", "setTimeout(() => {}, 900000)"])
                .working_dir(HostPath::from_posix(project)),
        )
        .await?;
        // 不 bind 宿主机快照，避免远端路径和 SELinux relabel 影响备份。
        checked(
            host,
            cli.command()
                .args(["cp", &backup, &format!("{helper}:/tmp/ncd-snowluma-legacy")]),
        )
        .await?;
        checked(
            host,
            cli.command().args([
                "exec",
                "--user",
                "0",
                &helper,
                "node",
                "-e",
                MERGE,
                "/tmp/ncd-snowluma-legacy",
                TARGET,
            ]),
        )
        .await
    }
    .await;
    let cleanup = checked(host, cli.command().args(["rm", "-f", "--", &helper])).await;
    result?;
    cleanup?;
    Ok(())
}

#[cfg(test)]
#[path = "snowluma_data_migration_tests.rs"]
mod orchestration_tests;

#[cfg(all(test, target_os = "linux"))]
#[path = "snowluma_data_migration_docker_tests.rs"]
mod docker_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_default_and_explicit_data_directory() {
        assert_eq!(data_dir("null").unwrap(), "/app/data");
        assert_eq!(
            data_dir(r#"["SNOWLUMA_DATA=/custom/data/"]"#).unwrap(),
            "/custom/data"
        );
        assert_eq!(
            data_dir(r#"["SNOWLUMA_DATA=/app/data","SNOWLUMA_DATA=/app/snowluma-data"]"#).unwrap(),
            TARGET
        );
    }

    #[test]
    fn invalid_data_directory_is_rejected_without_environment_secrets() {
        for value in ["/", "relative", "/app/../", "/app/./data", "/data\nother"] {
            let environment = serde_json::to_string(&vec![
                format!("SNOWLUMA_DATA={value}"),
                "PASSWORD=secret".into(),
            ])
            .unwrap();
            let error = data_dir(&environment).unwrap_err().to_string();
            assert!(!error.contains("secret"));
        }
        assert!(data_dir(r#"{"PASSWORD":"secret"}"#).is_err());
    }
}
