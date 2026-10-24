use std::collections::VecDeque;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use ncd_host::shell::BashShell;
use ncd_host::{
    Arch, ArchiveKind, CommandOutput, DirEntry, Host, HostCommand, HostError, HostPath,
    HostProcess, HostShell, Locality, Os,
};
use tokio::sync::Notify;

use super::launch::{remote_napcat_running_pid, stop_remote_napcat_on_host};
use super::log_io::{LogCursor, read_log};
use super::session::RemoteNativeNapcatSessionRegistry;

enum Reply {
    Output(String),
    Hold(Arc<Notify>, Arc<Notify>, String),
}

#[derive(Default)]
struct ScriptHost {
    replies: Mutex<VecDeque<Reply>>,
    commands: Mutex<Vec<HostCommand>>,
}

impl ScriptHost {
    fn outputs(values: &[&str]) -> Self {
        Self {
            replies: Mutex::new(
                values
                    .iter()
                    .map(|value| Reply::Output((*value).to_owned()))
                    .collect(),
            ),
            ..Self::default()
        }
    }
    fn count(&self) -> usize {
        self.commands.lock().unwrap().len()
    }
}

#[async_trait]
impl Host for ScriptHost {
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
        "nc-test-host"
    }
    fn shell(&self) -> &dyn HostShell {
        &BashShell
    }
    async fn run_to_string(&self, command: HostCommand) -> Result<CommandOutput, HostError> {
        self.commands.lock().unwrap().push(command);
        let reply = self
            .replies
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected remote operation");
        let stdout = match reply {
            Reply::Output(value) => value,
            Reply::Hold(entered, release, value) => {
                entered.notify_one();
                release.notified().await;
                value
            }
        };
        Ok(CommandOutput {
            exit_code: Some(0),
            stdout,
            stderr: String::new(),
        })
    }
    async fn read_file(&self, _: &HostPath) -> Result<Bytes, HostError> {
        panic!("whole-file SFTP log reads are forbidden")
    }
    async fn write_file(&self, _: &HostPath, _: &[u8]) -> Result<(), HostError> {
        panic!("unexpected write")
    }
    async fn list_dir(&self, _: &HostPath) -> Result<Vec<DirEntry>, HostError> {
        panic!("unexpected listing")
    }
    async fn create_dir_all(&self, _: &HostPath) -> Result<(), HostError> {
        panic!("unexpected mkdir")
    }
    async fn remove_file(&self, _: &HostPath) -> Result<(), HostError> {
        panic!("unexpected remove")
    }
    async fn remove_dir_all(&self, _: &HostPath) -> Result<(), HostError> {
        panic!("unexpected remove")
    }
    async fn exists(&self, _: &HostPath) -> Result<bool, HostError> {
        panic!("unexpected exists")
    }
    async fn upload(&self, _: &Path, _: &HostPath) -> Result<(), HostError> {
        panic!("unexpected upload")
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
        panic!("unexpected extraction")
    }
    async fn spawn(&self, _: HostCommand) -> Result<Box<dyn HostProcess>, HostError> {
        panic!("unexpected spawn")
    }
}

fn config(qq_id: u64) -> ncd_domain::BotConfig {
    ncd_test_support::BotConfigBuilder::new()
        .qq_id(qq_id)
        .runtime_target(ncd_domain::RuntimeTarget::Server("nc-server".into()))
        .build()
}

fn selected(base: &str) -> ncd_domain::RemoteSelectedPaths {
    ncd_domain::RemoteSelectedPaths {
        home: "/home/test".into(),
        qq_install_base: Some(base.into()),
        ..Default::default()
    }
}

#[tokio::test]
async fn log_seed_and_poll_use_bounded_chunks_and_resume_at_remote_offsets() {
    let host = ScriptHost::outputs(&["1:2 10000000 4\nseed", "1:2 10000004 3\nnew"]);
    let first = read_log(&host, "/data/a 'quoted/log", &LogCursor::default(), true)
        .await
        .unwrap()
        .unwrap();
    let next = read_log(&host, "/data/a 'quoted/log", &first.cursor, false)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(next.text, "new");
    assert!(!next.reset);
    let calls = host.commands.lock().unwrap();
    assert_eq!(calls[0].args[4], "/data/a 'quoted/log");
    assert_eq!(calls[0].args[7], "1");
    assert_eq!(calls[1].args[5], "10000004");
    assert_eq!(calls[1].args[7], "0");
    assert_eq!(calls[1].args[8], "524288");
    assert_eq!(calls[1].timeout, Some(Duration::from_secs(8)));
}

#[tokio::test]
async fn repeated_session_refresh_reuses_context_but_host_replacement_reopens() {
    let registry = RemoteNativeNapcatSessionRegistry::new();
    let host = Arc::new(ScriptHost::outputs(&["missing\n"]));
    let bus = Arc::new(ncd_traits::events::BroadcastEventBus::default());
    for _ in 0..2 {
        registry
            .start_session(
                "10001".into(),
                config(10001),
                host.clone(),
                bus.clone(),
                Some(selected("/data/qq")),
            )
            .await;
    }
    assert_eq!(host.count(), 1);
    let replacement = Arc::new(ScriptHost::outputs(&["missing\n"]));
    registry
        .start_session(
            "10001".into(),
            config(10001),
            replacement.clone(),
            bus,
            Some(selected("/data/qq")),
        )
        .await;
    assert_eq!(replacement.count(), 1);
    registry.shutdown_all().await;
}

