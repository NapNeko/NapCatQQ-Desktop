//! Agent 子进程的启动隔离与收尾。

use tokio::process::{Child, Command};

#[cfg(not(windows))]
pub(super) struct ProcessGroup;

#[cfg(not(windows))]
impl ProcessGroup {
    pub(super) fn prepare(command: &mut Command) -> Result<Self, String> {
        ncd_host::hide_console_window(command);
        Ok(Self)
    }

    pub(super) fn attach(&self, _child: &Child) -> Result<(), String> {
        Ok(())
    }

    pub(super) fn terminate(&self) {}
}

#[cfg(windows)]
#[expect(
    unsafe_code,
    reason = "Windows Job handle and suspended process ownership boundary; each FFI call documents its safety contract"
)]
mod windows {
    use std::{
        mem::size_of,
        os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
        ptr,
    };

    use windows_sys::Win32::{
        Foundation::{HANDLE, INVALID_HANDLE_VALUE},
        System::{
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First,
                Thread32Next,
            },
            JobObjects::{
                AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
                SetInformationJobObject, TerminateJobObject,
            },
            Threading::{
                CREATE_NO_WINDOW, CREATE_SUSPENDED, OpenThread, ResumeThread, THREAD_SUSPEND_RESUME,
            },
        },
    };

    use super::{Child, Command};

    pub(in super::super) struct ProcessGroup {
        handle: OwnedHandle,
    }

    impl ProcessGroup {
        pub(in super::super) fn prepare(command: &mut Command) -> Result<Self, String> {
            // 先挂起主线程，避免 CLI 在加入 Job 前启动未受管的子进程。
            command.creation_flags(CREATE_NO_WINDOW | CREATE_SUSPENDED);
            // SAFETY: 无名称、无继承属性；返回句柄立即交给 OwnedHandle。
            let raw = unsafe { CreateJobObjectW(ptr::null(), ptr::null()) };
            if raw.is_null() {
                return Err(system_error("无法创建 Agent 进程组"));
            }
            // SAFETY: raw 是刚创建且唯一拥有的有效句柄。
            let handle = unsafe { OwnedHandle::from_raw_handle(raw) };
            let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            // SAFETY: limits 的类型、大小与信息类别一致，调用期间始终有效。
            let configured = unsafe {
                SetInformationJobObject(
                    handle.as_raw_handle(),
                    JobObjectExtendedLimitInformation,
                    (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                    size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                )
            };
            if configured == 0 {
                return Err(system_error("无法配置 Agent 进程组"));
            }
            Ok(Self { handle })
        }

        pub(in super::super) fn attach(&self, child: &Child) -> Result<(), String> {
            let process = child
                .raw_handle()
                .ok_or_else(|| "Agent 进程已结束".to_string())?;
            let pid = child.id().ok_or_else(|| "Agent 进程已结束".to_string())?;
            // SAFETY: Job 与 Child 都持有句柄；主线程仍挂起，尚不能创建子进程。
            if unsafe { AssignProcessToJobObject(self.handle.as_raw_handle(), process) } == 0 {
                return Err(system_error("无法接管 Agent 进程组"));
            }
            resume_primary_thread(pid)
        }

        pub(in super::super) fn terminate(&self) {
            // SAFETY: 句柄归本对象持有；关闭句柄也会终止 Job 中剩余进程。
            unsafe { TerminateJobObject(self.handle.as_raw_handle(), 1) };
        }
    }

    fn resume_primary_thread(pid: u32) -> Result<(), String> {
        // SAFETY: 只枚举线程，不从快照读取未初始化内存。
        let raw = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
        if raw == INVALID_HANDLE_VALUE || raw.is_null() {
            return Err(system_error("无法定位 Agent 主线程"));
        }
        // SAFETY: 快照句柄有效且只在此处转移所有权。
        let snapshot = unsafe { OwnedHandle::from_raw_handle(raw) };
        let mut entry = THREADENTRY32 {
            dwSize: size_of::<THREADENTRY32>() as u32,
            ..Default::default()
        };
        // SAFETY: entry 已设置文档要求的结构大小。
        let mut found = unsafe { Thread32First(snapshot.as_raw_handle(), &mut entry) } != 0;
        while found {
            if entry.th32OwnerProcessID == pid {
                // SAFETY: 仅请求恢复线程所需的权限，不继承句柄。
                let raw: HANDLE =
                    unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID) };
                if raw.is_null() {
                    return Err(system_error("无法打开 Agent 主线程"));
                }
                // SAFETY: OpenThread 成功，当前代码唯一拥有返回句柄。
                let thread = unsafe { OwnedHandle::from_raw_handle(raw) };
                // SAFETY: 只恢复刚创建、已加入 Job 的进程主线程。
                let previous_count = unsafe { ResumeThread(thread.as_raw_handle()) };
                return if previous_count == 1 {
                    Ok(())
                } else {
                    Err("Agent 主线程挂起状态异常".into())
                };
            }
            entry.dwSize = size_of::<THREADENTRY32>() as u32;
            // SAFETY: 快照与结构均在当前作用域内有效。
            found = unsafe { Thread32Next(snapshot.as_raw_handle(), &mut entry) } != 0;
        }
        Err("找不到 Agent 主线程".into())
    }

    fn system_error(message: &str) -> String {
        let code = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
        format!("{message}（系统错误 {code}）")
    }
}

#[cfg(windows)]
pub(super) use windows::ProcessGroup;
