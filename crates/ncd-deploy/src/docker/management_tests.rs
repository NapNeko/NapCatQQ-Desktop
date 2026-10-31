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
    compose_available: bool,
    standalone_compose: bool,
    docker_compat: bool,
    formatted_podman_version_fails: bool,
    standalone_compose_error: Option<&'static str>,
    os: Os,
    id: &'static str,
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
            compose_available: false,
            standalone_compose: false,
            docker_compat: false,
            formatted_podman_version_fails: false,
            standalone_compose_error: None,
            os: Os::Linux,
            id: "docker-test",
        }
    }
}

#[async_trait]
impl Host for MockHost {
    fn os(&self) -> Os {
        self.os
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
        self.id
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
        let (code, stdout, stderr) = if command.program == "podman-compose" {
            if let Some(error) = self.standalone_compose_error {
                (1, "", error)
            } else if self.standalone_compose {
                (0, "podman-compose version 1.6.0", "")
            } else {
                (127, "", "podman-compose: command not found")
            }
        } else if self.missing {
            (127, "", "docker: command not found")
        } else {
            match command.args.first().map(String::as_str) {
                Some("--version") if self.docker_compat || command.program == "podman" => {
                    (0, "podman version 5.8.9\n", "")
                }
                Some("--version") => (0, "Docker version 27.3.1, build example\n", ""),
                Some("version")
                    if command.program == "podman" && self.formatted_podman_version_fails =>
                {
                    (125, "", "cannot re-exec process")
                }
                Some("version") => (1, "27.3.1\n", "daemon is unreachable"),
                Some("info") if !command.elevated && self.daemon_error.is_some() => {
                    (1, "", self.daemon_error.unwrap())
                }
                Some("info") => (0, "27.3.1", ""),
                Some("compose") if self.compose_available => (0, "compose version", ""),
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

#[tokio::test]
async fn podman_keeps_rootless_storage_for_probe_and_container_actions() {
    let host = MockHost {
        compose_available: true,
        ..MockHost::new()
    };
    let cli = DockerCli {
        engine: crate::docker::engine::ContainerEngine::Podman,
        ..DockerCli::new(&host)
    };
    cli.ensure_ready().await.unwrap();
    cli.container_action(ContainerAction::Start, "abc123", false)
        .await
        .unwrap();
    let commands = host.commands.lock().unwrap();
    assert!(
        commands
            .iter()
            .all(|command| command.program == "podman" && !command.elevated)
    );
    assert_eq!(commands.last().unwrap().args, ["start", "--", "abc123"]);
}

#[tokio::test]
async fn podman_permission_errors_never_select_root_storage() {
    let host = MockHost {
        daemon_error: Some("permission denied connecting to /var/run/docker.sock"),
        ..MockHost::new()
    };
    let cli = DockerCli {
        engine: crate::docker::engine::ContainerEngine::Podman,
        ..DockerCli::new(&host)
    };
    assert!(cli.ensure_daemon_ready().await.is_err());
    assert!(
        !host
            .commands
            .lock()
            .unwrap()
            .iter()
            .any(|command| command.elevated)
    );
}

#[tokio::test]
async fn podman_compose_does_not_pass_docker_only_pull_flag() {
    let host = MockHost {
        compose_available: true,
        ..MockHost::new()
    };
    let cli = DockerCli {
        engine: crate::docker::engine::ContainerEngine::Podman,
        ..DockerCli::new(&host)
    };
    cli.compose_up("/tmp/project").await.unwrap();
    let commands = host.commands.lock().unwrap();
    assert_eq!(
        commands.last().unwrap().args,
        ["compose", "up", "-d", "--remove-orphans"]
    );
    assert_eq!(commands.last().unwrap().program, "podman");
}

#[tokio::test]
async fn podman_standalone_provider_is_shared_by_probe_up_and_down_without_sudo() {
    let host = MockHost {
        standalone_compose: true,
        ..MockHost::new()
    };
    let cli = DockerCli {
        engine: crate::docker::engine::ContainerEngine::Podman,
        ..DockerCli::new(&host)
    };
    assert!(cli.probe().await.compose_available);
    cli.compose_up("/tmp/a project").await.unwrap();
    cli.compose_down("/tmp/a project", true).await.unwrap();
    let commands = host.commands.lock().unwrap();
    assert_eq!(
        commands
            .iter()
            .filter(|c| c.program == "podman-compose" && c.args == ["--version"])
            .count(),
        1
    );
    let up = commands
        .iter()
        .find(|c| c.args.first().is_some_and(|a| a == "up"))
        .unwrap();
    assert_eq!(up.program, "podman-compose");
    assert_eq!(up.args, ["up", "-d", "--remove-orphans"]);
    assert_eq!(
        up.working_dir.as_ref().unwrap().as_posix(),
        "/tmp/a project"
    );
    assert_eq!(commands.last().unwrap().program, "podman-compose");
    assert_eq!(commands.last().unwrap().args, ["down", "-v"]);
    assert!(commands.iter().all(|c| !c.elevated));
}

#[tokio::test]
async fn missing_podman_providers_preserve_both_failures_and_do_not_start_a_deployment() {
    let host = MockHost::new();
    let cli = DockerCli {
        engine: crate::docker::engine::ContainerEngine::Podman,
        ..DockerCli::new(&host)
    };
    assert!(!cli.probe().await.compose_available);
    let error = cli
        .compose_up("/tmp/project")
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("compose is not a docker command"));
    assert!(error.contains("podman-compose: command not found"));
    assert!(
        !host
            .commands
            .lock()
            .unwrap()
            .iter()
            .any(|c| c.args.iter().any(|a| a == "up"))
    );
}

#[tokio::test]
async fn windows_docker_compose_keeps_original_arguments_and_elevation() {
    let host = MockHost {
        os: Os::Windows,
        compose_available: true,
        ..MockHost::new()
    };
    let cli = DockerCli::new(&host);
    cli.elevated
        .store(true, std::sync::atomic::Ordering::Relaxed);
    cli.compose_up("C:/a project").await.unwrap();
    cli.compose_down("C:/a project", true).await.unwrap();
    let commands = host.commands.lock().unwrap();
    assert_eq!(commands.len(), 2);
    assert!(commands.iter().all(|c| c.program == "docker" && c.elevated));
    assert_eq!(
        commands[0].args,
        ["compose", "up", "-d", "--remove-orphans", "--pull", "never"]
    );
    assert_eq!(commands[1].args, ["compose", "down", "-v"]);
}

#[tokio::test]
async fn podman_runtime_failure_does_not_reinstall_packages_or_request_sudo() {
    use crate::docker::engine::{self, ContainerEngine};
    let host = MockHost {
        id: "podman-runtime-failure-test",
        formatted_podman_version_fails: true,
        compose_available: true,
        daemon_error: Some("rootless runtime namespace unavailable"),
        ..MockHost::new()
    };
    let mut preferences = engine::preferences();
    preferences.insert(host.id().into(), ContainerEngine::Podman);
    engine::replace_preferences(preferences);
    let report =
        crate::docker::install_docker_with_progress(&host, None, None, std::sync::Arc::new(|_| {}))
            .await;
    let mut preferences = engine::preferences();
    preferences.remove(host.id());
    engine::replace_preferences(preferences);
    let report = report.unwrap();
    assert!(matches!(
        report.status,
        ncd_domain::DockerInstallStatus::ManualRequired
    ));
    assert!(
        report
            .message
            .contains("rootless runtime namespace unavailable")
    );
    assert!(
        host.commands
            .lock()
            .unwrap()
            .iter()
            .all(|command| command.program == "podman" && !command.elevated)
    );
}

#[tokio::test]
async fn podman_docker_shim_without_banner_requires_explicit_engine_selection() {
    let host = MockHost {
        docker_compat: true,
        ..MockHost::new()
    };
    let cli = DockerCli::new(&host);
    let error = cli.probe_checked().await.unwrap_err();
    assert!(matches!(error, DockerCliError::PodmanCompatibilityShim));
    assert!(error.to_string().contains("选择 Podman"));
    assert!(cli.ensure_daemon_ready().await.is_err());
    assert!(cli.compose_up("/tmp/project").await.is_err());
    assert!(!cli.probe().await.installed);
    let commands = host.commands.lock().unwrap();
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].args, ["--version"]);
    assert!(!commands[0].elevated);
}

#[tokio::test]
async fn podman_docker_shim_does_not_install_docker_over_the_existing_engine() {
    let host = MockHost {
        docker_compat: true,
        ..MockHost::new()
    };
    let report =
        crate::docker::install_docker_with_progress(&host, None, None, std::sync::Arc::new(|_| {}))
            .await
            .unwrap();
    assert!(matches!(
        report.status,
        ncd_domain::DockerInstallStatus::ManualRequired
    ));
    assert!(report.message.contains("podman-docker"));
    let commands = host.commands.lock().unwrap();
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].program, "docker");
    assert_eq!(commands[0].args, ["--version"]);
    assert!(!commands[0].elevated);
}

