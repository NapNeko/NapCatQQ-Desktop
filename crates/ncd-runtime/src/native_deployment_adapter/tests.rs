use super::*;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use bytes::Bytes;
use ncd_deploy::DockerDeployment;
use ncd_domain::ids::BotId;
use ncd_domain::{BackendType, BotConfig, BotFlavor, DeploymentType, RuntimeTarget};
use ncd_host::remote::{ConnectionConfig, HostKeyPolicy, RemoteWindowsHost, SshCredentials};
use ncd_host::{
    Arch, ArchiveKind, CommandOutput, DirEntry, Host, HostCommand, HostError, HostPath,
    HostProcess, HostShell, Locality, Os, PackageManager, ShellKind,
};
use ncd_test_support::BotConfigBuilder;
use ncd_traits::runtime_backend::{
    BotBackend, BotBackendError, BotRuntimeConfig, BotStartCtx, TailOpts,
};
use serde_json::json;
use tempfile::tempdir;

use crate::bot_actor::BotActorState;

struct NoopShell;

impl HostShell for NoopShell {
    fn kind(&self) -> ShellKind {
        ShellKind::Bash
    }

    fn escape(&self, arg: &str) -> String {
        arg.to_string()
    }

    fn line_separator(&self) -> &'static str {
        "\n"
    }
}

static NOOP_SHELL: NoopShell = NoopShell;

#[derive(Clone)]
struct RecordedWrite {
    path: String,
}

struct DockerRuntimeMockHost {
    ps_json: String,
    commands: Mutex<Vec<HostCommand>>,
    writes: Mutex<Vec<RecordedWrite>>,
    created_dirs: Mutex<Vec<String>>,
}

impl DockerRuntimeMockHost {
    fn new(ps_json: impl Into<String>) -> Self {
        Self {
            ps_json: ps_json.into(),
            commands: Mutex::new(Vec::new()),
            writes: Mutex::new(Vec::new()),
            created_dirs: Mutex::new(Vec::new()),
        }
    }

    fn with_legacy_snowluma_container() -> Self {
        Self::new(
            r#"{"ID":"abc123","Names":"ncbot-10001","Image":"motricseven7/snowluma:latest","State":"running","Status":"Up","Ports":"0.0.0.0:5099->5099/tcp"}"#,
        )
    }

    fn commands(&self) -> Vec<HostCommand> {
        self.commands.lock().unwrap().clone()
    }

    fn writes(&self) -> Vec<RecordedWrite> {
        self.writes.lock().unwrap().clone()
    }

    fn created_dirs(&self) -> Vec<String> {
        self.created_dirs.lock().unwrap().clone()
    }
}

#[async_trait]
impl Host for DockerRuntimeMockHost {
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
        "mock-linux"
    }

    fn shell(&self) -> &dyn HostShell {
        &NOOP_SHELL
    }

    fn pkg_manager(&self) -> Option<&dyn PackageManager> {
        None
    }

    async fn read_file(&self, path: &HostPath) -> Result<Bytes, HostError> {
        Err(HostError::PathNotFound { path: path.clone() })
    }

    async fn write_file(&self, path: &HostPath, bytes: &[u8]) -> Result<(), HostError> {
        let _ = bytes;
        self.writes.lock().unwrap().push(RecordedWrite {
            path: path.as_posix().to_string(),
        });
        Ok(())
    }

    async fn list_dir(&self, _: &HostPath) -> Result<Vec<DirEntry>, HostError> {
        Err(HostError::Unsupported { operation: "mock" })
    }

    async fn create_dir_all(&self, path: &HostPath) -> Result<(), HostError> {
        self.created_dirs
            .lock()
            .unwrap()
            .push(path.as_posix().to_string());
        Ok(())
    }

    async fn remove_file(&self, _: &HostPath) -> Result<(), HostError> {
        Ok(())
    }

    async fn remove_dir_all(&self, _: &HostPath) -> Result<(), HostError> {
        Ok(())
    }

    async fn exists(&self, _: &HostPath) -> Result<bool, HostError> {
        Ok(false)
    }

    async fn upload(&self, _: &Path, _: &HostPath) -> Result<(), HostError> {
        Err(HostError::Unsupported { operation: "mock" })
    }

    async fn download(&self, _: &HostPath, _: &Path) -> Result<(), HostError> {
        Err(HostError::Unsupported { operation: "mock" })
    }

    async fn extract_archive(
        &self,
        _: &HostPath,
        _: &HostPath,
        _: ArchiveKind,
    ) -> Result<(), HostError> {
        Err(HostError::Unsupported { operation: "mock" })
    }

    async fn spawn(&self, _: HostCommand) -> Result<Box<dyn HostProcess>, HostError> {
        Err(HostError::Unsupported { operation: "mock" })
    }

    async fn run_to_string(&self, cmd: HostCommand) -> Result<CommandOutput, HostError> {
        self.commands.lock().unwrap().push(cmd.clone());
        if cmd.program == "sh" && cmd.args == ["-c", "echo $HOME"] {
            return Ok(command_output(0, "/home/napcat\n", ""));
        }
        if cmd.program != "docker" {
            return Ok(command_output(0, "", ""));
        }
        match cmd.args.first().map(String::as_str) {
            Some("ps") => Ok(command_output(0, self.ps_json.clone(), "")),
            Some("logs") => Ok(command_output(0, "legacy-line-a\nlegacy-line-b\n", "")),
            _ => Ok(command_output(0, "", "")),
        }
    }
}

