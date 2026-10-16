//! 持久意图和独立 runner；控制服务重启不会关闭 workload Job。
use crate::{
    ipc, native,
    store::{self, Entry, FileLock},
};
use ncd_domain::windows_remote::*;
use serde::{Deserialize, Serialize};
use std::os::windows::process::CommandExt;
use std::{
    fs,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, Mutex, atomic::AtomicBool},
    time::Duration,
};

#[derive(Clone, Serialize, Deserialize)]
pub struct SessionRegistration {
    pub sid: String,
    pub session: u32,
    pub pid: u32,
    pub created: u64,
}

pub struct Core {
    pub root: PathBuf,
    pub owner: String,
    pub session: Option<u32>,
    serial: Mutex<()>,
}

impl Core {
    pub fn new(root: PathBuf, owner: String, session: Option<u32>) -> Self {
        Self {
            root,
            owner,
            session,
            serial: Mutex::new(()),
        }
    }

    pub fn handle(&self, request: WindowsAgentRequest) -> WindowsAgentResult {
        let Ok(_guard) = self.serial.lock() else {
            return ipc::error("StateUnavailable", "supervisor lock poisoned", true);
        };
        match self.handle_inner(&request) {
            Ok(value) => value,
            Err(error) => ipc::error(
                if error.starts_with("NotFound:") {
                    "NotFound"
                } else if error.starts_with("GenerationConflict:") {
                    "GenerationConflict"
                } else {
                    "OperationFailed"
                },
                error,
                false,
            ),
        }
    }

    fn handle_inner(&self, request: &WindowsAgentRequest) -> Result<WindowsAgentResult, String> {
        if matches!(request.action, WindowsAgentAction::Hello) {
            return Ok(WindowsAgentResult::Hello(self.hello()?));
        }
        let is_mutation = matches!(
            request.action,
            WindowsAgentAction::Launch(_) | WindowsAgentAction::Stop { .. }
        );
        let operation = self
            .root
            .join("state/operations")
            .join(format!("{}.json", store::key(&request.request_id)));
        let _operation_lock = if is_mutation {
            Some(FileLock::acquire(&operation.with_extension("lock"))?)
        } else {
            None
        };
        let fingerprint =
            store::key(&serde_json::to_string(&request.action).map_err(|e| e.to_string())?);
        if is_mutation && operation.exists() {
            let saved: SavedOperation = store::read(&operation)?;
            if saved.fingerprint != fingerprint {
                return Err("request id was already used for a different operation".into());
            }
            return Ok(saved.result);
        }
        let result = match &request.action {
            WindowsAgentAction::Launch(spec) => self.launch(spec)?,
            WindowsAgentAction::Status { instance_id } => {
                WindowsAgentResult::Workload(self.status(instance_id)?)
            }
            WindowsAgentAction::Stop {
                instance_id,
                generation,
            } => self.stop(instance_id, generation)?,
            WindowsAgentAction::List => WindowsAgentResult::Workloads(
                store::entries(&self.root)?
                    .iter()
                    .map(|entry| self.status(&entry.spec.instance_id))
                    .collect::<Result<_, _>>()?,
            ),
            WindowsAgentAction::Tail {
                instance_id,
                offset,
                max_bytes,
            } => self.tail(instance_id, *offset, *max_bytes)?,
            _ => return Err("unsupported supervisor action".into()),
        };
        if is_mutation {
            store::write(
                &operation,
                &SavedOperation {
                    fingerprint,
                    result: result.clone(),
                },
            )?;
        }
        Ok(result)
    }

    pub fn hello(&self) -> Result<WindowsAgentStatus, String> {
        let sessions = self
            .session
            .map(|session_id| WindowsSessionInfo {
                user_sid: self.owner.clone(),
                session_id,
                worker_ready: true,
                worker_update_pending: false,
                locked: native::session_locked(session_id),
            })
            .into_iter()
            .collect();
        Ok(WindowsAgentStatus {
            v: 1,
            version: env!("CARGO_PKG_VERSION").into(),
            service_running: true,
            service_identity: native::own_sid()?,
            capabilities: vec![
                "durable_workloads_v1".into(),
                "generation_stop_v1".into(),
                "persistent_logs_v1".into(),
            ],
            sessions,
        })
    }

