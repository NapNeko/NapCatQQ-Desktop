//! 独立 runner 持有 Job 和输出，Agent 断线只影响控制连接。
#![expect(unsafe_code, reason = "Suspended process creation and Job ownership")]
use crate::{
    native::{self, Handle},
    store::{self, Entry, FileLock},
};
use ncd_domain::windows_remote::WindowsWorkloadState;
use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    io::Write,
    os::windows::io::AsRawHandle,
    path::{Path, PathBuf},
    ptr::{null, null_mut},
    time::Duration,
};
use windows_sys::Win32::{
    Foundation::*,
    Security::*,
    System::{JobObjects::*, Threading::*},
};

pub fn run(root: PathBuf, instance: String, generation: String) -> Result<(), String> {
    let directory = store::instance_dir(&root, &instance);
    let _runner_lease = FileLock::acquire(&directory.join("runner.lock"))?;
    let path = store::entry_path(&root, &instance);
    let mut entry: Entry = store::read(&path)?;
    native::publish_identity(&entry.owner_sid)?;
    if entry.status.generation != generation || !entry.desired_running {
        return Ok(());
    }
    let runner_pid = std::process::id();
    mutate(&path, &generation, |entry| {
        entry.runner_pid = Some(runner_pid);
        entry.runner_created = native::created(runner_pid).ok();
    })?;
    let mut attempts = 0u32;
    loop {
        entry = store::read(&path)?;
        if entry.status.generation != generation || !entry.desired_running {
            return Ok(());
        }
        let log = directory.join(format!("{generation}.log"));
        let process = match spawn(&entry, &log) {
            Ok(process) => process,
            Err(error) => {
                mutate(&path, &generation, |entry| {
                    entry.status.state = WindowsWorkloadState::Failed;
                    entry.status.error = Some(error.clone());
                    entry.desired_running = false;
                })?;
                return Err(error);
            }
        };
        mutate(&path, &generation, |entry| {
            entry.status.pid = Some(process.pid);
            entry.process_created = native::created(process.pid).ok();
            entry.status.session_id = native::session_id(process.pid).ok();
            entry.status.state = WindowsWorkloadState::Running;
            entry.status.started_at = Some(chrono::Utc::now().to_rfc3339());
            entry.status.error = None;
        })?;
        let mut stopped = false;
        loop {
            let desired: Entry = store::read(&path)?;
            if !desired.desired_running || desired.status.generation != generation {
                unsafe {
                    TerminateJobObject(process.job.0, 0);
                }
                stopped = true;
            }
            if unsafe { WaitForSingleObject(process.process.0, 250) } == WAIT_OBJECT_0 {
                break;
            }
        }
        let mut code = 1;
        if unsafe { GetExitCodeProcess(process.process.0, &mut code) } == 0 {
            return Err(native::last_error());
        }
        drop(process);
        let retry = !stopped && code != 0 && entry.spec.restart_on_failure && attempts < 5;
        mutate(&path, &generation, |entry| {
            entry.status.state = if retry {
                WindowsWorkloadState::Starting
            } else if stopped || code == 0 {
                WindowsWorkloadState::Stopped
            } else {
                WindowsWorkloadState::Failed
            };
            entry.status.exit_code = Some(code as i32);
            entry.status.finished_at = Some(chrono::Utc::now().to_rfc3339());
            entry.status.pid = None;
            entry.process_created = None;
            if !retry {
                entry.desired_running = false;
                entry.runner_pid = None;
                entry.runner_created = None;
            }
        })?;
        if !retry {
            return Ok(());
        }
        attempts += 1;
        std::thread::sleep(Duration::from_secs((1u64 << attempts).min(30)));
    }
}

fn mutate(path: &Path, generation: &str, apply: impl FnOnce(&mut Entry)) -> Result<(), String> {
    let _lock = FileLock::acquire(&path.with_file_name("entry.lock"))?;
    let mut entry: Entry = store::read(path)?;
    if entry.status.generation != generation {
        return Err("runner generation changed".into());
    }
    apply(&mut entry);
    store::write(path, &entry)
}

struct Process {
    process: Handle,
    job: Handle,
    pid: u32,
}