fn command_output(
    exit_code: i32,
    stdout: impl Into<String>,
    stderr: impl Into<String>,
) -> CommandOutput {
    CommandOutput {
        exit_code: Some(exit_code),
        stdout: stdout.into(),
        stderr: stderr.into(),
    }
}

fn docker_config(backend: BackendType) -> BotConfig {
    BotConfigBuilder::new()
        .qq_id(10001)
        .runtime_target(RuntimeTarget::server("remote-a"))
        .backend_type(backend)
        .deployment_type(DeploymentType::Docker)
        .build()
}

#[test]
fn runtime_root_is_derived_from_real_runtime_bot_config_path() {
    let bot_id = BotId::new("10001");
    let path = BotRuntimeConfig::default_path("/data", bot_id.clone()).config_path;
    assert_eq!(
        data_root_from_config_path(&path, &bot_id),
        Some(PathBuf::from("/data"))
    );
}

#[test]
fn data_root_rejects_unexpected_config_path_shape() {
    let bot_id = BotId::new("10001");
    let wrong_file = PathBuf::from("/data/config/bots/10002.json");
    let wrong_dir = PathBuf::from("/data/other/bots/10001.json");

    assert_eq!(data_root_from_config_path(&wrong_file, &bot_id), None);
    assert_eq!(data_root_from_config_path(&wrong_dir, &bot_id), None);
}

#[test]
fn docker_requires_real_config_when_bot_json_is_missing() {
    let root = tempdir().unwrap();
    let bot_id = BotId::new("10001");
    let ctx = BotStartCtx {
        config: BotRuntimeConfig::default_path(root.path(), bot_id.clone()),
        bot_config: None,
    };

    let err = real_bot_config_from_ctx(&ctx, BotFlavor::NapCat, true).unwrap_err();

    assert!(matches!(err, BotBackendError::ConfigNotFound(id) if id == bot_id));
}

#[test]
fn docker_loads_real_config_from_default_runtime_path() {
    let root = tempdir().unwrap();
    let bot_id = BotId::new("10001");
    let config_dir = root.path().join("config");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::write(
        config_dir.join("bot.json"),
        serde_json::to_vec(&json!({
            "bots": [{
                "bot": {
                    "name": "real-bot",
                    "QQID": 10001,
                    "musicSignUrl": "https://sign.example.com",
                    "autoRestartSchedule": {"enabled": false, "time": "04:00", "unit": "daily"},
                    "offlineAutoRestart": false,
                    "runtime_target": "remote_linux",
                    "backendType": "NapCat",
                    "deploymentType": "docker"
                },
                "connect": {},
                "advanced": {}
            }]
        }))
        .unwrap(),
    )
    .unwrap();
    let ctx = BotStartCtx {
        config: BotRuntimeConfig::default_path(root.path(), bot_id),
        bot_config: None,
    };

    let config = real_bot_config_from_ctx(&ctx, BotFlavor::NapCat, true).unwrap();

    assert_eq!(config.bot.name, "real-bot");
    assert_eq!(config.bot.music_sign_url, "https://sign.example.com");
}

