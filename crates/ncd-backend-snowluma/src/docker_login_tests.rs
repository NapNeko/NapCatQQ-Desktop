use super::*;
use async_trait::async_trait;
use bytes::Bytes;
use ncd_host::{ArchiveKind, CommandOutput, DirEntry, HostError, HostProcess, HostShell, Locality};
use std::{path::Path, sync::Mutex};

struct CaptureHost {
    commands: Mutex<Vec<HostCommand>>,
    session: Mutex<Vec<u8>>,
    replace_container: bool,
}

impl CaptureHost {
    fn new(replace_container: bool) -> Self {
        let session = DockerLoginSession {
            engine: None,
            method: "atspi".into(),
            directory: format!(
                "{}/ncd-login",
                ncd_domain::remote_paths::docker_bot_project_dir("/home/u", "slbot-123")
            ),
            session_id: "0123456789abcdef0123456789abcdef".into(),
            launch_id: "0123456789abcdef0123456789abcdef".into(),
            expected_uin: "123".into(),
            container: "slbot-123".into(),
            container_id: None,
            qq_exe: "/opt/QQ/qq".into(),
            uid: 1000,
            gid: 1000,
            binding: None,
            lease_started_at: None,
            scanned: false,
            login_detected: false,
        };
        Self {
            commands: Mutex::new(Vec::new()),
            session: Mutex::new(serde_json::to_vec(&session).unwrap()),
            replace_container,
        }
    }
}

#[async_trait]
impl Host for CaptureHost {
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
        "docker-qr-test"
    }
    fn shell(&self) -> &dyn HostShell {
        &ncd_host::shell::BashShell
    }
    async fn read_file(&self, _: &HostPath) -> Result<Bytes, HostError> {
        Ok(self.session.lock().unwrap().clone().into())
    }
    async fn write_file(&self, _: &HostPath, bytes: &[u8]) -> Result<(), HostError> {
        *self.session.lock().unwrap() = bytes.to_vec();
        Ok(())
    }
    async fn exists(&self, _: &HostPath) -> Result<bool, HostError> {
        Ok(true)
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
        let mut commands = self.commands.lock().unwrap();
        let prior_lists = commands
            .iter()
            .filter(|c| c.args.first().is_some_and(|a| a == "ps"))
            .count();
        commands.push(command.clone());
        let mut code = 0;
        let mut stderr = String::new();
        let stdout = if command.program == "sh" {
            "/home/u".to_owned()
        } else if !matches!(command.program.as_str(), "docker" | "podman") {
            assert_eq!(command.program, "rm");
            String::new()
        } else {
            match command.args.first().map(String::as_str) {
                Some("--version") if command.program == "podman" => "podman version 5.8.9".into(),
                Some("--version") => "Docker version 27.3.1, build test".into(),
                Some("version") => "27.3.1".into(),
                Some("info") if command.program == "docker" && !command.elevated => {
                    code = 1;
                    stderr = "permission denied connecting to /var/run/docker.sock".into();
                    String::new()
                }
                Some("info") => "27.3.1".into(),
                Some("compose") => "2.30.0".into(),
                Some("ps") => serde_json::json!({
                    "ID": if self.replace_container && prior_lists > 0 { "replacement" } else { "original" },
                    "Names": "slbot-123", "Image": "snowluma", "State": "running", "Status": "Up"
                }).to_string(),
                Some("exec") if command.args.iter().any(|a| a == PROCESS_PROBE) => {
                    r#"{"pid":24,"start_time":"100"}"#.into()
                }
                Some("exec") if command.args.iter().any(|a| a == "--bind") => {
                    serde_json::json!({"binding": {
                        "pid":24, "start_time":"100", "exe":"/opt/QQ/qq", "uid":1000,
                        "display":":1", "bus":"unix:path=/tmp/bus"
                    }}).to_string()
                }
                Some("exec") => {
                    let request: serde_json::Value = serde_json::from_slice(command.stdin.as_ref().unwrap()).unwrap();
                    serde_json::json!({
                        "session_id":request["session_id"], "v":1, "state":"waiting_for_qr",
                        "qq_pid":24, "qrcode_payload":"data:image/png;base64,qr", "uin":null,
                        "observer_removed":true, "message":null
                    }).to_string()
                }
                _ => panic!("unexpected command: {:?}", command.args),
            }
        };
        Ok(CommandOutput {
            exit_code: Some(code),
            stdout,
            stderr,
        })
    }
}