#[tokio::test]
async fn windows_docker_probe_does_not_add_linux_identity_commands() {
    let host = MockHost {
        os: Os::Windows,
        docker_compat: true,
        compose_available: true,
        ..MockHost::new()
    };
    assert!(
        DockerCli::new(&host)
            .probe_checked()
            .await
            .unwrap()
            .installed
    );
    let commands = host.commands.lock().unwrap();
    assert_eq!(
        commands[0].args,
        ["version", "--format", "{{.Client.Version}}"]
    );
    assert!(
        commands
            .iter()
            .all(|c| c.program == "docker" && !c.elevated)
    );
    assert!(!commands.iter().any(|c| c.args == ["--version"]));
}

#[tokio::test]
async fn podman_installation_is_detected_without_initializing_its_broken_runtime() {
    let host = MockHost {
        formatted_podman_version_fails: true,
        compose_available: true,
        daemon_error: Some("rootless runtime namespace unavailable"),
        ..MockHost::new()
    };
    let cli = DockerCli {
        engine: crate::docker::engine::ContainerEngine::Podman,
        ..DockerCli::new(&host)
    };
    let status = cli.probe().await;
    assert!(status.installed);
    assert_eq!(status.version, "5.8.9");
    assert!(status.compose_available);
    assert!(!status.daemon_running);
    assert!(host.commands.lock().unwrap().iter().all(|c| !c.elevated));
}

