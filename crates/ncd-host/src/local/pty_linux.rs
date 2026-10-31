//! Linux PTY：主端由 Tokio readiness 驱动，子进程持有从端作为控制终端。
#![expect(
    unsafe_code,
    reason = "POSIX PTY allocation, controlling terminal and ioctl"
)]

use std::ffi::CStr;
use std::fs::File;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd};
use std::process::Stdio;

use bytes::Bytes;
use tokio::io::unix::AsyncFd;
use tokio::process::Command;

use crate::HostError;
use crate::pty::{
    PTY_READ_BUF, PtyExit, PtyInput, PtyProgram, PtyRequest, PtySession, PtySize, pty_channel_pair,
};

fn resize(fd: i32, size: PtySize) -> io::Result<()> {
    let winsize = libc::winsize {
        ws_row: size.rows,
        ws_col: size.cols,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    // SAFETY: fd 是本模块持有的 PTY，winsize 在调用期间有效。
    if unsafe { libc::ioctl(fd, libc::TIOCSWINSZ, &winsize) } == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn allocate(size: PtySize) -> io::Result<(File, File)> {
    // SAFETY: 无指针参数，成功后的 fd 立即交给 File 管理。
    let fd = unsafe {
        libc::posix_openpt(libc::O_RDWR | libc::O_NOCTTY | libc::O_CLOEXEC | libc::O_NONBLOCK)
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    let master = unsafe { File::from_raw_fd(fd) };
    // SAFETY: master 持有有效的 PTY 主端 fd。
    if unsafe { libc::grantpt(fd) } == -1 || unsafe { libc::unlockpt(fd) } == -1 {
        return Err(io::Error::last_os_error());
    }
    let mut name = [0 as libc::c_char; 256];
    // SAFETY: name 指向完整可写缓冲区，长度与实际大小一致。
    let error = unsafe { libc::ptsname_r(fd, name.as_mut_ptr(), name.len()) };
    if error != 0 {
        return Err(io::Error::from_raw_os_error(error));
    }
    let name = unsafe { CStr::from_ptr(name.as_ptr()) };
    // SAFETY: ptsname_r 返回以 NUL 结尾的设备路径，fd 成功后交给 File 管理。
    let slave_fd = unsafe {
        libc::open(
            name.as_ptr(),
            libc::O_RDWR | libc::O_NOCTTY | libc::O_CLOEXEC,
        )
    };
    if slave_fd < 0 {
        return Err(io::Error::last_os_error());
    }
    let slave = unsafe { File::from_raw_fd(slave_fd) };
    resize(fd, size)?;
    Ok((master, slave))
}

fn read(fd: i32, buffer: &mut [u8]) -> io::Result<usize> {
    // SAFETY: buffer 指针和长度来自同一个有效借用。
    let count = unsafe { libc::read(fd, buffer.as_mut_ptr().cast(), buffer.len()) };
    if count >= 0 {
        return Ok(count as usize);
    }
    let error = io::Error::last_os_error();
    // Linux PTY 的从端关闭后主端返回 EIO，等同 EOF。
    if error.raw_os_error() == Some(libc::EIO) {
        Ok(0)
    } else {
        Err(error)
    }
}

async fn write(master: &AsyncFd<File>, mut bytes: &[u8]) -> io::Result<()> {
    while !bytes.is_empty() {
        let mut ready = master.writable().await?;
        match ready.try_io(|fd| {
            // SAFETY: bytes 在调用期间有效，fd 是持有的 PTY 主端。
            let count = unsafe { libc::write(fd.as_raw_fd(), bytes.as_ptr().cast(), bytes.len()) };
            if count < 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(count as usize)
            }
        }) {
            Ok(Ok(0)) => return Err(io::ErrorKind::WriteZero.into()),
            Ok(Ok(count)) => bytes = &bytes[count..],
            Ok(Err(error)) => return Err(error),
            Err(_) => continue,
        }
    }
    Ok(())
}

pub(super) fn open_pty(request: PtyRequest) -> Result<PtySession, HostError> {
    let (master, slave) = allocate(request.size)?;
    let mut command = match request.program {
        PtyProgram::Program { program, args } => {
            let mut command = Command::new(program);
            command.args(args);
            command
        }
        PtyProgram::LoginShell => {
            let mut command =
                Command::new(std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into()));
            command.arg("-l");
            command
        }
        PtyProgram::Script(script) => {
            let mut command = Command::new("/bin/sh");
            command.args(["-c", &script]);
            command
        }
    };
    command
        .stdin(Stdio::from(slave.try_clone()?))
        .stdout(Stdio::from(slave.try_clone()?))
        .stderr(Stdio::from(slave));
    command
        .env("TERM", "xterm-256color")
        .envs(request.env)
        .kill_on_drop(true);
    crate::appimage::clean_appimage_environment(command.as_std_mut());
    if let Some(cwd) = request.cwd {
        command.current_dir(cwd.as_posix());
    }
    // SAFETY: pre_exec 只调用异步信号安全的 setsid/ioctl，stdin 已被重定向到从端。
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() == -1 || libc::ioctl(0, libc::TIOCSCTTY, 0) == -1 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let master = AsyncFd::new(master)?;
    let mut child = command.spawn()?;
    let pgid = child.id().unwrap_or(0);
    let (session, mut backend) = pty_channel_pair();
    tokio::spawn(async move {
        let result: io::Result<Option<i32>> = async {
            let mut buffer = [0u8; PTY_READ_BUF];
            let mut output_open = true;
            loop {
                tokio::select! {
                    status = child.wait() => {
                        let status = status?;
                        // 子进程可能比最后一段输出先就绪，先把主端已到达的数据读完。
                        while let Ok(count) = read(master.as_raw_fd(), &mut buffer) {
                            if count == 0 { break; }
                            if backend.output.send(Bytes::copy_from_slice(&buffer[..count])).await.is_err() { break; }
                        }
                        return Ok(status.code());
                    }
                    ready = master.readable(), if output_open => {
                        match ready?.try_io(|fd| read(fd.as_raw_fd(), &mut buffer)) {
                            Ok(Ok(0)) => output_open = false,
                            Ok(Ok(count)) => {
                                if backend.output.send(Bytes::copy_from_slice(&buffer[..count])).await.is_err() { break; }
                            }
                            Ok(Err(error)) => return Err(error),
                            Err(_) => {}
                        }
                    }
                    input = backend.input.recv() => match input {
                        Some(PtyInput::Write(bytes)) => write(&master, &bytes).await?,
                        Some(PtyInput::Resize(size)) => resize(master.as_raw_fd(), size)?,
                        Some(PtyInput::Close) | None => break,
                    }
                }
            }
            if let Some(pid) = child.id() {
                // SAFETY: setsid 建立了以此子进程 pid 为组号的独立会话。
                unsafe { libc::kill(-(pid as i32), libc::SIGKILL); }
            }
            child.kill().await?;
            child.wait().await.map(|s| s.code())
        }.await;
        if pgid != 0 {
            // SAFETY: 此 PTY 子进程由 setsid 创建独立进程组，所有退出路径均清理。
            unsafe {
                libc::kill(-(pgid as i32), libc::SIGKILL);
            }
        }
        let _ = child.wait().await;
        let exit = match result {
            Ok(code) => PtyExit::Exited(code),
            Err(error) => PtyExit::Disconnected(error.to_string()),
        };
        let _ = backend.exit.send(exit);
    });
    Ok(session)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn pty_provides_a_terminal_cwd_env_resize_and_exit_status() {
        let workspace = tempfile::tempdir().unwrap();
        let mut request = PtyRequest::new(PtyProgram::Program {
            program: "/bin/sh".into(),
            args: vec!["-c".into(), "test -t 0 && test -t 1 || exit 99; pwd; printf '%s\n' \"$NCD_PTY_VALUE\"; read value; stty size; exit 7".into()],
        }, PtySize::new(80, 24));
        request.cwd = Some(crate::HostPath::from_posix(
            workspace.path().to_string_lossy(),
        ));
        request
            .env
            .insert("NCD_PTY_VALUE".into(), "pty-environment".into());
        let mut session = open_pty(request).unwrap();
        session.control.resize(PtySize::new(101, 35)).unwrap();
        session
            .control
            .write(Bytes::from_static(b"continue\n"))
            .unwrap();
        let mut output = Vec::new();
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while let Some(chunk) = session.output.recv().await {
                output.extend_from_slice(&chunk);
            }
        })
        .await
        .unwrap();
        assert_eq!(session.exit.await.unwrap(), PtyExit::Exited(Some(7)));
        let output = String::from_utf8_lossy(&output);
        assert!(
            output.contains(&*workspace.path().to_string_lossy()),
            "{output}"
        );
        assert!(output.contains("pty-environment"), "{output}");
        assert!(output.contains("35 101"), "{output}");
    }
}
