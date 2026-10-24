use super::qr_relogin::QrReloginPreparation;
use super::*;

use std::path::Path;
use std::sync::Mutex as StdMutex;
use std::sync::atomic::AtomicUsize;

use crate::remote_snowluma::layout::RemoteSnowLumaLayout;
use crate::snowluma::error::SnowLumaWebUiError;
use crate::snowluma::webui_client::{
    AgreementsPayload, AuthState, HookProcessInfo, HookProcessStatus, OneBotInstanceInfo,
    QqPortLoginInfo,
};
use bytes::Bytes;
use ncd_host::remote::tunnel::{TunnelHandle, TunnelSpec};
use ncd_host::shell::BashShell;
use ncd_host::{
    Arch, ArchiveKind, CommandOutput, DirEntry, HostCommand, HostError, HostProcess, HostShell,
    Locality, Os,
};

const BOT: &str = "2550419068";
const OTHER_BOT: &str = "2707600964";
const OLD_PID: u32 = 11;
const NEW_PID: u32 = 22;
const OTHER_PID: u32 = 33;

struct FakeProcess {
    birth: &'static str,
    argv: Vec<u8>,
}

struct ReloginHost {
    processes: StdMutex<HashMap<u32, FakeProcess>>,
    files: StdMutex<HashMap<String, Vec<u8>>>,
    candidate: u32,
    close_on_bind: StdMutex<Option<Arc<QrActivity>>>,
    captures: AtomicUsize,
    webui_port: AtomicU32,
}

impl ReloginHost {
    fn new(candidate: u32, candidate_uin: &str) -> Self {
        let process = |birth, uin: &str| FakeProcess {
            birth,
            argv: format!("/opt/QQ/qq\0-q\0{uin}\0--no-sandbox\0").into_bytes(),
        };
        let mut processes = HashMap::from([
            (OLD_PID, process("111", BOT)),
            (OTHER_PID, process("333", OTHER_BOT)),
        ]);
        if candidate != OLD_PID {
            processes.insert(candidate, process("222", candidate_uin));
        }
        Self {
            processes: StdMutex::new(processes),
            files: StdMutex::new(HashMap::new()),
            candidate,
            close_on_bind: StdMutex::new(None),
            captures: AtomicUsize::new(0),
            webui_port: AtomicU32::new(0),
        }
    }
}