#[tokio::test]
async fn existing_broken_podman_compose_keeps_diagnostic_and_never_installs_packages() {
    use crate::docker::engine::ContainerEngine;
    let host = MockHost {
        standalone_compose_error: Some("ModuleNotFoundError: No module named 'podman_compose'"),
        ..MockHost::new()
    };
    let cli = DockerCli::for_engine(&host, Some(ContainerEngine::Podman));
    let status = cli.probe_checked().await.unwrap();
    assert!(status.installed && status.daemon_running);
    assert!(!status.compose_available);
    assert!(
        status
            .compose_diagnostic
            .unwrap()
            .contains("ModuleNotFoundError")
    );
    let report = crate::docker::install_progress::install_container_engine_with_progress(
        &host,
        None,
        None,
        std::sync::Arc::new(|_| {}),
        ContainerEngine::Podman,
    )
    .await
    .unwrap();
    assert!(matches!(
        report.status,
        ncd_domain::DockerInstallStatus::ManualRequired
    ));
    assert!(report.message.contains("ModuleNotFoundError"));
    assert!(
        host.commands
            .lock()
            .unwrap()
            .iter()
            .all(|c| matches!(c.program.as_str(), "podman" | "podman-compose") && !c.elevated)
    );
}

#[tokio::test]
async fn bot_engine_overrides_host_selection_without_changing_other_instances() {
    use crate::docker::engine::ContainerEngine;
    let host = MockHost::new();
    let config = |engine| {
        serde_json::from_value::<ncd_domain::BotConfig>(serde_json::json!({
            "bot": { "name": "coexist", "QQID": 10001, "deploymentType": "docker" },
            "connect": {}, "advanced": {}, "docker": { "engine": engine }
        }))
        .unwrap()
    };
    let podman = DockerCli::for_bot(&host, &config("podman"));
    let docker = DockerCli::for_bot(&host, &config("docker"));
    assert_eq!(podman.engine(), ContainerEngine::Podman);
    assert_eq!(docker.engine(), ContainerEngine::Docker);
    podman
        .container_action(ContainerAction::Start, "same-name", false)
        .await
        .unwrap();
    docker
        .container_action(ContainerAction::Stop, "same-name", false)
        .await
        .unwrap();
    let commands = host.commands.lock().unwrap();
    assert_eq!(commands[0].program, "podman");
    assert_eq!(commands[1].program, "docker");
    assert!(commands.iter().all(|c| !c.elevated));
}
