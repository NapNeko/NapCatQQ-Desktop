//! 安装只在已提权目标进程中执行；服务身份与登录用户身份分别授权。
#![expect(unsafe_code, reason = "SCM installation and protected Windows ACLs")]
use crate::{native, service, store};
use serde::{Deserialize, Serialize};
use std::os::windows::{fs::MetadataExt, process::CommandExt};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    ptr::{null, null_mut},
    time::{Duration, Instant},
};
use windows_sys::Win32::{Foundation::*, System::Services::*};

#[derive(Clone, Serialize, Deserialize)]
struct Installation {
    owner_sid: String,
    executable: PathBuf,
}

pub fn owner_sid(root: &Path) -> Result<String, String> {
    Ok(store::read::<Installation>(&root.join("agent/installation.json"))?.owner_sid)
}

pub fn session_worker_update_pending(root: &Path, pid: u32) -> bool {
    let current = native::process_path(pid);
    current
        .and_then(|current| worker_executable_differs(root, &current))
        .unwrap_or(true)
}

fn worker_executable_differs(root: &Path, current: &Path) -> Result<bool, String> {
    let installed: Installation = store::read(&root.join("agent/installation.json"))?;
    let expected = fs::canonicalize(&installed.executable).map_err(|e| e.to_string())?;
    let current = fs::canonicalize(current).map_err(|e| e.to_string())?;
    Ok(!expected
        .to_string_lossy()
        .eq_ignore_ascii_case(&current.to_string_lossy()))
}

