//! Docker 反向对接的最终行为：宿主机/跨机路由、恢复与独立资源清理。
use super::*;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex as StdMutex;
use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};

use async_trait::async_trait;
use bytes::Bytes;
use ncd_host::{
    Arch, ArchiveKind, CommandOutput, DirEntry, HostError, HostProcess, HostShell, ShellKind,
    SshDialTarget,
};
use ncd_test_support::BotConfigBuilder;

const GATEWAY: &str = "172.17.0.1";
const APP_DIR: &str = "/apps/karin";
const APP_PORT: u16 = 7777;
const TOKEN: &str = "preserved-token";

struct Bash;
impl HostShell for Bash {
    fn kind(&self) -> ShellKind {
        ShellKind::Bash
    }
    fn escape(&self, value: &str) -> String {
        ncd_host::shell_single_quote(value)
    }
    fn line_separator(&self) -> &'static str {
        "\n"
    }
}
static BASH: Bash = Bash;

/// An isolated host filesystem plus observed transport state. Commands start/stop named
/// transports; readiness probes inspect that state rather than accepting every command.
struct LinkHost {
    name: String,
    local: bool,
    files: StdMutex<BTreeMap<String, Vec<u8>>>,
    dirs: StdMutex<BTreeSet<String>>,
    active: StdMutex<BTreeSet<String>>,
    commands: StdMutex<Vec<HostCommand>>,
    tunnels: StdMutex<Vec<TunnelSpec>>,
    next_remote: AtomicU16,
    fail_bridge: AtomicBool,
    fail_bridge_once: AtomicBool,
    podman_rootless: AtomicBool,
}

impl LinkHost {
    fn new(name: &str, local: bool) -> Arc<Self> {
        let host = Arc::new(Self {
            name: name.into(),
            local,
            files: StdMutex::new(BTreeMap::new()),
            dirs: StdMutex::new(BTreeSet::new()),
            active: StdMutex::new(BTreeSet::new()),
            commands: StdMutex::new(Vec::new()),
            tunnels: StdMutex::new(Vec::new()),
            next_remote: AtomicU16::new(42001),
            fail_bridge: AtomicBool::new(false),
            fail_bridge_once: AtomicBool::new(false),
            podman_rootless: AtomicBool::new(false),
        });
        host.put(
            &format!("{}/.ssh/authorized_keys", host.home()),
            "ssh-ed25519 unrelated original-user-key\n",
        );
        host
    }
    fn home(&self) -> String {
        format!("/home/{}", self.name)
    }
    fn put(&self, path: &str, text: &str) {
        self.files
            .lock()
            .unwrap()
            .insert(path.into(), text.as_bytes().to_vec());
    }
    fn text(&self, path: &str) -> String {
        String::from_utf8(
            self.files
                .lock()
                .unwrap()
                .get(path)
                .cloned()
                .unwrap_or_default(),
        )
        .unwrap()
    }
    fn bridge_path(&self, id: &AppInstanceId) -> String {
        format!("{}/ncd/links/{}-docker-bridge", self.home(), id.as_str())
    }
    fn bridge_state(&self, id: &AppInstanceId) -> serde_json::Value {
        serde_json::from_str(&self.text(&format!("{}/docker-bridge.json", self.bridge_path(id))))
            .unwrap()
    }
    fn authorization(&self) -> String {
        self.text(&format!("{}/.ssh/authorized_keys", self.home()))
    }
    fn running_in(&self, dir: &str) -> bool {
        self.active.lock().unwrap().contains(dir)
    }
    fn output(code: i32, text: impl Into<String>) -> Result<CommandOutput, HostError> {
        Ok(CommandOutput {
            exit_code: Some(code),
            stdout: text.into(),
            stderr: String::new(),
        })
    }
}

