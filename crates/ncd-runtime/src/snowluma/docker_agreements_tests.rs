use super::*;
use async_trait::async_trait;
use bytes::Bytes;
use ncd_domain::docker::{BotDockerConfig, ContainerEngine};
use ncd_domain::{DeploymentType, RuntimeTarget};
use ncd_host::{
    Arch, ArchiveKind, CommandOutput, DirEntry, HostError, HostProcess, HostShell, Locality, Os,
};
use ncd_test_support::BotConfigBuilder;
use std::{collections::HashMap, path::Path, sync::Mutex};

const ID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const EULA: &str = "# Fixture EULA\nVersion: 1.0\nTerms.\n";
const PRIVACY: &str = "# Fixture Privacy\nVersion: 1.0\nPrivacy.\n";

#[derive(Clone, Copy)]
enum Fault {
    Denied,
    Timeout,
}

struct DockerAgreementHost {
    container: Option<&'static str>,
    state: &'static str,
    engine: ContainerEngine,
    locality: Locality,
    environment: Vec<String>,
    files: Mutex<HashMap<String, Vec<u8>>>,
    temporary: Mutex<HashMap<String, Vec<u8>>>,
    staging: Mutex<HashMap<String, Vec<u8>>>,
    commands: Mutex<Vec<HostCommand>>,
    faults: HashMap<String, Fault>,
    sudo: bool,
    drop_write: bool,
}

impl DockerAgreementHost {
    fn new() -> Self {
        Self {
            container: Some("slbot-10001"),
            state: "exited",
            engine: ContainerEngine::Docker,
            locality: Locality::Remote,
            environment: vec![
                "SNOWLUMA_HOME=/app/runtime".into(),
                "SNOWLUMA_DATA=/app/data".into(),
            ],
            files: Mutex::new(HashMap::from([
                ("/app/runtime/EULA.md".into(), EULA.as_bytes().to_vec()),
                (
                    "/app/runtime/PRIVACY.md".into(),
                    PRIVACY.as_bytes().to_vec(),
                ),
                (
                    "/app/data/config/runtime.json".into(),
                    b"preserved-runtime".to_vec(),
                ),
                (
                    "/app/data/config/onebot_10001.json".into(),
                    b"preserved-onebot".to_vec(),
                ),
            ])),
            temporary: Mutex::new(HashMap::new()),
            staging: Mutex::new(HashMap::new()),
            commands: Mutex::new(Vec::new()),
            faults: HashMap::new(),
            sudo: false,
            drop_write: false,
        }
    }

    fn config(&self) -> BotConfig {
        let mut config = BotConfigBuilder::new()
            .qq_id(10001)
            .backend_type(BackendType::SnowLuma)
            .deployment_type(DeploymentType::Docker)
            .runtime_target(if self.locality == Locality::Local {
                RuntimeTarget::Local
            } else {
                RuntimeTarget::server("agreement-host")
            })
            .build();
        config.docker = Some(BotDockerConfig {
            engine: Some(self.engine),
            ..Default::default()
        });
        config
    }

    fn assert_clean(&self) {
        assert!(self.temporary.lock().unwrap().is_empty());
        for command in self.commands.lock().unwrap().iter() {
            if command.program == self.engine.program() {
                assert!(!command.args.iter().any(|arg| matches!(
                    arg.as_str(),
                    "exec" | "start" | "stop" | "restart" | "up" | "down" | "rm"
                )));
            }
        }
    }
}

fn output(code: i32, stdout: String, stderr: &str) -> CommandOutput {
    CommandOutput {
        exit_code: Some(code),
        stdout,
        stderr: stderr.into(),
    }
}