#[async_trait]
impl Host for ReloginHost {
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
        "qr-relogin-test"
    }
    fn shell(&self) -> &dyn HostShell {
        &BashShell
    }

    async fn read_file(&self, path: &HostPath) -> Result<Bytes, HostError> {
        if let Some(rest) = path.as_posix().strip_prefix("/proc/")
            && let Some((pid, name)) = rest.split_once('/')
            && let Ok(pid) = pid.parse::<u32>()
            && let Some(process) = self.processes.lock().unwrap().get(&pid)
        {
            return match name {
                "cmdline" => Ok(Bytes::from(process.argv.clone())),
                "stat" => {
                    let mut fields = vec!["S"; 20];
                    fields[19] = process.birth;
                    Ok(Bytes::from(format!("{pid} (qq) {}", fields.join(" "))))
                }
                _ => Err(HostError::PathNotFound { path: path.clone() }),
            };
        }
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
            .insert(path.as_posix().into(), bytes.to_vec());
        Ok(())
    }
    async fn list_dir(&self, _: &HostPath) -> Result<Vec<DirEntry>, HostError> {
        Ok(vec![])
    }
    async fn create_dir_all(&self, _: &HostPath) -> Result<(), HostError> {
        Ok(())
    }
    async fn remove_file(&self, path: &HostPath) -> Result<(), HostError> {
        self.files.lock().unwrap().remove(path.as_posix());
        Ok(())
    }
    async fn remove_dir_all(&self, _: &HostPath) -> Result<(), HostError> {
        panic!("relogin must not remove unrelated login sessions")
    }
    async fn exists(&self, path: &HostPath) -> Result<bool, HostError> {
        if let Some(pid) = path.as_posix().strip_prefix("/proc/")
            && let Ok(pid) = pid.parse::<u32>()
        {
            return Ok(self.processes.lock().unwrap().contains_key(&pid));
        }
        Ok(self.files.lock().unwrap().contains_key(path.as_posix()))
    }
    async fn upload(&self, _: &Path, _: &HostPath) -> Result<(), HostError> {
        panic!("relogin must not install components")
    }
    async fn download(&self, _: &HostPath, _: &Path) -> Result<(), HostError> {
        panic!("unexpected download")
    }
    async fn extract_archive(
        &self,
        _: &HostPath,
        _: &HostPath,
        _: ArchiveKind,
    ) -> Result<(), HostError> {
        panic!("relogin must not install components")
    }
    async fn spawn(&self, _: HostCommand) -> Result<Box<dyn HostProcess>, HostError> {
        panic!("relogin must not spawn or restart QQ")
    }
    async fn which(&self, name: &str) -> Result<Option<String>, HostError> {
        match name {
            "python3" => Ok(Some("/usr/bin/python3".into())),
            "bash" => Ok(Some("/bin/bash".into())),
            _ => panic!("unexpected executable lookup: {name}"),
        }
    }
    async fn open_tunnel(&self, spec: TunnelSpec) -> Result<TunnelHandle, HostError> {
        let port = self.webui_port.load(Ordering::SeqCst) as u16;
        assert_ne!(port, 0, "only namespace fixtures open virtual tunnels");
        Ok(TunnelHandle::detached(
            if spec.remote_port == 13105 {
                port
            } else {
                31313
            },
            0,
        ))
    }
    async fn run_to_string(&self, command: HostCommand) -> Result<CommandOutput, HostError> {
        if command.program == "head" {
            assert_eq!(&command.args[..3], ["-c", "4097", "--"]);
            assert_eq!(command.timeout, Some(Duration::from_secs(3)));
            let bytes = self
                .read_file(&HostPath::from_posix(command.args.last().unwrap()))
                .await?;
            return Ok(CommandOutput {
                exit_code: Some(0),
                stdout: String::from_utf8_lossy(&bytes).into_owned(),
                stderr: String::new(),
            });
        }
        let stdout = match command.program.as_str() {
            "id" => "1000\n".into(),
            "chmod" => String::new(),
            "sh" => {
                let script = command.args.last().unwrap();
                if script.starts_with("# ncd-snowluma-node-identity") {
                    return Ok(CommandOutput {
                        exit_code: Some(0),
                        stdout: "901:999\n".into(),
                        stderr: String::new(),
                    });
                }
                if script.contains("ss -ltnp") && script.contains("awk '/websockify/") {
                    assert_ne!(self.webui_port.load(Ordering::SeqCst), 0);
                    return Ok(CommandOutput {
                        exit_code: Some(0),
                        stdout: "6081\n".into(),
                        stderr: String::new(),
                    });
                }
                assert!(script.contains("allow_single='0'"));
                assert!(script.contains("pidfile=''"));
                format!("{}\n", self.candidate)
            }
            "readlink" => {
                assert_eq!(command.args[2], "/opt/QQ/qq");
                assert_eq!(command.args[3], format!("/proc/{}/exe", self.candidate));
                "/opt/QQ/qq\n/opt/QQ/qq\n".into()
            }
            "/bin/bash" => {
                let script = command.args.last().unwrap();
                assert!(!script.contains("nohup"));
                assert!(!script.contains("kill "));
                String::new()
            }
            "python3"
                if command
                    .args
                    .iter()
                    .any(|arg| arg.starts_with("# Read only same-user Node identities;")) =>
            {
                let mode = command.args[4].as_str();
                if mode == "snapshot" {
                    serde_json::json!({"state":"snapshot","snapshot":[{"pid":901,"start_time":"999"}]}).to_string()
                } else {
                    let directory = &command.args[7];
                    serde_json::json!({
                        "state":"present", "node": {
                            "pid":901,"uid":1000,"start_time":"999","exe":command.args[6],
                            "cwd":directory,"entry":format!("{directory}/index.mjs"),
                            "ns_pids":[901,19],"pid_namespace":"pid:[42]"
                        }, "snapshot":[{"pid":901,"start_time":"999"}],
                        "host":"127.0.0.1","port":13105,
                        "listeners":[{"host":"127.0.0.1","port":13105}]
                    })
                    .to_string()
                }
            }
            "python3"
                if command
                    .args
                    .iter()
                    .any(|arg| arg.starts_with("# Map two bound process identities")) =>
            {
                let request: serde_json::Value =
                    serde_json::from_str(command.args.last().unwrap()).unwrap();
                assert_eq!(request["qq"]["pid"], NEW_PID);
                assert_eq!(request["qq"]["start_time"], "222");
                serde_json::json!({"state":"mapped","process_pid":46,"snapshot":{
                    "node":{"pid":901,"uid":1000,"start_time":"999","ns_pids":[901,19]},
                    "qq":{"pid":NEW_PID,"uid":1000,"start_time":"222","ns_pids":[NEW_PID,46]}
                }})
                .to_string()
            }
            "/usr/bin/python3" if command.args.iter().any(|arg| arg == "--probe") => {
                r#"{"available":true}"#.into()
            }
            "/usr/bin/python3" if command.args.iter().any(|arg| arg == "--bind") => {
                if let Some(activity) = self.close_on_bind.lock().unwrap().as_ref() {
                    activity.close_revision.fetch_add(1, Ordering::SeqCst);
                    activity.cancel.cancel();
                }
                let position = command.args.iter().position(|arg| arg == "--pid").unwrap();
                let pid = command.args[position + 1].parse::<u32>().unwrap();
                let processes = self.processes.lock().unwrap();
                let process = processes.get(&pid).expect("only bind a live QQ process");
                serde_json::json!({ "binding": {
                    "pid": pid, "start_time": process.birth, "exe": "/opt/QQ/qq", "uid": 1000,
                    "display": ":0", "bus": "unix:path=/run/user/1000/bus",
                    "xauthority": null, "runtime_dir": null
                }})
                .to_string()
            }
            "/usr/bin/python3" => {
                self.captures.fetch_add(1, Ordering::SeqCst);
                let request: serde_json::Value = serde_json::from_slice(
                    command.stdin.as_ref().expect("private capture request"),
                )
                .unwrap();
                assert!(matches!(
                    request["action"].as_str(),
                    Some("refresh" | "snapshot")
                ));
                serde_json::json!({
                    "session_id": request["session_id"], "v": 1, "state": "waiting_for_qr",
                    "qq_pid": request["binding"]["pid"],
                    "qrcode_payload": "data:image/png;base64,YQ==",
                    "uin": null, "observer_removed": true, "message": null
                })
                .to_string()
            }
            _ => panic!(
                "recovery must not invoke other processes: {}",
                command.program
            ),
        };
        Ok(CommandOutput {
            exit_code: Some(0),
            stdout,
            stderr: String::new(),
        })
    }
}

