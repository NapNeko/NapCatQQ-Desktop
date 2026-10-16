//! 本地管道只接受登记 SID；SSH 桥不传递自报身份。
#![expect(unsafe_code, reason = "Named pipe authentication and handle ownership")]
use crate::{
    native::{self, Handle},
    wire,
};
use ncd_domain::windows_remote::*;
use std::{
    fs::File,
    io::{self, BufReader, Read, Write},
    os::windows::io::{AsRawHandle, FromRawHandle},
    path::Path,
    ptr::null_mut,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::*,
    Security::*,
    Storage::FileSystem::*,
    System::{Pipes::*, Threading::*},
};

pub fn pipe_name(root: &Path, session: Option<u32>) -> String {
    let root_key = crate::store::key(&root.to_string_lossy().to_lowercase());
    format!(
        r"\\.\pipe\NcdRemote-{}-{}",
        &root_key[..24],
        session.map_or_else(|| "machine".into(), |id| format!("session-{id}"))
    )
}

pub fn error(code: &str, message: impl Into<String>, retryable: bool) -> WindowsAgentResult {
    WindowsAgentResult::Error(WindowsAgentError {
        code: code.into(),
        message: message.into(),
        retryable,
    })
}

struct DeadlinePipe {
    file: File,
    deadline: Instant,
}
#[derive(serde::Serialize, serde::Deserialize)]
struct Acknowledgement {
    request_id: String,
}
impl DeadlinePipe {
    fn new(file: File, timeout: Duration) -> Result<Self, String> {
        let mode = PIPE_READMODE_BYTE | PIPE_NOWAIT;
        if unsafe {
            SetNamedPipeHandleState(
                file.as_raw_handle(),
                &mode,
                std::ptr::null(),
                std::ptr::null(),
            )
        } == 0
        {
            return Err(native::last_error());
        }
        Ok(Self {
            file,
            deadline: Instant::now() + timeout,
        })
    }
    fn retry(&self) -> io::Result<()> {
        if Instant::now() >= self.deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "local Agent IPC deadline exceeded; operation outcome may be unknown",
            ));
        }
        std::thread::sleep(Duration::from_millis(10));
        Ok(())
    }
}
impl Read for DeadlinePipe {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        loop {
            let mut read = 0;
            let result = unsafe {
                ReadFile(
                    self.file.as_raw_handle(),
                    buffer.as_mut_ptr(),
                    buffer.len().min(u32::MAX as usize) as u32,
                    &mut read,
                    null_mut(),
                )
            };
            if result != 0 {
                if read != 0 {
                    return Ok(read as usize);
                }
                self.retry()?;
                continue;
            }
            match unsafe { GetLastError() } {
                ERROR_NO_DATA => self.retry()?,
                ERROR_BROKEN_PIPE | ERROR_PIPE_NOT_CONNECTED => return Ok(0),
                _ => return Err(io::Error::last_os_error()),
            }
        }
    }
}
impl Write for DeadlinePipe {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        loop {
            match self.file.write(buffer) {
                Ok(0) if !buffer.is_empty() => self.retry()?,
                Err(error) if error.raw_os_error() == Some(ERROR_NO_DATA as i32) => self.retry()?,
                result => return result,
            }
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn deadline(action: &WindowsAgentAction) -> Duration {
    Duration::from_secs(match action {
        WindowsAgentAction::BotStart(_) | WindowsAgentAction::BotConfigure(_) => 300,
        WindowsAgentAction::BotQr { .. } => 90,
        _ => 30,
    })
}

pub fn request(
    root: &Path,
    session: Option<u32>,
    request: &WindowsAgentRequest,
) -> Result<WindowsAgentResponse, String> {
    let name = native::wide(&pipe_name(root, session));
    unsafe {
        WaitNamedPipeW(name.as_ptr(), 2000);
    }
    let raw = unsafe {
        CreateFileW(
            name.as_ptr(),
            0x12019b,
            0,
            std::ptr::null(),
            OPEN_EXISTING,
            SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION,
            null_mut(),
        )
    };
    let handle = Handle::new(raw).map_err(|e| format!("AgentUnavailable: {e}"))?;
    let mut pid = 0;
    if unsafe { GetNamedPipeServerProcessId(handle.0, &mut pid) } == 0 {
        return Err(native::last_error());
    }
    let expected_sid = if session.is_some() {
        native::own_sid()?
    } else {
        native::SERVICE_SID.into()
    };
    let server_sid = native::process_sid(pid)?;
    if server_sid != expected_sid
        && !(session.is_some() && server_sid == crate::install::owner_sid(root)?)
    {
        return Err("agent pipe server identity mismatch".into());
    }
    let executable =
        std::fs::canonicalize(native::process_path(pid)?).map_err(|e| e.to_string())?;
    let protected = [root.join("agent/current"), root.join("agent/versions")];
    if !protected
        .iter()
        .filter_map(|path| std::fs::canonicalize(path).ok())
        .any(|path| executable.starts_with(path))
    {
        return Err("agent pipe server executable is outside protected installation".into());
    }
    let raw = handle.0;
    std::mem::forget(handle);
    let file = unsafe { File::from_raw_handle(raw) };
    exchange(file, request)
}

fn exchange(file: File, request: &WindowsAgentRequest) -> Result<WindowsAgentResponse, String> {
    let mut file = DeadlinePipe::new(file, deadline(&request.action))?;
    wire::write_frame(&mut file, request)?;
    let response: WindowsAgentResponse =
        wire::read_frame(&mut BufReader::new(&mut file))?.ok_or("agent closed without response")?;
    if response.v != WINDOWS_AGENT_PROTOCOL || response.request_id != request.request_id {
        return Err("agent response identity mismatch".into());
    }
    wire::write_frame(
        &mut file,
        &Acknowledgement {
            request_id: request.request_id.clone(),
        },
    )?;
    Ok(response)
}

pub fn bridge(root: &Path, session: Option<&str>) -> Result<(), String> {
    let session = session
        .map(str::parse::<u32>)
        .transpose()
        .map_err(|e| e.to_string())?;
    let mut input = BufReader::new(std::io::stdin().lock());
    let mut output = std::io::stdout().lock();
    while let Some(request_value) = wire::read_frame::<WindowsAgentRequest>(&mut input)? {
        let response =
            request(root, session, &request_value).unwrap_or_else(|message| WindowsAgentResponse {
                v: 1,
                request_id: request_value.request_id.clone(),
                result: error("AgentUnavailable", message, true),
            });
        wire::write_frame(&mut output, &response)?;
    }
    Ok(())
}

fn authenticated_sid(pipe: HANDLE) -> Result<String, String> {
    unsafe {
        if ImpersonateNamedPipeClient(pipe) == 0 {
            return Err(native::last_error());
        }
        struct Revert;
        impl Drop for Revert {
            fn drop(&mut self) {
                unsafe {
                    RevertToSelf();
                }
            }
        }
        let _revert = Revert;
        let mut raw = null_mut();
        if OpenThreadToken(GetCurrentThread(), TOKEN_QUERY, 1, &mut raw) == 0 {
            return Err(native::last_error());
        }
        let token = Handle::new(raw)?;
        native::token_sid(token.0)
    }
}

pub fn serve(
    root: &Path,
    session: Option<u32>,
    owner: &str,
    stopping: Arc<AtomicBool>,
    handler: Arc<dyn Fn(WindowsAgentRequest) -> WindowsAgentResult + Send + Sync>,
) -> Result<(), String> {
    let pipe_name = native::wide(&pipe_name(root, session));
    let server_sid = native::own_sid()?;
    let descriptor = native::Descriptor::new(&format!(
        "D:P(A;;GA;;;SY)(A;;GA;;;LS)(A;;GA;;;{server_sid})(A;;0x12019b;;;{owner})"
    ))?;
    let attributes = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor.0,
        bInheritHandle: 0,
    };
    let mut first = true;
    while !stopping.load(Ordering::Acquire) {
        let mut flags = PIPE_ACCESS_DUPLEX;
        if first {
            flags |= FILE_FLAG_FIRST_PIPE_INSTANCE;
        }
        let raw = unsafe {
            CreateNamedPipeW(
                pipe_name.as_ptr(),
                flags,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                16,
                65536,
                65536,
                2000,
                &attributes,
            )
        };
        if raw == INVALID_HANDLE_VALUE && !first && unsafe { GetLastError() } == ERROR_PIPE_BUSY {
            std::thread::sleep(std::time::Duration::from_millis(100));
            continue;
        }
        let pipe = Handle::new(raw)?;
        first = false;
        let connected = unsafe { ConnectNamedPipe(pipe.0, null_mut()) } != 0
            || unsafe { GetLastError() } == ERROR_PIPE_CONNECTED;
        if !connected {
            continue;
        }
        if stopping.load(Ordering::Acquire) {
            break;
        }
        let owner = owner.to_string();
        let handler = Arc::clone(&handler);
        std::thread::spawn(move || {
            let raw = pipe.0;
            std::mem::forget(pipe);
            let file = unsafe { File::from_raw_handle(raw) };
            let Ok(mut file) = DeadlinePipe::new(file, Duration::from_secs(30)) else {
                return;
            };
            let result: Result<(), String> = (|| {
                let request: WindowsAgentRequest =
                    wire::read_frame(&mut BufReader::new(&mut file))?.ok_or("missing request")?;
                // Impersonation follows the first read, so the token belongs to the actual sender.
                let sid = authenticated_sid(file.file.as_raw_handle())?;
                if sid != owner && sid != native::SERVICE_SID && sid != "S-1-5-18" {
                    return Err("unauthorized pipe client".into());
                }
                let request_id = request.request_id.clone();
                let result = if request.v != WINDOWS_AGENT_PROTOCOL
                    || request_id.is_empty()
                    || request_id.len() > 128
                {
                    error(
                        "ProtocolMismatch",
                        "invalid protocol version or request id",
                        false,
                    )
                } else {
                    handler(request)
                };
                file.deadline = Instant::now() + Duration::from_secs(30);
                wire::write_frame(
                    &mut file,
                    &WindowsAgentResponse {
                        v: 1,
                        request_id: request_id.clone(),
                        result,
                    },
                )?;
                let acknowledgement: Acknowledgement =
                    wire::read_frame(&mut BufReader::new(&mut file))?
                        .ok_or("pipe response was not acknowledged")?;
                if acknowledgement.request_id != request_id {
                    return Err("pipe acknowledgement mismatch".into());
                }
                Ok(())
            })();
            if let Err(error) = result {
                eprintln!("agent pipe request failed: {error}");
            }
            // Closing after the framed write avoids an unbounded FlushFileBuffers on a vanished SSH peer.
            unsafe {
                DisconnectNamedPipe(file.file.as_raw_handle());
            }
        });
    }
    Ok(())
}

pub fn wake(root: &Path, session: Option<u32>) {
    let name = native::wide(&pipe_name(root, session));
    let raw = unsafe {
        CreateFileW(
            name.as_ptr(),
            0x12019b,
            0,
            std::ptr::null(),
            OPEN_EXISTING,
            0,
            null_mut(),
        )
    };
    if raw != INVALID_HANDLE_VALUE && !raw.is_null() {
        unsafe {
            CloseHandle(raw);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::supervisor::Core;
    use std::{path::PathBuf, sync::mpsc};

    struct Fixture {
        root: tempfile::TempDir,
        stopping: Arc<AtomicBool>,
        done: mpsc::Receiver<Result<(), String>>,
    }

    impl Fixture {
        fn start() -> Self {
            let root = tempfile::tempdir().unwrap();
            let owner = native::own_sid().unwrap();
            let path = root.path().to_path_buf();
            let core = Core::new(path.clone(), owner.clone(), None);
            let stopping = Arc::new(AtomicBool::new(false));
            let server_stop = Arc::clone(&stopping);
            let (sender, done) = mpsc::channel();
            std::thread::spawn(move || {
                let handler = Arc::new(move |request| core.handle(request));
                let _ = sender.send(serve(&path, None, &owner, server_stop, handler));
            });
            Self {
                root,
                stopping,
                done,
            }
        }

        fn connect(&self) -> File {
            let name = native::wide(&pipe_name(self.root.path(), None));
            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                let raw = unsafe {
                    CreateFileW(
                        name.as_ptr(),
                        0x12019b,
                        0,
                        std::ptr::null(),
                        OPEN_EXISTING,
                        SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION,
                        null_mut(),
                    )
                };
                if raw != INVALID_HANDLE_VALUE && !raw.is_null() {
                    let mut server = 0;
                    assert_ne!(unsafe { GetNamedPipeServerProcessId(raw, &mut server) }, 0);
                    assert_eq!(server, std::process::id());
                    return unsafe { File::from_raw_handle(raw) };
                }
                assert!(
                    Instant::now() < deadline,
                    "temporary pipe server did not become ready: {}",
                    native::last_error()
                );
                std::thread::sleep(Duration::from_millis(10));
            }
        }

        fn call(&self, id: &str, action: WindowsAgentAction) -> WindowsAgentResult {
            exchange(
                self.connect(),
                &WindowsAgentRequest {
                    v: 1,
                    request_id: id.into(),
                    action,
                },
            )
            .unwrap()
            .result
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            self.stopping.store(true, Ordering::Release);
            for _ in 0..20 {
                wake(self.root.path(), None);
                if self.done.recv_timeout(Duration::from_millis(50)).is_ok() {
                    break;
                }
            }
        }
    }

    #[test]
    fn isolated_pipe_preserves_generation_and_stop_intent_without_spawning() {
        let fixture = Fixture::start();
        let spec = WindowsWorkloadSpec {
            instance_id: "isolated-no-executable".into(),
            command: WindowsCommandSpec {
                program: PathBuf::from("Z:\\nonexistent.exe")
                    .to_string_lossy()
                    .into_owned(),
                ..Default::default()
            },
            interactive: false,
            session_id: None,
            restart_on_failure: false,
            service_owned: false,
        };
        let WindowsAgentResult::Workload(first) =
            fixture.call("launch", WindowsAgentAction::Launch(spec.clone()))
        else {
            panic!("expected durable queued workload");
        };
        assert_eq!(first.state, WindowsWorkloadState::WaitingForUserSession);
        assert_eq!(first.pid, None);
        let WindowsAgentResult::Workload(repeated) =
            fixture.call("launch", WindowsAgentAction::Launch(spec))
        else {
            panic!("expected idempotent reply");
        };
        assert_eq!(first.generation, repeated.generation);
        let stale = fixture.call(
            "stale-stop",
            WindowsAgentAction::Stop {
                instance_id: first.instance_id.clone(),
                generation: "stale".into(),
            },
        );
        assert!(
            matches!(stale, WindowsAgentResult::Error(error) if error.code == "GenerationConflict")
        );
        let stopped = fixture.call(
            "stop",
            WindowsAgentAction::Stop {
                instance_id: first.instance_id.clone(),
                generation: first.generation,
            },
        );
        assert!(
            matches!(stopped, WindowsAgentResult::Workload(status) if status.state == WindowsWorkloadState::Stopped)
        );
        let entry: crate::store::Entry = crate::store::read(&crate::store::entry_path(
            fixture.root.path(),
            &first.instance_id,
        ))
        .unwrap();
        assert!(!entry.desired_running);
        assert_eq!(entry.runner_pid, None);
    }

    #[test]
    fn production_client_rejects_an_uninstalled_pipe_server() {
        let fixture = Fixture::start();
        drop(fixture.connect());
        let request_value = WindowsAgentRequest {
            v: 1,
            request_id: "spoofed-server".into(),
            action: WindowsAgentAction::Hello,
        };
        assert!(request(fixture.root.path(), None, &request_value).is_err());
    }

    #[test]
    fn protocol_mismatch_never_reaches_mutating_handler() {
        let fixture = Fixture::start();
        let request_value = WindowsAgentRequest {
            v: 99,
            request_id: "wrong-version".into(),
            action: WindowsAgentAction::List,
        };
        let result = exchange(fixture.connect(), &request_value).unwrap().result;
        assert!(
            matches!(result, WindowsAgentResult::Error(error) if error.code == "ProtocolMismatch")
        );
        assert!(!fixture.root.path().join("state").exists());
    }
}