#[tokio::test]
async fn capture_reuses_checked_docker_permissions_and_still_revalidates_identity() {
    let host = CaptureHost::new(false);
    let snapshot =
        DockerLoginSession::snapshot(&host, &BotId::new("123"), false, ContainerEngine::Docker)
            .await
            .unwrap();
    assert_eq!(snapshot.state, SnowLumaQrLoginState::WaitingForQr);
    let commands = host.commands.lock().unwrap();
    assert_eq!(
        commands
            .iter()
            .filter(|c| c.args.first().is_some_and(|a| a == "version"))
            .count(),
        1
    );
    assert_eq!(
        commands
            .iter()
            .filter(|c| c.args.first().is_some_and(|a| a == "info"))
            .count(),
        2
    );
    let lists: Vec<_> = commands
        .iter()
        .filter(|c| c.args.first().is_some_and(|a| a == "ps"))
        .collect();
    assert_eq!(lists.len(), 2);
    assert!(lists.iter().all(|c| c.elevated));
    let captures: Vec<_> = commands
        .iter()
        .filter(|c| c.args.first().is_some_and(|a| a == "exec"))
        .collect();
    assert_eq!(captures.len(), 4);
    assert!(captures.iter().all(|c| c.elevated));
}

#[tokio::test]
async fn container_replacement_during_capture_rejects_late_qr() {
    let host = CaptureHost::new(true);
    assert!(
        DockerLoginSession::snapshot(&host, &BotId::new("123"), false, ContainerEngine::Docker)
            .await
            .is_err()
    );
    let saved: DockerLoginSession = serde_json::from_slice(&host.session.lock().unwrap()).unwrap();
    assert_eq!(saved.container_id.as_deref(), Some("original"));
    assert!(!saved.scanned);
}

#[tokio::test]
async fn legacy_capture_session_without_engine_uses_the_pinned_podman_instance() {
    let host = CaptureHost::new(false);
    let mut legacy: serde_json::Value =
        serde_json::from_slice(&host.session.lock().unwrap()).unwrap();
    legacy.as_object_mut().unwrap().remove("engine");
    *host.session.lock().unwrap() = serde_json::to_vec(&legacy).unwrap();
    let snapshot =
        DockerLoginSession::snapshot(&host, &BotId::new("123"), false, ContainerEngine::Podman)
            .await
            .unwrap();
    assert_eq!(snapshot.state, SnowLumaQrLoginState::WaitingForQr);
    let saved: DockerLoginSession = serde_json::from_slice(&host.session.lock().unwrap()).unwrap();
    assert_eq!(saved.engine, Some(ContainerEngine::Podman));
    let commands = host.commands.lock().unwrap();
    assert!(!commands.iter().any(|c| c.program == "docker"));
    assert!(
        commands
            .iter()
            .filter(|c| c.program == "podman")
            .all(|c| !c.elevated)
    );
    assert!(
        commands
            .iter()
            .any(|c| c.program == "podman" && c.args.first().is_some_and(|a| a == "exec"))
    );
}

#[tokio::test]
async fn legacy_capture_pid_probe_uses_the_pinned_podman_instance() {
    let host = CaptureHost::new(false);
    assert_eq!(
        DockerLoginSession::bound_pid(&host, &BotId::new("123"), ContainerEngine::Podman)
            .await
            .unwrap(),
        Some(24)
    );
    let commands = host.commands.lock().unwrap();
    assert!(!commands.iter().any(|c| c.program == "docker"));
    assert!(
        commands
            .iter()
            .filter(|c| c.program == "podman")
            .all(|c| !c.elevated)
    );
}

#[tokio::test]
async fn capture_rejects_a_conflicting_persisted_engine_before_container_commands() {
    let host = CaptureHost::new(false);
    let mut saved: DockerLoginSession =
        serde_json::from_slice(&host.session.lock().unwrap()).unwrap();
    saved.engine = Some(ContainerEngine::Docker);
    *host.session.lock().unwrap() = serde_json::to_vec(&saved).unwrap();
    let error =
        DockerLoginSession::snapshot(&host, &BotId::new("123"), false, ContainerEngine::Podman)
            .await
            .unwrap_err();
    assert!(error.to_string().contains("引擎与 Bot 设置不符"));
    assert!(
        host.commands
            .lock()
            .unwrap()
            .iter()
            .all(|c| !matches!(c.program.as_str(), "docker" | "podman"))
    );
}