    fn launch(&self, spec: &WindowsWorkloadSpec) -> Result<WindowsAgentResult, String> {
        validate_spec(spec)?;
        let waiting = !spec.service_owned && self.session.is_none();
        if let (Some(expected), Some(actual)) = (spec.session_id, self.session) {
            if expected != actual {
                return Err("requested interactive session does not match worker".into());
            }
        }
        let directory = store::instance_dir(&self.root, &spec.instance_id);
        let _lock = FileLock::acquire(&directory.join("entry.lock"))?;
        let path = store::entry_path(&self.root, &spec.instance_id);
        if path.exists() {
            let previous: Entry = store::read(&path)?;
            if previous.desired_running {
                if previous.spec != *spec {
                    return Err(
                        "instance is running with a different command; stop its generation first"
                            .into(),
                    );
                }
                return Ok(WindowsAgentResult::Workload(previous.status));
            }
            if !matches!(
                previous.status.state,
                WindowsWorkloadState::Stopped | WindowsWorkloadState::Failed
            ) && previous
                .runner_pid
                .zip(previous.runner_created)
                .is_some_and(|(pid, stamp)| native::alive(pid, stamp))
            {
                return Err("previous runner is still stopping".into());
            }
        }
        let generation = uuid::Uuid::new_v4().to_string();
        let entry = Entry {
            spec: spec.clone(),
            desired_running: true,
            owner_sid: self.owner.clone(),
            runner_pid: None,
            runner_created: None,
            process_created: None,
            status: WindowsWorkloadStatus {
                instance_id: spec.instance_id.clone(),
                generation: generation.clone(),
                pid: None,
                session_id: self.session.or(spec.session_id),
                state: if waiting {
                    WindowsWorkloadState::WaitingForUserSession
                } else {
                    WindowsWorkloadState::Starting
                },
                exit_code: None,
                error: None,
                started_at: None,
                finished_at: None,
            },
        };
        store::write(&path, &entry)?;
        drop(_lock);
        if !waiting {
            self.spawn_runner(&spec.instance_id, &generation)?;
        }
        Ok(WindowsAgentResult::Workload(entry.status))
    }

    fn spawn_runner(&self, instance: &str, generation: &str) -> Result<(), String> {
        let executable = std::env::current_exe().map_err(|e| e.to_string())?;
        let mut command = Command::new(executable);
        command
            .args(["runner", "--root"])
            .arg(&self.root)
            .args(["--instance", instance, "--generation", generation])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(0x08000000);
        let _child = command.spawn().map_err(|e| format!("runner spawn: {e}"))?;
        Ok(())
    }

    fn status(&self, instance: &str) -> Result<WindowsWorkloadStatus, String> {
        let path = store::entry_path(&self.root, instance);
        if !path.is_file() {
            return Err("NotFound: workload not found".into());
        }
        let entry: Entry = store::read(&path)?;
        let mut status = entry.status;
        if status.is_running()
            && entry
                .runner_pid
                .zip(entry.runner_created)
                .is_some_and(|(pid, stamp)| !native::alive(pid, stamp))
        {
            status.state = WindowsWorkloadState::Unknown;
            status.error = Some("runner is unavailable; recovering process ownership".into());
        }
        Ok(status)
    }

    fn stop(&self, instance: &str, generation: &str) -> Result<WindowsAgentResult, String> {
        let path = store::entry_path(&self.root, instance);
        if !path.is_file() {
            return Err("NotFound: workload not found".into());
        }
        let _lock =
            FileLock::acquire(&store::instance_dir(&self.root, instance).join("entry.lock"))?;
        let mut entry: Entry = store::read(&path)?;
        if entry.status.generation != generation {
            return Err("GenerationConflict: workload generation changed".into());
        }
        entry.desired_running = false;
        if entry
            .runner_pid
            .zip(entry.runner_created)
            .is_some_and(|(pid, stamp)| native::alive(pid, stamp))
        {
            entry.status.state = WindowsWorkloadState::Stopping;
        } else {
            entry.status.state = WindowsWorkloadState::Stopped;
            entry.status.pid = None;
            entry.status.finished_at = Some(chrono::Utc::now().to_rfc3339());
        }
        store::write(&path, &entry)?;
        Ok(WindowsAgentResult::Workload(entry.status))
    }

