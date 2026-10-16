//! Windows 身份和进程句柄封装；FFI 对象按所有权释放。
#![expect(unsafe_code, reason = "Windows token, process and ACL APIs")]
use ncd_domain::windows_remote::*;
use std::{
    path::{Path, PathBuf},
    ptr::null_mut,
};
use windows_sys::Win32::System::RemoteDesktop::{
    ProcessIdToSessionId, WTS_SESSIONSTATE_LOCK, WTS_SESSIONSTATE_UNLOCK, WTSFreeMemory,
    WTSINFOEXW, WTSQuerySessionInformationW, WTSSessionInfoEx,
};
use windows_sys::Win32::{
    Foundation::*,
    Security::{Authorization::*, *},
    Storage::FileSystem::*,
    System::Threading::*,
};

pub const SERVICE_SID: &str = "S-1-5-19";
pub fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}
pub fn last_error() -> String {
    std::io::Error::last_os_error().to_string()
}
pub struct Handle(pub HANDLE);
unsafe impl Send for Handle {}
unsafe impl Sync for Handle {}
impl Handle {
    pub fn new(value: HANDLE) -> Result<Self, String> {
        if value.is_null() || value == INVALID_HANDLE_VALUE {
            Err(last_error())
        } else {
            Ok(Self(value))
        }
    }
}
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

pub fn token_sid(token: HANDLE) -> Result<String, String> {
    unsafe {
        let mut length = 0;
        GetTokenInformation(token, TokenUser, null_mut(), 0, &mut length);
        if length == 0 || length > 65536 {
            return Err("invalid user token".into());
        }
        let mut bytes = vec![0usize; (length as usize).div_ceil(std::mem::size_of::<usize>())];
        if GetTokenInformation(
            token,
            TokenUser,
            bytes.as_mut_ptr().cast(),
            length,
            &mut length,
        ) == 0
        {
            return Err(last_error());
        }
        let user = &*bytes.as_ptr().cast::<TOKEN_USER>();
        let mut text = null_mut();
        if ConvertSidToStringSidW(user.User.Sid, &mut text) == 0 {
            return Err(last_error());
        }
        let mut size = 0;
        while *text.add(size) != 0 {
            size += 1;
        }
        let sid = String::from_utf16_lossy(std::slice::from_raw_parts(text, size));
        LocalFree(text.cast());
        Ok(sid)
    }
}

pub fn process_sid(pid: u32) -> Result<String, String> {
    unsafe {
        let process = Handle::new(OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid))?;
        let mut token = null_mut();
        if OpenProcessToken(process.0, TOKEN_QUERY, &mut token) == 0 {
            return Err(last_error());
        }
        let token = Handle::new(token)?;
        token_sid(token.0)
    }
}
pub fn own_sid() -> Result<String, String> {
    process_sid(std::process::id())
}
pub fn session_id(pid: u32) -> Result<u32, String> {
    let mut id = 0;
    if unsafe { ProcessIdToSessionId(pid, &mut id) } == 0 {
        Err(last_error())
    } else {
        Ok(id)
    }
}