#[tokio::test]
async fn shutdown_all_discards_an_initialization_that_finishes_late() {
    let registry = Arc::new(RemoteNativeNapcatSessionRegistry::new());
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let host = Arc::new(ScriptHost {
        replies: Mutex::new(VecDeque::from([
            Reply::Hold(entered.clone(), release.clone(), "missing\n".into()),
            Reply::Output("missing\n".into()),
        ])),
        ..Default::default()
    });
    let task_registry = registry.clone();
    let task_host = host.clone();
    let bus = Arc::new(ncd_traits::events::BroadcastEventBus::default());
    let task_bus = bus.clone();
    let start = tokio::spawn(async move {
        task_registry
            .start_session(
                "10001".into(),
                config(10001),
                task_host,
                task_bus,
                Some(selected("/data/qq")),
            )
            .await;
    });
    entered.notified().await;
    registry.shutdown_all().await;
    release.notify_one();
    start.await.unwrap();
    registry
        .start_session(
            "10001".into(),
            config(10001),
            host.clone(),
            bus,
            Some(selected("/data/qq")),
        )
        .await;
    assert_eq!(
        host.count(),
        2,
        "a late session must not remain registered after shutdown"
    );
    registry.shutdown_all().await;
}

#[tokio::test]
async fn shutdown_bot_cancels_its_pending_io_without_publishing_or_blocking_restart() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    let registry = Arc::new(RemoteNativeNapcatSessionRegistry::new());
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let text = "panel http://127.0.0.1:6123/webui?token=fixture\n";
    let host = Arc::new(ScriptHost {
        replies: Mutex::new(VecDeque::from([
            Reply::Hold(
                entered.clone(),
                release,
                format!("1:2 0 {}\n{text}", text.len()),
            ),
            Reply::Output("missing\n".into()),
        ])),
        ..Default::default()
    });
    let unreachable = Arc::new(AtomicUsize::new(0));
    let recorded = unreachable.clone();
    registry
        .set_on_webui_unreachable(Arc::new(move |_| {
            recorded.fetch_add(1, Ordering::SeqCst);
        }))
        .await;
    let task_registry = registry.clone();
    let task_host = host.clone();
    let bus = Arc::new(ncd_traits::events::BroadcastEventBus::default());
    let task_bus = bus.clone();
    let start = tokio::spawn(async move {
        task_registry
            .start_session(
                "10001".into(),
                config(10001),
                task_host,
                task_bus,
                Some(selected("/data/qq")),
            )
            .await;
    });
    entered.notified().await;
    tokio::time::timeout(
        Duration::from_secs(1),
        registry.shutdown_bot(&"10001".into()),
    )
    .await
    .unwrap();
    tokio::time::timeout(Duration::from_secs(1), start)
        .await
        .unwrap()
        .unwrap();
    registry
        .start_session(
            "10001".into(),
            config(10001),
            host.clone(),
            bus,
            Some(selected("/data/qq")),
        )
        .await;
    assert_eq!(
        host.count(),
        2,
        "cancelled initialization must release the per-bot gate"
    );
    assert_eq!(unreachable.load(Ordering::SeqCst), 0);
    registry.shutdown_all().await;
}

#[tokio::test]
async fn another_bots_session_does_not_wait_for_a_slow_initial_log_read() {
    let registry = Arc::new(RemoteNativeNapcatSessionRegistry::new());
    let entered = Arc::new(Notify::new());
    let release = Arc::new(Notify::new());
    let slow = Arc::new(ScriptHost {
        replies: Mutex::new(VecDeque::from([Reply::Hold(
            entered.clone(),
            release.clone(),
            "missing\n".into(),
        )])),
        ..Default::default()
    });
    let worker_registry = registry.clone();
    let bus = Arc::new(ncd_traits::events::BroadcastEventBus::default());
    let worker_bus = bus.clone();
    let worker = tokio::spawn(async move {
        worker_registry
            .start_session(
                "10001".into(),
                config(10001),
                slow,
                worker_bus,
                Some(selected("/data/qq")),
            )
            .await;
    });
    entered.notified().await;
    let fast = Arc::new(ScriptHost::outputs(&["missing\n"]));
    tokio::time::timeout(
        Duration::from_secs(1),
        registry.start_session(
            "10002".into(),
            config(10002),
            fast.clone(),
            bus,
            Some(selected("/data/qq")),
        ),
    )
    .await
    .unwrap();
    assert_eq!(fast.count(), 1);
    registry.shutdown_bot(&"10002".into()).await;
    release.notify_one();
    worker.await.unwrap();
    registry.shutdown_all().await;
}

