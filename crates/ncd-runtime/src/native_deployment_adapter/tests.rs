use super::*;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use bytes::Bytes;
use ncd_deploy::DockerDeployment;
use ncd_domain::ids::BotId;
use ncd_domain::{BackendType, BotConfig, BotFlavor, DeploymentType, RuntimeTarget};
use ncd_host::{
    Arch, ArchiveKind, CommandOutput, DirEntry, Host, HostCommand, HostError, HostPath,
    HostProcess, HostShell, Locality, Os, ShellKind,
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
    text: String,
}

struct DockerRuntimeMockHost {
    ps_json: String,
    commands: Mutex<Vec<HostCommand>>,
    writes: Mutex<Vec<RecordedWrite>>,
    created_dirs: Mutex<Vec<String>>,
    fail_login_overlay: bool,
    fail_home_probe: bool,
    fail_copy: bool,
    launched: std::sync::atomic::AtomicBool,
}

impl DockerRuntimeMockHost {
    fn new(ps_json: impl Into<String>) -> Self {
        Self {
            ps_json: ps_json.into(),
            commands: Mutex::new(Vec::new()),
            writes: Mutex::new(Vec::new()),
            created_dirs: Mutex::new(Vec::new()),
            fail_login_overlay: false,
            fail_home_probe: false,
            fail_copy: false,
            launched: std::sync::atomic::AtomicBool::new(false),
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

    async fn read_file(&self, path: &HostPath) -> Result<Bytes, HostError> {
        Err(HostError::PathNotFound { path: path.clone() })
    }

    async fn write_file(&self, path: &HostPath, bytes: &[u8]) -> Result<(), HostError> {
        let text = String::from_utf8_lossy(bytes).into_owned();
        let fail_overlay = self.fail_login_overlay
            && path.as_posix().ends_with("/docker-compose.yml")
            && text.contains("/ncd-login-assist");
        self.writes.lock().unwrap().push(RecordedWrite {
            path: path.as_posix().to_string(),
            text,
        });
        if fail_overlay {
            return Err(HostError::Unsupported {
                operation: "mock overlay write failure",
            });
        }
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

    async fn exists(&self, path: &HostPath) -> Result<bool, HostError> {
        Ok(self
            .created_dirs
            .lock()
            .unwrap()
            .iter()
            .any(|dir| dir == path.as_posix()))
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
            if self.fail_home_probe {
                return Err(HostError::Io(std::io::Error::other("HOME unavailable")));
            }
            return Ok(command_output(0, "/home/napcat\n", ""));
        }
        if cmd.program == "podman" && cmd.args == ["--version"] {
            return Ok(command_output(0, "podman version 5.8.9\n", ""));
        }
        if !matches!(cmd.program.as_str(), "docker" | "podman") {
            return Ok(command_output(0, "", ""));
        }
        match cmd.args.first().map(String::as_str) {
            Some("cp") if self.fail_copy => Ok(command_output(1, "", "copy failed")),
            Some("info") if cmd.args.iter().any(|arg| arg.contains("OSType")) => {
                Ok(command_output(0, "linux/x86_64", ""))
            }
            Some("image") => Ok(command_output(0, "linux/amd64", "")),
            Some("compose") if cmd.args.get(1).map(String::as_str) == Some("up") => {
                self.launched.store(true, std::sync::atomic::Ordering::SeqCst);
                Ok(command_output(0, "", ""))
            }
            Some("ps") if self.ps_json.is_empty() && self.launched.load(std::sync::atomic::Ordering::SeqCst) => {
                Ok(command_output(0, r#"{"ID":"started","Names":"slbot-10001","Image":"motricseven7/snowluma:latest","State":"running","Status":"Up","Ports":""}"#, ""))
            }
            Some("version" | "info") => Ok(command_output(0, "27.3.1\n", "")),
            Some("ps") => Ok(command_output(0, self.ps_json.clone(), "")),
            Some("logs") => Ok(command_output(0, "legacy-line-a\nlegacy-line-b\n", "")),
            Some("run") if self.fail_login_overlay && cmd.args.iter().any(|arg| arg == "--probe") => {
                Ok(command_output(0, r#"{"available":true}"#, ""))
            }
            Some("run") if self.fail_login_overlay => Ok(command_output(0, json!({
                "arch": "x64",
                "qq_exe": "/opt/QQ/qq",
                "supervisor": "[program:qq]\ncommand=qq --no-sandbox\nuser=snowluma\n[program:snowluma]\ncommand=node /app/runtime/index.mjs\n",
                "uid_remap": true, "uid": 1001, "gid": 1001,
            }).to_string(), "")),
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
fn docker_listener_hosts_are_normalized_only_in_derived_configuration() {
    for backend in [BackendType::NapCat, BackendType::SnowLuma] {
        let config = docker_config(backend);
        let mut raw = serde_json::to_value(config).unwrap();
        raw["connect"] = json!({
            "httpServers": [{"name":"http", "host":"127.0.0.1", "port":4000}],
            "httpSseServers": [{"name":"sse", "host":"[::1]", "port":4001}],
            "websocketServers": [{"name":"ws", "host":"", "port":4002},
                {"name":"off", "host":"localhost", "port":4003, "enable":false}],
            "websocketClients": [{"name":"client", "url":"ws://127.0.0.1:4004"}]
        });
        let config: BotConfig = serde_json::from_value(raw).unwrap();
        let derived = docker_helpers::config_with_docker_listener_hosts(&config);
        assert_eq!(derived.connect.http_servers[0].host, "0.0.0.0");
        assert_eq!(derived.connect.http_sse_servers[0].host, "0.0.0.0");
        assert_eq!(derived.connect.websocket_servers[0].host, "0.0.0.0");
        assert_eq!(derived.connect.websocket_servers[1].host, "localhost");
        assert_eq!(
            derived.connect.websocket_clients,
            config.connect.websocket_clients
        );
        assert_eq!(config.connect.http_servers[0].host, "127.0.0.1");
        let mut native = config;
        native.bot.deployment_type = DeploymentType::Native;
        assert_eq!(
            docker_helpers::config_with_docker_listener_hosts(&native),
            native
        );
    }
}

#[tokio::test]
async fn snowluma_start_copies_named_volume_configuration_before_final_restart() {
    let host = Arc::new(DockerRuntimeMockHost::new(""));
    let backend = DockerDeploymentBackend::new(
        Arc::new(DockerDeployment::with_sl_secrets("test-vnc", "test-webui")),
        host.clone(),
        "docker",
        BotFlavor::SnowLuma,
    );
    let config = docker_config(BackendType::SnowLuma);
    backend
        .start(&BotStartCtx {
            config: BotRuntimeConfig::default_path("/data", "10001").with_bot_config(&config),
            bot_config: Some(config),
        })
        .await
        .unwrap();
    let commands = host.commands();
    let copy = commands
        .iter()
        .position(|command| {
            command.program == "docker" && command.args.first().map(String::as_str) == Some("cp")
        })
        .unwrap();
    assert_eq!(
        commands[copy].args,
        [
            "cp",
            "/home/napcat/.napcat-bots/slbot-10001/snowluma-data/config/onebot_10001.json",
            "slbot-10001:/app/snowluma-data/config/onebot_10001.json"
        ]
    );
    let chown = commands
        .iter()
        .position(|command| command.args.iter().any(|arg| arg == "chown"))
        .unwrap();
    let chmod = commands
        .iter()
        .position(|command| command.args.iter().any(|arg| arg == "chmod"))
        .unwrap();
    let restart = commands
        .iter()
        .position(|command| command.args.first().map(String::as_str) == Some("restart"))
        .unwrap();
    assert!(copy < chown && chown < chmod && chmod < restart);
    assert!(
        commands[chown]
            .args
            .iter()
            .any(|arg| arg == "/app/snowluma-data/config")
    );
}

#[tokio::test]
async fn snowluma_start_stops_container_when_copy_fails_so_retry_does_not_skip_configuration() {
    let mut mock = DockerRuntimeMockHost::new("");
    mock.fail_copy = true;
    let host = Arc::new(mock);
    let backend = DockerDeploymentBackend::new(
        Arc::new(DockerDeployment::with_sl_secrets("test-vnc", "test-webui")),
        host.clone(),
        "docker",
        BotFlavor::SnowLuma,
    );
    let config = docker_config(BackendType::SnowLuma);
    let result = backend
        .start(&BotStartCtx {
            config: BotRuntimeConfig::default_path("/data", "10001").with_bot_config(&config),
            bot_config: Some(config),
        })
        .await;
    assert!(result.unwrap_err().to_string().contains("copy failed"));
    assert!(
        host.commands()
            .iter()
            .any(|command| command.args == ["stop", "--", "slbot-10001"])
    );
    assert!(
        !host
            .commands()
            .iter()
            .any(|command| command.args.first().map(String::as_str) == Some("restart"))
    );
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
    let mut host = DockerRuntimeMockHost::new("");
    host.fail_home_probe = true;

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

#[tokio::test]
async fn docker_login_start_never_recreates_active_containers() {
    for state in ["running", "paused", "restarting"] {
        let host = Arc::new(DockerRuntimeMockHost::new(format!(
            r#"{{"ID":"abc123","Names":"slbot-10001","Image":"motricseven7/snowluma:latest","State":"{state}","Status":"active","Ports":""}}"#,
        )));
        let backend = DockerDeploymentBackend::new(
            Arc::new(DockerDeployment::with_sl_secrets("test-vnc", "test-webui")),
            host.clone(),
            BotId::new("docker"),
            BotFlavor::SnowLuma,
        )
        .with_snowluma_qr_login(true);
        let config = docker_config(BackendType::SnowLuma);
        let ctx = BotStartCtx {
            config: BotRuntimeConfig::default_path("/data", "10001").with_bot_config(&config),
            bot_config: Some(config),
        };
        let result = backend.start(&ctx).await;
        assert_eq!(result.is_ok(), state == "running");
        assert!(host.writes().is_empty());
        assert!(host.created_dirs().is_empty());
        assert!(host.commands().iter().all(|cmd| {
            cmd.program == "docker"
                && matches!(
                    cmd.args.first().map(String::as_str),
                    // --version 只读取 Linux 的 CLI 身份，兼容 podman-docker。
                    Some("--version" | "version" | "info" | "compose" | "ps")
                )
        }));
        assert!(!host.commands().iter().any(|cmd| {
            cmd.args
                .iter()
                .any(|arg| matches!(arg.as_str(), "run" | "up" | "rm" | "pull"))
        }));
    }
}

#[tokio::test]
async fn docker_login_snapshot_waits_for_the_shared_lifecycle_lock() {
    let host = Arc::new(DockerRuntimeMockHost::new(""));
    let lock = Arc::new(tokio::sync::Mutex::new(()));
    let guard = lock.lock().await;
    let backend = Arc::new(
        DockerDeploymentBackend::new(
            Arc::new(DockerDeployment::new()),
            host.clone(),
            "docker",
            BotFlavor::SnowLuma,
        )
        .with_snowluma_qr_login(true)
        .with_qr_login_lock(lock.clone()),
    );
    let pending =
        tokio::spawn(async move { backend.qr_login_snapshot(&BotId::new("10001"), false).await });
    tokio::task::yield_now().await;
    assert!(host.commands().is_empty());
    assert!(!pending.is_finished());
    drop(guard);
    assert!(pending.await.unwrap().unwrap().qrcode_payload.is_none());
    assert!(
        host.commands()
            .iter()
            .any(|command| command.program == "sh")
    );
}

#[tokio::test]
async fn docker_login_prepare_or_overlay_failure_still_launches_with_vnc() {
    for fail_login_overlay in [false, true] {
        let mut mock = DockerRuntimeMockHost::new("");
        mock.fail_login_overlay = fail_login_overlay;
        let host = Arc::new(mock);
        let backend = DockerDeploymentBackend::new(
            Arc::new(DockerDeployment::with_sl_secrets("test-vnc", "test-webui")),
            host.clone(),
            "docker",
            BotFlavor::SnowLuma,
        )
        .with_snowluma_qr_login(true);
        let config = docker_config(BackendType::SnowLuma);
        let result = backend
            .start(&BotStartCtx {
                config: BotRuntimeConfig::default_path("/data", "10001").with_bot_config(&config),
                bot_config: Some(config),
            })
            .await;
        assert!(result.is_ok());
        let writes = host.writes();
        let compose = writes
            .iter()
            .filter(|write| write.path.ends_with("/docker-compose.yml"))
            .collect::<Vec<_>>();
        assert_eq!(compose.len(), if fail_login_overlay { 3 } else { 1 });
        assert!(!compose.last().unwrap().text.contains("/ncd-login-assist"));
        assert!(compose.last().unwrap().text.contains("VNC_PASSWD"));
        assert!(
            host.commands()
                .iter()
                .any(|command| command.program == "docker"
                    && command.args.starts_with(&["compose".into(), "up".into()]))
        );
        if fail_login_overlay {
            assert!(compose[1].text.contains("/ncd-login-assist"));
            assert!(host.commands().iter().any(|command| {
                command.program == "docker"
                    && command.args.first().map(String::as_str) == Some("run")
                    && command.args.iter().any(|arg| arg == "/usr/bin/python3")
                    && command.args.iter().any(|arg| arg == "--probe")
            }));
            assert!(host.commands().iter().any(|command| {
                command.program == "chown"
                    && command.args.iter().any(|arg| arg == "1000:1000")
                    && command
                        .args
                        .iter()
                        .any(|arg| arg.ends_with("/ncd-login/assets"))
            }));
            assert!(host.commands().iter().any(|command| {
                command.program == "rm"
                    && command.args.first().map(String::as_str) == Some("-f")
                    && command
                        .args
                        .iter()
                        .any(|arg| arg.ends_with("/libentry-overlay.so"))
            }));
        }
    }
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

#[tokio::test]
async fn podman_backend_status_and_logs_keep_the_instance_engine() {
    use ncd_domain::docker::ContainerEngine;
    let host = Arc::new(DockerRuntimeMockHost::new(
        r#"{"ID":"podman-id","Names":"ncbot-10001","Image":"docker.io/mlikiowa/napcat-docker:latest","State":"running","Status":"Up","Ports":""}"#,
    ));
    let backend = DockerDeploymentBackend::new(
        Arc::new(DockerDeployment::new().with_engine(ContainerEngine::Podman)),
        host.clone(),
        BotId::new("docker"),
        BotFlavor::NapCat,
    );
    let bot = BotId::new("10001");
    backend.status(bot.clone()).await.unwrap();
    let logs = backend.tail_log(bot, TailOpts { lines: 5 }).await.unwrap();
    assert!(logs.lines.iter().any(|line| line == "legacy-line-a"));
    assert!(
        host.commands()
            .iter()
            .all(|c| c.program == "podman" && !c.elevated)
    );
}