#[async_trait]
impl Host for LinkHost {
    fn os(&self) -> Os {
        Os::Linux
    }
    fn arch(&self) -> Arch {
        Arch::X86_64
    }
    fn locality(&self) -> Locality {
        if self.local {
            Locality::Local
        } else {
            Locality::Remote
        }
    }
    fn id(&self) -> &str {
        &self.name
    }
    fn shell(&self) -> &dyn HostShell {
        &BASH
    }
    fn ssh_dial_target(&self) -> Option<SshDialTarget> {
        Some(SshDialTarget {
            host: format!("{}.example.test", self.name),
            port: 22,
            username: self.name.clone(),
        })
    }
    fn trusted_ssh_host_key(&self) -> Option<String> {
        Some("ssh-ed25519 AAAhostkey".into())
    }
    async fn secure_private_key(&self, _: &HostPath) -> Result<(), HostError> {
        Ok(())
    }
    async fn command_exists(&self, _: &str) -> bool {
        true
    }
    async fn read_file(&self, path: &HostPath) -> Result<Bytes, HostError> {
        self.files
            .lock()
            .unwrap()
            .get(path.as_posix())
            .cloned()
            .map(Bytes::from)
            .ok_or_else(|| HostError::PathNotFound { path: path.clone() })
    }
    async fn write_file(&self, path: &HostPath, bytes: &[u8]) -> Result<(), HostError> {
        self.files
            .lock()
            .unwrap()
            .insert(path.as_posix().into(), bytes.into());
        Ok(())
    }
    async fn list_dir(&self, _: &HostPath) -> Result<Vec<DirEntry>, HostError> {
        Ok(Vec::new())
    }
    async fn create_dir_all(&self, path: &HostPath) -> Result<(), HostError> {
        self.dirs.lock().unwrap().insert(path.as_posix().into());
        Ok(())
    }
    async fn remove_file(&self, path: &HostPath) -> Result<(), HostError> {
        self.files.lock().unwrap().remove(path.as_posix());
        Ok(())
    }
    async fn remove_dir_all(&self, path: &HostPath) -> Result<(), HostError> {
        let prefix = format!("{}/", path.as_posix());
        self.files
            .lock()
            .unwrap()
            .retain(|name, _| !name.starts_with(&prefix));
        self.dirs
            .lock()
            .unwrap()
            .retain(|name| name != path.as_posix() && !name.starts_with(&prefix));
        self.active.lock().unwrap().remove(path.as_posix());
        Ok(())
    }
    async fn rename(&self, from: &HostPath, to: &HostPath) -> Result<(), HostError> {
        let mut files = self.files.lock().unwrap();
        let bytes = files
            .remove(from.as_posix())
            .ok_or_else(|| HostError::PathNotFound { path: from.clone() })?;
        files.insert(to.as_posix().into(), bytes);
        Ok(())
    }
    async fn exists(&self, path: &HostPath) -> Result<bool, HostError> {
        Ok(self.files.lock().unwrap().contains_key(path.as_posix())
            || self.dirs.lock().unwrap().contains(path.as_posix()))
    }
    async fn upload(&self, _: &Path, _: &HostPath) -> Result<(), HostError> {
        Err(HostError::Unsupported {
            operation: "test-upload",
        })
    }
    async fn download(&self, _: &HostPath, _: &Path) -> Result<(), HostError> {
        Err(HostError::Unsupported {
            operation: "test-download",
        })
    }
    async fn extract_archive(
        &self,
        _: &HostPath,
        _: &HostPath,
        _: ArchiveKind,
    ) -> Result<(), HostError> {
        Err(HostError::Unsupported {
            operation: "test-extract",
        })
    }
    async fn spawn(&self, _: HostCommand) -> Result<Box<dyn HostProcess>, HostError> {
        Err(HostError::Unsupported {
            operation: "test-spawn",
        })
    }
    async fn open_tunnel(&self, spec: TunnelSpec) -> Result<TunnelHandle, HostError> {
        self.tunnels.lock().unwrap().push(spec.clone());
        match spec.direction {
            ncd_host::remote::TunnelDirection::RemoteToLocal => Ok(TunnelHandle::detached(
                spec.local_port,
                self.next_remote.fetch_add(1, Ordering::SeqCst),
            )),
            ncd_host::remote::TunnelDirection::LocalToRemote => Ok(TunnelHandle::detached(
                if spec.local_port == 0 {
                    41001
                } else {
                    spec.local_port
                },
                0,
            )),
        }
    }
    async fn run_to_string(&self, command: HostCommand) -> Result<CommandOutput, HostError> {
        self.commands.lock().unwrap().push(command.clone());
        if command.program == "podman" {
            return match command.args.first().map(String::as_str) {
                Some("--version") => Self::output(0, "podman version 5.8.9\n"),
                Some("info") if command.args.iter().any(|arg| arg.contains("Rootless")) => {
                    Self::output(0, self.podman_rootless.load(Ordering::SeqCst).to_string())
                }
                Some("network") => Self::output(0, r#"[{"gateway":"10.88.0.1"}]"#),
                _ => Self::output(0, ""),
            };
        }
        if command.program == "docker" {
            return match command.args.first().map(String::as_str) {
                Some("version") => Self::output(0, "27.3.1\n"),
                Some("network") => Self::output(0, format!(r#"[{{"Gateway":"{GATEWAY}"}}]"#)),
                _ => Self::output(0, ""),
            };
        }
        if command.program == "ssh-keygen" {
            let key = &command.args[command.args.iter().position(|arg| arg == "-f").unwrap() + 1];
            self.put(key, "test private key");
            self.put(&format!("{key}.pub"), "ssh-ed25519 AAAtestkey generated");
            return Self::output(0, "");
        }
        if command.program == "ssh" {
            let active = self.active.lock().unwrap();
            return Self::output(
                if active
                    .iter()
                    .any(|dir| command.args.iter().any(|arg| arg.starts_with(dir)))
                {
                    0
                } else {
                    1
                },
                "",
            );
        }
        if command.program == "sh" && command.args.first().map(String::as_str) == Some("-c") {
            let script = &command.args[1];
            if script == "printf '%s' \"$HOME\"" || script == "echo $HOME" {
                return Self::output(0, self.home());
            }
            if script.starts_with("bash -c 'echo >/dev/tcp/") {
                return Self::output(1, "");
            }
            if script.contains("nohup setsid /bin/sh") {
                let dirs: Vec<_> = self
                    .dirs
                    .lock()
                    .unwrap()
                    .iter()
                    .filter(|dir| {
                        script.contains(dir.as_str())
                            && self
                                .files
                                .lock()
                                .unwrap()
                                .contains_key(&format!("{dir}/run.sh"))
                    })
                    .cloned()
                    .collect();
                self.active.lock().unwrap().extend(dirs);
                return Self::output(0, "RUNNING\n");
            }
            if script.starts_with("stop=") {
                self.active
                    .lock()
                    .unwrap()
                    .retain(|dir| !script.contains(dir.as_str()));
                return Self::output(0, "");
            }
            if script.contains("ssh -S") && script.contains("-O check") {
                if script.contains("-docker-bridge")
                    && (self.fail_bridge.load(Ordering::SeqCst)
                        || self.fail_bridge_once.swap(false, Ordering::SeqCst))
                {
                    return Err(HostError::Io(std::io::Error::other(
                        "bridge authentication rejected",
                    )));
                }
                let ready = self
                    .active
                    .lock()
                    .unwrap()
                    .iter()
                    .any(|dir| script.contains(dir.as_str()));
                return Self::output(if ready { 0 } else { 1 }, "");
            }
            return Self::output(0, "");
        }
        Self::output(0, "")
    }
}

struct Resolver {
    local: Arc<LinkHost>,
    bot: Arc<LinkHost>,
    app: Arc<LinkHost>,
}
#[async_trait]
impl HostResolver for Resolver {
    async fn resolve(
        &self,
        target: &RuntimeTarget,
    ) -> Result<Arc<dyn Host>, ncd_server::HostResolveError> {
        match target.server_id() {
            None => Ok(self.local.clone()),
            Some("bot") => Ok(self.bot.clone()),
            Some("app") => Ok(self.app.clone()),
            Some(name) => Err(ncd_server::HostResolveError::message(format!(
                "unknown host {name}"
            ))),
        }
    }
}
struct Bots(tokio::sync::Mutex<BotConfig>, AtomicBool);
#[async_trait]
impl BotConfigPort for Bots {
    async fn bot_config(&self, _: &BotId) -> Result<Option<BotConfig>, String> {
        Ok(Some(self.0.lock().await.clone()))
    }
    async fn upsert_bot_config(&self, config: BotConfig) -> Result<(), String> {
        *self.0.lock().await = config;
        if self.1.swap(false, Ordering::SeqCst) {
            return Err(
                "DOCKER_CONFIG_APPLY_FAILED: simulated container restart failure after persistence"
                    .into(),
            );
        }
        Ok(())
    }
    async fn list_bot_configs_for_link(&self) -> Result<Vec<BotConfig>, String> {
        Ok(vec![self.0.lock().await.clone()])
    }
}

struct Fixture {
    _tmp: tempfile::TempDir,
    manager: AppManager,
    bots: Arc<Bots>,
    bot: Arc<LinkHost>,
    app: Arc<LinkHost>,
    local: Arc<LinkHost>,
    id: AppInstanceId,
}
impl Fixture {
    async fn new(app_host_id: &str, backend: BackendType) -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("data");
        let bot = LinkHost::new("bot", false);
        let app = LinkHost::new("app", false);
        let local = LinkHost::new("local", true);
        let app_host = match app_host_id {
            "remote:bot" => &bot,
            "remote:app" => &app,
            _ => &local,
        };
        app_host.put(
            &format!("{APP_DIR}/.env"),
            &format!("HTTP_PORT={APP_PORT}\nWS_SERVER_AUTH_KEY={TOKEN}\n"),
        );
        app_host.put(
            &format!("{APP_DIR}/@karinjs/config/adapter.json"),
            r#"{"console":{"isLocal":true},"onebot":{}}"#,
        );
        let bots = Arc::new(Bots(
            tokio::sync::Mutex::new(
                BotConfigBuilder::new()
                    .qq_id(10001)
                    .runtime_target(RuntimeTarget::server("bot"))
                    .backend_type(backend)
                    .deployment_type(DeploymentType::Docker)
                    .build(),
            ),
            AtomicBool::new(false),
        ));
        let bus = Arc::new(BroadcastEventBus::default());
        let store = Arc::new(AppInstanceStore::empty(&root));
        let id = AppInstanceId::new("reverse-app");
        store
            .upsert(AppInstance {
                id: id.clone(),
                framework_id: AppFrameworkId::new("karin"),
                display_name: "Karin".into(),
                placement: AppPlacement::native_for_host(app_host_id),
                host_id: app_host_id.into(),
                install_dir: APP_DIR.into(),
                port: APP_PORT,
                state: AppInstanceState::Stopped,
                link: None,
                installed_version: Some("1.0.0".into()),
                last_error: None,
                created_at_ms: 1,
                install_renderer: false,
                origin: AppInstanceOrigin::Created,
                auto_start: false,
            })
            .await
            .unwrap();
        let manager = AppManager::new(
            Arc::new(AppFrameworkRegistry::with_builtin()),
            store.clone(),
            Arc::new(NativeAppRuntime::new(bus.clone(), store)),
            Arc::new(Resolver {
                local: local.clone(),
                bot: bot.clone(),
                app: app.clone(),
            }),
            bots.clone(),
            bus,
            &root,
        );
        Self {
            _tmp: tmp,
            manager,
            bots,
            bot,
            app,
            local,
            id,
        }
    }
    async fn linked_client(&self) -> ncd_domain::WebsocketClientConfig {
        self.bots
            .0
            .lock()
            .await
            .connect
            .websocket_clients
            .iter()
            .find(|client| client.base.name == app_link_connection_name(&self.id))
            .unwrap()
            .clone()
    }
}

#[tokio::test]
async fn docker_reverse_routes_each_topology_through_bot_gateway_and_preserves_path_and_token() {
    for backend in [BackendType::NapCat, BackendType::SnowLuma] {
        for app_host in ["remote:bot", LOCAL_HOST_ID, "remote:app"] {
            let f = Fixture::new(app_host, backend).await;
            let linked = f
                .manager
                .apply_link(&f.id, &BotId::new("10001"))
                .await
                .unwrap();
            assert_eq!(
                linked.link.as_ref().unwrap().mode,
                OneBotLinkMode::ReverseWs
            );
            let bridge = f.bot.bridge_state(&f.id);
            let client = f.linked_client().await;
            let parts = parse_ws_url(&client.url).unwrap();
            assert_eq!(parts.host, GATEWAY);
            assert_eq!(bridge["endpoint"]["port"], u64::from(parts.port));
            assert_eq!(parts.path, "/onebot/v11/ws");
            assert_eq!(client.base.token, TOKEN);
            assert!(f.bot.running_in(&f.bot.bridge_path(&f.id)));
            let expected_target = match app_host {
                "remote:bot" => APP_PORT,
                LOCAL_HOST_ID => 42001,
                _ => linked.link.as_ref().unwrap().resident_forward_port.unwrap(),
            };
            assert_eq!(bridge["target_port"], u64::from(expected_target));
            if app_host == LOCAL_HOST_ID {
                let tunnels = f.bot.tunnels.lock().unwrap();
                assert_eq!(tunnels.len(), 1);
                assert_eq!(
                    tunnels[0].direction,
                    ncd_host::remote::TunnelDirection::RemoteToLocal
                );
                assert_eq!(tunnels[0].local_port, APP_PORT);
            } else {
                assert!(f.bot.tunnels.lock().unwrap().is_empty());
            }
            if app_host == "remote:app" {
                assert!(
                    f.app
                        .running_in(&format!("{}/ncd/links/{}", f.app.home(), f.id.as_str()))
                );
                assert!(f.app.tunnels.lock().unwrap().is_empty());
            }
            assert!(
                f.local
                    .commands
                    .lock()
                    .unwrap()
                    .iter()
                    .all(|command| command.program != "docker")
            );
            assert!(
                f.app
                    .commands
                    .lock()
                    .unwrap()
                    .iter()
                    .all(|command| command.program != "docker")
            );
        }
    }
}

#[tokio::test]
async fn docker_reverse_recovery_updates_bridge_target_when_desktop_remote_port_changes() {
    let f = Fixture::new(LOCAL_HOST_ID, BackendType::NapCat).await;
    let linked = f
        .manager
        .apply_link(&f.id, &BotId::new("10001"))
        .await
        .unwrap();
    let original = f.bot.bridge_state(&f.id);
    let original_url = f.linked_client().await.url;
    f.manager.drop_instance_tunnel(&f.id).await;
    f.manager.reconcile_link_tunnel(&linked).await.unwrap();
    let recovered = f.bot.bridge_state(&f.id);
    assert_eq!(original["target_port"], 42001);
    assert_eq!(recovered["target_port"], 42002);
    assert_eq!(
        recovered["endpoint"], original["endpoint"],
        "稳定的网桥端点内部改接新隧道口"
    );
    assert_eq!(f.linked_client().await.url, original_url);
    assert!(
        f.bot
            .authorization()
            .contains("permitopen=\"127.0.0.1:42002\"")
    );
    assert!(
        !f.bot
            .authorization()
            .contains("permitopen=\"127.0.0.1:42001\"")
    );
}

#[tokio::test]
async fn docker_reverse_bridge_failure_rolls_back_primary_and_never_records_a_link() {
    let f = Fixture::new("remote:app", BackendType::SnowLuma).await;
    f.bot.fail_bridge.store(true, Ordering::SeqCst);
    let error = f
        .manager
        .apply_link(&f.id, &BotId::new("10001"))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("bridge authentication rejected"));
    assert!(f.manager.get_instance(&f.id).await.unwrap().link.is_none());
    assert!(f.bots.0.lock().await.connect.websocket_clients.is_empty());
    assert!(f.bot.active.lock().unwrap().is_empty());
    assert!(f.app.active.lock().unwrap().is_empty());
    assert_eq!(
        f.bot.authorization(),
        "ssh-ed25519 unrelated original-user-key\n"
    );
    assert!(
        !f.bot
            .dirs
            .lock()
            .unwrap()
            .contains(&f.bot.bridge_path(&f.id))
    );
}

#[tokio::test]
async fn docker_reverse_unlink_removes_bridge_and_primary_without_removing_unrelated_authorization()
{
    let f = Fixture::new("remote:app", BackendType::NapCat).await;
    f.manager
        .apply_link(&f.id, &BotId::new("10001"))
        .await
        .unwrap();
    assert!(f.bot.authorization().contains("ncd-link:reverse-app\n"));
    assert!(
        f.bot
            .authorization()
            .contains("ncd-link:reverse-app-docker-bridge")
    );
    f.manager.unlink(&f.id).await.unwrap();
    assert!(f.manager.get_instance(&f.id).await.unwrap().link.is_none());
    assert!(f.bots.0.lock().await.connect.websocket_clients.is_empty());
    assert!(f.bot.active.lock().unwrap().is_empty());
    assert!(f.app.active.lock().unwrap().is_empty());
    assert_eq!(
        f.bot.authorization(),
        "ssh-ed25519 unrelated original-user-key\n"
    );
    assert!(
        !f.bot
            .dirs
            .lock()
            .unwrap()
            .contains(&f.bot.bridge_path(&f.id))
    );
    assert!(!f.app.dirs.lock().unwrap().contains(&format!(
        "{}/ncd/links/{}",
        f.app.home(),
        f.id.as_str()
    )));
}

#[tokio::test]
async fn docker_reverse_failed_relink_restores_previous_configuration_and_working_transports() {
    let f = Fixture::new("remote:app", BackendType::SnowLuma).await;
    let previous = f
        .manager
        .apply_link(&f.id, &BotId::new("10001"))
        .await
        .unwrap();
    let previous_bot = f.bots.0.lock().await.clone();
    let previous_env = f.app.text(&format!("{APP_DIR}/.env"));
    let previous_adapter = f
        .app
        .text(&format!("{APP_DIR}/@karinjs/config/adapter.json"));
    let previous_target = f.bot.bridge_state(&f.id)["target_port"].clone();
    f.bot.fail_bridge_once.store(true, Ordering::SeqCst);

    let error = f
        .manager
        .apply_link(&f.id, &BotId::new("10001"))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("bridge authentication rejected"));
    assert!(!error.to_string().contains("恢复原对接转发失败"), "{error}");
    let current = f.manager.get_instance(&f.id).await.unwrap();
    assert_eq!(
        current.link, previous.link,
        "失败重接不能替换旧 link 元数据"
    );
    assert_eq!(
        *f.bots.0.lock().await,
        previous_bot,
        "旧 Bot URL、令牌及其它配置应完整恢复"
    );
    assert_eq!(f.app.text(&format!("{APP_DIR}/.env")), previous_env);
    assert_eq!(
        f.app
            .text(&format!("{APP_DIR}/@karinjs/config/adapter.json")),
        previous_adapter
    );
    assert!(
        f.app
            .running_in(&format!("{}/ncd/links/{}", f.app.home(), f.id.as_str()))
    );
    assert!(f.bot.running_in(&f.bot.bridge_path(&f.id)));
    assert_eq!(f.bot.bridge_state(&f.id)["target_port"], previous_target);
    assert!(f.bot.authorization().contains("ncd-link:reverse-app\n"));
    assert!(
        f.bot
            .authorization()
            .contains("ncd-link:reverse-app-docker-bridge")
    );
}

