//! Docker 资源管理；只返回展示所需字段，不把 inspect 的环境变量带过 IPC。

use std::{collections::BTreeMap, time::Duration};

use ncd_domain::{ContainerAction, ContainerState, DockerStatus, docker::*};
use ncd_host::HostCommand;
use serde::{Deserialize, de::DeserializeOwned};

use super::{DockerCli, DockerCliError};

impl DockerCli<'_> {
    pub fn validate_project_name(name: &str) -> Result<(), DockerCliError> {
        if !name.starts_with(|c: char| c.is_ascii_alphanumeric())
            || name.len() > 255
            || !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
        {
            return Err(DockerCliError::InvalidArgument("部署名称无效".into()));
        }
        Ok(())
    }
    pub(super) fn validate_reference(value: &str) -> Result<(), DockerCliError> {
        if value.is_empty()
            || value.starts_with('-')
            || value.len() > 512
            || value.chars().any(|c| c.is_whitespace() || c.is_control())
        {
            return Err(DockerCliError::InvalidArgument("资源名称或 ID 无效".into()));
        }
        Ok(())
    }

    pub(super) async fn management_output(
        &self,
        cmd: HostCommand,
    ) -> Result<String, DockerCliError> {
        let command = format!("{} {}", self.engine.program(), cmd.args.join(" "));
        let out = self.host.run_to_string(cmd).await?;
        if !out.success() {
            return Err(DockerCliError::CommandFailed {
                command,
                exit_code: out.exit_code,
                stderr: out.stderr.trim().to_owned(),
            });
        }
        Ok(out.stdout)
    }

    /// 管理页保留连接、权限和 daemon 错误，不能把故障画成“未安装”。
    pub async fn probe_checked(&self) -> Result<DockerStatus, DockerCliError> {
        self.verify_selected_engine().await?;
        let result = self.host.run_to_string(self.client_version_command()).await;
        let out = match result {
            Err(ncd_host::HostError::Io(error))
                if self.host.locality() == ncd_host::Locality::Local
                    && error.kind() == std::io::ErrorKind::NotFound =>
            {
                return Ok(DockerStatus::absent());
            }
            result => result?,
        };
        if out.exit_code == Some(127) {
            return Ok(DockerStatus::absent());
        }
        // version 在 daemon 不可达时仍输出 Client.Version，但退出码为 1。
        let version = self.client_version_text(&out.stdout);
        if version.is_empty() {
            return Err(DockerCliError::CommandFailed {
                command: if self.is_linux_podman() {
                    "podman --version"
                } else {
                    "docker version"
                }
                .into(),
                exit_code: out.exit_code,
                stderr: out.stderr,
            });
        }
        self.management_probe_daemon().await?;
        let compose_available = self.docker_compose_ok().await;
        Ok(DockerStatus {
            compose_diagnostic: self.compose_diagnostic().await,
            installed: true,
            version,
            daemon_running: true,
            compose_available,
        })
    }

    pub(super) async fn management_probe_daemon(&self) -> Result<(), DockerCliError> {
        use std::sync::atomic::Ordering;
        self.elevated.store(false, Ordering::Relaxed);
        let cmd = if self.engine == crate::docker::engine::ContainerEngine::Podman {
            HostCommand::new("podman").arg("info")
        } else {
            HostCommand::new("docker").args(["info", "--format", "{{.ServerVersion}}"])
        }
        .timeout(Duration::from_secs(15));
        let out = self.host.run_to_string(cmd.clone()).await?;
        if out.success() {
            return Ok(());
        }
        let error = out.stderr.to_ascii_lowercase();
        // 只在 socket 权限不足时提权；context 不可达不能悄悄切到 root 的 daemon。
        if self.engine != crate::docker::engine::ContainerEngine::Podman
            && self.host.os() != ncd_host::Os::Windows
            && error.contains("permission denied")
            && (error.contains("/var/run/docker.sock") || error.contains("unix:///run/docker.sock"))
        {
            self.management_output(cmd.elevated()).await?;
            self.elevated.store(true, Ordering::Relaxed);
            return Ok(());
        }
        Err(DockerCliError::CommandFailed {
            command: format!("{} info", self.engine.program()),
            exit_code: out.exit_code,
            stderr: out.stderr,
        })
    }

    pub async fn container_action(
        &self,
        action: ContainerAction,
        id: &str,
        force: bool,
    ) -> Result<(), DockerCliError> {
        Self::validate_reference(id)?;
        if action != ContainerAction::Remove {
            return self.lifecycle(action.as_str(), id).await;
        }
        let mut cmd = self.command().arg("rm");
        if force {
            cmd = cmd.arg("--force");
        }
        self.management_output(cmd.arg("--").arg(id).timeout(Duration::from_secs(60)))
            .await?;
        Ok(())
    }

    pub async fn container_details(
        &self,
        id: &str,
    ) -> Result<DockerContainerDetails, DockerCliError> {
        Self::validate_reference(id)?;
        let out = self
            .management_output(
                self.command()
                    .args(["container", "inspect", "--", id])
                    .timeout(Duration::from_secs(30)),
            )
            .await?;
        parse_details(&out)
    }

    pub async fn container_stats(&self, id: &str) -> Result<DockerContainerStats, DockerCliError> {
        Self::validate_reference(id)?;
        let out = self
            .management_output(
                self.command()
                    .args(["stats", "--no-stream", "--format", if self.engine == crate::docker::engine::ContainerEngine::Podman {
                        r#"{"CPUPerc":{{json .CPUPerc}},"MemUsage":{{json .MemUsage}},"MemPerc":{{json .MemPerc}},"NetIO":{{json .NetIO}},"BlockIO":{{json .BlockIO}},"PIDs":{{json (printf "%v" .PIDs)}}}"#
                    } else { "{{json .}}" }, "--", id])
                    .timeout(Duration::from_secs(20)),
            )
            .await?;
        let raw: StatsLine = serde_json::from_str(out.trim())
            .map_err(|e| DockerCliError::ParseFailed(format!("docker stats: {e}")))?;
        Ok(DockerContainerStats {
            cpu_percent: raw.cpu,
            memory_usage: raw.memory,
            memory_percent: raw.memory_percent,
            network_io: raw.network,
            block_io: raw.block,
            pids: raw.pids,
        })
    }

    pub async fn list_networks(&self) -> Result<Vec<DockerNetworkInfo>, DockerCliError> {
        let out = self
            .management_output(
                self.command()
                    .args(["network", "ls", "--no-trunc", "--format", "{{json .}}"])
                    .timeout(Duration::from_secs(30)),
            )
            .await?;
        Ok(parse_lines::<NetworkLine>(&out)?
            .into_iter()
            .map(|r| DockerNetworkInfo {
                id: r.id,
                name: r.name,
                driver: r.driver,
                scope: r.scope,
            })
            .collect())
    }

    pub async fn list_volumes(&self) -> Result<Vec<DockerVolumeInfo>, DockerCliError> {
        let out = self
            .management_output(
                self.command()
                    .args(["volume", "ls", "--format", "{{json .}}"])
                    .timeout(Duration::from_secs(30)),
            )
            .await?;
        Ok(parse_lines::<VolumeLine>(&out)?
            .into_iter()
            .map(|r| DockerVolumeInfo {
                name: r.name,
                driver: r.driver,
                scope: r.scope,
            })
            .collect())
    }

    pub async fn remove_resource(
        &self,
        kind: DockerResourceKind,
        id: &str,
    ) -> Result<(), DockerCliError> {
        Self::validate_reference(id)?;
        if kind == DockerResourceKind::Network && matches!(id, "bridge" | "host" | "none") {
            return Err(DockerCliError::InvalidArgument(
                "不能删除 Docker 内置网络".into(),
            ));
        }
        // 不加 force；Docker 会拒绝删除仍在使用的网络或卷。
        self.management_output(
            self.command()
                .args([kind.as_str(), "rm", "--", id])
                .timeout(Duration::from_secs(60)),
        )
        .await?;
        Ok(())
    }
}