pub fn worker_job() -> Result<Handle, String> {
    unsafe {
        let job = Handle::new(CreateJobObjectW(null(), null()))?;
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if SetInformationJobObject(
            job.0,
            JobObjectExtendedLimitInformation,
            (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            std::mem::size_of_val(&limits) as u32,
        ) == 0
        {
            return Err(native::last_error());
        }
        if AssignProcessToJobObject(job.0, GetCurrentProcess()) == 0 {
            return Err(native::last_error());
        }
        Ok(job)
    }
}

fn spawn(entry: &Entry, log_path: &Path) -> Result<Process, String> {
    crate::supervisor::validate_spec(&entry.spec)?;
    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
        .map_err(|e| e.to_string())?;
    let stdin_path = log_path.with_extension("stdin");
    struct InputCleanup(PathBuf);
    impl Drop for InputCleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    let _input_cleanup = InputCleanup(stdin_path.clone());
    let mut input = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&stdin_path)
        .map_err(|e| e.to_string())?;
    input
        .write_all(entry.spec.command.stdin.as_deref().unwrap_or("").as_bytes())
        .map_err(|e| e.to_string())?;
    drop(input);
    let input = File::open(&stdin_path).map_err(|e| e.to_string())?;
    let mut environment: BTreeMap<String, String> = std::env::vars()
        .map(|(key, value)| (key.to_uppercase(), value))
        .collect();
    for (key, value) in &entry.spec.command.environment {
        environment.insert(key.to_uppercase(), value.clone());
    }
    let mut block = Vec::new();
    for (key, value) in environment {
        block.extend(format!("{key}={value}").encode_utf16());
        block.push(0);
    }
    block.push(0);
    let program = resolve_program(&entry.spec.command.program, &entry.spec.command.environment)?;
    let application = native::wide(&program.to_string_lossy());
    let command = std::iter::once(program.to_string_lossy().into_owned())
        .chain(entry.spec.command.args.iter().cloned())
        .map(|arg| native::quote(&arg))
        .collect::<Vec<_>>()
        .join(" ");
    let mut command = native::wide(&command);
    let cwd = entry.spec.command.working_dir.as_deref().map(native::wide);
    unsafe {
        let job = Handle::new(CreateJobObjectW(null(), null()))?;
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if SetInformationJobObject(
            job.0,
            JobObjectExtendedLimitInformation,
            (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            std::mem::size_of_val(&limits) as u32,
        ) == 0
        {
            return Err(native::last_error());
        }
        if SetHandleInformation(
            log.as_raw_handle(),
            HANDLE_FLAG_INHERIT,
            HANDLE_FLAG_INHERIT,
        ) == 0
            || SetHandleInformation(
                input.as_raw_handle(),
                HANDLE_FLAG_INHERIT,
                HANDLE_FLAG_INHERIT,
            ) == 0
        {
            return Err(native::last_error());
        }
        let mut startup: STARTUPINFOW = std::mem::zeroed();
        startup.cb = std::mem::size_of_val(&startup) as u32;
        startup.dwFlags = STARTF_USESTDHANDLES;
        startup.hStdInput = input.as_raw_handle();
        startup.hStdOutput = log.as_raw_handle();
        startup.hStdError = log.as_raw_handle();
        let mut info: PROCESS_INFORMATION = std::mem::zeroed();
        let flags = CREATE_SUSPENDED | CREATE_NO_WINDOW | CREATE_UNICODE_ENVIRONMENT;
        let result = if native::own_sid()? == native::SERVICE_SID {
            let mut raw = null_mut();
            if OpenProcessToken(
                GetCurrentProcess(),
                TOKEN_DUPLICATE | TOKEN_QUERY | TOKEN_ASSIGN_PRIMARY,
                &mut raw,
            ) == 0
            {
                return Err(native::last_error());
            }
            let token = Handle::new(raw)?;
            let mut restricted = null_mut();
            if CreateRestrictedToken(
                token.0,
                DISABLE_MAX_PRIVILEGE,
                0,
                null(),
                0,
                null(),
                0,
                null(),
                &mut restricted,
            ) == 0
            {
                return Err(native::last_error());
            }
            let restricted = Handle::new(restricted)?;
            CreateProcessAsUserW(
                restricted.0,
                application.as_ptr(),
                command.as_mut_ptr(),
                null(),
                null(),
                1,
                flags,
                block.as_ptr().cast(),
                cwd.as_ref().map_or(null(), |s| s.as_ptr()),
                &startup,
                &mut info,
            )
        } else {
            if native::elevated()? {
                return Err("refusing to run workload with an elevated user token".into());
            }
            CreateProcessW(
                application.as_ptr(),
                command.as_mut_ptr(),
                null(),
                null(),
                1,
                flags,
                block.as_ptr().cast(),
                cwd.as_ref().map_or(null(), |s| s.as_ptr()),
                &startup,
                &mut info,
            )
        };
        if result == 0 {
            return Err(native::last_error());
        }
        let process = Handle::new(info.hProcess)?;
        let thread = Handle::new(info.hThread)?;
        if AssignProcessToJobObject(job.0, process.0) == 0 {
            let error = native::last_error();
            TerminateProcess(process.0, 1);
            return Err(error);
        }
        if ResumeThread(thread.0) == u32::MAX {
            let error = native::last_error();
            TerminateJobObject(job.0, 1);
            return Err(error);
        }
        Ok(Process {
            process,
            job,
            pid: info.dwProcessId,
        })
    }
}

fn resolve_program(
    program: &str,
    environment: &BTreeMap<String, String>,
) -> Result<PathBuf, String> {
    let candidate = PathBuf::from(program);
    if candidate.is_absolute() {
        // Canonicalization introduces a verbatim prefix that changes legacy .NET app-config resolution.
        return if candidate.is_file() {
            Ok(candidate)
        } else {
            Err(format!("executable not found: {program}"))
        };
    }
    let path = environment
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case("PATH"))
        .map(|(_, value)| value.clone())
        .or_else(|| std::env::var("PATH").ok())
        .unwrap_or_default();
    for directory in std::env::split_paths(&path) {
        for name in [program.to_string(), format!("{program}.exe")] {
            let candidate = directory.join(name);
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }
    Err(format!("executable not found: {program}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use ncd_domain::windows_remote::*;
    use std::time::Instant;

    fn encoded(script: &str) -> String {
        let bytes: Vec<_> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
        base64::engine::general_purpose::STANDARD.encode(bytes)
    }

    fn fixture(root: &Path) -> Entry {
        let powershell =
            PathBuf::from(std::env::var_os("SystemRoot").expect("Windows system root"))
                .join("System32/WindowsPowerShell/v1.0/powershell.exe");
        assert!(
            powershell.is_file(),
            "Windows PowerShell is required for the isolated process fixture"
        );
        let child = encoded("Start-Sleep -Seconds 45");
        let parent = format!(
            r#"
$ErrorActionPreference='Stop'
$start=New-Object System.Diagnostics.ProcessStartInfo
$start.FileName=[Diagnostics.Process]::GetCurrentProcess().MainModule.FileName
$start.Arguments='-NoLogo -NoProfile -NonInteractive -EncodedCommand {child}'
$start.UseShellExecute=$false
$start.CreateNoWindow=$true
$child=[Diagnostics.Process]::Start($start)
[Console]::WriteLine('NCD_CHILD='+$child.Id)
[Console]::WriteLine('NCD_RUNNER_READY')
[Console]::Out.Flush()
Start-Sleep -Seconds 45
"#
        );
        let instance_id = "isolated-win32-job".to_string();
        Entry {
            spec: WindowsWorkloadSpec {
                instance_id: instance_id.clone(),
                command: WindowsCommandSpec {
                    program: powershell.to_string_lossy().into_owned(),
                    args: vec![
                        "-NoLogo".into(),
                        "-NoProfile".into(),
                        "-NonInteractive".into(),
                        "-EncodedCommand".into(),
                        encoded(&parent),
                    ],
                    working_dir: Some(root.to_string_lossy().into_owned()),
                    ..Default::default()
                },
                interactive: false,
                session_id: None,
                restart_on_failure: false,
                service_owned: false,
            },
            status: WindowsWorkloadStatus {
                instance_id,
                generation: uuid::Uuid::new_v4().to_string(),
                pid: None,
                session_id: None,
                state: WindowsWorkloadState::Starting,
                exit_code: None,
                error: None,
                started_at: None,
                finished_at: None,
            },
            desired_running: true,
            owner_sid: native::own_sid().unwrap(),
            runner_pid: None,
            runner_created: None,
            process_created: None,
        }
    }

    fn wait_until(label: &str, mut condition: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(15);
        while !condition() {
            assert!(
                Instant::now() < deadline,
                "isolated runner condition timed out: {label}"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn child_from_log(log: &Path) -> u32 {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let text = std::fs::read_to_string(log).unwrap_or_default();
            if text.contains("NCD_RUNNER_READY") {
                if let Some(child) = text.lines().find_map(|line| {
                    line.strip_prefix("NCD_CHILD=")
                        .and_then(|pid| pid.trim().parse().ok())
                }) {
                    return child;
                }
            }
            if Instant::now() >= deadline {
                let bytes = std::fs::read(log);
                panic!(
                    "runner fixture produced no ready child at {}: {:?}",
                    log.display(),
                    bytes.map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
                );
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn process_handle(pid: u32) -> Handle {
        Handle::new(unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | 0x0010_0000, 0, pid) })
            .unwrap()
    }

    struct RunningThread {
        path: PathBuf,
        generation: String,
        thread: Option<std::thread::JoinHandle<Result<(), String>>>,
    }
    impl RunningThread {
        fn start(root: &Path, entry: &Entry) -> Self {
            let path = store::entry_path(root, &entry.spec.instance_id);
            store::write(&path, entry).unwrap();
            let instance = entry.spec.instance_id.clone();
            let generation = entry.status.generation.clone();
            let runner_generation = generation.clone();
            let root = root.to_path_buf();
            let thread = std::thread::spawn(move || run(root, instance, runner_generation));
            Self {
                path,
                generation,
                thread: Some(thread),
            }
        }

        fn finish(&mut self) {
            wait_until("runner thread completion", || {
                self.thread.as_ref().unwrap().is_finished()
            });
            self.thread.take().unwrap().join().unwrap().unwrap();
        }
    }
    impl Drop for RunningThread {
        fn drop(&mut self) {
            // A failed assertion still clears only this temporary instance's exact generation.
            let _ = mutate(&self.path, &self.generation, |entry| {
                entry.desired_running = false
            });
            if let Some(thread) = self.thread.take() {
                let deadline = Instant::now() + Duration::from_secs(5);
                while !thread.is_finished() && Instant::now() < deadline {
                    std::thread::sleep(Duration::from_millis(20));
                }
                if thread.is_finished() {
                    let _ = thread.join();
                }
            }
        }
    }

    #[test]
    fn real_hidden_process_job_closure_and_generation_stop_are_isolated() {
        let elevated = native::elevated().unwrap();
        if let Ok(owner) = std::env::var("NCD_AGENT_TEST_OWNER_SID") {
            assert_eq!(native::own_sid().unwrap(), owner);
            assert!(
                !elevated,
                "the CI owner fixture must use a standard account"
            );
        }
        if elevated {
            let root = tempfile::tempdir().unwrap();
            let entry = fixture(root.path());
            assert!(matches!(
                spawn(&entry, &root.path().join("elevated.log")),
                Err(error) if error == "refusing to run workload with an elevated user token"
            ));
            return;
        }
        let root = tempfile::tempdir().unwrap();
        let entry = fixture(root.path());

        let drop_log = root.path().join("job-drop.log");
        let process = spawn(&entry, &drop_log).unwrap();
        let child_pid = child_from_log(&drop_log);
        let parent = process_handle(process.pid);
        let child = process_handle(child_pid);
        for member in [&parent, &child] {
            let mut in_job = 0;
            assert_ne!(
                unsafe { IsProcessInJob(member.0, process.job.0, &mut in_job) },
                0
            );
            assert_ne!(
                in_job, 0,
                "both fixture processes must belong to the runner's exact Job"
            );
        }
        drop(process);
        assert_eq!(
            unsafe { WaitForSingleObject(parent.0, 5000) },
            WAIT_OBJECT_0
        );
        assert_eq!(unsafe { WaitForSingleObject(child.0, 5000) }, WAIT_OBJECT_0);

        let mut runner = RunningThread::start(root.path(), &entry);
        let log = store::instance_dir(root.path(), &entry.spec.instance_id)
            .join(format!("{}.log", entry.status.generation));
        let child_pid = child_from_log(&log);
        let running: Entry = store::read(&runner.path).unwrap();
        assert_eq!(running.status.state, WindowsWorkloadState::Running);
        let parent_pid = running.status.pid.unwrap();
        let parent = process_handle(parent_pid);
        let child = process_handle(child_pid);

        let request = |id: &str, generation: &str| WindowsAgentRequest {
            v: 1,
            request_id: id.into(),
            action: WindowsAgentAction::Stop {
                instance_id: entry.spec.instance_id.clone(),
                generation: generation.into(),
            },
        };
        let core = crate::supervisor::Core::new(
            root.path().to_path_buf(),
            entry.owner_sid.clone(),
            Some(native::session_id(std::process::id()).unwrap()),
        );
        let stale = core.handle(request("stale-stop", "previous-generation"));
        assert!(
            matches!(stale, WindowsAgentResult::Error(error) if error.code == "GenerationConflict")
        );
        assert_eq!(unsafe { WaitForSingleObject(parent.0, 0) }, WAIT_TIMEOUT);
        assert!(store::read::<Entry>(&runner.path).unwrap().desired_running);

        // A reconstructed controller observes the existing runner rather than spawning another.
        core.recover().unwrap();
        assert_eq!(
            store::read::<Entry>(&runner.path).unwrap().status.pid,
            Some(parent_pid)
        );
        let stopped = core.handle(request("exact-stop", &entry.status.generation));
        assert!(
            matches!(stopped, WindowsAgentResult::Workload(status) if status.state == WindowsWorkloadState::Stopping)
        );
        runner.finish();
        assert_eq!(
            unsafe { WaitForSingleObject(parent.0, 5000) },
            WAIT_OBJECT_0
        );
        assert_eq!(unsafe { WaitForSingleObject(child.0, 5000) }, WAIT_OBJECT_0);
        let terminal: Entry = store::read(&runner.path).unwrap();
        assert!(!terminal.desired_running);
        assert_eq!(terminal.status.state, WindowsWorkloadState::Stopped);
        assert_eq!(terminal.status.generation, entry.status.generation);
        assert_eq!(terminal.status.pid, None);
        assert_eq!(terminal.runner_pid, None);
        assert!(
            std::fs::read_to_string(log)
                .unwrap()
                .contains("NCD_RUNNER_READY")
        );
    }
}