    fn tail(
        &self,
        instance: &str,
        offset: u64,
        max_bytes: u32,
    ) -> Result<WindowsAgentResult, String> {
        let from_end = offset == u64::MAX;
        let entry: Entry = store::read(&store::entry_path(&self.root, instance))?;
        let path = store::instance_dir(&self.root, instance)
            .join(format!("{}.log", entry.status.generation));
        let mut data = Vec::new();
        let (mut offset, length) = match fs::File::open(path) {
            Ok(mut file) => {
                let length = file.metadata().map_err(|e| e.to_string())?.len();
                let start = if offset == u64::MAX {
                    length.saturating_sub(u64::from(max_bytes.clamp(4, 128 * 1024)))
                } else {
                    offset.min(length)
                };
                file.seek(SeekFrom::Start(start))
                    .map_err(|e| e.to_string())?;
                file.take(u64::from(max_bytes.clamp(4, 128 * 1024)))
                    .read_to_end(&mut data)
                    .map_err(|e| e.to_string())?;
                (start, length)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (0, 0),
            Err(error) => return Err(error.to_string()),
        };
        if from_end && offset > 0 && std::str::from_utf8(&data).is_err() {
            let skip = data
                .iter()
                .take(3)
                .take_while(|byte| (**byte & 0xc0) == 0x80)
                .count();
            if skip > 0 {
                data.drain(..skip);
                offset += skip as u64;
            }
        }
        let mut cursor = 0;
        while let Err(error) = std::str::from_utf8(&data[cursor..]) {
            cursor += error.valid_up_to();
            if let Some(length) = error.error_len() {
                cursor += length;
            } else {
                // A live writer may still append the rest of a UTF-8 codepoint.
                if offset + (data.len() as u64) < length || entry.desired_running {
                    data.truncate(cursor);
                }
                break;
            }
        }
        let next = offset + data.len() as u64;
        let encoding_lossy = std::str::from_utf8(&data).is_err();
        Ok(WindowsAgentResult::Log(WindowsLogChunk {
            instance_id: instance.into(),
            generation: entry.status.generation,
            offset,
            next_offset: next,
            data: String::from_utf8_lossy(&data).into_owned(),
            encoding_lossy,
            eof: next >= length,
        }))
    }

    pub fn recover(&self) -> Result<(), String> {
        for entry in store::entries(&self.root)? {
            if !entry.desired_running {
                continue;
            }
            if !entry.spec.service_owned && self.session.is_none() {
                continue;
            }
            if self.session.is_some()
                && entry.spec.session_id.is_some()
                && self.session != entry.spec.session_id
            {
                continue;
            }
            if entry
                .runner_pid
                .zip(entry.runner_created)
                .is_some_and(|(pid, stamp)| native::alive(pid, stamp))
            {
                continue;
            }
            if entry
                .status
                .pid
                .zip(entry.process_created)
                .is_some_and(|(pid, stamp)| native::alive(pid, stamp))
            {
                continue;
            }
            // An exclusive runner lease arbitrates recovery after a lost reply or Agent restart.
            self.spawn_runner(&entry.spec.instance_id, &entry.status.generation)?;
        }
        Ok(())
    }
}

pub fn registrations(root: &Path, owner: &str) -> Result<Vec<SessionRegistration>, String> {
    let mut output = Vec::new();
    let directory = root.join("sessions");
    if !directory.exists() {
        return Ok(output);
    }
    for item in fs::read_dir(directory).map_err(|e| e.to_string())? {
        let path = item.map_err(|e| e.to_string())?.path();
        if path.extension().is_none_or(|ext| ext != "json") {
            continue;
        }
        let Ok(registration) = store::read::<SessionRegistration>(&path) else {
            continue;
        };
        if registration.sid == owner
            && native::alive(registration.pid, registration.created)
            && native::process_sid(registration.pid).is_ok_and(|sid| sid == owner)
            && native::session_id(registration.pid)
                .is_ok_and(|session| session == registration.session)
        {
            output.push(registration);
        }
    }
    Ok(output)
}

pub fn session_worker(root: PathBuf) -> Result<(), String> {
    let sid = native::own_sid()?;
    if sid != crate::install::owner_sid(&root)? {
        return Err("session worker is not the registered owner".into());
    }
    let session = native::session_id(std::process::id())?;
    if session == 0 || native::elevated()? {
        return Err("session worker requires a non-elevated interactive user session".into());
    }
    native::publish_identity(&sid)?;
    let _job = crate::runner::worker_job()?;
    let user_root = root.join("users").join(store::key(&sid));
    let _lease = FileLock::acquire(&user_root.join("worker.lock"))?;
    let registration = SessionRegistration {
        sid: sid.clone(),
        session,
        pid: std::process::id(),
        created: native::created(std::process::id())?,
    };
    let registration_path = root
        .join("sessions")
        .join(format!("{}.json", store::key(&sid)));
    store::write(&registration_path, &registration)?;
    let core = Arc::new(Core::new(user_root.clone(), sid.clone(), Some(session)));
    core.recover()?;
    let runtime = Arc::new(
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?,
    );
    let bot_service = Arc::new(crate::bot::BotService::new(user_root));
    runtime.block_on(bot_service.resume());
    let recovery = Arc::clone(&core);
    let recovery_runtime = Arc::clone(&runtime);
    let recovery_bots = Arc::clone(&bot_service);
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_secs(5));
            if let Err(error) = recovery.recover() {
                eprintln!("worker recovery: {error}");
            }
            recovery_runtime.block_on(recovery_bots.retry_pending());
        }
    });
    let handler = Arc::new(move |request: WindowsAgentRequest| {
        if let Some(result) = runtime.block_on(bot_service.handle(&request.action)) {
            return result;
        }
        core.handle(request)
    });
    let result = ipc::serve(
        &root,
        Some(session),
        &sid,
        Arc::new(AtomicBool::new(false)),
        handler,
    );
    let _ = fs::remove_file(registration_path);
    result
}