fn require_admin() -> Result<(), String> {
    if !native::elevated()? {
        return Err("ElevationRequired: run Agent install in an elevated target session".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_update_compares_the_running_version_with_the_installed_version() {
        let root = tempfile::tempdir().unwrap();
        let old = root.path().join("agent/versions/old/ncd-agent.exe");
        let new = root.path().join("agent/versions/new/ncd-agent.exe");
        fs::create_dir_all(old.parent().unwrap()).unwrap();
        fs::create_dir_all(new.parent().unwrap()).unwrap();
        fs::write(&old, []).unwrap();
        fs::write(&new, []).unwrap();
        store::write(
            &root.path().join("agent/installation.json"),
            &Installation {
                owner_sid: "S-1-5-21-test".into(),
                executable: new.clone(),
            },
        )
        .unwrap();
        assert!(worker_executable_differs(root.path(), &old).unwrap());
        assert!(!worker_executable_differs(root.path(), &new).unwrap());
    }

    #[test]
    fn unavailable_installation_record_conservatively_reports_a_pending_update() {
        let root = tempfile::tempdir().unwrap();
        assert!(session_worker_update_pending(
            root.path(),
            std::process::id()
        ));
    }
}

fn validate_directory(path: &Path) -> Result<(), String> {
    if !path.is_absolute() {
        return Err("managed root must be an absolute Windows directory".into());
    }
    let mut cursor = Some(path);
    while let Some(item) = cursor {
        if let Ok(metadata) = fs::symlink_metadata(item) {
            if metadata.file_attributes() & 0x400 != 0 {
                return Err(format!(
                    "managed path contains a reparse point: {}",
                    item.display()
                ));
            }
        }
        cursor = item.parent();
    }
    Ok(())
}

fn directory(path: &Path, sddl: &str) -> Result<(), String> {
    validate_directory(path)?;
    fs::create_dir_all(path).map_err(|e| e.to_string())?;
    native::set_acl(path, sddl)
}

struct ServiceHandle(SC_HANDLE);
impl ServiceHandle {
    fn new(handle: SC_HANDLE) -> Result<Self, String> {
        if handle.is_null() {
            Err(native::last_error())
        } else {
            Ok(Self(handle))
        }
    }
}
impl Drop for ServiceHandle {
    fn drop(&mut self) {
        unsafe {
            CloseServiceHandle(self.0);
        }
    }
}

pub fn install(root: &Path, requested_owner: Option<&str>) -> Result<(), String> {
    require_admin()?;
    validate_directory(root)?;
    let existing: Option<Installation> = store::read(&root.join("agent/installation.json")).ok();
    let owner = requested_owner
        .map(str::to_string)
        .or_else(|| existing.as_ref().map(|i| i.owner_sid.clone()))
        .unwrap_or(native::own_sid()?);
    if !owner.starts_with("S-1-")
        || owner
            .chars()
            .any(|c| !c.is_ascii_digit() && c != 'S' && c != '-')
    {
        return Err("invalid owner SID".into());
    }
    if existing.as_ref().is_some_and(|i| i.owner_sid != owner) {
        return Err("changing the owner requires an explicit ownership migration".into());
    }
    let root_acl = format!(
        "O:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;0x1201bf;;;LS)(A;;0x1201bf;;;{owner})"
    );
    directory(root, &root_acl)?;
    let protected =
        format!("O:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;FRFX;;;LS)(A;OICI;FRFX;;;{owner})");
    directory(&root.join("agent"), &protected)?;
    let writable =
        format!("O:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;FA;;;LS)(A;OICI;FA;;;{owner})");
    for name in [
        "agent/staging",
        "components",
        "cache",
        "tmp",
        "sessions",
        "users",
    ] {
        directory(&root.join(name), &writable)?;
    }
    directory(&root.join("users").join(store::key(&owner)), &writable)?;
    directory(
        &root.join("state"),
        &format!("O:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)(A;OICI;FA;;;LS)(A;OICI;FRFX;;;{owner})"),
    )?;
    let version = root
        .join("agent/versions")
        .join(uuid::Uuid::new_v4().to_string());
    directory(&version, &protected)?;
    let executable = version.join("ncd-agent.exe");
    fs::copy(
        std::env::current_exe().map_err(|e| e.to_string())?,
        &executable,
    )
    .map_err(|e| e.to_string())?;
    native::set_acl(&executable, &protected)?;
    let manager = ServiceHandle::new(unsafe {
        OpenSCManagerW(
            null(),
            null(),
            SC_MANAGER_CONNECT | SC_MANAGER_CREATE_SERVICE,
        )
    })?;
    let name = native::wide(service::NAME);
    let old = unsafe { OpenServiceW(manager.0, name.as_ptr(), SERVICE_ALL_ACCESS) };
    let service = if old.is_null() {
        if unsafe { GetLastError() } != ERROR_SERVICE_DOES_NOT_EXIST {
            return Err(native::last_error());
        }
        let command = native::wide(&format!(
            "{} service --root {}",
            native::quote(&executable.to_string_lossy()),
            native::quote(&root.to_string_lossy())
        ));
        let account = native::wide("NT AUTHORITY\\LocalService");
        ServiceHandle::new(unsafe {
            CreateServiceW(
                manager.0,
                name.as_ptr(),
                native::wide("NapCatQQ Desktop Remote Agent").as_ptr(),
                SERVICE_ALL_ACCESS,
                SERVICE_WIN32_OWN_PROCESS,
                SERVICE_AUTO_START,
                SERVICE_ERROR_NORMAL,
                command.as_ptr(),
                null(),
                null_mut(),
                null(),
                account.as_ptr(),
                null(),
            )
        })?
    } else {
        let service = ServiceHandle::new(old)?;
        stop_service(&service)?;
        let command = native::wide(&format!(
            "{} service --root {}",
            native::quote(&executable.to_string_lossy()),
            native::quote(&root.to_string_lossy())
        ));
        if unsafe {
            ChangeServiceConfigW(
                service.0,
                SERVICE_WIN32_OWN_PROCESS,
                SERVICE_AUTO_START,
                SERVICE_ERROR_NORMAL,
                command.as_ptr(),
                null(),
                null_mut(),
                null(),
                native::wide("NT AUTHORITY\\LocalService").as_ptr(),
                null(),
                null(),
            )
        } == 0
        {
            let error = native::last_error();
            unsafe {
                StartServiceW(service.0, 0, null());
            }
            return Err(error);
        }
        service
    };
    let mut rollback = InstallRollback {
        root: root.to_path_buf(),
        previous: existing,
        service: service.0,
        armed: true,
    };
    let config = Installation {
        owner_sid: owner,
        executable: executable.clone(),
    };
    store::write(&root.join("agent/installation.json"), &config)?;
    native::set_acl(&root.join("agent/installation.json"), &protected)?;
    directory(&root.join("agent/current"), &protected)?;
    let staging = root.join("agent/current/ncd-agent.new.exe");
    fs::copy(&executable, &staging).map_err(|e| e.to_string())?;
    native::set_acl(&staging, &protected)?;
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        match native::replace_file(&staging, &root.join("agent/current/ncd-agent.exe")) {
            Ok(()) => break,
            Err(error) if Instant::now() >= deadline => {
                return Err(format!("cannot replace bridge: {error}"));
            }
            Err(_) => std::thread::sleep(Duration::from_millis(250)),
        }
    }
    let mut actions = [
        SC_ACTION {
            Type: SC_ACTION_RESTART,
            Delay: 5000,
        },
        SC_ACTION {
            Type: SC_ACTION_RESTART,
            Delay: 15000,
        },
        SC_ACTION {
            Type: SC_ACTION_NONE,
            Delay: 0,
        },
    ];
    let failures = SERVICE_FAILURE_ACTIONSW {
        dwResetPeriod: 3600,
        lpRebootMsg: null_mut(),
        lpCommand: null_mut(),
        cActions: actions.len() as u32,
        lpsaActions: actions.as_mut_ptr(),
    };
    if unsafe {
        ChangeServiceConfig2W(
            service.0,
            SERVICE_CONFIG_FAILURE_ACTIONS,
            (&failures as *const SERVICE_FAILURE_ACTIONSW).cast(),
        )
    } == 0
    {
        return Err(native::last_error());
    }
    if unsafe { StartServiceW(service.0, 0, null()) } == 0
        && unsafe { GetLastError() } != ERROR_SERVICE_ALREADY_RUNNING
    {
        return Err(native::last_error());
    }
    wait_state(&service, SERVICE_RUNNING)?;
    let request = ncd_domain::windows_remote::WindowsAgentRequest {
        v: 1,
        request_id: uuid::Uuid::new_v4().to_string(),
        action: ncd_domain::windows_remote::WindowsAgentAction::Hello,
    };
    let response = crate::ipc::request(root, None, &request)?;
    if !matches!(
        response.result,
        ncd_domain::windows_remote::WindowsAgentResult::Hello(_)
    ) {
        return Err("new agent did not complete its protocol health check".into());
    }
    register_session(root)?;
    rollback.armed = false;
    Ok(())
}

struct InstallRollback {
    root: PathBuf,
    previous: Option<Installation>,
    service: SC_HANDLE,
    armed: bool,
}
impl Drop for InstallRollback {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        unsafe {
            let mut status: SERVICE_STATUS = std::mem::zeroed();
            ControlService(self.service, SERVICE_CONTROL_STOP, &mut status);
            if let Some(previous) = &self.previous {
                let command = native::wide(&format!(
                    "{} service --root {}",
                    native::quote(&previous.executable.to_string_lossy()),
                    native::quote(&self.root.to_string_lossy())
                ));
                ChangeServiceConfigW(
                    self.service,
                    SERVICE_WIN32_OWN_PROCESS,
                    SERVICE_AUTO_START,
                    SERVICE_ERROR_NORMAL,
                    command.as_ptr(),
                    null(),
                    null_mut(),
                    null(),
                    null(),
                    null(),
                    null(),
                );
                let _ = store::write(&self.root.join("agent/installation.json"), previous);
                let staging = self.root.join("agent/current/rollback.exe");
                if fs::copy(&previous.executable, &staging).is_ok() {
                    let _ = native::replace_file(
                        &staging,
                        &self.root.join("agent/current/ncd-agent.exe"),
                    );
                }
                StartServiceW(self.service, 0, null());
            } else {
                DeleteService(self.service);
                let _ = fs::remove_file(self.root.join("agent/installation.json"));
            }
        }
    }
}