pub fn session_locked(session: u32) -> Option<bool> {
    unsafe {
        let mut buffer = null_mut();
        let mut length = 0;
        if WTSQuerySessionInformationW(
            null_mut(),
            session,
            WTSSessionInfoEx,
            &mut buffer,
            &mut length,
        ) == 0
        {
            return None;
        }
        struct Buffer(*mut core::ffi::c_void);
        impl Drop for Buffer {
            fn drop(&mut self) {
                unsafe {
                    WTSFreeMemory(self.0);
                }
            }
        }
        let _buffer = Buffer(buffer.cast());
        if buffer.is_null() || (length as usize) < std::mem::size_of::<WTSINFOEXW>() {
            return None;
        }
        let info = &*buffer.cast::<WTSINFOEXW>();
        if info.Level != 1 || info.Data.WTSInfoExLevel1.SessionId != session {
            return None;
        }
        match info.Data.WTSInfoExLevel1.SessionFlags as u32 {
            WTS_SESSIONSTATE_LOCK => Some(true),
            WTS_SESSIONSTATE_UNLOCK => Some(false),
            _ => None,
        }
    }
}
pub fn elevated() -> Result<bool, String> {
    unsafe {
        let mut raw = null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut raw) == 0 {
            return Err(last_error());
        }
        let token = Handle::new(raw)?;
        let mut elevation: TOKEN_ELEVATION = std::mem::zeroed();
        let mut length = 0;
        if GetTokenInformation(
            token.0,
            TokenElevation,
            (&mut elevation as *mut TOKEN_ELEVATION).cast(),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut length,
        ) == 0
        {
            return Err(last_error());
        }
        Ok(elevation.TokenIsElevated != 0)
    }
}
pub fn created(pid: u32) -> Result<u64, String> {
    unsafe {
        let process = Handle::new(OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid))?;
        let (mut creation, mut exit, mut kernel, mut user) = (
            std::mem::zeroed(),
            std::mem::zeroed(),
            std::mem::zeroed(),
            std::mem::zeroed(),
        );
        if GetProcessTimes(process.0, &mut creation, &mut exit, &mut kernel, &mut user) == 0 {
            return Err(last_error());
        }
        Ok((u64::from(creation.dwHighDateTime) << 32) | u64::from(creation.dwLowDateTime))
    }
}

pub fn process_identity(pid: u32) -> Result<Option<(u64, u32)>, String> {
    unsafe {
        let raw = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | 0x0010_0000, 0, pid);
        if raw.is_null() {
            return if GetLastError() == ERROR_INVALID_PARAMETER {
                Ok(None)
            } else {
                Err(last_error())
            };
        }
        let process = Handle::new(raw)?;
        let (mut creation, mut exit, mut kernel, mut user) = (
            std::mem::zeroed(),
            std::mem::zeroed(),
            std::mem::zeroed(),
            std::mem::zeroed(),
        );
        if GetProcessTimes(process.0, &mut creation, &mut exit, &mut kernel, &mut user) == 0 {
            return Err(last_error());
        }
        let session = session_id(pid)?;
        match WaitForSingleObject(process.0, 0) {
            WAIT_OBJECT_0 => Ok(None),
            WAIT_TIMEOUT => Ok(Some((
                (u64::from(creation.dwHighDateTime) << 32) | u64::from(creation.dwLowDateTime),
                session,
            ))),
            _ => Err(last_error()),
        }
    }
}
pub fn process_path(pid: u32) -> Result<PathBuf, String> {
    unsafe {
        let process = Handle::new(OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid))?;
        let mut buffer = vec![0u16; 32768];
        let mut length = buffer.len() as u32;
        if QueryFullProcessImageNameW(process.0, 0, buffer.as_mut_ptr(), &mut length) == 0 {
            return Err(last_error());
        }
        Ok(PathBuf::from(String::from_utf16_lossy(
            &buffer[..length as usize],
        )))
    }
}
pub fn alive(pid: u32, creation: u64) -> bool {
    if !created(pid).is_ok_and(|value| value == creation) {
        return false;
    }
    unsafe {
        let Ok(process) = Handle::new(OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid))
        else {
            return false;
        };
        let mut code = 0;
        GetExitCodeProcess(process.0, &mut code) != 0 && code == STILL_ACTIVE as u32
    }
}

pub fn replace_file(source: &Path, destination: &Path) -> Result<(), String> {
    let source = wide(&source.to_string_lossy());
    let destination = wide(&destination.to_string_lossy());
    if unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    } == 0
    {
        Err(last_error())
    } else {
        Ok(())
    }
}

