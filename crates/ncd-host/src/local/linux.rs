//! Linux 本地主机：POSIX 文件系统、进程组、sudo 与交互 PTY。

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, Command};
use tokio::sync::{RwLock, mpsc};

use crate::command::{CommandOutput, DEFAULT_COMMAND_TIMEOUT, HostCommand, HostProcessWaitPolicy};
use crate::error::HostError;
use crate::host::{Arch, Host, Locality, Os};
use crate::path::{ArchiveKind, DirEntry, HostPath};
use crate::process::{ExitStatus, HostProcess, ProcessId};
use crate::shell::{BashShell, HostShell};

#[derive(Default)]
pub struct LocalLinuxHost {
    elevation_password: RwLock<Option<String>>,
    managed_root: Option<HostPath>,
}

impl LocalLinuxHost {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_managed_root(mut self, root: &Path) -> Self {
        self.managed_root = Some(HostPath::from_posix(root.to_string_lossy().into_owned()));
        self
    }

    fn to_local(&self, path: &HostPath) -> PathBuf {
        PathBuf::from(path.as_posix())
    }

    async fn prepare_command(&self, mut cmd: HostCommand) -> Result<HostCommand, HostError> {
        // SAFETY: geteuid 只读取当前用户身份。
        #[expect(unsafe_code, reason = "read effective uid for sudo selection")]
        let is_root = unsafe { libc::geteuid() == 0 };
        if !cmd.elevated || is_root {
            return Ok(cmd);
        }
        if let Some(password) = self.elevation_password.read().await.as_ref() {
            // 密码仅交给校验进程，避免 sudo 缓存或 NOPASSWD 时落入目标程序 stdin。
            let mut validate = Command::new("sudo");
            crate::appimage::clean_appimage_environment(validate.as_std_mut());
            validate
                .args(["-S", "-p", "", "-v"])
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .kill_on_drop(true);
            let mut child = validate.spawn()?;
            if let Some(mut input) = child.stdin.take() {
                let _ = input.write_all(format!("{password}\n").as_bytes()).await;
            }
            let output = tokio::time::timeout(DEFAULT_COMMAND_TIMEOUT, child.wait_with_output())
                .await
                .map_err(|_| HostError::Timeout {
                    operation: "sudo_validate",
                })??;
            if !output.status.success() {
                return Err(HostError::ElevationFailed {
                    locality: "local",
                    reason: String::from_utf8_lossy(&output.stderr).into_owned(),
                });
            }
        }
        let mut args = vec!["-n".into(), "--".into()];
        // sudo 默认过滤环境，安装任务的变量显式交给 env。
        if !cmd.environment.is_empty() {
            args.push("env".into());
            args.extend(cmd.environment.iter().map(|(k, v)| format!("{k}={v}")));
        }
        args.push(cmd.program);
        args.extend(cmd.args);
        cmd.program = "sudo".into();
        cmd.args = args;
        cmd.elevated = false;
        Ok(cmd)
    }
}

#[async_trait]
impl Host for LocalLinuxHost {
    fn os(&self) -> Os {
        Os::Linux
    }
    fn arch(&self) -> Arch {
        match std::env::consts::ARCH {
            "aarch64" => Arch::Aarch64,
            "x86" => Arch::X86,
            "arm" => Arch::Armv7,
            _ => Arch::X86_64,
        }
    }
    fn locality(&self) -> Locality {
        Locality::Local
    }
    fn id(&self) -> &str {
        "local"
    }
    fn shell(&self) -> &dyn HostShell {
        &BashShell
    }
    fn target_home(&self) -> Option<HostPath> {
        std::env::var("HOME").ok().map(HostPath::from_posix)
    }