fn stop_service(service: &ServiceHandle) -> Result<(), String> {
    let mut status: SERVICE_STATUS = unsafe { std::mem::zeroed() };
    if unsafe { ControlService(service.0, SERVICE_CONTROL_STOP, &mut status) } == 0
        && unsafe { GetLastError() } != ERROR_SERVICE_NOT_ACTIVE
    {
        return Err(native::last_error());
    }
    wait_state(service, SERVICE_STOPPED)
}

fn wait_state(service: &ServiceHandle, state: u32) -> Result<(), String> {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let mut status: SERVICE_STATUS = unsafe { std::mem::zeroed() };
        if unsafe { QueryServiceStatus(service.0, &mut status) } == 0 {
            return Err(native::last_error());
        }
        if status.dwCurrentState == state {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(format!("service did not reach state {state}"));
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

fn task_name(sid: &str) -> String {
    format!("NcdRemote-Session-{}", &store::key(sid)[..24])
}

fn powershell(script: &str) -> Result<(), String> {
    use base64::Engine;
    let bytes: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    let output = Command::new("powershell.exe")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-EncodedCommand",
            &encoded,
        ])
        .creation_flags(0x08000000)
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "scheduled task: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

pub fn register_session(root: &Path) -> Result<(), String> {
    let installation: Installation = store::read(&root.join("agent/installation.json"))?;
    if native::own_sid()? != installation.owner_sid && !native::elevated()? {
        return Err("session task must be registered by its owner or an administrator".into());
    }
    let payload = serde_json::json!({ "sid": installation.owner_sid, "name": task_name(&installation.owner_sid), "executable": installation.executable, "arguments": format!("session-worker --root {}", native::quote(&root.to_string_lossy())) });
    let json = serde_json::to_string(&payload)
        .map_err(|e| e.to_string())?
        .replace('\'', "''");
    powershell(&format!(
        r#"
$ErrorActionPreference='Stop'
$p='{json}' | ConvertFrom-Json
$principal=New-ScheduledTaskPrincipal -UserId $p.sid -LogonType Interactive -RunLevel Limited
$action=New-ScheduledTaskAction -Execute $p.executable -Argument $p.arguments
$trigger=New-ScheduledTaskTrigger -AtLogOn -User $p.sid
$settings=New-ScheduledTaskSettingsSet -ExecutionTimeLimit ([TimeSpan]::Zero) -MultipleInstances IgnoreNew -RestartCount 3 -RestartInterval (New-TimeSpan -Minutes 1) -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries
$null=Register-ScheduledTask -TaskName $p.name -Action $action -Trigger $trigger -Principal $principal -Settings $settings -Force
$scheduler=New-Object -ComObject Schedule.Service
$scheduler.Connect()
$registered=$scheduler.GetFolder('\').GetTask($p.name)
$registered.SetSecurityDescriptor(('D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GA;;;'+$p.sid+')'),0)
try {{ Start-ScheduledTask -TaskName $p.name -ErrorAction Stop }} catch {{ [Console]::Error.WriteLine('Session worker will start at the next interactive logon.') }}
"#
    ))
}

pub fn uninstall(root: &Path) -> Result<(), String> {
    require_admin()?;
    validate_directory(root)?;
    let installation: Installation = store::read(&root.join("agent/installation.json"))?;
    let user_root = root.join("users").join(store::key(&installation.owner_sid));
    for location in [root, user_root.as_path()] {
        if store::entries(location)?.iter().any(|entry| {
            entry.desired_running
                || matches!(
                    entry.status.state,
                    ncd_domain::windows_remote::WindowsWorkloadState::Running
                        | ncd_domain::windows_remote::WindowsWorkloadState::Starting
                        | ncd_domain::windows_remote::WindowsWorkloadState::Stopping
                        | ncd_domain::windows_remote::WindowsWorkloadState::Unknown
                )
        }) {
            return Err(
                "WorkloadRunning: stop managed workloads before uninstalling the agent".into(),
            );
        }
    }
    if crate::bot::has_managed_bots(root)? {
        return Err("WorkloadRunning: stop managed bots before uninstalling the agent".into());
    }
    let manager =
        ServiceHandle::new(unsafe { OpenSCManagerW(null(), null(), SC_MANAGER_CONNECT) })?;
    let name = native::wide(service::NAME);
    let raw = unsafe { OpenServiceW(manager.0, name.as_ptr(), SERVICE_ALL_ACCESS) };
    if !raw.is_null() {
        let service = ServiceHandle::new(raw)?;
        stop_service(&service)?;
        if unsafe { DeleteService(service.0) } == 0 {
            return Err(native::last_error());
        }
    } else if unsafe { GetLastError() } != ERROR_SERVICE_DOES_NOT_EXIST {
        return Err(native::last_error());
    }
    let name = task_name(&installation.owner_sid).replace('\'', "''");
    powershell(&format!(
        "$ErrorActionPreference='Stop';$task=Get-ScheduledTask -TaskName '{name}' -ErrorAction SilentlyContinue;if($null -ne $task){{Stop-ScheduledTask -TaskName '{name}';Unregister-ScheduledTask -TaskName '{name}' -Confirm:$false}}"
    ))?;
    for session in crate::supervisor::registrations(root, &installation.owner_sid)? {
        if native::process_path(session.pid).is_ok_and(|path| path.starts_with(root.join("agent")))
        {
            unsafe {
                let process =
                    native::Handle::new(windows_sys::Win32::System::Threading::OpenProcess(
                        windows_sys::Win32::System::Threading::PROCESS_TERMINATE,
                        0,
                        session.pid,
                    ))?;
                if native::alive(session.pid, session.created) {
                    windows_sys::Win32::System::Threading::TerminateProcess(process.0, 0);
                }
            }
        }
    }
    Ok(())
}