#[tokio::test(start_paused = true)]
async fn stalled_log_transport_has_an_overall_budget() {
    let host = ScriptHost {
        replies: Mutex::new(VecDeque::from([Reply::Hold(
            Arc::new(Notify::new()),
            Arc::new(Notify::new()),
            String::new(),
        )])),
        ..Default::default()
    };
    let result = read_log(&host, "/data/log", &LogCursor::default(), true).await;
    assert!(matches!(result, Err(HostError::Timeout { .. })));
}

const IDENTITY: &str = r#"{"pid":417,"start_time":"1234","qq_bin":"/opt/QQ/qq"}"#;
const STOPPED: &str = r#"{"v":1,"pid":417,"start_time":"1234","state":"stopped","uid":1001}"#;

#[tokio::test]
async fn napcat_stop_uses_bound_pidfd_request_and_confirms_account_disappearance() {
    let host = ScriptHost::outputs(&["417\n", IDENTITY, STOPPED, ""]);
    stop_remote_napcat_on_host(&host, 10001).await.unwrap();
    let calls = host.commands.lock().unwrap();
    assert_eq!(calls.len(), 4);
    assert!(calls.iter().all(|call| call.program != "pgrep"));
    assert_eq!(calls[2].program, "python3");
    assert_eq!(calls[2].args[3], ncd_component::LINUX_QQ_PROCESS_PROBE);
    let request: serde_json::Value =
        serde_json::from_slice(calls[2].stdin.as_ref().unwrap()).unwrap();
    assert_eq!(request["pid"], 417);
    assert_eq!(request["qq_id"], 10001);
    assert_eq!(request["start_time"], "1234");
    assert_eq!(request["action"], "stop");
    assert!(request.get("expected_uid").is_none());
    assert!(!calls[2].elevated);
}

#[tokio::test]
async fn napcat_stop_does_not_hide_replacement_or_service_ownership() {
    for (reply, expected) in [
        (
            r#"{"v":1,"pid":417,"start_time":"1234","state":"replaced","uid":1001}"#,
            "进程已变化",
        ),
        (
            r#"{"v":1,"pid":417,"start_time":"1234","state":"managed_service","unit":"imported-qq.service","uid":1001}"#,
            "imported-qq.service",
        ),
    ] {
        let host = ScriptHost::outputs(&["417\n", IDENTITY, reply]);
        let error = stop_remote_napcat_on_host(&host, 10001).await.unwrap_err();
        assert!(error.to_string().contains(expected));
        assert_eq!(host.count(), 3);
        assert!(!host.commands.lock().unwrap()[2].elevated);
    }
}

#[tokio::test]
async fn napcat_stop_elevates_only_a_permission_reply_and_detects_restart() {
    let permission = r#"{"v":1,"pid":417,"start_time":"1234","state":"permission","uid":1001}"#;
    let stopped_root = r#"{"v":1,"pid":417,"start_time":"1234","state":"stopped","uid":0}"#;
    let host = ScriptHost::outputs(&["417\n", IDENTITY, permission, stopped_root, "418\n"]);
    let error = stop_remote_napcat_on_host(&host, 10001).await.unwrap_err();
    assert!(error.to_string().contains("重新启动"));
    let calls = host.commands.lock().unwrap();
    assert!(!calls[2].elevated);
    assert!(calls[3].elevated);
    let normal: serde_json::Value =
        serde_json::from_slice(calls[2].stdin.as_ref().unwrap()).unwrap();
    let elevated: serde_json::Value =
        serde_json::from_slice(calls[3].stdin.as_ref().unwrap()).unwrap();
    assert!(normal.get("expected_uid").is_none());
    assert_eq!(elevated["expected_uid"], 1001);
    assert_eq!(normal["pid"], elevated["pid"]);
    assert_eq!(normal["start_time"], elevated["start_time"]);
}

#[tokio::test]
async fn napcat_sudo_reply_cannot_change_the_expected_user_binding() {
    let permission = r#"{"v":1,"pid":417,"start_time":"1234","state":"permission","uid":1001}"#;
    let invalid_sudo = r#"{"v":1,"pid":417,"start_time":"1234","state":"stopped","uid":2002}"#;
    let host = ScriptHost::outputs(&["417\n", IDENTITY, permission, invalid_sudo]);
    assert!(stop_remote_napcat_on_host(&host, 10001).await.is_err());
    assert_eq!(host.count(), 4);
    let calls = host.commands.lock().unwrap();
    let request: serde_json::Value =
        serde_json::from_slice(calls[3].stdin.as_ref().unwrap()).unwrap();
    assert_eq!(request["expected_uid"], 1001);
}

#[tokio::test]
async fn napcat_status_rejects_ambiguous_output_and_has_a_short_budget() {
    for output in ["0\n", "417\n418\n", "not-a-pid"] {
        let host = ScriptHost::outputs(&[output]);
        assert!(remote_napcat_running_pid(&host, 10001).await.is_err());
        assert_eq!(
            host.commands.lock().unwrap()[0].timeout,
            Some(Duration::from_secs(12))
        );
    }
}