#[tokio::test]
async fn docker_reverse_persisted_apply_failure_restores_bot_app_and_unlinked_state() {
    let f = Fixture::new("remote:app", BackendType::NapCat).await;
    let previous_bot = f.bots.0.lock().await.clone();
    let previous_env = f.app.text(&format!("{APP_DIR}/.env"));
    let previous_adapter = f
        .app
        .text(&format!("{APP_DIR}/@karinjs/config/adapter.json"));
    f.bots.1.store(true, Ordering::SeqCst);

    let error = f
        .manager
        .apply_link(&f.id, &BotId::new("10001"))
        .await
        .unwrap_err();
    assert!(error.to_string().contains("DOCKER_CONFIG_APPLY_FAILED"));
    assert_eq!(
        *f.bots.0.lock().await,
        previous_bot,
        "先落盘再失败也必须撤掉新建 ncd-app 连接"
    );
    assert!(f.manager.get_instance(&f.id).await.unwrap().link.is_none());
    assert_eq!(f.app.text(&format!("{APP_DIR}/.env")), previous_env);
    assert_eq!(
        f.app
            .text(&format!("{APP_DIR}/@karinjs/config/adapter.json")),
        previous_adapter
    );
    assert!(f.bot.active.lock().unwrap().is_empty());
    assert!(f.app.active.lock().unwrap().is_empty());
    assert_eq!(
        f.bot.authorization(),
        "ssh-ed25519 unrelated original-user-key\n"
    );
}

