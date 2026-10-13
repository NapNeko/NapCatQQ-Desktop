use super::*;
use async_trait::async_trait;
use bytes::Bytes;
use ncd_host::{
    Arch, ArchiveKind, CommandOutput, DirEntry, Host, HostError, HostPath, HostProcess, HostShell,
    Locality, Os,
};
use std::{path::Path, sync::Mutex};

struct MockHost {
    commands: Mutex<Vec<HostCommand>>,
    daemon_error: Option<&'static str>,
    missing: bool,
    local_missing: bool,
    disconnected: bool,
    fail_remove: bool,
}

impl MockHost {
    fn new() -> Self {
        Self {
            commands: Mutex::new(Vec::new()),
            daemon_error: None,
            missing: false,
            local_missing: false,
            disconnected: false,
            fail_remove: false,
        }
    }
}

#[async_trait]
impl Host for MockHost {
    fn os(&self) -> Os {
        Os::Linux
    }
    fn arch(&self) -> Arch {
        Arch::X86_64
    }
    fn locality(&self) -> Locality {
        if self.local_missing {
            Locality::Local
        } else {
            Locality::Remote
        }
    }
    fn id(&self) -> &str {
        "docker-test"
    }
    fn shell(&self) -> &dyn HostShell {
        &ncd_host::shell::BashShell
    }
    async fn read_file(&self, _: &HostPath) -> Result<Bytes, HostError> {
        unreachable!()
    }
    async fn write_file(&self, _: &HostPath, _: &[u8]) -> Result<(), HostError> {
        unreachable!()
    }
    async fn list_dir(&self, _: &HostPath) -> Result<Vec<DirEntry>, HostError> {
        unreachable!()
    }
    async fn create_dir_all(&self, _: &HostPath) -> Result<(), HostError> {
        unreachable!()
    }
    async fn remove_file(&self, _: &HostPath) -> Result<(), HostError> {
        unreachable!()
    }
    async fn remove_dir_all(&self, _: &HostPath) -> Result<(), HostError> {
        unreachable!()
    }
    async fn exists(&self, _: &HostPath) -> Result<bool, HostError> {
        unreachable!()
    }
    async fn upload(&self, _: &Path, _: &HostPath) -> Result<(), HostError> {
        unreachable!()
    }
    async fn download(&self, _: &HostPath, _: &Path) -> Result<(), HostError> {
        unreachable!()
    }
    async fn extract_archive(
        &self,
        _: &HostPath,
        _: &HostPath,
        _: ArchiveKind,
    ) -> Result<(), HostError> {
        unreachable!()
    }
    async fn spawn(&self, _: HostCommand) -> Result<Box<dyn HostProcess>, HostError> {
        unreachable!()
    }
    async fn run_to_string(&self, command: HostCommand) -> Result<CommandOutput, HostError> {
        self.commands.lock().unwrap().push(command.clone());
        if self.local_missing {
            return Err(HostError::Io(std::io::Error::from(
                std::io::ErrorKind::NotFound,
            )));
        }
        if self.disconnected {
            return Err(HostError::Unsupported {
                operation: "SSH disconnected",
            });
        }
        let (code, stdout, stderr) = if self.missing {
            (127, "", "docker: command not found")
        } else {
            match command.args.first().map(String::as_str) {
                Some("version") => (1, "27.3.1\n", "daemon is unreachable"),
                Some("info") if !command.elevated && self.daemon_error.is_some() => {
                    (1, "", self.daemon_error.unwrap())
                }
                Some("info") => (0, "27.3.1", ""),
                Some("compose") => (1, "", "compose is not a docker command"),
                Some("volume" | "network") if self.fail_remove => (1, "", "resource is in use"),
                _ => (0, "", ""),
            }
        };
        Ok(CommandOutput {
            exit_code: Some(code),
            stdout: stdout.into(),
            stderr: stderr.into(),
        })
    }
}

#[tokio::test]
async fn docker_management_prefers_user_daemon_without_compose() {
    let host = MockHost::new();
    let cli = DockerCli::new(&host);
    let status = cli.probe_checked().await.unwrap();
    assert_eq!(status.version, "27.3.1");
    assert!(status.daemon_running);
    assert!(!status.compose_available);
    assert!(!host.commands.lock().unwrap().iter().any(|c| c.elevated));
}

#[tokio::test]
async fn docker_management_uses_sudo_only_for_default_socket_permission() {
    let host = MockHost {
        daemon_error: Some("permission denied connecting to /var/run/docker.sock"),
        ..MockHost::new()
    };
    let cli = DockerCli::new(&host);
    cli.ensure_daemon_ready().await.unwrap();
    cli.container_action(ContainerAction::Pause, "abc123", false)
        .await
        .unwrap();
    let commands = host.commands.lock().unwrap();
    let info: Vec<_> = commands.iter().filter(|c| c.args[0] == "info").collect();
    assert_eq!(info.len(), 2);
    assert!(!info[0].elevated);
    assert!(info[1].elevated);
    assert!(commands.last().unwrap().elevated);
    assert_eq!(commands.last().unwrap().args, ["pause", "--", "abc123"]);
}