#[async_trait]
impl Host for DockerAgreementHost {
    fn os(&self) -> Os {
        Os::Linux
    }
    fn arch(&self) -> Arch {
        Arch::X86_64
    }
    fn locality(&self) -> Locality {
        self.locality
    }
    fn id(&self) -> &str {
        "agreement-docker-fixture"
    }
    fn shell(&self) -> &dyn HostShell {
        &ncd_host::shell::BashShell
    }
    fn target_managed_root(&self) -> Option<HostPath> {
        Some(HostPath::from_posix("/fixture/managed"))
    }
    async fn read_file(&self, path: &HostPath) -> Result<Bytes, HostError> {
        self.temporary
            .lock()
            .unwrap()
            .get(path.as_posix())
            .cloned()
            .map(Bytes::from)
            .ok_or_else(|| HostError::PathNotFound { path: path.clone() })
    }
    async fn write_file(&self, path: &HostPath, data: &[u8]) -> Result<(), HostError> {
        let mut files = if path.as_posix().starts_with("/tmp/") {
            self.temporary.lock().unwrap()
        } else {
            self.staging.lock().unwrap()
        };
        files.insert(path.as_posix().into(), data.to_vec());
        Ok(())
    }
    async fn create_dir_all(&self, _: &HostPath) -> Result<(), HostError> {
        Ok(())
    }
    async fn remove_file(&self, path: &HostPath) -> Result<(), HostError> {
        self.temporary.lock().unwrap().remove(path.as_posix());
        Ok(())
    }
    async fn remove_dir_all(&self, path: &HostPath) -> Result<(), HostError> {
        let prefix = format!("{}/", path.as_posix());
        self.temporary
            .lock()
            .unwrap()
            .retain(|path, _| !path.starts_with(&prefix));
        Ok(())
    }
    async fn list_dir(&self, _: &HostPath) -> Result<Vec<DirEntry>, HostError> {
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
        if command.program == "chmod" {
            return Ok(output(0, String::new(), ""));
        }
        if command.program == "sh" && command.args == ["-c", "echo $HOME"] {
            return Ok(output(0, "/home/u\n".into(), ""));
        }
        if command.program == "rm" {
            self.temporary
                .lock()
                .unwrap()
                .remove(command.args.last().unwrap());
            return Ok(output(0, String::new(), ""));
        }
        assert_eq!(
            command.program,
            self.engine.program(),
            "must use instance engine"
        );
        match command.args[0].as_str() {
            "--version" | "version" => Ok(output(0, "27.0.0".into(), "")),
            "info" if self.sudo && !command.elevated => Ok(output(
                1,
                String::new(),
                "permission denied /var/run/docker.sock",
            )),
            "info" | "compose" => Ok(output(0, "ready".into(), "")),
            "ps" => Ok(output(
                0,
                self.container
                    .map(|name| {
                        serde_json::json!({
                            "ID": ID, "Names": name, "Image": "fixture/snowluma:latest",
                            "State": self.state, "Status": "fixture", "Ports": "",
                        })
                        .to_string()
                    })
                    .unwrap_or_default(),
                "",
            )),
            "inspect" => {
                assert_eq!(command.args.last().unwrap(), ID);
                Ok(output(
                    0,
                    serde_json::to_string(&self.environment).unwrap(),
                    "",
                ))
            }
            "cp" => {
                let source = &command.args[1];
                let target = &command.args[2];
                if let Some((id, path)) = source.split_once(':') {
                    assert_eq!(id, ID);
                    match self.faults.get(path) {
                        Some(Fault::Denied) => {
                            return Ok(output(1, String::new(), "permission denied"));
                        }
                        Some(Fault::Timeout) => {
                            return Err(HostError::Timeout {
                                operation: "docker_cp",
                            });
                        }
                        None => {}
                    }
                    if let Some(data) = self.files.lock().unwrap().get(path) {
                        self.temporary
                            .lock()
                            .unwrap()
                            .insert(target.clone(), data.clone());
                        Ok(output(0, String::new(), ""))
                    } else {
                        Ok(output(
                            1,
                            String::new(),
                            "Could not find the file in container",
                        ))
                    }
                } else {
                    let (id, data_dir) = target.split_once(':').unwrap();
                    assert_eq!(id, ID);
                    assert!(source.ends_with("/config"));
                    if !self.drop_write {
                        for (path, data) in self.temporary.lock().unwrap().iter() {
                            if let Some(file) = path.strip_prefix(&format!("{source}/")) {
                                self.files
                                    .lock()
                                    .unwrap()
                                    .insert(format!("{data_dir}/config/{file}"), data.clone());
                            }
                        }
                    }
                    Ok(output(0, String::new(), ""))
                }
            }
            other => panic!("unexpected command: {other}"),
        }
    }
}