#[tokio::test]
async fn local_linux_docker_reverse_link_binds_gateway_without_ssh() {
    for backend in [BackendType::NapCat, BackendType::SnowLuma] {
        let f = Fixture::new(LOCAL_HOST_ID, backend).await;
        f.bots.0.lock().await.bot.runtime_target = RuntimeTarget::Local;
        f.manager
            .apply_link(&f.id, &BotId::new("10001"))
            .await
            .unwrap();
        let client = f.linked_client().await;
        let parts = parse_ws_url(&client.url).unwrap();
        assert_eq!(parts.host, GATEWAY);
        assert_eq!(parts.port, 41001);
        assert_eq!(parts.path, "/onebot/v11/ws");
        assert_eq!(client.base.token, TOKEN);
        let tunnels = f.local.tunnels.lock().unwrap();
        assert_eq!(tunnels.len(), 1);
        assert_eq!(tunnels[0].local_host, GATEWAY);
        assert_eq!(tunnels[0].remote_host, "127.0.0.1");
        assert_eq!(tunnels[0].remote_port, APP_PORT);
        assert!(
            f.local
                .commands
                .lock()
                .unwrap()
                .iter()
                .all(|command| !matches!(command.program.as_str(), "ssh" | "ssh-keygen"))
        );
        drop(tunnels);
        f.manager.unlink(&f.id).await.unwrap();
        assert!(
            !f.manager
                .tunnels
                .lock()
                .await
                .keys()
                .any(|key| key.contains(":docker-bridge:"))
        );
    }
}