fn parse_lines<T: DeserializeOwned>(output: &str) -> Result<Vec<T>, DockerCliError> {
    output
        .lines()
        .filter(|line| !line.trim().is_empty())
        .enumerate()
        .map(|(i, line)| {
            serde_json::from_str(line)
                .map_err(|e| DockerCliError::ParseFailed(format!("第 {} 行: {e}", i + 1)))
        })
        .collect()
}

#[derive(Deserialize)]
struct NetworkLine {
    #[serde(rename = "ID", alias = "id", alias = "Id")]
    id: String,
    #[serde(rename = "Name", alias = "name")]
    name: String,
    #[serde(rename = "Driver", alias = "driver")]
    driver: String,
    #[serde(rename = "Scope", default)]
    scope: String,
}

#[derive(Deserialize)]
struct VolumeLine {
    #[serde(rename = "Name", alias = "name")]
    name: String,
    #[serde(rename = "Driver", alias = "driver")]
    driver: String,
    #[serde(rename = "Scope", default)]
    scope: String,
}

#[derive(Deserialize)]
struct StatsLine {
    #[serde(rename = "CPUPerc")]
    cpu: String,
    #[serde(rename = "MemUsage")]
    memory: String,
    #[serde(rename = "MemPerc", default)]
    memory_percent: String,
    #[serde(rename = "NetIO")]
    network: String,
    #[serde(rename = "BlockIO")]
    block: String,
    #[serde(rename = "PIDs")]
    pids: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct InspectContainer {
    id: String,
    name: String,
    created: String,
    state: InspectState,
    config: InspectConfig,
    host_config: InspectHostConfig,
    #[serde(default)]
    restart_count: u32,
    #[serde(default)]
    mounts: Vec<InspectMount>,
    network_settings: InspectNetworks,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct InspectState {
    status: String,
    #[serde(default)]
    started_at: String,
    #[serde(default)]
    finished_at: String,
    #[serde(default)]
    exit_code: i32,
    #[serde(rename = "OOMKilled", default)]
    oom_killed: bool,
    #[serde(default)]
    error: String,
    health: Option<InspectHealth>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct InspectHealth {
    status: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct InspectConfig {
    image: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct InspectHostConfig {
    network_mode: String,
    restart_policy: InspectRestartPolicy,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct InspectRestartPolicy {
    name: String,
    #[serde(default)]
    maximum_retry_count: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct InspectMount {
    #[serde(rename = "Type")]
    kind: String,
    #[serde(default)]
    source: String,
    destination: String,
    #[serde(rename = "RW", default)]
    writable: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct InspectNetworks {
    networks: Option<BTreeMap<String, serde::de::IgnoredAny>>,
    ports: Option<BTreeMap<String, Option<Vec<InspectPort>>>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct InspectPort {
    host_ip: String,
    host_port: String,
}

fn parse_details(output: &str) -> Result<DockerContainerDetails, DockerCliError> {
    let mut rows: Vec<InspectContainer> = serde_json::from_str(output)
        .map_err(|e| DockerCliError::ParseFailed(format!("docker inspect: {e}")))?;
    if rows.len() != 1 {
        return Err(DockerCliError::ParseFailed("inspect 应返回一个容器".into()));
    }
    let raw = rows.remove(0);
    let mut ports = Vec::new();
    for (port, bindings) in raw.network_settings.ports.unwrap_or_default() {
        match bindings.filter(|b| !b.is_empty()) {
            None => ports.push(format!("{port}（未发布）")),
            Some(bindings) => {
                for binding in bindings {
                    let ip = if binding.host_ip.contains(':') {
                        format!("[{}]", binding.host_ip)
                    } else {
                        binding.host_ip
                    };
                    ports.push(format!("{ip}:{} → {port}", binding.host_port));
                }
            }
        }
    }
    let policy = raw.host_config.restart_policy;
    let restart_policy = if policy.name == "on-failure" && policy.maximum_retry_count > 0 {
        format!("on-failure:{}", policy.maximum_retry_count)
    } else {
        policy.name
    };
    Ok(DockerContainerDetails {
        id: raw.id,
        name: raw.name.trim_start_matches('/').into(),
        image: raw.config.image,
        created: raw.created,
        state: ContainerState::parse(&raw.state.status),
        started_at: raw.state.started_at,
        finished_at: raw.state.finished_at,
        exit_code: raw.state.exit_code,
        oom_killed: raw.state.oom_killed,
        error: raw.state.error,
        restart_count: raw.restart_count,
        restart_policy,
        health: raw.state.health.map(|h| h.status),
        network_mode: raw.host_config.network_mode,
        networks: raw
            .network_settings
            .networks
            .unwrap_or_default()
            .into_keys()
            .collect(),
        ports,
        mounts: raw
            .mounts
            .into_iter()
            .map(|m| DockerMountInfo {
                kind: m.kind,
                source: m.source,
                destination: m.destination,
                read_only: !m.writable,
            })
            .collect(),
    })
}

#[cfg(test)]
#[path = "management_tests.rs"]
mod tests;
