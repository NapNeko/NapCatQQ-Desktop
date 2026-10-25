//! 用受保护 DACL 去除 ProgramData 的继承读取权限。

#![expect(
    unsafe_code,
    reason = "受审查的 Windows ACL FFI；每处调用保留 SAFETY 依据，并由实际 DACL 测试核对边界"
)]

use std::{ffi::c_void, mem::size_of, os::windows::ffi::OsStrExt, path::Path, ptr};

use windows_sys::Win32::{
    Foundation::{
        CloseHandle, ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS, GetLastError, HANDLE, LocalFree,
    },
    Security::{
        ACL,
        Authorization::{
            ConvertStringSidToSidW, EXPLICIT_ACCESS_W, NO_MULTIPLE_TRUSTEE, SE_FILE_OBJECT,
            SET_ACCESS, SetEntriesInAclW, SetNamedSecurityInfoW, TRUSTEE_IS_SID, TRUSTEE_IS_USER,
            TRUSTEE_IS_WELL_KNOWN_GROUP, TRUSTEE_W,
        },
        CONTAINER_INHERIT_ACE, DACL_SECURITY_INFORMATION, GetTokenInformation, OBJECT_INHERIT_ACE,
        PROTECTED_DACL_SECURITY_INFORMATION, PSID, TOKEN_QUERY, TOKEN_USER, TokenUser,
    },
    Storage::FileSystem::FILE_ALL_ACCESS,
    System::Threading::{GetCurrentProcess, OpenProcessToken},
};

struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: 句柄由 OpenProcessToken 成功返回，且只在这里释放一次。
        unsafe {
            CloseHandle(self.0);
        }
    }
}

struct LocalAllocation(*mut c_void);
impl Drop for LocalAllocation {
    fn drop(&mut self) {
        // SAFETY: 两个使用点均由 Win32 的 LocalAlloc 系列 API 分配。
        unsafe {
            LocalFree(self.0);
        }
    }
}

struct UserSid(Vec<usize>);
impl UserSid {
    fn get(&self) -> PSID {
        // SAFETY: 存储按 usize 对齐，GetTokenInformation 已写入完整 TOKEN_USER。
        unsafe { (*self.0.as_ptr().cast::<TOKEN_USER>()).User.Sid }
    }
}

fn failure(code: u32) -> String {
    format!("无法保护 MiniMax 登录目录的 Windows 权限（错误 {code}）")
}

fn current_user() -> Result<UserSid, String> {
    let mut token = ptr::null_mut();
    // SAFETY: 当前进程伪句柄有效，输出指针指向本地 HANDLE。
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        // SAFETY: 在失败的 Win32 调用之后读取线程错误码。
        return Err(failure(unsafe { GetLastError() }));
    }
    let token = Handle(token);
    let mut required = 0;
    // SAFETY: 首次只查询所需大小，不提供输出缓冲区。
    let measured =
        unsafe { GetTokenInformation(token.0, TokenUser, ptr::null_mut(), 0, &mut required) };
    // SAFETY: 紧跟失败调用读取其错误码。
    if measured == 0 && unsafe { GetLastError() } != ERROR_INSUFFICIENT_BUFFER {
        return Err("无法取得 MiniMax 登录目录的当前用户标识".into());
    }
    if required < size_of::<TOKEN_USER>() as u32 || required > 64 * 1024 {
        return Err("Windows 返回了无效的当前用户标识大小".into());
    }
    let words = (required as usize + size_of::<usize>() - 1) / size_of::<usize>();
    let mut storage = vec![0_usize; words];
    // SAFETY: 对齐缓冲区至少 required 字节，输出长度指针有效。
    if unsafe {
        GetTokenInformation(
            token.0,
            TokenUser,
            storage.as_mut_ptr().cast(),
            required,
            &mut required,
        )
    } == 0
    {
        // SAFETY: 紧跟失败调用读取其错误码。
        return Err(failure(unsafe { GetLastError() }));
    }
    Ok(UserSid(storage))
}

fn system_sid() -> Result<LocalAllocation, String> {
    let text = "S-1-5-18\0".encode_utf16().collect::<Vec<_>>();
    let mut sid = ptr::null_mut();
    // SAFETY: text 有终止符，输出是 Win32 分配的 SID 指针。
    if unsafe { ConvertStringSidToSidW(text.as_ptr(), &mut sid) } == 0 {
        // SAFETY: 紧跟失败调用读取其错误码。
        return Err(failure(unsafe { GetLastError() }));
    }
    Ok(LocalAllocation(sid))
}

fn access(sid: PSID, trustee_type: i32) -> EXPLICIT_ACCESS_W {
    EXPLICIT_ACCESS_W {
        grfAccessPermissions: FILE_ALL_ACCESS,
        grfAccessMode: SET_ACCESS,
        grfInheritance: CONTAINER_INHERIT_ACE | OBJECT_INHERIT_ACE,
        Trustee: TRUSTEE_W {
            pMultipleTrustee: ptr::null_mut(),
            MultipleTrusteeOperation: NO_MULTIPLE_TRUSTEE,
            TrusteeForm: TRUSTEE_IS_SID,
            TrusteeType: trustee_type,
            ptstrName: sid.cast(),
        },
    }
}