    fn target_managed_root(&self) -> Option<HostPath> {
        self.managed_root.clone()
    }
    async fn set_elevation_password(&self, password: Option<String>) {
        *self.elevation_password.write().await = password;
    }
    async fn has_elevation_password(&self) -> bool {
        self.elevation_password.read().await.is_some()
    }
    async fn read_file(&self, path: &HostPath) -> Result<Bytes, HostError> {
        let local = self.to_local(path);
        match tokio::fs::read(&local).await {
            Ok(buf) => Ok(Bytes::from(buf)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Err(HostError::PathNotFound { path: path.clone() })
            }
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                Err(HostError::PermissionDenied {
                    path: path.clone(),
                    operation: "read",
                })
            }
            Err(e) => Err(HostError::Io(e)),
        }
    }

    async fn write_file(&self, path: &HostPath, bytes: &[u8]) -> Result<(), HostError> {
        let local = self.to_local(path);
        if let Some(parent) = local.parent() {
            // 父目录不存在的话先建出来,贴近"用户预期"(legacy 这里也是自动建)
            if !parent.as_os_str().is_empty() && !parent.exists() {
                tokio::fs::create_dir_all(parent).await?;
            }
        }
        match tokio::fs::write(&local, bytes).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                Err(HostError::PermissionDenied {
                    path: path.clone(),
                    operation: "write",
                })
            }
            Err(e) => Err(HostError::Io(e)),
        }
    }

    async fn list_dir(&self, path: &HostPath) -> Result<Vec<DirEntry>, HostError> {
        let local = self.to_local(path);
        let mut rd = match tokio::fs::read_dir(&local).await {
            Ok(rd) => rd,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(HostError::PathNotFound { path: path.clone() });
            }
            Err(e) => return Err(HostError::Io(e)),
        };

        let mut entries = Vec::new();
        while let Some(entry) = rd.next_entry().await? {
            let meta = entry.metadata().await?;
            let modified = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .and_then(|d| i64::try_from(d.as_secs()).ok());
            entries.push(DirEntry {
                name: entry.file_name().to_string_lossy().into_owned(),
                is_dir: meta.is_dir(),
                size: meta.len(),
                modified,
                is_symlink: meta.file_type().is_symlink(),
                mode: Some(meta.permissions().mode()),
            });
        }
        // 排序保证测试稳定
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(entries)
    }

    async fn create_dir_all(&self, path: &HostPath) -> Result<(), HostError> {
        let local = self.to_local(path);
        tokio::fs::create_dir_all(&local).await?;
        Ok(())
    }

    async fn remove_file(&self, path: &HostPath) -> Result<(), HostError> {
        let local = self.to_local(path);
        match tokio::fs::remove_file(&local).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Err(HostError::PathNotFound { path: path.clone() })
            }
            Err(e) => Err(HostError::Io(e)),
        }
    }

    async fn rename(&self, from: &HostPath, to: &HostPath) -> Result<(), HostError> {
        match tokio::fs::rename(self.to_local(from), self.to_local(to)).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Err(HostError::PathNotFound { path: from.clone() })
            }
            Err(e) => Err(HostError::Io(e)),
        }
    }

    async fn remove_dir_all(&self, path: &HostPath) -> Result<(), HostError> {
        let local = self.to_local(path);
        match tokio::fs::remove_dir_all(&local).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Err(HostError::PathNotFound { path: path.clone() })
            }
            Err(e) => Err(HostError::Io(e)),
        }
    }

    async fn exists(&self, path: &HostPath) -> Result<bool, HostError> {
        let local = self.to_local(path);
        match tokio::fs::metadata(&local).await {
            Ok(_) => Ok(true),
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
                ) =>
            {
                Ok(false)
            }
            Err(e) => Err(HostError::Io(e)),
        }
    }

    async fn file_size(&self, path: &HostPath) -> Result<Option<u64>, HostError> {
        // metadata 跟随链接，拿到的是目标的大小
        match tokio::fs::metadata(self.to_local(path)).await {
            Ok(m) if m.is_file() => Ok(Some(m.len())),
            Ok(_) => Ok(None),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(HostError::Io(e)),
        }
    }

    async fn upload(&self, local_src: &Path, remote: &HostPath) -> Result<(), HostError> {
        // 本地 Host 的 upload 等同于 copy,目标用 HostPath 表达
        let dest = self.to_local(remote);
        if let Some(parent) = dest.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                tokio::fs::create_dir_all(parent).await?;
            }
        }
        tokio::fs::copy(local_src, &dest).await?;
        Ok(())
    }

    async fn download(&self, remote: &HostPath, local_dst: &Path) -> Result<(), HostError> {
        let src = self.to_local(remote);
        if let Some(parent) = local_dst.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                tokio::fs::create_dir_all(parent).await?;
            }
        }
        tokio::fs::copy(&src, local_dst).await?;
        Ok(())
    }

    async fn extract_archive(
        &self,
        archive: &HostPath,
        dest: &HostPath,
        kind: ArchiveKind,
    ) -> Result<(), HostError> {
        self.create_dir_all(dest).await?;
        if matches!(kind, ArchiveKind::Msi) {
            return Err(HostError::Unsupported {
                operation: "extract_msi",
            });
        }
        if matches!(kind, ArchiveKind::Zip) {
            let source = self.to_local(archive);
            let destination = self.to_local(dest);
            return tokio::task::spawn_blocking(move || {
                let file = std::fs::File::open(source)?;
                let mut zip = zip::ZipArchive::new(file).map_err(std::io::Error::other)?;
                zip.extract(destination).map_err(std::io::Error::other)
            })
            .await
            .map_err(|e| HostError::Io(std::io::Error::other(e)))?
            .map_err(HostError::Io);
        }
        let flag = if matches!(kind, ArchiveKind::TarGz) {
            "-xzf"
        } else {
            "-xJf"
        };
        let out = self
            .run_to_string(HostCommand::new("tar").args([
                flag,
                archive.as_posix(),
                "-C",
                dest.as_posix(),
            ]))
            .await?;
        if out.success() {
            Ok(())
        } else {
            Err(HostError::ExtractFailed {
                archive: archive.clone(),
                reason: out.stderr,
            })
        }
    }

    async fn open_pty(
        &self,
        req: crate::pty::PtyRequest,
    ) -> Result<crate::pty::PtySession, HostError> {
        super::pty_linux::open_pty(req)
    }

    async fn open_tunnel(
        &self,
        spec: crate::remote::TunnelSpec,
    ) -> Result<crate::remote::TunnelHandle, HostError> {
        crate::remote::TunnelHandle::local_forward(spec).await
    }

    async fn spawn(&self, cmd: HostCommand) -> Result<Box<dyn HostProcess>, HostError> {
        let cmd = self.prepare_command(cmd).await?;
        let mut tokio_cmd = build_tokio_command(&cmd)?;
        tokio_cmd
            .stdin(if cmd.stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = tokio_cmd.spawn().map_err(HostError::Io)?;
        let pgid = child.id().unwrap_or(0);

        // 写 stdin(如果指定了)
        if let Some(stdin_data) = cmd.stdin {
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(&stdin_data).await?;
                drop(stdin);
            }
        }

        let pid = pgid;
        let process = ChildHostProcess {
            child: Some(child),
            id: ProcessId {
                native: pid,
                origin: self.id().to_string(),
            },
            timeout: cmd.timeout,
            wait_policy: cmd.wait_policy,
        };
        Ok(Box::new(process))
    }

    async fn run_to_string(&self, cmd: HostCommand) -> Result<CommandOutput, HostError> {
        let cmd = self.prepare_command(cmd).await?;
        let mut tokio_cmd = build_tokio_command(&cmd)?;
        tokio_cmd
            .stdin(if cmd.stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = tokio_cmd.spawn().map_err(HostError::Io)?;
        let pgid = child.id().unwrap_or(0);

        if let Some(stdin_data) = cmd.stdin {
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(&stdin_data).await?;
                drop(stdin);
            }
        }

        // 不用 wait_with_output：它拿走 child，超时后就杀不了了。进程树不收掉的话超时
        // 只是这边不等了，命令和它起的子进程照样跑下去，还攥着本进程当时可继承的句柄
        let timeout = cmd.timeout.unwrap_or(DEFAULT_COMMAND_TIMEOUT);
        let stdout_pipe = child.stdout.take();
        let stderr_pipe = child.stderr.take();
        let collected = tokio::time::timeout(timeout, async {
            let (status, stdout, stderr) = tokio::join!(
                child.wait(),
                read_pipe_to_end(stdout_pipe),
                read_pipe_to_end(stderr_pipe)
            );
            Ok::<_, std::io::Error>((status?, stdout?, stderr?))
        })
        .await;
        let (status, stdout, stderr) = match collected {
            Ok(Ok(out)) => out,
            Ok(Err(e)) => return Err(HostError::Io(e)),
            Err(_) => {
                force_kill_local_child(&mut child, pgid).await;
                return Err(HostError::Timeout {
                    operation: "run_to_string",
                });
            }
        };

        Ok(CommandOutput {
            exit_code: status.code(),
            stdout: String::from_utf8_lossy(&stdout).into_owned(),
            stderr: String::from_utf8_lossy(&stderr).into_owned(),
        })
    }

    async fn run_streaming(
        &self,
        cmd: HostCommand,
        mut on_line: Box<dyn FnMut(crate::host::StreamSource, String) + Send>,
    ) -> Result<CommandOutput, HostError> {
        use crate::host::StreamSource;

        let cmd = self.prepare_command(cmd).await?;
        let mut tokio_cmd = build_tokio_command(&cmd)?;
        tokio_cmd
            .stdin(if cmd.stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = tokio_cmd.spawn().map_err(HostError::Io)?;
        let pgid = child.id().unwrap_or(0);

        if let Some(stdin_data) = &cmd.stdin {
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(stdin_data).await?;
                drop(stdin);
            }
        }

        // on_line 是 FnMut,不能跨 task 共享用 mpsc channel:两个 reader task
        // 各自发 (StreamSource, String),主循环收到后调 on_line
        let (tx, mut rx) = mpsc::channel::<(StreamSource, String)>(256);

        let stdout_pipe = child.stdout.take();
        let stderr_pipe = child.stderr.take();

        let tx_out = tx.clone();
        let stdout_task = tokio::spawn(async move {
            if let Some(mut pipe) = stdout_pipe {
                let mut chunk = [0u8; 4096];
                let mut buf = Vec::new();
                loop {
                    match pipe.read(&mut chunk).await {
                        Ok(0) => break,
                        Ok(n) => {
                            crate::stream_chunk::feed_stream_chunk(&mut buf, &chunk[..n], |s| {
                                let _ = tx_out.try_send((StreamSource::Stdout, s));
                            });
                        }
                        Err(_) => break,
                    }
                }
                crate::stream_chunk::flush_stream_remainder(&mut buf, |s| {
                    let _ = tx_out.try_send((StreamSource::Stdout, s));
                });
            }
        });

        let tx_err = tx.clone();
        let stderr_task = tokio::spawn(async move {
            if let Some(mut pipe) = stderr_pipe {
                let mut chunk = [0u8; 4096];
                let mut buf = Vec::new();
                loop {
                    match pipe.read(&mut chunk).await {
                        Ok(0) => break,
                        Ok(n) => {
                            crate::stream_chunk::feed_stream_chunk(&mut buf, &chunk[..n], |s| {
                                let _ = tx_err.try_send((StreamSource::Stderr, s));
                            });
                        }
                        Err(_) => break,
                    }
                }
                crate::stream_chunk::flush_stream_remainder(&mut buf, |s| {
                    let _ = tx_err.try_send((StreamSource::Stderr, s));
                });
            }
        });

        // tx 本体 drop 掉,让 rx 在两个 reader task 都结束后自然关闭
        drop(tx);

        let timeout = cmd.timeout.unwrap_or(DEFAULT_COMMAND_TIMEOUT);
        let deadline = tokio::time::Instant::now() + timeout;
        let cancel = cmd.cancel.clone();

        let mut stdout_lines: Vec<String> = Vec::new();
        let mut stderr_lines: Vec<String> = Vec::new();

        // 收行循环:回调收 owned String(trait 签名如此,避开 async_trait 下
        // &str 生命周期被 box 固定的问题)行很小,clone 进缓冲可忽略
        loop {
            if cancel.as_ref().is_some_and(|c| c.is_cancelled()) {
                force_kill_local_child(&mut child, pgid).await;
                let _ = stdout_task.await;
                let _ = stderr_task.await;
                return Err(HostError::Cancelled);
            }

            let recv = rx.recv();
            let wait = async {
                if let Some(token) = cancel.as_ref() {
                    tokio::select! {
                        biased;
                        _ = token.cancelled() => None,
                        msg = recv => msg,
                    }
                } else {
                    recv.await
                }
            };

            match tokio::time::timeout_at(deadline, wait).await {
                Ok(Some((src, line))) => match src {
                    StreamSource::Stdout => {
                        on_line(StreamSource::Stdout, line.clone());
                        stdout_lines.push(line);
                    }
                    StreamSource::Stderr => {
                        on_line(StreamSource::Stderr, line.clone());
                        stderr_lines.push(line);
                    }
                },
                Ok(None) => {
                    if cancel.as_ref().is_some_and(|c| c.is_cancelled()) {
                        force_kill_local_child(&mut child, pgid).await;
                        let _ = stdout_task.await;
                        let _ = stderr_task.await;
                        return Err(HostError::Cancelled);
                    }
                    break; // channel 关闭,两个 reader task 都结束了
                }
                Err(_) => {
                    force_kill_local_child(&mut child, pgid).await;
                    let _ = stdout_task.await;
                    let _ = stderr_task.await;
                    return Err(HostError::Timeout {
                        operation: "run_streaming",
                    });
                }
            }
        }

        // reader task 结束后等进程退出拿 exit code
        let _ = stdout_task.await;
        let _ = stderr_task.await;
        let status = match tokio::time::timeout_at(deadline, child.wait()).await {
            Ok(result) => result?,
            Err(_) => {
                force_kill_local_child(&mut child, pgid).await;
                return Err(HostError::Timeout {
                    operation: "run_streaming",
                });
            }
        };

        Ok(CommandOutput {
            exit_code: status.code(),
            stdout: stdout_lines.join("\n"),
            stderr: stderr_lines.join("\n"),
        })
    }
}
async fn read_pipe_to_end(
    pipe: Option<impl tokio::io::AsyncRead + Unpin>,
) -> std::io::Result<Vec<u8>> {
    let mut buf = Vec::new();
    if let Some(mut pipe) = pipe {
        pipe.read_to_end(&mut buf).await?;
    }
    Ok(buf)
}