#[derive(Serialize, Deserialize)]
struct SavedOperation {
    fingerprint: String,
    result: WindowsAgentResult,
}

pub fn validate_spec(spec: &WindowsWorkloadSpec) -> Result<(), String> {
    if spec.interactive && spec.service_owned {
        return Err("interactive workloads cannot use the service identity".into());
    }
    if spec.instance_id.is_empty()
        || spec.instance_id.len() > 256
        || spec.instance_id.contains('\0')
    {
        return Err("invalid workload id".into());
    }
    if spec.command.elevated {
        return Err(
            "ElevationRequired: workloads cannot request service privilege escalation".into(),
        );
    }
    if spec.command.program.is_empty()
        || spec.command.program.contains('\0')
        || spec.command.args.iter().any(|s| s.contains('\0'))
    {
        return Err("invalid executable or argument".into());
    }
    if spec
        .command
        .environment
        .iter()
        .any(|(key, value)| key.is_empty() || key.contains(['=', '\0']) || value.contains('\0'))
    {
        return Err("invalid environment entry".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn spec() -> WindowsWorkloadSpec {
        WindowsWorkloadSpec {
            instance_id: "app-one".into(),
            command: WindowsCommandSpec {
                program: "C:\\node\\node.exe".into(),
                ..Default::default()
            },
            interactive: false,
            session_id: None,
            restart_on_failure: false,
            service_owned: false,
        }
    }
    #[test]
    fn user_app_does_not_implicitly_gain_service_identity() {
        let mut spec = spec();
        spec.interactive = true;
        spec.service_owned = true;
        assert!(validate_spec(&spec).is_err());
    }
    #[test]
    fn embedded_environment_variable_assignment_is_rejected() {
        let mut spec = spec();
        spec.command
            .environment
            .insert("PATH=bad".into(), "value".into());
        assert!(validate_spec(&spec).is_err());
    }
    #[test]
    fn malformed_arguments_cannot_reach_create_process() {
        let mut spec = spec();
        spec.command.args.push("bad\0argument".into());
        assert!(validate_spec(&spec).is_err());
    }
    #[test]
    fn stop_before_user_logon_clears_durable_intent_and_rejects_stale_generation() {
        let directory = tempfile::tempdir().unwrap();
        let core = Core::new(directory.path().to_path_buf(), "S-1-5-21-test".into(), None);
        let request = |id: &str, action| WindowsAgentRequest {
            v: 1,
            request_id: id.into(),
            action,
        };
        let WindowsAgentResult::Workload(status) =
            core.handle(request("launch", WindowsAgentAction::Launch(spec())))
        else {
            panic!("launch should queue");
        };
        assert_eq!(status.state, WindowsWorkloadState::WaitingForUserSession);
        let stale = core.handle(request(
            "stale",
            WindowsAgentAction::Stop {
                instance_id: status.instance_id.clone(),
                generation: "old".into(),
            },
        ));
        assert!(
            matches!(stale, WindowsAgentResult::Error(error) if error.code == "GenerationConflict")
        );
        core.handle(request(
            "stop",
            WindowsAgentAction::Stop {
                instance_id: status.instance_id.clone(),
                generation: status.generation,
            },
        ));
        let entry: Entry =
            store::read(&store::entry_path(directory.path(), &status.instance_id)).unwrap();
        assert!(!entry.desired_running);
        assert_eq!(entry.status.state, WindowsWorkloadState::Stopped);
    }
    #[test]
    fn operation_id_cannot_be_reused_for_another_side_effect() {
        let directory = tempfile::tempdir().unwrap();
        let core = Core::new(directory.path().to_path_buf(), "S-1-5-21-test".into(), None);
        let request = WindowsAgentRequest {
            v: 1,
            request_id: "same".into(),
            action: WindowsAgentAction::Launch(spec()),
        };
        let first = core.handle(request.clone());
        let second = core.handle(request);
        let (WindowsAgentResult::Workload(first), WindowsAgentResult::Workload(second)) =
            (first, second)
        else {
            panic!("idempotent launch should queue once");
        };
        assert_eq!(first.generation, second.generation);
        let conflicting = core.handle(WindowsAgentRequest {
            v: 1,
            request_id: "same".into(),
            action: WindowsAgentAction::Stop {
                instance_id: first.instance_id,
                generation: first.generation,
            },
        });
        assert!(matches!(conflicting, WindowsAgentResult::Error(_)));
    }

    fn log_fixture(bytes: &[u8]) -> (tempfile::TempDir, Core) {
        let directory = tempfile::tempdir().unwrap();
        let core = Core::new(directory.path().to_path_buf(), "S-1-5-21-test".into(), None);
        let WindowsAgentResult::Workload(status) = core.handle(WindowsAgentRequest {
            v: 1,
            request_id: "log-fixture".into(),
            action: WindowsAgentAction::Launch(spec()),
        }) else {
            panic!("fixture should queue");
        };
        fs::write(
            store::instance_dir(directory.path(), &status.instance_id)
                .join(format!("{}.log", status.generation)),
            bytes,
        )
        .unwrap();
        (directory, core)
    }

    fn read_log(core: &Core, offset: u64, size: u32) -> WindowsLogChunk {
        let WindowsAgentResult::Log(chunk) = core.tail("app-one", offset, size).unwrap() else {
            panic!("expected log chunk");
        };
        chunk
    }

    #[test]
    fn utf8_log_pages_never_consume_a_partial_character() {
        let (_directory, core) = log_fixture("A中文Z".as_bytes());
        let first = read_log(&core, 0, 5);
        assert_eq!(first.data, "A中");
        assert_eq!(first.next_offset, 4);
        assert!(!first.encoding_lossy);
        let next = read_log(&core, first.next_offset, 5);
        assert_eq!(next.data, "文Z");
        assert!(next.eof);
    }

    #[test]
    fn tail_preview_skips_a_leading_character_fragment() {
        let (_directory, core) = log_fixture("前中文".as_bytes());
        let chunk = read_log(&core, u64::MAX, 4);
        assert_eq!(chunk.data, "文");
        assert_eq!(chunk.offset, 6);
        assert_eq!(chunk.next_offset, 9);
        assert!(!chunk.encoding_lossy);
    }

    #[test]
    fn live_writer_can_complete_a_character_at_the_next_poll() {
        let (directory, core) = log_fixture(&[b'A', 0xe4, 0xb8]);
        let first = read_log(&core, 0, 4);
        assert_eq!(first.data, "A");
        assert_eq!(first.next_offset, 1);
        assert!(!first.eof);
        let entry: Entry = store::read(&store::entry_path(directory.path(), "app-one")).unwrap();
        let log = store::instance_dir(directory.path(), "app-one")
            .join(format!("{}.log", entry.status.generation));
        fs::write(log, "A中".as_bytes()).unwrap();
        let next = read_log(&core, first.next_offset, 4);
        assert_eq!(next.data, "中");
        assert!(!next.encoding_lossy);
    }
}