#[tokio::test]
async fn docker_management_does_not_switch_context_on_daemon_failure() {
    for error in [
        "Cannot connect to the Docker daemon",
        "permission denied on /run/user/1000/docker.sock",
    ] {
        let host = MockHost {
            daemon_error: Some(error),
            ..MockHost::new()
        };
        assert!(DockerCli::new(&host).probe_checked().await.is_err());
        assert!(!host.commands.lock().unwrap().iter().any(|c| c.elevated));
    }
}

#[tokio::test]
async fn docker_management_distinguishes_missing_cli_and_connection_errors() {
    let local_missing = MockHost {
        local_missing: true,
        ..MockHost::new()
    };
    assert!(
        !DockerCli::new(&local_missing)
            .probe_checked()
            .await
            .unwrap()
            .installed
    );
    let missing = MockHost {
        missing: true,
        ..MockHost::new()
    };
    assert!(
        !DockerCli::new(&missing)
            .probe_checked()
            .await
            .unwrap()
            .installed
    );
    let disconnected = MockHost {
        disconnected: true,
        ..MockHost::new()
    };
    assert!(matches!(
        DockerCli::new(&disconnected).probe_checked().await,
        Err(DockerCliError::Host(_))
    ));
}

#[tokio::test]
async fn docker_management_removal_requires_explicit_force_and_preserves_volumes() {
    let host = MockHost::new();
    let cli = DockerCli::new(&host);
    cli.container_action(ContainerAction::Remove, "abc123", false)
        .await
        .unwrap();
    cli.container_action(ContainerAction::Remove, "abc123", true)
        .await
        .unwrap();
    cli.remove_resource(DockerResourceKind::Volume, "saved-data")
        .await
        .unwrap();
    cli.logs("abc123", u32::MAX).await.unwrap();
    let commands = host.commands.lock().unwrap();
    assert_eq!(commands[0].args, ["rm", "--", "abc123"]);
    assert_eq!(commands[1].args, ["rm", "--force", "--", "abc123"]);
    assert_eq!(commands[2].args, ["volume", "rm", "--", "saved-data"]);
    assert_eq!(commands[3].args, ["logs", "--tail", "2000", "--", "abc123"]);
    assert!(commands.iter().all(|c| c.timeout.is_some()));
}

#[tokio::test]
async fn docker_management_rejects_flags_and_builtin_network_without_execution() {
    let host = MockHost::new();
    let cli = DockerCli::new(&host);
    assert!(
        cli.remove_resource(DockerResourceKind::Network, "bridge")
            .await
            .is_err()
    );
    for id in ["", "--all", "a\nb", " a"] {
        assert!(
            cli.container_action(ContainerAction::Remove, id, true)
                .await
                .is_err()
        );
        assert!(cli.remove_image(id, true).await.is_err());
    }
    assert!(cli.lifecycle("prune", "abc").await.is_err());
    for name in ["../bot", "..", "/root", "a/b", "a\\b"] {
        assert!(DockerCli::validate_project_name(name).is_err());
    }
    assert!(host.commands.lock().unwrap().is_empty());
}

#[tokio::test]
async fn docker_management_propagates_resource_in_use_errors() {
    let host = MockHost {
        fail_remove: true,
        ..MockHost::new()
    };
    let err = DockerCli::new(&host)
        .remove_resource(DockerResourceKind::Volume, "data")
        .await
        .unwrap_err();
    assert!(err.to_string().contains("resource is in use"));
}

#[test]
fn docker_management_resource_parsing_is_strict_but_accepts_empty_lists() {
    assert!(parse_lines::<NetworkLine>(" \n").unwrap().is_empty());
    let networks = parse_lines::<NetworkLine>(
        r#"{"ID":"a","Name":"custom","Driver":"bridge","Scope":"local"}"#,
    )
    .unwrap();
    assert_eq!(networks[0].name, "custom");
    assert!(parse_lines::<VolumeLine>("{broken}").is_err());
    assert!(parse_lines::<VolumeLine>(r#"{"Name":"data","Driver":"local"}"#).is_ok());
}

#[test]
fn docker_management_inspect_handles_null_networks_and_filters_secrets() {
    let raw = r#"[{"Id":"abc","Name":"/demo","Created":"2026-01-01T00:00:00Z",
      "Config":{"Image":"nginx:alpine","Env":["TOKEN=never-export-this"]},
      "State":{"Status":"exited","OOMKilled":true,"ExitCode":137},
      "HostConfig":{"NetworkMode":"bridge","RestartPolicy":{"Name":"on-failure","MaximumRetryCount":3}},
      "Mounts":[{"Type":"bind","Source":"/srv/data","Destination":"/data","RW":false}],
      "NetworkSettings":{"Networks":null,"Ports":{"80/tcp":null,"81/tcp":[{"HostIp":"::","HostPort":"8081"}]}}}]"#;
    let details = parse_details(raw).unwrap();
    assert_eq!(details.name, "demo");
    assert_eq!(details.exit_code, 137);
    assert!(details.oom_killed);
    assert_eq!(details.restart_policy, "on-failure:3");
    assert!(details.health.is_none());
    assert!(details.mounts[0].read_only);
    assert!(details.ports.iter().any(|p| p.contains("[::]:8081")));
    assert!(
        !serde_json::to_string(&details)
            .unwrap()
            .contains("never-export-this")
    );
    assert!(parse_details("[]").is_err());
}
