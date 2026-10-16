//! SCM 只拥有控制面；停止控制服务不会主动终止独立 runner。
#![expect(unsafe_code, reason = "SCM callback ABI and service status")]
use crate::{
    ipc, native, store,
    supervisor::{self, Core},
};
use ncd_domain::windows_remote::*;
use std::{
    path::{Path, PathBuf},
    ptr::{null, null_mut},
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};
use windows_sys::Win32::System::Services::*;

pub const NAME: &str = "NcdRemoteAgent";
static ROOT: OnceLock<PathBuf> = OnceLock::new();
static STOP: OnceLock<Arc<AtomicBool>> = OnceLock::new();
static STATUS: AtomicUsize = AtomicUsize::new(0);

pub fn dispatch(root: PathBuf) -> Result<(), String> {
    ROOT.set(root)
        .map_err(|_| "service root already initialized")?;
    let name = native::wide(NAME);
    let table = [
        SERVICE_TABLE_ENTRYW {
            lpServiceName: name.as_ptr() as *mut _,
            lpServiceProc: Some(service_main),
        },
        SERVICE_TABLE_ENTRYW {
            lpServiceName: null_mut(),
            lpServiceProc: None,
        },
    ];
    if unsafe { StartServiceCtrlDispatcherW(table.as_ptr()) } == 0 {
        Err(native::last_error())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_owner_cannot_execute_a_command_as_local_service() {
        let root = tempfile::tempdir().unwrap();
        let core = Core::new(root.path().to_path_buf(), "S-1-5-21-test".into(), None);
        let spec = WindowsWorkloadSpec {
            instance_id: "forbidden".into(),
            command: WindowsCommandSpec {
                program: "cmd.exe".into(),
                ..Default::default()
            },
            interactive: false,
            session_id: None,
            restart_on_failure: false,
            service_owned: true,
        };
        let request = WindowsAgentRequest {
            v: 1,
            request_id: "forbidden-service-command".into(),
            action: WindowsAgentAction::Launch(spec),
        };
        let result = dispatch_request(root.path(), "S-1-5-21-test", &core, request);
        assert!(matches!(result, WindowsAgentResult::Error(error) if error.code == "AccessDenied"));
        assert!(!root.path().join("state").exists());
    }

    #[test]
    fn stop_without_a_worker_persists_a_json_revision_and_removes_desire() {
        let root = tempfile::tempdir().unwrap();
        let owner = "S-1-5-21-test";
        let user = root.path().join("users").join(store::key(owner));
        let manifest = user.join("session-bots/12345.json");
        store::write(&manifest, &"old desired manifest").unwrap();
        let core = Core::new(root.path().to_path_buf(), owner.into(), None);
        let stop = |id: &str| WindowsAgentRequest {
            v: 1,
            request_id: id.into(),
            action: WindowsAgentAction::BotStop {
                bot_id: "12345".into(),
            },
        };
        assert!(matches!(
            dispatch_request(root.path(), owner, &core, stop("stop-one")),
            WindowsAgentResult::Done
        ));
        assert!(!manifest.exists());
        let marker = user.join("session-bots/12345.stop.json");
        let first: String = store::read(&marker).unwrap();
        assert!(!first.is_empty());
        assert!(matches!(
            dispatch_request(root.path(), owner, &core, stop("stop-two")),
            WindowsAgentResult::Done
        ));
        let second: String = store::read(&marker).unwrap();
        assert_ne!(first, second);
    }
}

fn report(state: u32, code: u32) {
    let handle = STATUS.load(Ordering::Acquire) as SERVICE_STATUS_HANDLE;
    if handle.is_null() {
        return;
    }
    let status = SERVICE_STATUS {
        dwServiceType: SERVICE_WIN32_OWN_PROCESS,
        dwCurrentState: state,
        dwControlsAccepted: if state == SERVICE_RUNNING {
            SERVICE_ACCEPT_STOP | SERVICE_ACCEPT_SHUTDOWN
        } else {
            0
        },
        dwWin32ExitCode: code,
        dwServiceSpecificExitCode: 0,
        dwCheckPoint: 0,
        dwWaitHint: if state == SERVICE_STOP_PENDING {
            5000
        } else {
            0
        },
    };
    unsafe {
        SetServiceStatus(handle, &status);
    }
}

unsafe extern "system" fn control(
    code: u32,
    _: u32,
    _: *mut core::ffi::c_void,
    _: *mut core::ffi::c_void,
) -> u32 {
    if code == SERVICE_CONTROL_STOP || code == SERVICE_CONTROL_SHUTDOWN {
        if let Some(stop) = STOP.get() {
            stop.store(true, Ordering::Release);
        }
        report(SERVICE_STOP_PENDING, 0);
        if let Some(root) = ROOT.get() {
            ipc::wake(root, None);
        }
    }
    0
}

unsafe extern "system" fn service_main(_: u32, _: *mut *mut u16) {
    let name = native::wide(NAME);
    let handle = unsafe { RegisterServiceCtrlHandlerExW(name.as_ptr(), Some(control), null()) };
    if handle.is_null() {
        return;
    }
    STATUS.store(handle as usize, Ordering::Release);
    report(SERVICE_START_PENDING, 0);
    let Some(root) = ROOT.get() else {
        report(SERVICE_STOPPED, 1);
        return;
    };
    let result = run(root);
    if let Err(error) = &result {
        eprintln!("agent service failed: {error}");
    }
    report(SERVICE_STOPPED, if result.is_ok() { 0 } else { 1 });
}

fn run(root: &Path) -> Result<(), String> {
    if native::own_sid()? != native::SERVICE_SID {
        return Err("service must run as LocalService".into());
    }
    let owner = crate::install::owner_sid(root)?;
    native::publish_identity(&owner)?;
    let _service_lock = store::FileLock::acquire(&root.join("state/service.lock"))?;
    let core = Arc::new(Core::new(root.to_path_buf(), owner.clone(), None));
    core.recover()?;
    let stopping = Arc::new(AtomicBool::new(false));
    let _ = STOP.set(Arc::clone(&stopping));
    let watch_paths = crate::watch::initialize(root, &owner)?;
    let _watch_lease = store::FileLock::acquire(&watch_paths.state_dir.join("host.lock"))?;
    let watch_runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    let (watch_shutdown, watch_receiver) = tokio::sync::watch::channel(false);
    let watch_task = watch_runtime.spawn(crate::watch::run(watch_paths, watch_receiver));
    let recovery = Arc::clone(&core);
    let recovery_stop = Arc::clone(&stopping);
    std::thread::spawn(move || {
        while !recovery_stop.load(Ordering::Acquire) {
            for _ in 0..20 {
                if recovery_stop.load(Ordering::Acquire) {
                    return;
                }
                std::thread::sleep(Duration::from_millis(250));
            }
            if let Err(error) = recovery.recover() {
                eprintln!("agent recovery: {error}");
            }
        }
    });
    let root = root.to_path_buf();
    let handler_root = root.clone();
    let handler_owner = owner.clone();
    let handler = Arc::new(move |request: WindowsAgentRequest| {
        dispatch_request(&handler_root, &handler_owner, &core, request)
    });
    report(SERVICE_RUNNING, 0);
    let result = ipc::serve(&root, None, &owner, stopping, handler);
    let _ = watch_shutdown.send(true);
    let _ = watch_runtime
        .block_on(async { tokio::time::timeout(Duration::from_secs(5), watch_task).await });
    watch_runtime.shutdown_timeout(Duration::from_secs(2));
    result
}

fn dispatch_request(
    root: &Path,
    owner: &str,
    core: &Core,
    mut request: WindowsAgentRequest,
) -> WindowsAgentResult {
    if matches!(&request.action, WindowsAgentAction::Launch(spec) if spec.service_owned) {
        return ipc::error(
            "AccessDenied",
            "service identity workloads are not exposed through the remote control pipe",
            false,
        );
    }
    let sessions = supervisor::registrations(root, owner).unwrap_or_default();
    let user_root = root.join("users").join(store::key(owner));
    if let WindowsAgentAction::BotStart(spec) | WindowsAgentAction::BotConfigure(spec) =
        &mut request.action
    {
        if spec.bot_id.is_empty()
            || spec.bot_id.len() > 20
            || !spec.bot_id.bytes().all(|ch| ch.is_ascii_digit())
        {
            return ipc::error("InvalidBotId", "invalid bot identity", false);
        }
        let path = user_root
            .join("session-bots")
            .join(format!("{}.stop.json", spec.bot_id));
        spec.intent_revision = if path.exists() {
            match store::read::<String>(&path) {
                Ok(revision) => Some(revision),
                Err(error) => return ipc::error("StateUnavailable", error, true),
            }
        } else {
            None
        };
    }
    if sessions.is_empty() {
        match &request.action {
            WindowsAgentAction::Launch(spec) if !spec.service_owned => {
                return Core::new(user_root, owner.into(), None).handle(request);
            }
            WindowsAgentAction::Stop { instance_id, .. }
                if store::entry_path(&user_root, instance_id).is_file() =>
            {
                return Core::new(user_root, owner.into(), None).handle(request);
            }
            WindowsAgentAction::Status { instance_id }
                if store::entry_path(&user_root, instance_id).is_file() =>
            {
                return match store::read::<store::Entry>(&store::entry_path(
                    &user_root,
                    instance_id,
                )) {
                    Ok(mut entry) => {
                        entry.status.state = if entry.desired_running {
                            WindowsWorkloadState::WaitingForUserSession
                        } else {
                            WindowsWorkloadState::Stopped
                        };
                        entry.status.pid = None;
                        WindowsAgentResult::Workload(entry.status)
                    }
                    Err(error) => ipc::error("StateUnavailable", error, true),
                };
            }
            WindowsAgentAction::Tail { instance_id, .. }
                if store::entry_path(&user_root, instance_id).is_file() =>
            {
                return Core::new(user_root, owner.into(), None).handle(request);
            }
            _ => {}
        }
    }
    if let WindowsAgentAction::BotStop { bot_id } = &request.action {
        if bot_id.is_empty() || bot_id.len() > 20 || !bot_id.bytes().all(|ch| ch.is_ascii_digit()) {
            return ipc::error(
                "InvalidBotId",
                "bot id must contain decimal account digits",
                false,
            );
        }
        let revision = user_root
            .join("session-bots")
            .join(format!("{bot_id}.stop.json"));
        if let Err(error) = store::write(&revision, &uuid::Uuid::new_v4().to_string()) {
            return ipc::error("StateUnavailable", error, true);
        }
        let path = root
            .join("users")
            .join(store::key(owner))
            .join("session-bots")
            .join(format!("{bot_id}.json"));
        if let Err(error) = std::fs::remove_file(path) {
            if error.kind() != std::io::ErrorKind::NotFound {
                return ipc::error("StateUnavailable", error.to_string(), true);
            }
        }
        if sessions.is_empty() {
            return WindowsAgentResult::Done;
        }
    }
    if matches!(request.action, WindowsAgentAction::Hello) {
        return match core.hello() {
            Ok(mut status) => {
                status.sessions = sessions
                    .iter()
                    .map(|session| {
                        let locked = native::session_locked(session.session).or_else(|| {
                            let WindowsAgentResponse {
                                result: WindowsAgentResult::Hello(worker),
                                ..
                            } = ipc::request(root, Some(session.session), &request).ok()?
                            else {
                                return None;
                            };
                            worker
                                .sessions
                                .iter()
                                .find(|info| {
                                    info.session_id == session.session
                                        && info.user_sid == session.sid
                                })
                                .and_then(|info| info.locked)
                        });
                        WindowsSessionInfo {
                            user_sid: session.sid.clone(),
                            session_id: session.session,
                            worker_ready: true,
                            worker_update_pending: crate::install::session_worker_update_pending(
                                root,
                                session.pid,
                            ),
                            locked,
                        }
                    })
                    .collect();
                if !sessions.is_empty() {
                    status.capabilities.push("user_session_v1".into());
                    status.capabilities.push("bot_runtime_v1".into());
                }
                status.capabilities.push("watch_v1".into());
                status
                    .capabilities
                    .push(ncd_watch::relogin_config::RELOGIN_CAPABILITY.into());
                WindowsAgentResult::Hello(status)
            }
            Err(error) => ipc::error("StateUnavailable", error, true),
        };
    }
    let interactive = match &request.action {
        WindowsAgentAction::Launch(spec) => !spec.service_owned || spec.interactive,
        WindowsAgentAction::Status { instance_id }
        | WindowsAgentAction::Stop { instance_id, .. }
        | WindowsAgentAction::Tail { instance_id, .. } => {
            !store::entry_path(root, instance_id).exists()
        }
        WindowsAgentAction::BotStart(_)
        | WindowsAgentAction::BotConfigure(_)
        | WindowsAgentAction::BotStop { .. }
        | WindowsAgentAction::BotStatus { .. }
        | WindowsAgentAction::BotQr { .. }
        | WindowsAgentAction::BotCloseQr { .. }
        | WindowsAgentAction::BotTail { .. }
        | WindowsAgentAction::BotEvents { .. }
        | WindowsAgentAction::BotEndpoint { .. } => true,
        WindowsAgentAction::List => {
            let mut result = match core.handle(request.clone()) {
                WindowsAgentResult::Workloads(items) => items,
                error => return error,
            };
            for session in &sessions {
                if let Ok(WindowsAgentResponse {
                    result: WindowsAgentResult::Workloads(items),
                    ..
                }) = ipc::request(root, Some(session.session), &request)
                {
                    result.extend(items);
                }
            }
            return WindowsAgentResult::Workloads(result);
        }
        _ => false,
    };
    if !interactive {
        return core.handle(request);
    }
    let requested_session = match &request.action {
        WindowsAgentAction::Launch(spec) => spec.session_id,
        _ => None,
    };
    let selected: Vec<_> = sessions
        .iter()
        .filter(|session| requested_session.is_none_or(|id| id == session.session))
        .collect();
    if selected.len() != 1 {
        return ipc::error(
            if selected.is_empty() {
                "WaitingForUserSession"
            } else {
                "SessionSelectionRequired"
            },
            "an authorized non-elevated user session worker is required",
            true,
        );
    }
    ipc::request(root, Some(selected[0].session), &request)
        .map(|response| response.result)
        .unwrap_or_else(|error| ipc::error("WorkerUnavailable", error, true))
}
