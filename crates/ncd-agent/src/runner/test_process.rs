//! 以同一用户的受限令牌运行真实进程测试，兼容管理员 CI runner。
use super::*;

pub(super) const OWNER_SID: &str = "NCD_AGENT_RUNNER_TEST_OWNER_SID";
const LAUNCHER: &str = "NCD_AGENT_RUNNER_TEST_LAUNCHER";

pub(super) fn run_limited(test: &str) -> Result<(), String> {
    let root = tempfile::tempdir().map_err(|error| error.to_string())?;
    let log_path = root.path().join("test.log");
    let log = File::create(&log_path).map_err(|error| error.to_string())?;
    let input = File::open("NUL").map_err(|error| error.to_string())?;
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let application = native::wide(&executable.to_string_lossy());
    let mut command = native::wide(
        &[
            executable.to_string_lossy().into_owned(),
            "--exact".into(),
            test.into(),
            "--nocapture".into(),
            "--test-threads=1".into(),
        ]
        .iter()
        .map(|argument| native::quote(argument))
        .collect::<Vec<_>>()
        .join(" "),
    );
    let owner = native::own_sid()?;
    let launcher = std::env::var(LAUNCHER).is_ok_and(|value| value == test);
    // Window-station selection is process-wide, so only the dedicated launcher changes it.
    let mut desktop = if launcher && native::elevated()? {
        Some(super::test_desktop::Desktop::new(&owner)?)
    } else {
        None
    };
    let mut environment: BTreeMap<String, String> = std::env::vars()
        .map(|(key, value)| (key.to_uppercase(), value))
        .collect();
    if launcher {
        environment.remove(LAUNCHER);
        environment.insert(OWNER_SID.into(), owner.clone());
    } else {
        environment.insert(LAUNCHER.into(), test.into());
    }
    let mut block = Vec::new();
    for (key, value) in environment {
        block.extend(format!("{key}={value}").encode_utf16());
        block.push(0);
    }
    block.push(0);

    unsafe {
        let limited = if launcher {
            Some(limited_token(&owner)?)
        } else {
            None
        };
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
        let handles = [log.as_raw_handle(), input.as_raw_handle()];
        for handle in handles {
            if SetHandleInformation(handle, HANDLE_FLAG_INHERIT, HANDLE_FLAG_INHERIT) == 0 {
                return Err(native::last_error());
            }
        }
        let mut attribute_bytes = 0;
        InitializeProcThreadAttributeList(null_mut(), 1, 0, &mut attribute_bytes);
        if attribute_bytes == 0 {
            return Err(native::last_error());
        }
        let mut storage = vec![0usize; attribute_bytes.div_ceil(std::mem::size_of::<usize>())];
        let attributes = storage.as_mut_ptr().cast();
        if InitializeProcThreadAttributeList(attributes, 1, 0, &mut attribute_bytes) == 0 {
            return Err(native::last_error());
        }
        struct Attributes(LPPROC_THREAD_ATTRIBUTE_LIST);
        impl Drop for Attributes {
            fn drop(&mut self) {
                unsafe { DeleteProcThreadAttributeList(self.0) }
            }
        }
        let attributes = Attributes(attributes);
        if UpdateProcThreadAttribute(
            attributes.0,
            0,
            PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
            handles.as_ptr().cast(),
            std::mem::size_of_val(&handles),
            null_mut(),
            null(),
        ) == 0
        {
            return Err(native::last_error());
        }
        let mut startup: STARTUPINFOEXW = std::mem::zeroed();
        startup.StartupInfo.cb = std::mem::size_of_val(&startup) as u32;
        startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
        startup.StartupInfo.hStdInput = input.as_raw_handle();
        startup.StartupInfo.hStdOutput = log.as_raw_handle();
        startup.StartupInfo.hStdError = log.as_raw_handle();
        startup.StartupInfo.lpDesktop = desktop
            .as_mut()
            .map_or(null_mut(), |desktop| desktop.path.as_mut_ptr());
        startup.lpAttributeList = attributes.0;
        if let (Some(desktop), Some(limited)) = (&desktop, &limited) {
            desktop.verify_access(limited.0)?;
        }
        let mut info: PROCESS_INFORMATION = std::mem::zeroed();
        let flags = CREATE_SUSPENDED
            | CREATE_NO_WINDOW
            | CREATE_UNICODE_ENVIRONMENT
            | EXTENDED_STARTUPINFO_PRESENT;
        let result = if let Some(limited) = &limited {
            CreateProcessAsUserW(
                limited.0,
                application.as_ptr(),
                command.as_mut_ptr(),
                null(),
                null(),
                1,
                flags,
                block.as_ptr().cast(),
                null(),
                &startup.StartupInfo,
                &mut info,
            )
        } else {
            CreateProcessW(
                application.as_ptr(),
                command.as_mut_ptr(),
                null(),
                null(),
                1,
                flags,
                block.as_ptr().cast(),
                null(),
                &startup.StartupInfo,
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
            return Err(native::last_error());
        }
        if WaitForSingleObject(process.0, 90_000) != WAIT_OBJECT_0 {
            return Err(
                "isolated owner-identity subprocess did not finish within 90 seconds".into(),
            );
        }
        let mut code = 1;
        if GetExitCodeProcess(process.0, &mut code) == 0 {
            return Err(native::last_error());
        }
        let output = std::fs::read_to_string(&log_path).map_err(|error| error.to_string())?;
        if code != 0 {
            return Err(format!(
                "isolated owner-identity subprocess exited 0x{code:08X}:\n{output}"
            ));
        }
        if !output
            .lines()
            .any(|line| line.starts_with("test result: ok. 1 passed; 0 failed; 0 ignored;"))
        {
            return Err(format!(
                "isolated subprocess did not execute the exact smoke test:\n{output}"
            ));
        }
    }
    Ok(())
}

fn limited_token(owner: &str) -> Result<Handle, String> {
    unsafe {
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
        let mut raw = null_mut();
        if CreateRestrictedToken(
            token.0,
            DISABLE_MAX_PRIVILEGE | LUA_TOKEN,
            0,
            null(),
            0,
            null(),
            0,
            null(),
            &mut raw,
        ) == 0
        {
            return Err(native::last_error());
        }
        let limited = Handle::new(raw)?;
        if native::token_sid(limited.0)? != owner {
            return Err("restricted test token changed its owner SID".into());
        }
        let mut elevation: TOKEN_ELEVATION = std::mem::zeroed();
        let mut length = 0;
        if GetTokenInformation(
            limited.0,
            TokenElevation,
            (&mut elevation as *mut TOKEN_ELEVATION).cast(),
            std::mem::size_of_val(&elevation) as u32,
            &mut length,
        ) == 0
        {
            return Err(native::last_error());
        }
        if elevation.TokenIsElevated != 0 {
            return Err("restricted test token is still elevated".into());
        }
        Ok(limited)
    }
}