struct ReloginClient {
    processes: StdMutex<Vec<HookProcessInfo>>,
    loads: StdMutex<Vec<u32>>,
    close_on_load: Option<Arc<QrActivity>>,
}

#[async_trait]
impl SnowLumaWebUiClient for ReloginClient {
    async fn wait_ready(
        &self,
        _: Duration,
        _: Box<dyn Fn() -> bool + Send + Sync>,
    ) -> Result<(), SnowLumaWebUiError> {
        Ok(())
    }
    async fn login(&self) -> Result<(), SnowLumaWebUiError> {
        Ok(())
    }
    async fn logout(&self) -> Result<(), SnowLumaWebUiError> {
        panic!("do not log out shared WebUI")
    }
    async fn list_processes(&self) -> Result<Vec<HookProcessInfo>, SnowLumaWebUiError> {
        Ok(self.processes.lock().unwrap().clone())
    }
    async fn list_qq_instances(&self) -> Result<Vec<OneBotInstanceInfo>, SnowLumaWebUiError> {
        Ok(vec![])
    }
    async fn probe_process_login_info(
        &self,
        _: u32,
    ) -> Result<Option<QqPortLoginInfo>, SnowLumaWebUiError> {
        Ok(None)
    }
    async fn load_process(&self, pid: u32) -> Result<HookProcessInfo, SnowLumaWebUiError> {
        self.loads.lock().unwrap().push(pid);
        if let Some(activity) = &self.close_on_load {
            activity.close_revision.fetch_add(1, Ordering::SeqCst);
            activity.cancel.cancel();
        }
        let mut processes = self.processes.lock().unwrap();
        let process = processes
            .iter_mut()
            .find(|process| process.pid == pid)
            .unwrap();
        process.injected = Some(true);
        process.connected = Some(true);
        process.status = HookProcessStatus::Loaded;
        Ok(process.clone())
    }
    async fn unload_process(&self, _: u32) -> Result<HookProcessInfo, SnowLumaWebUiError> {
        panic!("do not unload QQ")
    }
    async fn get_auth_state(&self) -> Result<AuthState, SnowLumaWebUiError> {
        panic!("unexpected auth request")
    }
    async fn get_agreements(&self) -> Result<AgreementsPayload, SnowLumaWebUiError> {
        panic!("unexpected agreements request")
    }
    async fn record_agreement_consent(&self, _: &str) -> Result<(), SnowLumaWebUiError> {
        panic!("unexpected consent change")
    }
    async fn update_onebot_config(
        &self,
        _: &str,
        _: &serde_json::Value,
    ) -> Result<bool, SnowLumaWebUiError> {
        panic!("do not change OneBot configuration")
    }
}