// 每条命令独立进程组，取消与超时可以收掉整棵命令树。
fn build_tokio_command(cmd: &HostCommand) -> Result<Command, HostError> {
    let mut command = Command::new(&cmd.program);
    command
        .args(&cmd.args)
        .envs(&cmd.environment)
        .process_group(0)
        .kill_on_drop(true);
    crate::appimage::clean_appimage_environment(command.as_std_mut());
    if let Some(cwd) = &cmd.working_dir {
        command.current_dir(cwd.as_posix());
    }
    Ok(command)
}

#[expect(
    unsafe_code,
    reason = "kill the process group created for this command"
)]
fn kill_group(pgid: u32) {
    if pgid != 0 {
        // SAFETY: pgid 来自独立进程组的 leader pid，0 不会传给 kill。
        unsafe {
            libc::kill(-(pgid as i32), libc::SIGKILL);
        }
    }
}

async fn force_kill_local_child(child: &mut Child, pgid: u32) {
    kill_group(pgid);
    let _ = child.start_kill();
    let _ = child.wait().await;
}

struct ChildHostProcess {
    child: Option<Child>,
    id: ProcessId,
    timeout: Option<Duration>,
    wait_policy: HostProcessWaitPolicy,
}

impl Drop for ChildHostProcess {
    fn drop(&mut self) {
        if self.child.is_some() {
            kill_group(self.id.native);
        }
    }
}