pub struct Descriptor(pub PSECURITY_DESCRIPTOR);
impl Descriptor {
    pub fn new(sddl: &str) -> Result<Self, String> {
        let text = wide(sddl);
        let mut raw = null_mut();
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                text.as_ptr(),
                1,
                &mut raw,
                null_mut(),
            )
        } == 0
        {
            return Err(last_error());
        }
        Ok(Self(raw))
    }
}
impl Drop for Descriptor {
    fn drop(&mut self) {
        unsafe {
            LocalFree(self.0);
        }
    }
}
pub fn set_acl(path: &Path, sddl: &str) -> Result<(), String> {
    let descriptor = Descriptor::new(sddl)?;
    let path = wide(&path.to_string_lossy());
    let information = DACL_SECURITY_INFORMATION
        | PROTECTED_DACL_SECURITY_INFORMATION
        | if sddl.starts_with("O:") {
            OWNER_SECURITY_INFORMATION
        } else {
            0
        };
    if unsafe { SetFileSecurityW(path.as_ptr(), information, descriptor.0) } == 0 {
        Err(last_error())
    } else {
        Ok(())
    }
}

pub fn publish_identity(owner: &str) -> Result<(), String> {
    // Query-only grants permit mutual pipe endpoint verification without debug privilege.
    let process_acl = Descriptor::new(&format!(
        "D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GA;;;{})(A;;0x101000;;;LS)(A;;0x101000;;;{owner})",
        own_sid()?
    ))?;
    let token_acl = Descriptor::new(&format!(
        "D:P(A;;GA;;;SY)(A;;GA;;;BA)(A;;GA;;;{})(A;;0x8;;;LS)(A;;0x8;;;{owner})",
        own_sid()?
    ))?;
    unsafe {
        if SetKernelObjectSecurity(
            GetCurrentProcess(),
            DACL_SECURITY_INFORMATION,
            process_acl.0,
        ) == 0
        {
            return Err(last_error());
        }
        let mut raw = null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY | 0x40000, &mut raw) == 0 {
            return Err(last_error());
        }
        let token = Handle::new(raw)?;
        if SetKernelObjectSecurity(token.0, DACL_SECURITY_INFORMATION, token_acl.0) == 0 {
            return Err(last_error());
        }
    }
    Ok(())
}

pub fn probe(root: &Path) -> Result<RemoteWindowsInfo, String> {
    let request = WindowsAgentRequest {
        v: 1,
        request_id: uuid::Uuid::new_v4().to_string(),
        action: WindowsAgentAction::Hello,
    };
    let agent = crate::ipc::request(root, None, &request)
        .ok()
        .and_then(|response| match response.result {
            WindowsAgentResult::Hello(status) => Some(status),
            _ => None,
        });
    let mut command = std::process::Command::new("cmd.exe");
    use std::os::windows::process::CommandExt;
    command
        .args(["/d", "/c", "ver"])
        .creation_flags(CREATE_NO_WINDOW);
    let os_version = command
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "Windows".into());
    let user_sid = own_sid()?;
    let user_root = root
        .join("users")
        .join(crate::store::key(&user_sid))
        .to_string_lossy()
        .into_owned();
    Ok(RemoteWindowsInfo {
        v: 1,
        os_version,
        architecture: std::env::consts::ARCH.into(),
        user_sid,
        username: std::env::var("USERNAME").unwrap_or_default(),
        session_id: session_id(std::process::id())?,
        elevated: elevated()?,
        home_dir: std::env::var("USERPROFILE").unwrap_or_default(),
        program_data: std::env::var("ProgramData").unwrap_or_default(),
        temp_dir: std::env::temp_dir().to_string_lossy().into_owned(),
        managed_root: root.to_string_lossy().into_owned(),
        user_root,
        agent,
    })
}

pub fn quote(value: &str) -> String {
    let mut output = String::from("\"");
    let mut slashes = 0;
    for ch in value.chars() {
        if ch == '\\' {
            slashes += 1;
            continue;
        }
        output.extend(std::iter::repeat_n(
            '\\',
            if ch == '"' { slashes * 2 + 1 } else { slashes },
        ));
        slashes = 0;
        output.push(ch);
    }
    output.extend(std::iter::repeat_n('\\', slashes * 2));
    output.push('"');
    output
}