#[tokio::test]
async fn stopped_docker_can_read_accept_and_recheck_without_webui_or_restarting() {
    for (engine, locality, state, sudo) in [
        (ContainerEngine::Docker, Locality::Remote, "exited", true),
        (ContainerEngine::Docker, Locality::Local, "created", false),
        (ContainerEngine::Podman, Locality::Remote, "exited", false),
    ] {
        let mut host = DockerAgreementHost::new();
        host.engine = engine;
        host.locality = locality;
        host.state = state;
        host.sudo = sudo;
        let config = host.config();
        let payload = load_docker_agreements(&host, &config)
            .await
            .unwrap()
            .unwrap();
        assert!(payload.consent_required);
        assert_eq!(payload.documents[0].text, EULA);
        assert_eq!(payload.documents[1].text, PRIVACY);
        assert!(
            record_docker_consent(&host, &config, &payload.version)
                .await
                .unwrap()
        );
        let accepted = load_docker_agreements(&host, &config)
            .await
            .unwrap()
            .unwrap();
        assert!(!accepted.consent_required);
        assert_eq!(accepted.version, payload.version);
        let staging = host.staging.lock().unwrap();
        assert_eq!(staging.len(), 1);
        assert_eq!(
            parse_consent_version_json(
                std::str::from_utf8(staging.values().next().unwrap()).unwrap()
            )
            .as_deref(),
            Some(payload.version.as_str())
        );
        drop(staging);
        let files = host.files.lock().unwrap();
        assert_eq!(files["/app/data/config/runtime.json"], b"preserved-runtime");
        assert_eq!(
            files["/app/data/config/onebot_10001.json"],
            b"preserved-onebot"
        );
        drop(files);
        host.assert_clean();
    }
}

#[tokio::test]
async fn legacy_container_name_and_dist_documents_remain_readable() {
    let mut host = DockerAgreementHost::new();
    host.container = Some("ncbot-10001");
    host.environment.clear();
    *host.files.lock().unwrap() = HashMap::from([
        (
            "/app/snowluma/dist/EULA.md".into(),
            EULA.as_bytes().to_vec(),
        ),
        (
            "/app/snowluma/dist/PRIVACY.md".into(),
            PRIVACY.as_bytes().to_vec(),
        ),
    ]);
    let config = host.config();
    let payload = load_docker_agreements(&host, &config)
        .await
        .unwrap()
        .unwrap();
    assert!(
        record_docker_consent(&host, &config, &payload.version)
            .await
            .unwrap()
    );
    assert!(
        !load_docker_agreements(&host, &config)
            .await
            .unwrap()
            .unwrap()
            .consent_required
    );
    host.assert_clean();
}

#[tokio::test]
async fn first_start_without_a_container_defers_until_creation() {
    let mut host = DockerAgreementHost::new();
    host.container = None;
    assert!(
        load_docker_agreements(&host, &host.config())
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        record_docker_consent(&host, &host.config(), "stale")
            .await
            .is_err()
    );
    host.assert_clean();
}

#[tokio::test]
async fn updated_documents_require_new_consent_and_reject_a_stale_dialog() {
    let host = DockerAgreementHost::new();
    let config = host.config();
    let old = load_docker_agreements(&host, &config)
        .await
        .unwrap()
        .unwrap();
    record_docker_consent(&host, &config, &old.version)
        .await
        .unwrap();
    host.files.lock().unwrap().insert(
        "/app/runtime/EULA.md".into(),
        b"# EULA\nUpdated terms".to_vec(),
    );
    let current = load_docker_agreements(&host, &config)
        .await
        .unwrap()
        .unwrap();
    assert!(current.consent_required);
    assert_ne!(current.version, old.version);
    assert!(
        record_docker_consent(&host, &config, &old.version)
            .await
            .is_err()
    );
    assert_eq!(
        parse_consent_version_json(
            std::str::from_utf8(&host.files.lock().unwrap()["/app/data/config/consent.json"])
                .unwrap()
        )
        .as_deref(),
        Some(old.version.as_str())
    );
    host.assert_clean();
}

#[tokio::test]
async fn missing_or_unreadable_documents_cannot_skip_consent() {
    for fault in [None, Some(Fault::Denied), Some(Fault::Timeout)] {
        let mut host = DockerAgreementHost::new();
        if let Some(fault) = fault {
            host.faults.insert("/app/runtime/PRIVACY.md".into(), fault);
        } else {
            host.files.lock().unwrap().remove("/app/runtime/PRIVACY.md");
        }
        assert!(load_docker_agreements(&host, &host.config()).await.is_err());
        host.assert_clean();
    }
}