#[async_trait]
impl HostProcess for ChildHostProcess {
    fn id(&self) -> ProcessId {
        self.id.clone()
    }

    async fn wait(mut self: Box<Self>) -> Result<CommandOutput, HostError> {
        let pgid = self.id.native;
        let mut child = self
            .child
            .take()
            .ok_or_else(|| HostError::InvalidArgument {
                reason: "child already consumed".into(),
            })?;
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let collect = async {
            let (status, stdout, stderr) = tokio::join!(
                child.wait(),
                read_pipe_to_end(stdout),
                read_pipe_to_end(stderr)
            );
            Ok::<_, std::io::Error>((status?, stdout?, stderr?))
        };
        let collected = match self.wait_policy.resolve_timeout(self.timeout) {
            Some(timeout) => tokio::time::timeout(timeout, collect).await,
            None => Ok(collect.await),
        };
        let (status, stdout, stderr) = match collected {
            Ok(result) => result?,
            Err(_) => {
                force_kill_local_child(&mut child, pgid).await;
                return Err(HostError::Timeout {
                    operation: "process_wait",
                });
            }
        };

        Ok(CommandOutput {
            exit_code: status.code(),
            stdout: String::from_utf8_lossy(&stdout).into_owned(),
            stderr: String::from_utf8_lossy(&stderr).into_owned(),
        })
    }