fn process(pid: u32, online: bool) -> HookProcessInfo {
    serde_json::from_value(serde_json::json!({
        "pid": pid, "name": "qq", "path": "/opt/QQ/qq",
        "uin": if online { BOT } else { "0" },
        "status": if online { "online" } else { "available" },
        "loggedIn": online, "connected": online, "injected": online
    }))
    .unwrap()
}

struct Fixture {
    backend: RemoteSnowLumaBackend,
    host: Arc<ReloginHost>,
    client: Arc<ReloginClient>,
    tracked: TrackedQq,
    activity: Arc<QrActivity>,
    other_activity: Arc<QrActivity>,
    _snowluma_root: tempfile::TempDir,
}

async fn fixture(
    candidate: u32,
    candidate_uin: &str,
    online: bool,
    close_on_load: bool,
) -> Fixture {
    let host = Arc::new(ReloginHost::new(candidate, candidate_uin));
    let bus = Arc::new(BroadcastEventBus::default());
    let tunnels = Arc::new(RemoteSnowLumaTunnelRegistry::new());
    let layout = RemoteSnowLumaLayout {
        home: "/home/test".into(),
        paths: SnowLumaRemotePaths::from_remote_home("/home/test"),
        node_bin: "/opt/snowluma/node".into(),
        qq_bin: "/opt/QQ/qq".into(),
        qq_install_base: "/".into(),
    };
    let old_session = LoginSession::attach_running(
        host.as_ref(),
        &layout.paths,
        &layout,
        &BotId::new(BOT),
        OLD_PID,
        "111",
        CancellationToken::new(),
    )
    .await
    .unwrap()
    .unwrap();
    remember_remote_bot_pid(host.as_ref(), &layout.paths, BOT, OLD_PID)
        .await
        .unwrap();
    let daemon = Arc::new(RemoteSnowLumaDaemon::from_layout(
        "server".into(),
        host.clone(),
        tunnels.clone(),
        bus.clone(),
        layout,
    ));
    let snowluma_root = tempfile::tempdir().unwrap();
    let backend = RemoteSnowLumaBackend::new(
        "server",
        daemon,
        bus,
        tunnels,
        Arc::new(ncd_deploy::remote_coordinator::RemoteQqEntryCoordinator::default()),
    )
    .with_snowluma_data_root(snowluma_root.path());
    let tracked = TrackedQq {
        pid: OLD_PID,
        start_time: "111".into(),
    };
    let activity = Arc::new(QrActivity::new());
    activity.scanned.store(true, Ordering::SeqCst);
    activity.login_detected.store(true, Ordering::SeqCst);
    activity.cancel.cancel();
    let mut terminal = SnowLumaQrLoginSnapshot::unavailable("previous login");
    terminal.state = SnowLumaQrLoginState::LoggedIn;
    terminal.qq_pid = Some(OLD_PID);
    terminal.uin = Some(BOT.into());
    *activity.terminal.lock().await = Some(terminal);
    let other_activity = Arc::new(QrActivity::new());
    let mut other_process = process(OTHER_PID, true);
    other_process.uin = OTHER_BOT.into();
    let client = Arc::new(ReloginClient {
        processes: StdMutex::new(vec![process(candidate, online), other_process]),
        loads: StdMutex::new(vec![]),
        close_on_load: close_on_load.then(|| activity.clone()),
    });
    backend.login_pids.lock().await.extend([
        (BotId::new(BOT), tracked.clone()),
        (
            BotId::new(OTHER_BOT),
            TrackedQq {
                pid: OTHER_PID,
                start_time: "333".into(),
            },
        ),
    ]);
    backend.qr_activity.lock().await.extend([
        (BotId::new(BOT), activity.clone()),
        (BotId::new(OTHER_BOT), other_activity.clone()),
    ]);
    backend
        .login_sessions
        .lock()
        .await
        .insert(BotId::new(BOT), old_session);
    backend
        .start_modes
        .lock()
        .await
        .insert(BotId::new(BOT), SnowLumaStartMode::ColdStart);
    backend
        .login_clients
        .lock()
        .await
        .insert(BotId::new(BOT), client.clone());
    if candidate != OLD_PID {
        host.processes.lock().unwrap().remove(&OLD_PID);
    }
    Fixture {
        backend,
        host,
        client,
        tracked,
        activity,
        other_activity,
        _snowluma_root: snowluma_root,
    }
}

