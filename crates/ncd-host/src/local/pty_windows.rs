//! 本机交互终端：Windows ConPTY
//!
//! CreatePseudoConsole 这一组 Windows 10 1809 / Server 2019 起才有。静态链接的话老系统上
//! 整个桌面端会因为缺导入函数起不来，所以运行时从 kernel32 里取，取不到只让终端报错。
//!
//! 线程分工照微软的建议：读输出、写输入、等退出各占一个，互不阻塞。关伪终端时读线程
//! 还在抽输出，老版本的 ClosePseudoConsole 在输出管道满着时会一直等。

#![allow(unsafe_code)]

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString, c_void};
use std::fs::File;
use std::io::{Read, Write};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle, RawHandle};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use bytes::Bytes;
use windows::Win32::Foundation::{HANDLE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows::Win32::System::Console::{COORD, HPCON};
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
use windows::Win32::System::Pipes::CreatePipe;
use windows::Win32::System::Threading::{
    CREATE_UNICODE_ENVIRONMENT, CreateProcessW, DeleteProcThreadAttributeList,
    EXTENDED_STARTUPINFO_PRESENT, GetExitCodeProcess, InitializeProcThreadAttributeList,
    LPPROC_THREAD_ATTRIBUTE_LIST, PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE, PROCESS_INFORMATION,
    STARTF_USESTDHANDLES, STARTUPINFOEXW, TerminateProcess, UpdateProcThreadAttribute,
    WaitForSingleObject,
};
use windows::core::{HRESULT, PCWSTR, PWSTR, s, w};

use crate::error::HostError;
use crate::path::PathStyle;
use crate::pty::{
    PTY_READ_BUF, PtyBackend, PtyExit, PtyInput, PtyProgram, PtyRequest, PtySession, PtySize,
    pty_channel_pair, windows_command_line,
};

type CreatePseudoConsoleFn =
    unsafe extern "system" fn(COORD, HANDLE, HANDLE, u32, *mut HPCON) -> HRESULT;
type ResizePseudoConsoleFn = unsafe extern "system" fn(HPCON, COORD) -> HRESULT;
type ClosePseudoConsoleFn = unsafe extern "system" fn(HPCON);

/// 关掉伪终端后进程还不走，等这么久再强杀
const FORCE_KILL_AFTER: Duration = Duration::from_secs(3);

/// 等进程退出的轮询切片，给「正在关」的判断留出响应窗口
const WAIT_SLICE_MS: u32 = 200;

struct ConPtyApi {
    create: CreatePseudoConsoleFn,
    resize: ResizePseudoConsoleFn,
    close: ClosePseudoConsoleFn,
}

fn conpty_api() -> Option<&'static ConPtyApi> {
    static API: OnceLock<Option<ConPtyApi>> = OnceLock::new();
    API.get_or_init(load_conpty_api).as_ref()
}

fn load_conpty_api() -> Option<ConPtyApi> {
    // SAFETY: kernel32 在进程里常驻；三个函数的签名照 consoleapi.h 声明，
    // 取到的地址转成对应的函数指针类型
    unsafe {
        let module = GetModuleHandleW(w!("kernel32.dll")).ok()?;
        let create = GetProcAddress(module, s!("CreatePseudoConsole"))?;
        let resize = GetProcAddress(module, s!("ResizePseudoConsole"))?;
        let close = GetProcAddress(module, s!("ClosePseudoConsole"))?;
        Some(ConPtyApi {
            create: std::mem::transmute::<unsafe extern "system" fn() -> isize, CreatePseudoConsoleFn>(
                create,
            ),
            resize: std::mem::transmute::<unsafe extern "system" fn() -> isize, ResizePseudoConsoleFn>(
                resize,
            ),
            close: std::mem::transmute::<unsafe extern "system" fn() -> isize, ClosePseudoConsoleFn>(
                close,
            ),
        })
    }
}

/// 这台 Windows 有没有 ConPTY
pub fn conpty_available() -> bool {
    conpty_api().is_some()
}

fn coord(size: PtySize) -> COORD {
    COORD {
        X: i16::try_from(size.cols).unwrap_or(i16::MAX),
        Y: i16::try_from(size.rows).unwrap_or(i16::MAX),
    }
}

/// 伪终端句柄，三个线程共用；关只关一次，关了之后改大小是空操作
struct PseudoConsole {
    api: &'static ConPtyApi,
    handle: Mutex<Option<HPCON>>,
}

