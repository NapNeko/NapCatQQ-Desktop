//! Linux Podman 的包装命令与独立 provider 必须在探测和部署时使用同一条路径。
//! Docker（包括 Windows）继续使用原有 `docker compose` 和提权决策。

use super::{DockerCli, DockerCliError};
use crate::docker::engine::ContainerEngine;
use ncd_host::{HostCommand, Os};
use std::time::Duration;

#[derive(Clone, Copy)]
enum Provider {
    PodmanWrapper,
    Standalone,
}

pub(super) struct PodmanComposeProbe {
    provider: Option<Provider>,
    broken_provider: bool,
    diagnostic: String,
}

impl DockerCli<'_> {
    pub(super) fn is_linux_podman(&self) -> bool {
        self.host.os() == Os::Linux && self.engine == ContainerEngine::Podman
    }

    async fn probe_podman_compose(&self) -> &PodmanComposeProbe {
        self.podman_compose
            .get_or_init(|| async {
                let mut errors = Vec::new();
                let mut broken_provider = false;
                for (provider, command, label) in [
                    (
                        Provider::PodmanWrapper,
                        HostCommand::new("podman").args(["compose", "version"]),
                        "podman compose version",
                    ),
                    (
                        Provider::Standalone,
                        HostCommand::new("podman-compose").arg("--version"),
                        "podman-compose --version",
                    ),
                ] {
                    // 不允许提权：root 的 Podman 存储和当前用户完全独立。
                    match self
                        .host
                        .run_to_string(command.timeout(Duration::from_secs(5)))
                        .await
                    {
                        Ok(output) if output.success() => {
                            return PodmanComposeProbe {
                                provider: Some(provider),
                                broken_provider: false,
                                diagnostic: String::new(),
                            };
                        }
                        Ok(output) => {
                            if matches!(provider, Provider::Standalone) && output.exit_code != Some(127) {
                                broken_provider = true;
                            }
                            let detail = if output.stderr.trim().is_empty() {
                                output.stdout.trim()
                            } else {
                                output.stderr.trim()
                            };
                            errors.push(format!("{label}: exit={:?}: {detail}", output.exit_code));
                        }
                        Err(error) => {
                            if matches!(provider, Provider::Standalone)
                                && !matches!(&error, ncd_host::HostError::Io(e) if e.kind() == std::io::ErrorKind::NotFound)
                            {
                                broken_provider = true;
                            }
                            errors.push(format!("{label}: {error}"));
                        }
                    }
                }
                let diagnostic = errors.join("\n");
                tracing::warn!(
                    "Podman Compose 探测失败（主机 {}）: {diagnostic}",
                    self.host.id()
                );
                PodmanComposeProbe {
                    provider: None,
                    broken_provider,
                    diagnostic,
                }
            })
            .await
    }

    pub(super) async fn podman_compose_available(&self) -> bool {
        self.probe_podman_compose().await.provider.is_some()
    }

    pub(crate) async fn podman_compose_needs_repair(&self) -> bool {
        self.is_linux_podman() && self.probe_podman_compose().await.broken_provider
    }

    pub(crate) async fn compose_diagnostic(&self) -> Option<String> {
        if !self.is_linux_podman() {
            return None;
        }
        let probe = self.probe_podman_compose().await;
        probe.provider.is_none().then(|| probe.diagnostic.clone())
    }

    pub async fn compose_command(&self) -> Result<HostCommand, DockerCliError> {
        self.verify_selected_engine().await?;
        if !self.is_linux_podman() {
            return Ok(self.command().arg("compose"));
        }
        let probe = self.probe_podman_compose().await;
        match probe.provider {
            Some(Provider::PodmanWrapper) => Ok(HostCommand::new("podman").arg("compose")),
            Some(Provider::Standalone) => Ok(HostCommand::new("podman-compose")),
            None => Err(DockerCliError::CommandFailed {
                command: "Podman Compose 探测".into(),
                exit_code: None,
                stderr: probe.diagnostic.clone(),
            }),
        }
    }
}