async fn namespace_fixture(
    close_during_bind: bool,
) -> (Fixture, wiremock::MockServer, Arc<dyn SnowLumaWebUiClient>) {
    use wiremock::matchers::{body_json, header, method, path};
    use wiremock::{Mock, MockServer, Request, ResponseTemplate};

    let value = fixture(NEW_PID, BOT, false, false).await;
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/status"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/login"))
        .and(body_json(
            serde_json::json!({"password":"fixture-password"}),
        ))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"token":"fixture-token"})),
        )
        .mount(&server)
        .await;
    let loaded = Arc::new(AtomicBool::new(false));
    Mock::given(method("GET"))
        .and(path("/api/processes"))
        .and(header("authorization", "Bearer fixture-token"))
        .respond_with({
            let loaded = Arc::clone(&loaded);
            move |_: &Request| {
                let mut target = process(46, false);
                if loaded.load(Ordering::SeqCst) {
                    target.status = HookProcessStatus::Loaded;
                    target.injected = Some(true);
                    target.connected = Some(true);
                }
                ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "list":[target, process(NEW_PID, true)]
                }))
            }
        })
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/processes/46/load"))
        .and(header("authorization", "Bearer fixture-token"))
        .respond_with(move |_: &Request| {
            loaded.store(true, Ordering::SeqCst);
            let mut target = process(46, false);
            target.status = HookProcessStatus::Loaded;
            target.injected = Some(true);
            target.connected = Some(true);
            ResponseTemplate::new(200)
                .set_body_json(serde_json::json!({"success":true,"process":target}))
        })
        .expect(1)
        .mount(&server)
        .await;
    value
        .host
        .webui_port
        .store(u32::from(server.address().port()), Ordering::SeqCst);
    {
        let mut files = value.host.files.lock().unwrap();
        files.insert(
            value.backend.daemon.paths().webui_secret.clone(),
            b"fixture-password".to_vec(),
        );
        files.insert(
            value.backend.daemon.paths().vnc_secret.clone(),
            b"fixture-vnc".to_vec(),
        );
    }
    value
        .backend
        .daemon
        .ensure_running_for_reconcile_bot(&BotId::new(BOT))
        .await
        .unwrap();
    *value.client.processes.lock().unwrap() = vec![process(45, false), process(46, false)];
    let old_client: Arc<dyn SnowLumaWebUiClient> = Arc::new(
        super::super::mapped_webui_client::MappedSnowLumaWebUiClient::new(
            value.client.clone(),
            OLD_PID,
            45,
        )
        .unwrap(),
    );
    value
        .backend
        .login_clients
        .lock()
        .await
        .insert(BotId::new(BOT), Arc::clone(&old_client));
    if close_during_bind {
        *value.host.close_on_bind.lock().unwrap() = Some(Arc::clone(&value.activity));
    }
    (value, server, old_client)
}