impl PseudoConsole {
    fn resize(&self, size: PtySize) {
        let guard = self.handle.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(hpc) = *guard {
            // SAFETY: 句柄还没关（锁里拿到的是 Some）
            let _ = unsafe { (self.api.resize)(hpc, coord(size)) };
        }
    }

    fn close(&self) {
        // 先拿出来再关，关的过程中别的线程改大小不用陪着等
        let taken = self
            .handle
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        if let Some(hpc) = taken {
            // SAFETY: take 保证每个句柄只关一次
            unsafe { (self.api.close)(hpc) };
        }
    }
}

impl Drop for PseudoConsole {
    fn drop(&mut self) {
        self.close();
    }
}

/// 进程线程属性表，析构时 DeleteProcThreadAttributeList
struct AttributeList {
    // 用 usize 数组保证指针对齐
    buf: Vec<usize>,
}

impl AttributeList {
    fn with_pseudo_console(hpc: HPCON) -> Result<Self, HostError> {
        let mut size = 0usize;
        // SAFETY: 第一次只量大小，按约定返回「缓冲区不足」，结果不用
        let _ = unsafe { InitializeProcThreadAttributeList(None, 1, None, &mut size) };
        let words = size.div_ceil(std::mem::size_of::<usize>()).max(1);
        let mut buf = vec![0usize; words];
        // SAFETY: 缓冲区按上面量出的大小分配
        unsafe {
            InitializeProcThreadAttributeList(
                Some(LPPROC_THREAD_ATTRIBUTE_LIST(buf.as_mut_ptr().cast())),
                1,
                None,
                &mut size,
            )
        }
        .map_err(|e| win_error("InitializeProcThreadAttributeList", &e))?;
        // 初始化成功之后才包成 Self：失败时这里还只是个普通 Vec，析构不会去 Delete 一张没初始化的表。
        // Vec 挪进结构体不会搬动堆上的缓冲区，表的地址不变
        let mut list = Self { buf };
        // SAFETY: 伪终端属性的值就是句柄本身（不是指向句柄的指针），微软示例同样这么传
        unsafe {
            UpdateProcThreadAttribute(
                list.as_list_mut(),
                0,
                PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE as usize,
                Some(hpc.0 as *const c_void),
                std::mem::size_of::<HPCON>(),
                None,
                None,
            )
        }
        .map_err(|e| win_error("UpdateProcThreadAttribute", &e))?;
        Ok(list)
    }

    fn as_list_mut(&mut self) -> LPPROC_THREAD_ATTRIBUTE_LIST {
        LPPROC_THREAD_ATTRIBUTE_LIST(self.buf.as_mut_ptr().cast())
    }
}

impl Drop for AttributeList {
    fn drop(&mut self) {
        let list = self.as_list_mut();
        // SAFETY: 只有 Initialize 成功后才会构造出 Self
        unsafe { DeleteProcThreadAttributeList(list) };
    }
}

fn win_error(context: &str, err: &windows::core::Error) -> HostError {
    HostError::Io(std::io::Error::other(format!("{context}: {err}")))
}

fn create_pipe() -> Result<(OwnedHandle, OwnedHandle), HostError> {
    let mut read = HANDLE::default();
    let mut write = HANDLE::default();
    // SAFETY: 出参是本地变量
    unsafe { CreatePipe(&mut read, &mut write, None, 0) }
        .map_err(|e| win_error("CreatePipe", &e))?;
    // SAFETY: CreatePipe 成功后两个句柄归调用方所有
    Ok(unsafe {
        (
            OwnedHandle::from_raw_handle(read.0 as RawHandle),
            OwnedHandle::from_raw_handle(write.0 as RawHandle),
        )
    })
}

fn wide(s: &OsStr) -> Vec<u16> {
    s.encode_wide().chain(std::iter::once(0)).collect()
}

/// 当前进程环境 + 覆盖项，拼成 CreateProcessW 要的 UTF-16 环境块
///
/// Windows 变量名不分大小写，按大写去重排序，块内顺序也要按名字排好
fn environment_block(overrides: &BTreeMap<String, String>) -> Vec<u16> {
    let mut vars: BTreeMap<String, (OsString, OsString)> = BTreeMap::new();
    for (key, value) in std::env::vars_os() {
        vars.insert(key.to_string_lossy().to_uppercase(), (key, value));
    }
    for (key, value) in overrides {
        vars.insert(
            key.to_uppercase(),
            (OsString::from(key), OsString::from(value)),
        );
    }
    let mut block = Vec::new();
    for (key, value) in vars.values() {
        block.extend(key.encode_wide());
        block.push(u16::from(b'='));
        block.extend(value.encode_wide());
        block.push(0);
    }
    if block.is_empty() {
        block.push(0);
    }
    block.push(0);
    block
}