    async fn try_wait(&mut self) -> Result<ExitStatus, HostError> {
        let child = self
            .child
            .as_mut()
            .ok_or_else(|| HostError::InvalidArgument {
                reason: "child already consumed".into(),
            })?;
        match child.try_wait()? {
            None => Ok(ExitStatus::Running),
            Some(status) => match status.code() {
                Some(code) => Ok(ExitStatus::Exited(code)),
                None => Ok(ExitStatus::Killed),
            },
        }
    }

    async fn kill(&mut self) -> Result<(), HostError> {
        let child = self
            .child
            .as_mut()
            .ok_or_else(|| HostError::InvalidArgument {
                reason: "child already consumed".into(),
            })?;
        force_kill_local_child(child, self.id.native).await;
        Ok(())
    }

    async fn write_stdin(&mut self, data: &[u8]) -> Result<(), HostError> {
        let child = self
            .child
            .as_mut()
            .ok_or_else(|| HostError::InvalidArgument {
                reason: "child already consumed".into(),
            })?;
        let stdin = child.stdin.as_mut().ok_or(HostError::Unsupported {
            operation: "stdin pipe not available",
        })?;
        stdin.write_all(data).await?;
        Ok(())
    }

    async fn close_stdin(&mut self) -> Result<(), HostError> {
        let child = self
            .child
            .as_mut()
            .ok_or_else(|| HostError::InvalidArgument {
                reason: "child already consumed".into(),
            })?;
        child.stdin = None;
        Ok(())
    }