#[tokio::test]
async fn invalid_utf8_is_an_error_and_temporary_files_are_removed() {
    let host = DockerAgreementHost::new();
    host.files
        .lock()
        .unwrap()
        .insert("/app/runtime/EULA.md".into(), vec![0xff]);
    assert!(load_docker_agreements(&host, &host.config()).await.is_err());
    host.assert_clean();
}

#[tokio::test]
async fn running_container_keeps_webui_consent_notification_path() {
    let mut host = DockerAgreementHost::new();
    host.state = "running";
    assert!(
        !record_docker_consent(&host, &host.config(), "version")
            .await
            .unwrap()
    );
    assert!(
        !host
            .files
            .lock()
            .unwrap()
            .contains_key("/app/data/config/consent.json")
    );
    host.assert_clean();
}

#[tokio::test]
async fn consent_write_is_verified_before_reporting_success() {
    let mut host = DockerAgreementHost::new();
    host.drop_write = true;
    let config = host.config();
    let payload = load_docker_agreements(&host, &config)
        .await
        .unwrap()
        .unwrap();
    assert!(
        record_docker_consent(&host, &config, &payload.version)
            .await
            .is_err()
    );
    host.assert_clean();
}

/// 夹具镜像含公开测试 EULA/PRIVACY、/app/data/config/runtime.json 和可正常退出的 /exit。
/// VOLUME /app/data 用于验证停止容器的卷可读写；测试只创建并清理自己的容器与匿名卷。
#[cfg(target_os = "linux")]
#[tokio::test]
#[ignore = "需要本机 Docker 及 NCD_DOCKER_AGREEMENT_FIXTURE_IMAGE 离线夹具镜像"]
async fn real_stopped_container_preserves_volume_and_accepts_without_webui() {
    use futures_util::FutureExt;
    use ncd_host::local::LocalLinuxHost;

    let image = std::env::var("NCD_DOCKER_AGREEMENT_FIXTURE_IMAGE").expect("fixture image");
    let data_root = tempfile::tempdir().unwrap();
    let host = LocalLinuxHost::new().with_managed_root(data_root.path());
    let qq = 1_000_000_000 + (uuid::Uuid::new_v4().as_u128() % 8_000_000_000) as u64;
    let mut config = BotConfigBuilder::new()
        .qq_id(qq)
        .backend_type(BackendType::SnowLuma)
        .deployment_type(DeploymentType::Docker)
        .build();
    config.docker = Some(BotDockerConfig {
        engine: Some(ContainerEngine::Docker),
        ..Default::default()
    });
    let cli = DockerCli::for_bot(&host, &config);
    cli.ensure_daemon_ready().await.unwrap();
    let name = format!("slbot-{qq}");
    let created = host
        .run_to_string(
            cli.command()
                .args(["create", "--name", &name, &image, "/exit"]),
        )
        .await
        .unwrap();
    assert!(created.success(), "{}", created.stderr);
    let container = created.stdout.trim();
    let result = std::panic::AssertUnwindSafe(async {
        let exited = host
            .run_to_string(cli.command().args(["start", "--attach", container]))
            .await
            .map_err(failure)?;
        assert!(exited.success(), "{}", exited.stderr);
        let payload = load_docker_agreements(&host, &config).await?.unwrap();
        assert!(payload.consent_required);
        assert!(record_docker_consent(&host, &config, &payload.version).await?);
        assert!(
            !load_docker_agreements(&host, &config)
                .await?
                .unwrap()
                .consent_required
        );
        let restarted = host
            .run_to_string(cli.command().args(["start", "--attach", container]))
            .await
            .map_err(failure)?;
        assert!(restarted.success(), "{}", restarted.stderr);
        assert!(
            !load_docker_agreements(&host, &config)
                .await?
                .unwrap()
                .consent_required
        );
        assert_eq!(
            read_container_text(&host, &cli, container, "/app/data/config/runtime.json")
                .await?
                .as_deref(),
            Some("preserved-runtime")
        );
        // 同意写入没有启动容器，卷内原配置仍在。
        assert_eq!(
            find_container(&cli, &config).await?.unwrap().state,
            ContainerState::Exited
        );
        Ok::<(), BotManagerError>(())
    })
    .catch_unwind()
    .await;
    let removed = host
        .run_to_string(
            cli.command()
                .args(["rm", "--force", "--volumes", container]),
        )
        .await
        .unwrap();
    assert!(removed.success(), "{}", removed.stderr);
    result.unwrap().unwrap();
}