#[tokio::test]
async fn namespaced_qq_relogin_maps_the_replacement_pid_and_publishes_only_after_binding() {
    let (value, _server, old_client) = namespace_fixture(false).await;
    let bot = BotId::new(BOT);
    let prepared = value
        .backend
        .prepare_qr_relogin(&bot, &value.tracked, &value.activity, 0)
        .await
        .unwrap();
    let QrReloginPreparation::Bound(current, renewed) = prepared else {
        panic!("a verified replacement must not remain hidden by the old mapped client");
    };
    assert_eq!(current.pid, NEW_PID);
    assert_eq!(current.start_time, "222");
    assert!(!renewed.cancel.is_cancelled());
    let client = value
        .backend
        .login_clients
        .lock()
        .await
        .get(&bot)
        .cloned()
        .unwrap();
    assert!(!Arc::ptr_eq(&client, &old_client));
    let processes = client.list_processes().await.unwrap();
    assert_eq!(processes.len(), 1);
    assert_eq!(processes[0].pid, NEW_PID);
    assert_eq!(processes[0].status, HookProcessStatus::Loaded);
    assert!(
        value.client.loads.lock().unwrap().is_empty(),
        "old mapping must never receive the new host PID"
    );
    assert!(!value.other_activity.cancel.is_cancelled());
    value.backend.detach_login_pollers().await;
}

#[tokio::test]
async fn namespaced_qq_relogin_closed_during_binding_keeps_the_old_client_and_pid() {
    let (value, _server, old_client) = namespace_fixture(true).await;
    let bot = BotId::new(BOT);
    let prepared = value
        .backend
        .prepare_qr_relogin(&bot, &value.tracked, &value.activity, 0)
        .await
        .unwrap();
    assert!(matches!(prepared, QrReloginPreparation::Unchanged));
    assert_eq!(
        value.backend.login_pids.lock().await.get(&bot).unwrap().pid,
        OLD_PID
    );
    assert!(Arc::ptr_eq(
        value.backend.login_clients.lock().await.get(&bot).unwrap(),
        &old_client
    ));
    assert!(value.client.loads.lock().unwrap().is_empty());
    assert!(!value.other_activity.cancel.is_cancelled());
    value.backend.detach_login_pollers().await;
}

#[tokio::test]
async fn opening_qr_after_phone_logout_recovers_the_new_process_and_captures_a_code() {
    let value = fixture(NEW_PID, BOT, false, false).await;
    let bot = BotId::new(BOT);
    let pending = value.backend.qr_login_snapshot(&bot, true).await;
    assert_eq!(pending.state, SnowLumaQrLoginState::Starting);
    assert!(pending.qrcode_payload.is_none());
    assert_eq!(value.host.captures.load(Ordering::SeqCst), 0);
    assert_eq!(*value.client.loads.lock().unwrap(), vec![NEW_PID]);
    assert_eq!(
        value.backend.login_pids.lock().await.get(&bot).unwrap().pid,
        OLD_PID
    );
    let snapshot = value.backend.qr_login_snapshot(&bot, false).await;
    assert_eq!(snapshot.state, SnowLumaQrLoginState::WaitingForQr);
    assert_eq!(snapshot.qq_pid, Some(NEW_PID));
    assert!(
        snapshot
            .qrcode_payload
            .as_deref()
            .is_some_and(|payload| { payload.starts_with("data:image/png;base64,") })
    );
    assert_eq!(*value.client.loads.lock().unwrap(), vec![NEW_PID]);
    assert_eq!(
        value.backend.login_pids.lock().await.get(&bot).unwrap().pid,
        NEW_PID
    );
    assert!(!value.other_activity.cancel.is_cancelled());
    value.backend.detach_login_pollers().await;
}

