//! 本机 Windows UAC 提权执行(ShellExecuteExW + 等待退出码)
//!
//! 提权进程没有管道可读,UAC 子进程的 stdout/stderr 拿不到,这里只等退出码。
//! 等待期间不杀已提权进程:它可能正在写 Program Files。

#![allow(unsafe_code)]

use std::ffi::OsStr;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
use windows::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject};
use windows::Win32::UI::Shell::{
    IsUserAnAdmin, SEE_MASK_FLAG_NO_UI, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW,
};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::PCWSTR;

use crate::command::{CommandOutput, DEFAULT_COMMAND_TIMEOUT, HostCommand};
use crate::error::HostError;
use crate::path::PathStyle;

/// 单次 WaitForSingleObject 的轮询切片,给取消信号和超时判断留出响应窗口
const WAIT_SLICE_MS: u32 = 200;

/// 把参数列表拼成 lpParameters 可解析的单行命令行。
/// 含空格或引号的参数用双引号包住,内部双引号转义为 \"。
/// program 不在这里拼,它走 lpFile。
pub(crate) fn join_windows_args(args: &[String]) -> String {
    args.iter()
        .map(|arg| {
            if arg.is_empty() || arg.contains(' ') || arg.contains('"') {
                format!("\"{}\"", arg.replace('"', "\\\""))
            } else {
                arg.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// 已是管理员用 open 直接跑(不弹 UAC),否则 runas 触发 UAC
pub(crate) fn elevate_verb(is_admin: bool) -> &'static str {
    if is_admin { "open" } else { "runas" }
}

/// 把 ShellExecuteExW 失败时的 last_error 映射成 HostError。
/// ERROR_CANCELLED(1223)= 用户在 UAC 弹窗点了取消,单独给 reason 方便上层识别。
pub(crate) fn map_shellexecute_error(last_error: u32) -> HostError {
    const ERROR_CANCELLED: u32 = 1223;
    if last_error == ERROR_CANCELLED {
        HostError::ElevationFailed {
            locality: "local",
            reason: "user cancelled UAC".into(),
        }
    } else {
        HostError::ElevationFailed {
            locality: "local",
            reason: format!("ShellExecuteExW last_error={last_error}"),
        }
    }
}

fn wide(s: &str) -> Vec<u16> {
    OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

fn io_from_blocking(err: tokio::task::JoinError) -> HostError {
    HostError::Io(io::Error::other(format!("spawn_blocking failed: {err}")))
}

fn io_from_win32(context: &str, err: windows::core::Error) -> HostError {
    HostError::Io(io::Error::other(format!("{context}: {err}")))
}

// SAFETY: file/params/cwd/verb 均为 NUL 结尾的宽字符串,生命周期覆盖本次调用;
// 成功时 hProcess 归调用方所有,失败路径由系统回收。
unsafe fn shellexecute_launch(
    program: &str,
    args: &[String],
    working_dir: Option<&str>,
) -> Result<windows::Win32::Foundation::HANDLE, HostError> {
    let file = wide(program);
    let params_str = join_windows_args(args);
    let params = wide(&params_str);
    let cwd = wide(working_dir.unwrap_or("."));
    // SAFETY: shell32 IsUserAnAdmin 无输出缓冲,只读当前进程令牌。
    let admin = unsafe { IsUserAnAdmin() }.as_bool();
    let verb = wide(elevate_verb(admin));

    let mut sei = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        // NO_UI:lpFile 不存在/打不开时让 Shell 返回失败码而不是弹系统错误框
        // (组件页 docker 探测的 elevated fallback 曾因缺这个弹过
        //  "Windows 找不到文件 'docker'" 的系统对话框)
        // NOCLOSEPROCESS:要 hProcess 才能等退出码
        fMask: SEE_MASK_FLAG_NO_UI | SEE_MASK_NOCLOSEPROCESS,
        lpVerb: PCWSTR(verb.as_ptr()),
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: PCWSTR(params.as_ptr()),
        lpDirectory: PCWSTR(cwd.as_ptr()),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };

    // SAFETY: sei 为合法初始化结构,调用期间独占访问。
    // 失败时 ShellExecuteExW 返回 Err,last_error 已由 windows-rs 填进 Error;
    // ERROR_CANCELLED(1223)= 用户在 UAC 弹窗点了取消。
    unsafe { ShellExecuteExW(&mut sei) }.map_err(|e| map_shellexecute_error(e.code().0 as u32))?;
    Ok(sei.hProcess)
}

/// launch + 分片轮询等待都留在同一个 blocking 线程上:HANDLE 不能跨线程移动,
/// 取消信号用原子标志从 async 侧传入。超时或取消时故意不杀提权进程也不关句柄
/// (它可能正在写 Program Files),让进程自然结束后由系统回收资源。
fn wait_on_blocking_thread(
    handle: windows::Win32::Foundation::HANDLE,
    budget: Duration,
    cancelled: &AtomicBool,
) -> Result<CommandOutput, HostError> {
    let started = Instant::now();
    loop {
        if cancelled.load(Ordering::Relaxed) {
            return Err(HostError::Cancelled);
        }
        let remaining = budget
            .checked_sub(started.elapsed())
            .unwrap_or(Duration::ZERO);
        if remaining.is_zero() {
            return Err(HostError::Timeout {
                operation: "elevated_wait",
            });
        }
        let slice_ms = remaining
            .min(Duration::from_millis(WAIT_SLICE_MS as u64))
            .as_millis()
            .max(1) as u32;
        // SAFETY: handle 来自成功的 ShellExecuteExW,归本线程所有。
        if unsafe { WaitForSingleObject(handle, slice_ms) } == WAIT_OBJECT_0 {
            let mut code: u32 = 0;
            // SAFETY: 进程已退出,取码必成功;失败按 IO 错误上报。
            unsafe { GetExitCodeProcess(handle, &mut code) }
                .map_err(|e| io_from_win32("GetExitCodeProcess", e))?;
            // SAFETY: 关闭自有句柄;失败只是延迟回收。
            let _ = unsafe { CloseHandle(handle) };
            return Ok(CommandOutput {
                exit_code: Some(code as i32),
                stdout: String::new(),
                stderr: String::new(),
            });
        }
        // WAIT_TIMEOUT 继续下一轮(先查取消/超时)
    }
}

/// 提权执行一条命令并等到进程退出(runas/open 由是否已是管理员决定)。
///
/// UAC 弹窗与子进程运行都放在 spawn_blocking 里,主 runtime 不被卡住;
/// 取消信号通过原子标志传入 blocking 线程。
pub(crate) async fn run_elevated_wait(cmd: HostCommand) -> Result<CommandOutput, HostError> {
    if cmd.program.trim().is_empty() {
        return Err(HostError::InvalidArgument {
            reason: "elevated command needs a non-empty program".into(),
        });
    }
    // UAC 子进程没有管道,stdin/env 无法透传;QQ 安装器不需要这些
    if cmd.stdin.is_some() {
        return Err(HostError::Unsupported {
            operation: "elevated_stdin",
        });
    }
    if !cmd.environment.is_empty() {
        return Err(HostError::Unsupported {
            operation: "elevated_env",
        });
    }

    let budget = cmd.timeout.unwrap_or(DEFAULT_COMMAND_TIMEOUT);
    let program = cmd.program.clone();
    let args = cmd.args.clone();
    let working_dir = cmd
        .working_dir
        .as_ref()
        .map(|p| p.render(PathStyle::Windows));
    let cancelled = Arc::new(AtomicBool::new(false));

    // cancel 触发时置位原子标志;blocking 线程在下一个轮询切片里看到后返回。
    // clone 出 token 避免 borrow cmd 到函数尾
    let cancel_task = cmd.cancel.clone().map(|token| {
        let cancelled = Arc::clone(&cancelled);
        tokio::spawn(async move {
            token.cancelled().await;
            cancelled.store(true, Ordering::Relaxed);
        })
    });

    let result = tokio::task::spawn_blocking(move || {
        // SAFETY: 参数均为本闭包持有的独立数据,无别名借用。
        let handle = unsafe { shellexecute_launch(&program, &args, working_dir.as_deref()) }?;
        wait_on_blocking_thread(handle, budget, &cancelled)
    })
    .await
    .map_err(io_from_blocking)?;

    if let Some(task) = cancel_task {
        task.abort();
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_windows_args_quotes_spaces() {
        assert_eq!(join_windows_args(&["/s".into()]), "/s");
        assert_eq!(
            join_windows_args(&[r"C:\Program Files\a.exe".into(), "/s".into()]),
            r#""C:\Program Files\a.exe" /s"#
        );
    }

    #[test]
    fn join_windows_args_empty_is_empty_string() {
        assert_eq!(join_windows_args(&[]), "");
    }

    #[test]
    fn join_windows_args_escapes_inner_quotes() {
        assert_eq!(
            join_windows_args(&[r#"say "hi""#.into()]),
            r#""say \"hi\"""#
        );
    }

    #[test]
    fn elevate_verb_open_when_already_admin() {
        assert_eq!(elevate_verb(true), "open");
        assert_eq!(elevate_verb(false), "runas");
    }

    #[test]
    fn maps_user_cancelled_uac() {
        let err = map_shellexecute_error(1223);
        assert!(matches!(
            err,
            HostError::ElevationFailed {
                locality: "local",
                ..
            }
        ));
        assert!(err.to_string().contains("cancelled") || err.to_string().contains("UAC"));
    }

    #[test]
    fn maps_other_shell_errors_with_last_error_code() {
        let err = map_shellexecute_error(2);
        assert!(matches!(
            err,
            HostError::ElevationFailed {
                locality: "local",
                ..
            }
        ));
        assert!(err.to_string().contains("last_error=2"));
    }
}
