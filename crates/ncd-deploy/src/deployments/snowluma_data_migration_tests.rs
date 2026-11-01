use super::migrate;
use async_trait::async_trait;
use bytes::Bytes;
use ncd_domain::{ContainerInfo, ContainerState};
use ncd_host::{
    Arch, ArchiveKind, CommandOutput, DirEntry, Host, HostCommand, HostError, HostPath,
    HostProcess, HostShell, Locality, Os,
};
use std::{collections::HashMap, path::Path, sync::Mutex};

use crate::{DockerCli, docker::engine::ContainerEngine};

const PROJECT: &str = "/home/test/bots/slbot-10001";
const ID: &str = "0123456789abcdef";

fn backup() -> String {
    format!("{PROJECT}/snowluma-legacy/{ID}")
}

fn container() -> ContainerInfo {
    ContainerInfo {
        id: ID.into(),
        name: "slbot-10001".into(),
        image: "snowluma/snowluma:latest".into(),
        state: ContainerState::Exited,
        status: "Exited (0)".into(),
        ports: Vec::new(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Event {
    Snapshot,
    PublishBackup,
    StartHelper,
    CopyToHelper,
    Merge,
    RemoveHelper,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Fault {
    Snapshot,
    PublishBackup,
    Merge,
}

#[derive(Default)]
struct State {
    commands: Vec<HostCommand>,
    events: Vec<Event>,
    snapshots: HashMap<String, u8>,
    source_generation: u8,
    fail_next: Option<Fault>,
}

struct MigrationHost {
    environment: String,
    needs_sudo: bool,
    standalone_podman_compose: bool,
    state: Mutex<State>,
}

impl MigrationHost {
    fn new() -> Self {
        Self {
            environment: "[]".into(),
            needs_sudo: false,
            standalone_podman_compose: false,
            state: Mutex::new(State::default()),
        }
    }

    fn fail_once(&self, fault: Fault) {
        self.state.lock().unwrap().fail_next = Some(fault);
    }
}

fn output(code: i32, stdout: &str, stderr: &str) -> CommandOutput {
    CommandOutput {
        exit_code: Some(code),
        stdout: stdout.into(),
        stderr: stderr.into(),
    }
}

fn fail(state: &mut State, fault: Fault) -> bool {
    if state.fail_next == Some(fault) {
        state.fail_next = None;
        true
    } else {
        false
    }
}

#[async_trait]
impl Host for MigrationHost {
    fn os(&self) -> Os {
        Os::Linux
    }
    fn arch(&self) -> Arch {
        Arch::X86_64
    }
    fn locality(&self) -> Locality {
        Locality::Remote
    }
    fn id(&self) -> &str {
        "migration-test"
    }
    fn shell(&self) -> &dyn HostShell {
        &ncd_host::shell::BashShell
    }
    async fn read_file(&self, _: &HostPath) -> Result<Bytes, HostError> {
        unreachable!("migration must not read live configuration through the host")
    }
    async fn write_file(&self, _: &HostPath, _: &[u8]) -> Result<(), HostError> {
        unreachable!()
    }
    async fn list_dir(&self, _: &HostPath) -> Result<Vec<DirEntry>, HostError> {
        unreachable!()
    }
    async fn create_dir_all(&self, _: &HostPath) -> Result<(), HostError> {
        Ok(())
    }
    async fn remove_file(&self, _: &HostPath) -> Result<(), HostError> {
        unreachable!("complete backups must be retained")
    }
    async fn remove_dir_all(&self, _: &HostPath) -> Result<(), HostError> {
        unreachable!("complete backups must be retained")
    }
    async fn rename(&self, from: &HostPath, to: &HostPath) -> Result<(), HostError> {
        let mut state = self.state.lock().unwrap();
        state.events.push(Event::PublishBackup);
        if fail(&mut state, Fault::PublishBackup) {
            return Err(HostError::Unsupported {
                operation: "simulated rename failure",
            });
        }
        assert!(!state.snapshots.contains_key(to.as_posix()));
        let generation = state.snapshots.remove(from.as_posix()).unwrap();
        state.snapshots.insert(to.as_posix().into(), generation);
        Ok(())
    }
    async fn exists(&self, path: &HostPath) -> Result<bool, HostError> {
        Ok(self
            .state
            .lock()
            .unwrap()
            .snapshots
            .contains_key(path.as_posix()))
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
        unreachable!("migration must not start the application")
    }
    async fn run_to_string(&self, command: HostCommand) -> Result<CommandOutput, HostError> {
        let mut state = self.state.lock().unwrap();
        state.commands.push(command.clone());
        let args = &command.args;
        let first = args.first().map(String::as_str);
        if command.program == "chmod" {
            return Ok(output(0, "", ""));
        }
        if command.program == "rm" {
            let partial = args.last().unwrap();
            assert!(
                partial.ends_with(".partial"),
                "never remove complete backups"
            );
            state.snapshots.remove(partial);
            return Ok(output(0, "", ""));
        }
        if first == Some("--version") {
            return Ok(output(
                0,
                if command.program == "docker" {
                    "Docker version 27.3.1"
                } else {
                    "podman version 5.8.9"
                },
                "",
            ));
        }
        if first == Some("version") {
            return Ok(output(0, "27.3.1", ""));
        }
        if first == Some("info") {
            return Ok(if self.needs_sudo && !command.elevated {
                output(1, "", "permission denied: /var/run/docker.sock")
            } else {
                output(0, "27.3.1", "")
            });
        }
        if args.iter().any(|arg| arg == "version") {
            return Ok(
                if command.program == "podman" && self.standalone_podman_compose {
                    output(127, "", "compose provider unavailable")
                } else {
                    output(0, "compose version", "")
                },
            );
        }
        if first == Some("inspect") {
            return Ok(output(0, &self.environment, ""));
        }
        if first == Some("cp") && args[1].starts_with(&format!("{ID}:")) {
            state.events.push(Event::Snapshot);
            // Represents the entire directory, including files not managed by Desktop.
            let generation = state.source_generation;
            state.snapshots.insert(args[2].clone(), generation);
            return Ok(if fail(&mut state, Fault::Snapshot) {
                output(1, "", "simulated disk full")
            } else {
                output(0, "", "")
            });
        }
        if args.iter().any(|arg| arg == "run") {
            state.events.push(Event::StartHelper);
            assert!(state.snapshots.contains_key(&backup()));
            return Ok(output(0, "helper-id", ""));
        }
        if first == Some("cp") {
            state.events.push(Event::CopyToHelper);
            assert!(state.snapshots.contains_key(&args[1]));
            return Ok(output(0, "", ""));
        }
        if first == Some("exec") {
            state.events.push(Event::Merge);
            return Ok(if fail(&mut state, Fault::Merge) {
                output(1, "", "simulated merge failure")
            } else {
                output(0, "", "")
            });
        }
        if first == Some("rm") {
            assert_ne!(args.last().unwrap(), ID, "never delete the old container");
            state.events.push(Event::RemoveHelper);
            return Ok(output(0, "", ""));
        }
        panic!("unexpected migration command: {command:?}")
    }
}

#[tokio::test]
async fn persistent_data_directory_does_not_copy_or_start_helper() {
    let host = MigrationHost {
        environment: r#"["SNOWLUMA_DATA=/app/snowluma-data"]"#.into(),
        ..MigrationHost::new()
    };
    migrate(&host, &DockerCli::new(&host), PROJECT, &container())
        .await
        .unwrap();
    let state = host.state.lock().unwrap();
    assert!(state.events.is_empty());
    assert!(state.snapshots.is_empty());
    assert!(
        state
            .commands
            .iter()
            .all(|command| command.args[0] == "inspect")
    );
}

#[tokio::test]
async fn untagged_snowluma_is_migrated_and_unknown_containers_are_preserved() {
    let mut host = MigrationHost::new();
    host.environment = r#"["SNOWLUMA_DATA=/app/data"]"#.into();
    let mut old = container();
    old.image = "sha256:abcdef0123456789".into();
    migrate(&host, &DockerCli::new(&host), PROJECT, &old)
        .await
        .unwrap();
    assert!(host.state.lock().unwrap().snapshots.contains_key(&backup()));

    let host = MigrationHost::new();
    assert!(
        migrate(&host, &DockerCli::new(&host), PROJECT, &old)
            .await
            .is_err()
    );
    assert!(host.state.lock().unwrap().events.is_empty());
    old.image = "mlikiowa/napcat-docker:latest".into();
    migrate(&host, &DockerCli::new(&host), PROJECT, &old)
        .await
        .unwrap();
    assert!(host.state.lock().unwrap().events.is_empty());
}

#[tokio::test]
async fn entire_legacy_directory_is_published_before_starting_helper() {
    let host = MigrationHost::new();
    migrate(&host, &DockerCli::new(&host), PROJECT, &container())
        .await
        .unwrap();
    let state = host.state.lock().unwrap();
    assert_eq!(
        state.events,
        [
            Event::Snapshot,
            Event::PublishBackup,
            Event::StartHelper,
            Event::CopyToHelper,
            Event::Merge,
            Event::RemoveHelper
        ]
    );
    let snapshot = state
        .commands
        .iter()
        .find(|command| command.args.first().map(String::as_str) == Some("cp"))
        .unwrap();
    assert_eq!(snapshot.args[1], format!("{ID}:/app/data/."));
    assert!(state.snapshots.contains_key(&backup()));
}

#[tokio::test]
async fn failed_snapshot_never_starts_helper_and_can_be_retried() {
    let host = MigrationHost::new();
    host.fail_once(Fault::Snapshot);
    let cli = DockerCli::new(&host);
    assert!(migrate(&host, &cli, PROJECT, &container()).await.is_err());
    {
        let state = host.state.lock().unwrap();
        assert_eq!(state.events, [Event::Snapshot]);
        assert!(!state.snapshots.contains_key(&backup()));
    }
    migrate(&host, &cli, PROJECT, &container()).await.unwrap();
    let state = host.state.lock().unwrap();
    assert!(state.snapshots.contains_key(&backup()));
    assert_eq!(
        state
            .events
            .iter()
            .filter(|&&event| event == Event::StartHelper)
            .count(),
        1
    );
}

#[tokio::test]
async fn failed_backup_publish_never_starts_helper() {
    let host = MigrationHost::new();
    host.fail_once(Fault::PublishBackup);
    assert!(
        migrate(&host, &DockerCli::new(&host), PROJECT, &container())
            .await
            .is_err()
    );
    let state = host.state.lock().unwrap();
    assert_eq!(state.events, [Event::Snapshot, Event::PublishBackup]);
    assert!(!state.snapshots.contains_key(&backup()));
}

#[tokio::test]
async fn failed_merge_cleans_helper_and_reuses_immutable_snapshot_on_retry() {
    let host = MigrationHost::new();
    host.fail_once(Fault::Merge);
    let cli = DockerCli::new(&host);
    assert!(migrate(&host, &cli, PROJECT, &container()).await.is_err());
    {
        let mut state = host.state.lock().unwrap();
        assert_eq!(state.events.last(), Some(&Event::RemoveHelper));
        assert_eq!(state.snapshots.get(&backup()), Some(&0));
        state.source_generation = 1;
    }
    migrate(&host, &cli, PROJECT, &container()).await.unwrap();
    let state = host.state.lock().unwrap();
    assert_eq!(
        state.snapshots.get(&backup()),
        Some(&0),
        "retry must not replace the original backup"
    );
    assert_eq!(
        state
            .events
            .iter()
            .filter(|&&event| event == Event::Snapshot)
            .count(),
        1
    );
    assert_eq!(
        state
            .events
            .iter()
            .filter(|&&event| event == Event::Merge)
            .count(),
        2
    );
    assert_eq!(
        state
            .events
            .iter()
            .filter(|&&event| event == Event::RemoveHelper)
            .count(),
        2
    );
}

#[tokio::test]
async fn migration_keeps_selected_engine_and_privilege_without_starting_application() {
    for (engine, needs_sudo, standalone) in [
        (ContainerEngine::Docker, false, false),
        (ContainerEngine::Docker, true, false),
        (ContainerEngine::Podman, false, false),
        (ContainerEngine::Podman, false, true),
    ] {
        let host = MigrationHost {
            needs_sudo,
            standalone_podman_compose: standalone,
            ..MigrationHost::new()
        };
        let cli = DockerCli::for_engine(&host, Some(engine));
        cli.ensure_daemon_ready().await.unwrap();
        host.state.lock().unwrap().commands.clear();
        migrate(&host, &cli, PROJECT, &container()).await.unwrap();
        let state = host.state.lock().unwrap();
        for command in &state.commands {
            let operation = command.args.first().map(String::as_str);
            if matches!(operation, Some("inspect" | "cp" | "exec" | "rm"))
                && command.program != "rm"
            {
                assert_eq!(command.program, engine.program());
                assert_eq!(command.elevated, needs_sudo);
            }
            assert!(
                !command
                    .args
                    .iter()
                    .any(|arg| matches!(arg.as_str(), "up" | "start" | "restart"))
            );
        }
        let helper = state
            .commands
            .iter()
            .find(|command| command.args.iter().any(|arg| arg == "run"))
            .unwrap();
        assert_eq!(
            helper.program,
            if standalone {
                "podman-compose"
            } else {
                engine.program()
            }
        );
        assert_eq!(helper.elevated, needs_sudo);
        assert_eq!(helper.working_dir.as_ref().unwrap().as_posix(), PROJECT);
        assert!(helper.args.iter().any(|arg| arg == "--no-deps"));
        assert!(!helper.args.iter().any(|arg| arg == "--service-ports"));
        let entrypoint = helper
            .args
            .iter()
            .position(|arg| arg == "--entrypoint")
            .unwrap();
        assert_eq!(helper.args[entrypoint + 1], "node");
        let merge = state
            .commands
            .iter()
            .find(|command| command.args.first().map(String::as_str) == Some("exec"))
            .unwrap();
        assert!(merge.args.windows(2).any(|pair| pair == ["node", "-e"]));
    }
}