#[tokio::test]
async fn exited_qq_rebinds_and_injects_only_its_new_process_once() {
    let value = fixture(NEW_PID, BOT, false, false).await;
    let bot = BotId::new(BOT);
    let pending = value
        .backend
        .prepare_qr_relogin(&bot, &value.tracked, &value.activity, 0)
        .await
        .unwrap();
    assert!(matches!(pending, QrReloginPreparation::Waiting));
    assert_eq!(*value.client.loads.lock().unwrap(), vec![NEW_PID]);
    assert_eq!(
        value.backend.login_pids.lock().await.get(&bot).unwrap().pid,
        OLD_PID
    );
    let prepared = value
        .backend
        .prepare_qr_relogin(&bot, &value.tracked, &value.activity, 0)
        .await
        .unwrap();
    let QrReloginPreparation::Bound(current, renewed) = prepared else {
        panic!("new QQ process must be recovered");
    };
    assert_eq!(current.pid, NEW_PID);
    assert_eq!(current.start_time, "222");
    assert!(!Arc::ptr_eq(&renewed, &value.activity));
    assert!(!renewed.scanned.load(Ordering::SeqCst));
    assert!(!renewed.login_detected.load(Ordering::SeqCst));
    assert!(!renewed.cancel.is_cancelled());
    assert!(renewed.terminal.lock().await.is_none());
    assert!(value.activity.cancel.is_cancelled());
    assert_eq!(*value.client.loads.lock().unwrap(), vec![NEW_PID]);
    let session = value
        .backend
        .login_sessions
        .lock()
        .await
        .get(&bot)
        .cloned()
        .unwrap();
    assert_eq!(session.pid(), NEW_PID);
    assert_eq!(session.start_time(), Some("222"));
    assert!(value.backend.pollers.lock().await.contains_key(&bot));
    let paths = value.backend.daemon.paths();
    let pidfile = value
        .host
        .read_file(&HostPath::from_posix(paths.pid_bot_path(BOT)))
        .await
        .unwrap();
    assert_eq!(pidfile.as_ref(), b"22\n");
    let session_path = format!("{}/login-assist/{BOT}/session.json", paths.runtime_dir);
    let bytes = value
        .host
        .read_file(&HostPath::from_posix(session_path))
        .await
        .unwrap();
    let saved: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(saved["qq_pid"], NEW_PID);
    assert_eq!(saved["binding"]["start_time"], "222");
    assert!(Arc::ptr_eq(
        value
            .backend
            .qr_activity
            .lock()
            .await
            .get(&BotId::new(OTHER_BOT))
            .unwrap(),
        &value.other_activity,
    ));
    assert!(!value.other_activity.cancel.is_cancelled());
    assert_eq!(
        value
            .backend
            .login_pids
            .lock()
            .await
            .get(&BotId::new(OTHER_BOT))
            .unwrap()
            .pid,
        OTHER_PID
    );
    assert!(matches!(
        value
            .backend
            .prepare_qr_relogin(&bot, &current, &renewed, 0)
            .await
            .unwrap(),
        QrReloginPreparation::Unchanged
    ));
    assert_eq!(*value.client.loads.lock().unwrap(), vec![NEW_PID]);
    value.backend.detach_login_pollers().await;
}

#[tokio::test]
async fn logged_in_current_process_keeps_its_session_and_does_not_load() {
    let value = fixture(OLD_PID, BOT, true, false).await;
    let files = value.host.files.lock().unwrap().clone();
    let bot = BotId::new(BOT);
    assert!(matches!(
        value
            .backend
            .prepare_qr_relogin(&bot, &value.tracked, &value.activity, 0)
            .await
            .unwrap(),
        QrReloginPreparation::Unchanged
    ));
    assert!(value.client.loads.lock().unwrap().is_empty());
    assert_eq!(*value.host.files.lock().unwrap(), files);
    assert!(Arc::ptr_eq(
        value.backend.qr_activity.lock().await.get(&bot).unwrap(),
        &value.activity
    ));
    assert_eq!(
        value
            .backend
            .login_sessions
            .lock()
            .await
            .get(&bot)
            .unwrap()
            .pid(),
        OLD_PID
    );
}

#[tokio::test]
async fn a_new_process_with_another_account_argument_is_not_adopted() {
    let value = fixture(NEW_PID, OTHER_BOT, false, false).await;
    let files = value.host.files.lock().unwrap().clone();
    let bot = BotId::new(BOT);
    assert!(
        value
            .backend
            .prepare_qr_relogin(&bot, &value.tracked, &value.activity, 0)
            .await
            .is_err()
    );
    assert!(value.client.loads.lock().unwrap().is_empty());
    assert_eq!(*value.host.files.lock().unwrap(), files);
    assert_eq!(
        value.backend.login_pids.lock().await.get(&bot).unwrap().pid,
        OLD_PID
    );
    assert_eq!(
        value
            .backend
            .login_sessions
            .lock()
            .await
            .get(&bot)
            .unwrap()
            .pid(),
        OLD_PID
    );
    assert!(Arc::ptr_eq(
        value.backend.qr_activity.lock().await.get(&bot).unwrap(),
        &value.activity
    ));
}