#[tokio::test]
async fn podman_link_uses_its_own_network_schema_and_never_docker() {
    use ncd_domain::docker::ContainerEngine;
    let host = LinkHost::new("podman-link", false);
    let gateway = docker_link::gateway(host.as_ref(), ContainerEngine::Podman)
        .await
        .unwrap();
    assert_eq!(gateway.to_string(), "10.88.0.1");
    let commands = host.commands.lock().unwrap();
    assert!(
        commands
            .iter()
            .all(|c| c.program == "podman" && !c.elevated)
    );
    assert!(commands.iter().any(|c| c.args
        == [
            "network",
            "inspect",
            "podman",
            "--format",
            "{{json .Subnets}}"
        ]));
}

#[tokio::test]
async fn rootless_podman_link_does_not_bind_an_unreachable_host_bridge_or_elevate() {
    use ncd_domain::docker::ContainerEngine;
    let host = LinkHost::new("podman-rootless-link", false);
    host.podman_rootless.store(true, Ordering::SeqCst);
    let error = docker_link::gateway(host.as_ref(), ContainerEngine::Podman)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("rootless"));
    let commands = host.commands.lock().unwrap();
    assert!(
        commands
            .iter()
            .all(|c| c.program == "podman" && !c.elevated)
    );
    assert!(
        !commands
            .iter()
            .any(|c| c.args.first().is_some_and(|arg| arg == "network"))
    );
}