/// 起一个接在 ConPTY 上的程序；阻塞调用，放在 spawn_blocking 里跑
pub(crate) fn open_conpty(req: PtyRequest) -> Result<PtySession, HostError> {
    let api = conpty_api().ok_or_else(|| HostError::InvalidArgument {
        reason: "这台 Windows 没有内嵌终端要用的 ConPTY（需要 Windows 10 1809 / Server 2019 以上）"
            .into(),
    })?;
    let (program, args) = match req.program {
        PtyProgram::Program { program, args } => (program, args),
        PtyProgram::LoginShell | PtyProgram::Script(_) => {
            return Err(HostError::InvalidArgument {
                reason: "本机终端要指定程序".into(),
            });
        }
    };
    let cwd = req.cwd.as_ref().map(|p| p.render(PathStyle::Windows));
    if let Some(dir) = &cwd {
        if !Path::new(dir).is_dir() {
            return Err(HostError::InvalidArgument {
                reason: format!("目录不存在: {dir}"),
            });
        }
    }

    let (pty_in_read, in_write) = create_pipe()?;
    let (out_read, pty_out_write) = create_pipe()?;
    let mut hpc = HPCON::default();
    // SAFETY: 两个句柄有效；伪终端会复制一份，下面立刻关掉我们手里这份
    let created = unsafe {
        (api.create)(
            coord(req.size),
            HANDLE(pty_in_read.as_raw_handle()),
            HANDLE(pty_out_write.as_raw_handle()),
            0,
            &mut hpc,
        )
    };
    drop(pty_in_read);
    drop(pty_out_write);
    if let Err(e) = created.ok() {
        return Err(win_error("CreatePseudoConsole", &e));
    }
    let console = Arc::new(PseudoConsole {
        api,
        handle: Mutex::new(Some(hpc)),
    });

    let process = match spawn_attached(hpc, &program, &args, cwd.as_deref(), &req.env) {
        Ok(process) => process,
        Err(err) => {
            // 先放掉读端，伪终端往满管道里写会失败而不是卡在 Close 里
            drop(out_read);
            drop(console);
            return Err(err);
        }
    };

    let (session, backend) = pty_channel_pair();
    start_threads(console, process, File::from(out_read), File::from(in_write), backend)?;
    Ok(session)
}

fn spawn_attached(
    hpc: HPCON,
    program: &str,
    args: &[String],
    cwd: Option<&str>,
    env: &BTreeMap<String, String>,
) -> Result<OwnedHandle, HostError> {
    let mut attrs = AttributeList::with_pseudo_console(hpc)?;
    let mut si = STARTUPINFOEXW::default();
    si.StartupInfo.cb = u32::try_from(std::mem::size_of::<STARTUPINFOEXW>()).unwrap_or(u32::MAX);
    // 标准句柄显式给无效值：桌面端自己的 stdout 被重定向时（开发模式、测试），
    // 子进程会继承那份句柄，输出跑到父进程那里而不是伪终端
    si.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    si.StartupInfo.hStdInput = INVALID_HANDLE_VALUE;
    si.StartupInfo.hStdOutput = INVALID_HANDLE_VALUE;
    si.StartupInfo.hStdError = INVALID_HANDLE_VALUE;
    si.lpAttributeList = attrs.as_list_mut();

    let mut command_line = wide(OsStr::new(&windows_command_line(program, args)));
    let cwd_wide = cwd.map(|dir| wide(OsStr::new(dir)));
    let env_block = environment_block(env);
    let mut pi = PROCESS_INFORMATION::default();
    // SAFETY: 所有缓冲区活到调用结束；命令行缓冲区可写（CreateProcessW 会改它）
    unsafe {
        CreateProcessW(
            PCWSTR::null(),
            Some(PWSTR(command_line.as_mut_ptr())),
            None,
            None,
            false,
            EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT,
            Some(env_block.as_ptr().cast()),
            cwd_wide
                .as_ref()
                .map_or(PCWSTR::null(), |dir| PCWSTR(dir.as_ptr())),
            &si.StartupInfo,
            &mut pi,
        )
    }
    .map_err(|e| HostError::InvalidArgument {
        reason: format!("启动 {program} 失败: {e}"),
    })?;
    // SAFETY: CreateProcessW 成功后两个句柄归调用方所有；线程句柄用不上，直接放掉
    let (process, thread) = unsafe {
        (
            OwnedHandle::from_raw_handle(pi.hProcess.0 as RawHandle),
            OwnedHandle::from_raw_handle(pi.hThread.0 as RawHandle),
        )
    };
    drop(thread);
    Ok(process)
}