#[tokio::test]
async fn closing_during_recovery_cannot_register_or_persist_the_new_pid() {
    let value = fixture(NEW_PID, BOT, false, true).await;
    let bot = BotId::new(BOT);
    assert!(matches!(
        value
            .backend
            .prepare_qr_relogin(&bot, &value.tracked, &value.activity, 0)
            .await
            .unwrap(),
        QrReloginPreparation::Unchanged
    ));
    assert_eq!(
        value.backend.login_pids.lock().await.get(&bot).unwrap().pid,
        OLD_PID
    );
    assert_eq!(
        value
            .backend
            .login_sessions
            .lock()
            .await
            .get(&bot)
            .unwrap()
            .pid(),
        OLD_PID
    );
    assert!(Arc::ptr_eq(
        value.backend.qr_activity.lock().await.get(&bot).unwrap(),
        &value.activity
    ));
    assert!(!value.backend.pollers.lock().await.contains_key(&bot));
    let pidfile = value
        .host
        .read_file(&HostPath::from_posix(
            value.backend.daemon.paths().pid_bot_path(BOT),
        ))
        .await
        .unwrap();
    assert_eq!(pidfile.as_ref(), b"11\n");
    assert!(!value.other_activity.cancel.is_cancelled());
}

#[tokio::test]
async fn discovered_qq_waits_for_its_own_webui_process_before_binding_or_injection() {
    let value = fixture(NEW_PID, BOT, false, false).await;
    value
        .client
        .processes
        .lock()
        .unwrap()
        .retain(|process| process.pid != NEW_PID);
    let files = value.host.files.lock().unwrap().clone();
    let bot = BotId::new(BOT);
    let prepared = value
        .backend
        .prepare_qr_relogin(&bot, &value.tracked, &value.activity, 0)
        .await
        .unwrap();
    assert!(matches!(prepared, QrReloginPreparation::Waiting));
    assert!(value.client.loads.lock().unwrap().is_empty());
    assert_eq!(*value.host.files.lock().unwrap(), files);
    assert_eq!(
        value.backend.login_pids.lock().await.get(&bot).unwrap().pid,
        OLD_PID
    );
    assert_eq!(
        value
            .backend
            .login_sessions
            .lock()
            .await
            .get(&bot)
            .unwrap()
            .pid(),
        OLD_PID
    );
    let snapshot = value.backend.qr_login_snapshot(&bot, true).await;
    assert_eq!(snapshot.state, SnowLumaQrLoginState::Starting);
    assert!(snapshot.qrcode_payload.is_none());
    assert_eq!(value.host.captures.load(Ordering::SeqCst), 0);
    assert!(!value.other_activity.cancel.is_cancelled());
}

#[tokio::test]
async fn closing_during_process_binding_stops_injection_and_cannot_capture_a_code() {
    let value = fixture(NEW_PID, BOT, false, false).await;
    {
        let mut processes = value.client.processes.lock().unwrap();
        let process = processes
            .iter_mut()
            .find(|process| process.pid == NEW_PID)
            .unwrap();
        process.status = HookProcessStatus::Loaded;
        process.injected = Some(true);
        process.connected = Some(true);
    }
    *value.host.close_on_bind.lock().unwrap() = Some(value.activity.clone());
    let bot = BotId::new(BOT);
    let snapshot = value.backend.qr_login_snapshot(&bot, true).await;
    assert_ne!(snapshot.state, SnowLumaQrLoginState::WaitingForQr);
    assert!(snapshot.qrcode_payload.is_none());
    assert!(value.client.loads.lock().unwrap().is_empty());
    assert_eq!(value.host.captures.load(Ordering::SeqCst), 0);
    assert_eq!(
        value.backend.login_pids.lock().await.get(&bot).unwrap().pid,
        OLD_PID
    );
    assert_eq!(
        value
            .backend
            .login_sessions
            .lock()
            .await
            .get(&bot)
            .unwrap()
            .pid(),
        OLD_PID
    );
    assert!(Arc::ptr_eq(
        value.backend.qr_activity.lock().await.get(&bot).unwrap(),
        &value.activity
    ));
    assert!(!value.backend.pollers.lock().await.contains_key(&bot));
    assert!(!value.other_activity.cancel.is_cancelled());
}