#[tokio::test]
async fn docker_project_dir_home_probe_failure_is_hard_error() {
    let host = RemoteWindowsHost::new_stub(
        "stub",
        ConnectionConfig::new(
            "example.com",
            22,
            SshCredentials::password("u", "p"),
            HostKeyPolicy::Insecure,
        ),
    );

    let err = docker_project_dir(&host, "ncbot-10001").await.unwrap_err();

    assert!(matches!(err, BotBackendError::Io(message) if message.contains("HOME")));
}

#[tokio::test]
async fn docker_snowluma_config_uses_slbot_project_dir() {
    let host = DockerRuntimeMockHost::new("");
    let bot_id = BotId::new("10001");
    let config = docker_config(BackendType::SnowLuma);

    render_docker_config_on_host(&host, &bot_id, &config)
        .await
        .unwrap();

    let expected_dir = "/home/napcat/.napcat-bots/slbot-10001/snowluma-data/config";
    assert!(host.created_dirs().iter().any(|path| path == expected_dir));
    let writes = host.writes();
    assert!(
        writes
            .iter()
            .any(|write| write.path == format!("{expected_dir}/onebot_10001.json"))
    );
    assert!(
        host.created_dirs()
            .iter()
            .chain(writes.iter().map(|write| &write.path))
            .all(|path| !path.contains("ncbot-10001"))
    );
}

#[tokio::test]
async fn docker_tail_log_uses_resolved_running_container_name() {
    let host = Arc::new(DockerRuntimeMockHost::with_legacy_snowluma_container());
    let backend = DockerDeploymentBackend::new(
        Arc::new(DockerDeployment::new()),
        host.clone(),
        BotId::new("docker"),
        BotFlavor::SnowLuma,
    );

    let snap = backend
        .tail_log(BotId::new("10001"), TailOpts { lines: 20 })
        .await
        .unwrap();

    assert_eq!(snap.lines, vec!["legacy-line-a", "legacy-line-b"]);
    let logs_cmd = host
        .commands()
        .into_iter()
        .find(|cmd| cmd.program == "docker" && cmd.args.first().map(String::as_str) == Some("logs"))
        .unwrap();
    assert_eq!(
        logs_cmd.args.last().map(String::as_str),
        Some("ncbot-10001")
    );
}

#[tokio::test]
async fn docker_tail_log_falls_back_to_flavor_name_when_container_is_absent() {
    let host = Arc::new(DockerRuntimeMockHost::new(""));
    let backend = DockerDeploymentBackend::new(
        Arc::new(DockerDeployment::new()),
        host.clone(),
        BotId::new("docker"),
        BotFlavor::NapCat,
    );

    backend
        .tail_log(BotId::new("10001"), TailOpts { lines: 20 })
        .await
        .unwrap();

    let logs_cmd = host
        .commands()
        .into_iter()
        .find(|cmd| cmd.program == "docker" && cmd.args.first().map(String::as_str) == Some("logs"))
        .unwrap();
    assert_eq!(
        logs_cmd.args.last().map(String::as_str),
        Some("ncbot-10001")
    );
}

#[test]
fn docker_starting_status_is_not_stopped() {
    let status = status_for_deployment_state(BotId::new("10001"), DeploymentState::Starting);

    assert_eq!(status.state, BotActorState::Starting);
    assert_eq!(status.extra["deployment_state"], "starting");
}

#[test]
fn docker_failed_status_keeps_reason() {
    let status = status_for_deployment_state(
        BotId::new("10001"),
        DeploymentState::Failed {
            reason: "docker ps failed".to_string(),
        },
    );

    assert_eq!(status.state, BotActorState::Crashed);
    assert_eq!(status.extra["deployment_state"], "failed");
    assert_eq!(status.extra["reason"], "docker ps failed");
}