fn start_threads(
    console: Arc<PseudoConsole>,
    process: OwnedHandle,
    mut output_pipe: File,
    mut input_pipe: File,
    backend: PtyBackend,
) -> Result<(), HostError> {
    let PtyBackend {
        mut input,
        output,
        exit,
    } = backend;
    let closing = Arc::new(AtomicBool::new(false));

    let reader = std::thread::Builder::new()
        .name("ncd-pty-read".into())
        .spawn(move || {
            let mut buf = vec![0u8; PTY_READ_BUF];
            loop {
                match output_pipe.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if output.blocking_send(Bytes::copy_from_slice(&buf[..n])).is_err() {
                            break;
                        }
                    }
                }
            }
        })
        .map_err(HostError::Io)?;

    let writer_console = Arc::clone(&console);
    let writer_closing = Arc::clone(&closing);
    std::thread::Builder::new()
        .name("ncd-pty-write".into())
        .spawn(move || {
            while let Some(msg) = input.blocking_recv() {
                match msg {
                    PtyInput::Write(data) => {
                        if input_pipe.write_all(&data).is_err() {
                            break;
                        }
                    }
                    PtyInput::Resize(size) => writer_console.resize(size),
                    PtyInput::Close => break,
                }
            }
            // 上层要关或者丢了句柄：关伪终端，挂在上面的进程树跟着收掉
            writer_closing.store(true, Ordering::SeqCst);
            writer_console.close();
        })
        .map_err(HostError::Io)?;

    std::thread::Builder::new()
        .name("ncd-pty-wait".into())
        .spawn(move || {
            let code = wait_for_exit(&process, &closing);
            // 进程自己退了也要关伪终端，读线程这才读到管道断开
            console.close();
            let _ = reader.join();
            let _ = exit.send(PtyExit::Exited(code));
        })
        .map_err(HostError::Io)?;
    Ok(())
}