pub(super) fn protect(path: &Path) -> Result<(), String> {
    let user = current_user()?;
    let system = system_sid()?;
    let entries = [
        access(user.get(), TRUSTEE_IS_USER),
        access(system.0, TRUSTEE_IS_WELL_KNOWN_GROUP),
    ];
    let mut acl: *mut ACL = ptr::null_mut();
    // SAFETY: 两个 SID 在本作用域内有效；空旧 ACL 保证不保留继承或其他用户条目。
    let result = unsafe {
        SetEntriesInAclW(
            entries.len() as u32,
            entries.as_ptr(),
            ptr::null(),
            &mut acl,
        )
    };
    if result != ERROR_SUCCESS {
        return Err(failure(result));
    }
    if acl.is_null() {
        return Err("无法建立 MiniMax 登录目录的访问控制列表".into());
    }
    let _acl = LocalAllocation(acl.cast());
    let mut name = path.as_os_str().encode_wide().collect::<Vec<_>>();
    if name.contains(&0) {
        return Err("MiniMax 登录目录包含无效字符".into());
    }
    name.push(0);
    // SAFETY: 路径和 ACL 在调用期间有效；只修改调用方已核实的专用目录 DACL。
    let result = unsafe {
        SetNamedSecurityInfoW(
            name.as_ptr(),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            ptr::null_mut(),
            ptr::null_mut(),
            acl,
            ptr::null(),
        )
    };
    if result != ERROR_SUCCESS {
        return Err(failure(result));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::Security::{
        ACCESS_ALLOWED_ACE, Authorization::GetNamedSecurityInfoW, EqualSid, GetAce,
        GetSecurityDescriptorControl, GetSecurityDescriptorLength, PSECURITY_DESCRIPTOR,
        SE_DACL_PROTECTED,
    };

    fn descriptor(path: &Path) -> (LocalAllocation, *mut ACL) {
        let name = path
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        let mut acl = ptr::null_mut();
        let mut descriptor: PSECURITY_DESCRIPTOR = ptr::null_mut();
        // SAFETY: 仅读取临时测试目录；返回的描述符由 LocalAllocation 接管。
        let result = unsafe {
            GetNamedSecurityInfoW(
                name.as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION,
                ptr::null_mut(),
                ptr::null_mut(),
                &mut acl,
                ptr::null_mut(),
                &mut descriptor,
            )
        };
        assert_eq!(result, ERROR_SUCCESS);
        assert!(!descriptor.is_null() && !acl.is_null());
        (LocalAllocation(descriptor), acl)
    }

    fn descriptor_bytes(path: &Path) -> Vec<u8> {
        let (descriptor, _) = descriptor(path);
        // SAFETY: Windows 返回的完整描述符在复制完成前一直有效。
        unsafe {
            std::slice::from_raw_parts(
                descriptor.0.cast::<u8>(),
                GetSecurityDescriptorLength(descriptor.0) as usize,
            )
            .to_vec()
        }
    }

    #[test]
    fn dacl_is_protected_and_grants_only_current_user_and_system() {
        let temp = tempfile::tempdir().expect("ACL fixture");
        let home = temp.path().join("private-home");
        std::fs::create_dir(&home).expect("private directory");
        let parent_before = descriptor_bytes(temp.path());
        protect(&home).expect("private ACL");
        let (descriptor, acl) = descriptor(&home);
        let mut control = 0;
        let mut revision = 0;
        // SAFETY: 所有指针均指向已取得且仍持有的测试安全描述符。
        assert_ne!(
            unsafe { GetSecurityDescriptorControl(descriptor.0, &mut control, &mut revision) },
            0
        );
        assert_ne!(control & SE_DACL_PROTECTED, 0);
        // SAFETY: acl 是描述符内部的有效 ACL 指针。
        assert_eq!(unsafe { (*acl).AceCount }, 2);
        let user = current_user().expect("current test user");
        let system = system_sid().expect("SYSTEM SID");
        let mut user_seen = false;
        let mut system_seen = false;
        for index in 0..2 {
            let mut ace = ptr::null_mut();
            // SAFETY: index 在 AceCount 范围内，输出由描述符持有。
            assert_ne!(unsafe { GetAce(acl, index, &mut ace) }, 0);
            // SAFETY: SetEntriesInAcl 产生的两个条目均为 ACCESS_ALLOWED_ACE。
            let ace = unsafe { &*ace.cast::<ACCESS_ALLOWED_ACE>() };
            assert_eq!(ace.Header.AceType, 0);
            assert_eq!(ace.Mask, FILE_ALL_ACCESS);
            assert_eq!(
                u32::from(ace.Header.AceFlags),
                CONTAINER_INHERIT_ACE | OBJECT_INHERIT_ACE
            );
            let sid = ptr::addr_of!(ace.SidStart).cast_mut().cast::<c_void>();
            // SAFETY: SID 起点属于仍有效的 ACE；比较目标 SID 也保持存活。
            let is_user = unsafe { EqualSid(sid, user.get()) } != 0;
            // SAFETY: 同上。
            let is_system = unsafe { EqualSid(sid, system.0) } != 0;
            assert!(is_user || is_system);
            user_seen |= is_user;
            system_seen |= is_system;
        }
        assert!(user_seen && system_seen);
        assert_eq!(descriptor_bytes(temp.path()), parent_before);
    }
}