    fn take_stdout(&mut self) -> Option<Box<dyn tokio::io::AsyncRead + Send + Unpin>> {
        let child = self.child.as_mut()?;
        // 注意:take_stdout 只能调一次spawn 时已经 .stdout(Stdio::piped()),
        // 这里 take 走 ChildStdout,wait 阶段 wait_with_output 自然只读 stderr
        child
            .stdout
            .take()
            .map(|s| Box::new(s) as Box<dyn tokio::io::AsyncRead + Send + Unpin>)
    }

    fn take_stderr(&mut self) -> Option<Box<dyn tokio::io::AsyncRead + Send + Unpin>> {
        let child = self.child.as_mut()?;
        child
            .stderr
            .take()
            .map(|s| Box::new(s) as Box<dyn tokio::io::AsyncRead + Send + Unpin>)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn appimage_archive_extraction_uses_the_host_lzma_library() {
        const MARKER: &str = "NCD_APPIMAGE_ARCHIVE_FIXTURE";
        if let Some(root) = std::env::var_os(MARKER) {
            let root = PathBuf::from(root);
            LocalLinuxHost::new()
                .extract_archive(
                    &HostPath::from_posix(root.join("node.tar.xz").to_string_lossy()),
                    &HostPath::from_posix(root.join("out").to_string_lossy()),
                    ArchiveKind::TarXz,
                )
                .await
                .unwrap();
            assert_eq!(std::fs::read(root.join("out/node")).unwrap(), b"fixture");
            return;
        }
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("node"), b"fixture").unwrap();
        let archive = std::process::Command::new("tar")
            .args(["-cJf", "node.tar.xz", "node"])
            .current_dir(root.path())
            .status()
            .unwrap();
        assert!(archive.success());
        let appdir = root.path().join("appimage");
        std::fs::create_dir_all(appdir.join("usr/lib")).unwrap();
        let libs = appdir.join("usr/lib");
        // 模拟 AppImage 中不兼容的 liblzma；未经清理的系统 xz 必须失败。
        std::fs::write(libs.join("liblzma.so.5"), b"incompatible bundled library").unwrap();
        let broken = std::process::Command::new("xz")
            .arg("--version")
            .env("LD_LIBRARY_PATH", &libs)
            .output()
            .unwrap();
        assert!(!broken.status.success());
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "local::linux::tests::appimage_archive_extraction_uses_the_host_lzma_library",
                "--nocapture",
            ])
            .env(MARKER, root.path())
            .env("APPDIR", &appdir)
            .env("LD_LIBRARY_PATH", libs)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[tokio::test]
    async fn exists_returns_false_when_parent_is_a_file() {
        let workspace = tempfile::tempdir().unwrap();
        let binary = workspace.path().join("node");
        tokio::fs::write(&binary, b"binary").await.unwrap();
        let host = LocalLinuxHost::new();
        assert!(
            host.exists(&HostPath::from_posix(binary.to_string_lossy()))
                .await
                .unwrap()
        );
        assert!(
            !host
                .exists(&HostPath::from_posix(
                    binary.join("bin/node").to_string_lossy()
                ))
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn timeout_stops_the_entire_command_group() {
        let workspace = tempfile::tempdir().unwrap();
        let host = LocalLinuxHost::new();
        let result = host
            .run_to_string(
                HostCommand::new("sh")
                    .args(["-c", "(sleep 0.2; printf orphan > survived) & exit 0"])
                    .working_dir(HostPath::from_posix(workspace.path().to_string_lossy()))
                    .timeout(Duration::from_millis(30)),
            )
            .await;
        assert!(matches!(result, Err(HostError::Timeout { .. })));
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(!workspace.path().join("survived").exists());
    }
}