fn wait_for_exit(process: &OwnedHandle, closing: &AtomicBool) -> Option<i32> {
    let handle = HANDLE(process.as_raw_handle());
    let mut closing_since: Option<Instant> = None;
    loop {
        // SAFETY: 进程句柄由 OwnedHandle 持有，活到本函数结束
        let waited = unsafe { WaitForSingleObject(handle, WAIT_SLICE_MS) };
        if waited == WAIT_OBJECT_0 {
            break;
        }
        if waited != WAIT_TIMEOUT {
            return None;
        }
        if closing.load(Ordering::SeqCst) {
            let since = *closing_since.get_or_insert_with(Instant::now);
            if since.elapsed() >= FORCE_KILL_AFTER {
                // SAFETY: 同上；强杀失败也继续等，句柄 signaled 后循环自然退出
                let _ = unsafe { TerminateProcess(handle, 1) };
            }
        }
    }
    let mut code = 0u32;
    // SAFETY: 同上
    unsafe { GetExitCodeProcess(handle, &mut code) }
        .ok()
        .map(|()| code as i32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::path::HostPath;
    use tokio::time::timeout;

    fn cmd(args: &[&str]) -> PtyRequest {
        PtyRequest::new(
            PtyProgram::Program {
                program: "cmd.exe".into(),
                args: args.iter().map(|a| (*a).to_string()).collect(),
            },
            PtySize::new(100, 30),
        )
    }

    /// 收输出直到结束（或超时），返回全部输出和退出方式
    async fn collect(mut session: PtySession) -> (String, Option<PtyExit>) {
        let mut all = Vec::new();
        let deadline = Duration::from_secs(20);
        let _ = timeout(deadline, async {
            while let Some(chunk) = session.output.recv().await {
                all.extend_from_slice(&chunk);
            }
        })
        .await;
        let exit = timeout(Duration::from_secs(10), session.exit).await.ok().and_then(Result::ok);
        (String::from_utf8_lossy(&all).into_owned(), exit)
    }

    async fn wait_for_text(session: &mut PtySession, needle: &str) -> String {
        let mut all = Vec::new();
        let _ = timeout(Duration::from_secs(20), async {
            while let Some(chunk) = session.output.recv().await {
                all.extend_from_slice(&chunk);
                if String::from_utf8_lossy(&all).contains(needle) {
                    break;
                }
            }
        })
        .await;
        String::from_utf8_lossy(&all).into_owned()
    }

    #[tokio::test]
    async fn runs_a_command_and_reports_its_exit_code() {
        assert!(conpty_available(), "ConPTY should exist on the dev machine");
        let session = tokio::task::spawn_blocking(|| open_conpty(cmd(&["/c", "echo ncd-hello& exit 3"])))
            .await
            .unwrap()
            .unwrap();
        let (text, exit) = collect(session).await;
        assert!(text.contains("ncd-hello"), "output was: {text:?}");
        assert_eq!(exit, Some(PtyExit::Exited(Some(3))));
    }

    #[tokio::test]
    async fn interactive_shell_takes_input() {
        let mut req = cmd(&[]);
        let dir = tempfile::tempdir().unwrap();
        req.cwd = Some(HostPath::from_windows(&dir.path().to_string_lossy()));
        req.env.insert("NCD_PTY_PROBE".into(), "probe-value".into());
        let mut session = tokio::task::spawn_blocking(move || open_conpty(req))
            .await
            .unwrap()
            .unwrap();
        session
            .control
            .write(Bytes::from_static(b"echo %NCD_PTY_PROBE%& cd\r"))
            .unwrap();
        let text = wait_for_text(&mut session, "probe-value").await;
        assert!(text.contains("probe-value"), "output was: {text:?}");
        session.control.write(Bytes::from_static(b"exit 0\r")).unwrap();
        let (_, exit) = collect(session).await;
        assert_eq!(exit, Some(PtyExit::Exited(Some(0))));
    }

    #[tokio::test]
    async fn resize_reaches_the_console() {
        let req = PtyRequest::new(
            PtyProgram::Program {
                program: "powershell.exe".into(),
                args: vec![
                    "-NoLogo".into(),
                    "-NoProfile".into(),
                    "-Command".into(),
                    "Start-Sleep -Milliseconds 800; 'cols=' + $Host.UI.RawUI.WindowSize.Width".into(),
                ],
            },
            PtySize::new(90, 20),
        );
        let session = tokio::task::spawn_blocking(move || open_conpty(req))
            .await
            .unwrap()
            .unwrap();
        session.control.resize(PtySize::new(123, 40)).unwrap();
        let (text, _) = collect(session).await;
        assert!(text.contains("cols=123"), "output was: {text:?}");
    }

    #[tokio::test]
    async fn close_ends_the_whole_tree() {
        let session = tokio::task::spawn_blocking(|| open_conpty(cmd(&["/c", "ping -n 30 127.0.0.1"])))
            .await
            .unwrap()
            .unwrap();
        tokio::time::sleep(Duration::from_millis(500)).await;
        session.control.close();
        let started = Instant::now();
        let (_, exit) = collect(session).await;
        assert!(matches!(exit, Some(PtyExit::Exited(_))), "exit was {exit:?}");
        assert!(started.elapsed() < Duration::from_secs(8));
    }

    #[test]
    fn missing_directory_is_reported_before_spawning() {
        let mut req = cmd(&["/c", "echo x"]);
        req.cwd = Some(HostPath::from_windows("C:\\definitely\\not\\here\\ncd"));
        let err = open_conpty(req).err().map(|e| e.to_string()).unwrap_or_default();
        assert!(err.contains("目录不存在"), "error was {err}");
    }

    #[test]
    fn environment_block_is_sorted_and_double_terminated() {
        let mut overrides = BTreeMap::new();
        overrides.insert("zz_ncd_probe".to_string(), "1".to_string());
        overrides.insert("Path".to_string(), "C:\\x".to_string());
        let block = environment_block(&overrides);
        assert_eq!(&block[block.len() - 2..], &[0, 0]);
        let text = String::from_utf16_lossy(&block);
        let entries: Vec<&str> = text.split('\0').filter(|s| !s.is_empty()).collect();
        let keys: Vec<String> = entries
            .iter()
            .map(|e| e.split('=').next().unwrap_or_default().to_uppercase())
            .collect();
        let mut sorted = keys.clone();
        sorted.sort();
        assert_eq!(keys, sorted);
        assert!(entries.contains(&"Path=C:\\x"));
        assert_eq!(entries.iter().filter(|e| e.to_uppercase().starts_with("PATH=")).count(), 1);
    }
}
