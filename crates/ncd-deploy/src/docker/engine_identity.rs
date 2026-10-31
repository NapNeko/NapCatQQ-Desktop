//! podman-docker 提供的 docker 命令不能套用 Docker 模板、安装或 sudo 路径。

use super::{DockerCli, DockerCliError};
use crate::docker::engine::ContainerEngine;
use ncd_host::{HostCommand, Os};

impl DockerCli<'_> {
    pub(crate) async fn verify_selected_engine(&self) -> Result<(), DockerCliError> {
        if self.host.os() != Os::Linux || self.engine != ContainerEngine::Docker {
            return Ok(());
        }
        let is_podman = self
            .docker_is_podman
            .get_or_init(|| async {
                // --version 不依赖运行时，也不需要 sudo；nodocker 文件可关闭兼容警告，
                // 因此不能只通过 stderr 的警告判断身份。
                match self
                    .host
                    .run_to_string(
                        HostCommand::new("docker")
                            .arg("--version")
                            .timeout(std::time::Duration::from_secs(5)),
                    )
                    .await
                {
                    Ok(out) => {
                        out.stdout
                            .trim()
                            .to_ascii_lowercase()
                            .starts_with("podman version ")
                            || out
                                .stderr
                                .to_ascii_lowercase()
                                .contains("emulate docker cli using podman")
                    }
                    // 缺失 CLI、连接等错误仍由原有探测保留具体原因。
                    Err(_) => false,
                }
            })
            .await;
        if *is_podman {
            Err(DockerCliError::PodmanCompatibilityShim)
        } else {
            Ok(())
        }
    }
}
