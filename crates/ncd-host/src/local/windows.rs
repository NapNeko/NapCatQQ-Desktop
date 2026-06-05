//! `LocalWindowsHost`:本地 Windows 实装。
//!
//! 把 [`Host`](crate::Host) trait 在本地 Windows 上跑通。
//!
//! 实装映射:
//! - 文件 IO:`tokio::fs`
//! - 进程:`tokio::process::Command`
//! - 解压 zip:`zip` crate
//! - 解压 tar.gz:`tar` + `flate2`
//! - 解压 tar.xz:`HostError::Unsupported`(暂不实装,后续按需补)
//! - 解压 msi:走 `msiexec /a` 静默提取(简化版)
//! - 提权:`HostCommand::elevated` 走 ShellExecuteW("runas") —— 暂返回
//!   `Unsupported`,完整提权链留给 `ncd-update` crate 的 `DesktopSelfComponent::SelfUpdate`。
//!
//! 注意:
//! - 本实装只在 `target_os = "windows"` 下编译(由 `local/mod.rs` 的 `#[cfg(windows)]` 控制)
//! - PackageManager 默认返回 `None`(暂不接 winget / choco,后续统一处理)

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;

use crate::command::{CommandOutput, HostCommand};
use crate::error::HostError;
use crate::subprocess::hide_console_window;
use crate::host::{Arch, Host, Locality, Os};
use crate::package_manager::PackageManager;
use crate::path::{ArchiveKind, DirEntry, HostPath, PathStyle};
use crate::process::{ExitStatus, HostProcess, ProcessId};
use crate::shell::{HostShell, PowerShellShell};

/// 本地 Windows 主机。
///
/// 调用 [`LocalWindowsHost::new`] 拿到一个零状态实例。
pub struct LocalWindowsHost {
    id: String,
    shell: PowerShellShell,
}

impl LocalWindowsHost {
    /// 创建新实例。`id` 默认为 `"local"`,跨 Host 区分日志用。
    pub fn new() -> Self {
        Self {
            id: "local".to_string(),
            shell: PowerShellShell,
        }
    }

    /// 自定义 id(多本地 Host 协同时用,正常单实例不需要)。
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    /// 当前进程架构(用编译期常量,没必要运行时探测)。
    fn detect_arch() -> Arch {
        #[cfg(target_arch = "x86_64")]
        return Arch::X86_64;
        #[cfg(target_arch = "aarch64")]
        return Arch::Aarch64;
        #[cfg(target_arch = "x86")]
        return Arch::X86;
        #[cfg(target_arch = "arm")]
        return Arch::Armv7;
        #[allow(unreachable_code)]
        Arch::X86_64
    }

    /// HostPath → 本地 PathBuf
    fn to_local(&self, path: &HostPath) -> PathBuf {
        PathBuf::from(path.render(PathStyle::Windows))
    }
}

impl Default for LocalWindowsHost {
    fn default() -> Self {
        Self::new()
    }
}


// ============================================================
// Host trait 实装
// ============================================================

#[async_trait]
impl Host for LocalWindowsHost {
    // ===== 身份 =====

    fn os(&self) -> Os {
        Os::Windows
    }

    fn arch(&self) -> Arch {
        Self::detect_arch()
    }

    fn locality(&self) -> Locality {
        Locality::Local
    }

    fn id(&self) -> &str {
        &self.id
    }

    fn shell(&self) -> &dyn HostShell {
        &self.shell
    }

    fn pkg_manager(&self) -> Option<&dyn PackageManager> {
        // 暂不实装 winget / choco;调用方应走"手动下载 + extract_archive"路径
        None
    }

    // ===== 文件操作 =====

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
                return Err(HostError::PathNotFound { path: path.clone() })
            }
            Err(e) => return Err(HostError::Io(e)),
        };

        let mut entries = Vec::new();
        while let Some(entry) = rd.next_entry().await? {
            let meta = entry.metadata().await?;
            entries.push(DirEntry {
                name: entry.file_name().to_string_lossy().into_owned(),
                is_dir: meta.is_dir(),
                size: meta.len(),
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
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
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
        let archive_local = self.to_local(archive);
        let dest_local = self.to_local(dest);

        // 确保目标目录存在
        if !dest_local.exists() {
            tokio::fs::create_dir_all(&dest_local).await?;
        }

        let archive_clone = archive.clone();
        let dest_clone = dest.clone();

        // 同步 IO 在 spawn_blocking 里跑,避免阻塞 tokio runtime
        match kind {
            ArchiveKind::Zip => {
                tokio::task::spawn_blocking(move || -> Result<(), HostError> {
                    extract_zip(&archive_local, &dest_local).map_err(|reason| {
                        HostError::ExtractFailed {
                            archive: archive_clone,
                            reason,
                        }
                    })
                })
                .await
                .map_err(|e| HostError::ExtractFailed {
                    archive: dest.clone(),
                    reason: format!("blocking task panicked: {e}"),
                })??;
                Ok(())
            }
            ArchiveKind::TarGz => {
                tokio::task::spawn_blocking(move || -> Result<(), HostError> {
                    extract_tar_gz(&archive_local, &dest_local).map_err(|reason| {
                        HostError::ExtractFailed {
                            archive: archive_clone,
                            reason,
                        }
                    })
                })
                .await
                .map_err(|e| HostError::ExtractFailed {
                    archive: dest_clone,
                    reason: format!("blocking task panicked: {e}"),
                })??;
                Ok(())
            }
            ArchiveKind::TarXz => Err(HostError::Unsupported {
                operation: "extract_tar_xz",
            }),
            ArchiveKind::Msi => {
                // 走 msiexec /a <pkg> /qn TARGETDIR=<dest>(管理员安装,只解压不写注册表)
                let mut cmd = Command::new("msiexec.exe");
                cmd.arg("/a")
                    .arg(&archive_local)
                    .arg("/qn")
                    .arg(format!("TARGETDIR={}", dest_local.display()))
                    .stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped());

                let output = cmd.output().await.map_err(|e| HostError::ExtractFailed {
                    archive: archive.clone(),
                    reason: format!("msiexec spawn failed: {e}"),
                })?;
                if !output.status.success() {
                    return Err(HostError::ExtractFailed {
                        archive: archive.clone(),
                        reason: format!(
                            "msiexec exit={:?}: {}",
                            output.status.code(),
                            String::from_utf8_lossy(&output.stderr)
                        ),
                    });
                }
                Ok(())
            }
        }
    }


    // ===== 进程操作 =====

    async fn spawn(&self, cmd: HostCommand) -> Result<Box<dyn HostProcess>, HostError> {
        if cmd.elevated {
            // 提权链路留给 ncd-update 的 SelfUpdate Action,Host trait 这层不实装
            return Err(HostError::ElevationFailed {
                locality: "local",
                reason: "elevation via UAC must go through ncd-update::desktop_self".into(),
            });
        }

        let mut tokio_cmd = build_tokio_command(&cmd, self)?;
        tokio_cmd
            .stdin(if cmd.stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = tokio_cmd.spawn().map_err(HostError::Io)?;

        // 写 stdin(如果指定了)
        if let Some(stdin_data) = cmd.stdin {
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(&stdin_data).await?;
                drop(stdin);
            }
        }

        let pid = child.id().unwrap_or(0);
        let process = ChildHostProcess {
            child: Some(child),
            id: ProcessId {
                native: pid,
                origin: self.id.clone(),
            },
            timeout: cmd.timeout,
        };
        Ok(Box::new(process))
    }

    async fn run_to_string(&self, cmd: HostCommand) -> Result<CommandOutput, HostError> {
        if cmd.elevated {
            return Err(HostError::ElevationFailed {
                locality: "local",
                reason: "elevation via UAC must go through ncd-update::desktop_self".into(),
            });
        }
        let mut tokio_cmd = build_tokio_command(&cmd, self)?;
        tokio_cmd
            .stdin(if cmd.stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = tokio_cmd.spawn().map_err(HostError::Io)?;

        if let Some(stdin_data) = cmd.stdin {
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(&stdin_data).await?;
                drop(stdin);
            }
        }

        let timeout = cmd.timeout.unwrap_or(Duration::from_secs(300));
        let output_future = child.wait_with_output();
        let output = match tokio::time::timeout(timeout, output_future).await {
            Ok(Ok(out)) => out,
            Ok(Err(e)) => return Err(HostError::Io(e)),
            Err(_) => {
                return Err(HostError::Timeout {
                    operation: "run_to_string",
                });
            }
        };

        Ok(CommandOutput {
            exit_code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }

    async fn run_streaming(
        &self,
        cmd: HostCommand,
        mut on_line: Box<dyn FnMut(crate::host::StreamSource, String) + Send>,
    ) -> Result<CommandOutput, HostError> {
        use crate::host::StreamSource;

        if cmd.elevated {
            return Err(HostError::ElevationFailed {
                locality: "local",
                reason: "elevation via UAC must go through ncd-update::desktop_self".into(),
            });
        }

        let mut tokio_cmd = build_tokio_command(&cmd, self)?;
        tokio_cmd
            .stdin(if cmd.stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = tokio_cmd.spawn().map_err(HostError::Io)?;

        if let Some(stdin_data) = &cmd.stdin {
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(stdin_data).await?;
                drop(stdin);
            }
        }

        // on_line 是 FnMut，不能跨 task 共享。用 mpsc channel：两个 reader task
        // 各自发 (StreamSource, String)，主循环收到后调 on_line。
        let (tx, mut rx) = mpsc::channel::<(StreamSource, String)>(256);

        let stdout_pipe = child.stdout.take();
        let stderr_pipe = child.stderr.take();

        let tx_out = tx.clone();
        let stdout_task = tokio::spawn(async move {
            if let Some(pipe) = stdout_pipe {
                let mut reader = BufReader::new(pipe).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    if tx_out.send((StreamSource::Stdout, line)).await.is_err() {
                        break;
                    }
                }
            }
        });

        let tx_err = tx.clone();
        let stderr_task = tokio::spawn(async move {
            if let Some(pipe) = stderr_pipe {
                let mut reader = BufReader::new(pipe).lines();
                while let Ok(Some(line)) = reader.next_line().await {
                    if tx_err.send((StreamSource::Stderr, line)).await.is_err() {
                        break;
                    }
                }
            }
        });

        // tx 本体 drop 掉，让 rx 在两个 reader task 都结束后自然关闭。
        drop(tx);

        let timeout = cmd.timeout.unwrap_or(Duration::from_secs(300));
        let deadline = tokio::time::Instant::now() + timeout;

        let mut stdout_lines: Vec<String> = Vec::new();
        let mut stderr_lines: Vec<String> = Vec::new();

        // 收行循环：回调收 owned String（trait 签名如此，避开 async_trait 下
        // &str 生命周期被 box 固定的问题）。行很小，clone 进缓冲可忽略。
        loop {
            match tokio::time::timeout_at(deadline, rx.recv()).await {
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
                Ok(None) => break, // channel 关闭，两个 reader task 都结束了
                Err(_) => {
                    return Err(HostError::Timeout {
                        operation: "run_streaming",
                    });
                }
            }
        }

        // reader task 结束后等进程退出拿 exit code。
        let _ = stdout_task.await;
        let _ = stderr_task.await;
        let status = child.wait().await.map_err(HostError::Io)?;

        Ok(CommandOutput {
            exit_code: status.code(),
            stdout: stdout_lines.join("\n"),
            stderr: stderr_lines.join("\n"),
        })
    }
}

// ============================================================
// build_tokio_command:HostCommand → tokio::process::Command
// ============================================================

fn build_tokio_command(
    cmd: &HostCommand,
    host: &LocalWindowsHost,
) -> Result<Command, HostError> {
    if cmd.program.is_empty() {
        return Err(HostError::InvalidArgument {
            reason: "program is empty".into(),
        });
    }
    let mut tokio_cmd = Command::new(&cmd.program);
    tokio_cmd.args(&cmd.args);

    if let Some(wd) = &cmd.working_dir {
        tokio_cmd.current_dir(host.to_local(wd));
    }

    // 环境变量(BTreeMap 已保证有序,不会跑出非确定顺序)
    for (k, v) in &cmd.environment {
        tokio_cmd.env(k, v);
    }

    hide_console_window(&mut tokio_cmd);

    Ok(tokio_cmd)
}

// ============================================================
// ChildHostProcess:tokio::process::Child 包装成 HostProcess
// ============================================================

struct ChildHostProcess {
    child: Option<Child>,
    id: ProcessId,
    timeout: Option<Duration>,
}

#[async_trait]
impl HostProcess for ChildHostProcess {
    fn id(&self) -> ProcessId {
        self.id.clone()
    }

    async fn wait(mut self: Box<Self>) -> Result<CommandOutput, HostError> {
        let child = self.child.take().ok_or_else(|| HostError::InvalidArgument {
            reason: "child already consumed".into(),
        })?;

        let timeout = self.timeout.unwrap_or(Duration::from_secs(300));
        let output = match tokio::time::timeout(timeout, child.wait_with_output()).await {
            Ok(Ok(out)) => out,
            Ok(Err(e)) => return Err(HostError::Io(e)),
            Err(_) => {
                return Err(HostError::Timeout {
                    operation: "process_wait",
                });
            }
        };

        Ok(CommandOutput {
            exit_code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }

    async fn try_wait(&mut self) -> Result<ExitStatus, HostError> {
        let child = self.child.as_mut().ok_or_else(|| HostError::InvalidArgument {
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
        let child = self.child.as_mut().ok_or_else(|| HostError::InvalidArgument {
            reason: "child already consumed".into(),
        })?;
        child.kill().await?;
        Ok(())
    }

    async fn write_stdin(&mut self, data: &[u8]) -> Result<(), HostError> {
        let child = self.child.as_mut().ok_or_else(|| HostError::InvalidArgument {
            reason: "child already consumed".into(),
        })?;
        let stdin = child
            .stdin
            .as_mut()
            .ok_or_else(|| HostError::Unsupported {
                operation: "stdin pipe not available",
            })?;
        stdin.write_all(data).await?;
        Ok(())
    }

    async fn close_stdin(&mut self) -> Result<(), HostError> {
        let child = self.child.as_mut().ok_or_else(|| HostError::InvalidArgument {
            reason: "child already consumed".into(),
        })?;
        child.stdin = None;
        Ok(())
    }

    fn take_stdout(&mut self) -> Option<Box<dyn tokio::io::AsyncRead + Send + Unpin>> {
        let child = self.child.as_mut()?;
        // 注意：take_stdout 只能调一次。spawn 时已经 .stdout(Stdio::piped())，
        // 这里 take 走 ChildStdout，wait 阶段 wait_with_output 自然只读 stderr。
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

// _Arc 已使用 prelude 引入但 Rust analyzer 可能报 unused —— 这里实际未用,删除即可


// ============================================================
// 同步解压辅助(在 spawn_blocking 内调用)
// ============================================================

fn extract_zip(archive: &Path, dest: &Path) -> Result<(), String> {
    let file = std::fs::File::open(archive).map_err(|e| format!("open zip: {e}"))?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| format!("zip open: {e}"))?;

    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| format!("zip entry {i}: {e}"))?;
        let raw_name = entry.name().to_string();
        // 路径越界保护:禁止 `..` 跳出
        if raw_name.contains("..") {
            return Err(format!("zip entry escapes dest: {raw_name}"));
        }
        let out_path = dest.join(&raw_name);
        if entry.is_dir() {
            std::fs::create_dir_all(&out_path)
                .map_err(|e| format!("mkdir {}: {e}", out_path.display()))?;
            continue;
        }
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("mkdir parent {}: {e}", parent.display()))?;
        }
        let mut out_file = std::fs::File::create(&out_path)
            .map_err(|e| format!("create {}: {e}", out_path.display()))?;
        std::io::copy(&mut entry, &mut out_file).map_err(|e| format!("copy {raw_name}: {e}"))?;
    }
    Ok(())
}

fn extract_tar_gz(archive: &Path, dest: &Path) -> Result<(), String> {
    let file = std::fs::File::open(archive).map_err(|e| format!("open tar.gz: {e}"))?;
    let gz = flate2::read::GzDecoder::new(file);
    let mut tar = tar::Archive::new(gz);
    // tar 已自动检测路径越界(默认安全)
    tar.unpack(dest).map_err(|e| format!("tar unpack: {e}"))?;
    Ok(())
}

// ============================================================
// 测试
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn temp_host_path(workspace: &tempfile::TempDir, sub: &str) -> HostPath {
        let full = workspace.path().join(sub);
        HostPath::from_windows(full.to_str().unwrap())
    }

    #[tokio::test]
    async fn identity_methods_return_expected_values() {
        let host = LocalWindowsHost::new();
        assert_eq!(host.os(), Os::Windows);
        assert_eq!(host.locality(), Locality::Local);
        assert_eq!(host.id(), "local");
        assert!(host.pkg_manager().is_none());
        // shell 应是 PowerShell
        assert_eq!(host.shell().kind(), crate::shell::ShellKind::PowerShell);
    }

    #[tokio::test]
    async fn write_then_read_round_trips() {
        let host = LocalWindowsHost::new();
        let ws = tempdir().unwrap();
        let path = temp_host_path(&ws, "sub/dir/hello.txt");
        host.write_file(&path, b"hello world").await.unwrap();
        let bytes = host.read_file(&path).await.unwrap();
        assert_eq!(bytes.as_ref(), b"hello world");
    }

    #[tokio::test]
    async fn read_missing_file_returns_path_not_found() {
        let host = LocalWindowsHost::new();
        let ws = tempdir().unwrap();
        let path = temp_host_path(&ws, "missing.txt");
        let err = host.read_file(&path).await.unwrap_err();
        assert!(matches!(err, HostError::PathNotFound { .. }));
    }

    #[tokio::test]
    async fn list_dir_lists_files_sorted() {
        let host = LocalWindowsHost::new();
        let ws = tempdir().unwrap();
        host.write_file(&temp_host_path(&ws, "z.txt"), b"z").await.unwrap();
        host.write_file(&temp_host_path(&ws, "a.txt"), b"a").await.unwrap();
        host.write_file(&temp_host_path(&ws, "m.txt"), b"m").await.unwrap();
        let dir = HostPath::from_windows(ws.path().to_str().unwrap());
        let entries = host.list_dir(&dir).await.unwrap();
        let names: Vec<_> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["a.txt", "m.txt", "z.txt"]);
    }

    #[tokio::test]
    async fn create_and_remove_dir() {
        let host = LocalWindowsHost::new();
        let ws = tempdir().unwrap();
        let dir = temp_host_path(&ws, "deep/nested/dir");
        host.create_dir_all(&dir).await.unwrap();
        assert!(host.exists(&dir).await.unwrap());
        host.remove_dir_all(&dir).await.unwrap();
        assert!(!host.exists(&dir).await.unwrap());
    }

    #[tokio::test]
    async fn upload_local_file_copies_it() {
        let host = LocalWindowsHost::new();
        let ws = tempdir().unwrap();
        let src = ws.path().join("src.bin");
        std::fs::write(&src, b"payload").unwrap();
        let dst = temp_host_path(&ws, "uploaded/dst.bin");
        host.upload(&src, &dst).await.unwrap();
        let read = host.read_file(&dst).await.unwrap();
        assert_eq!(read.as_ref(), b"payload");
    }

    #[tokio::test]
    async fn run_to_string_captures_stdout() {
        let host = LocalWindowsHost::new();
        // 用 cmd.exe /c echo,Windows 上一定能跑
        let cmd = HostCommand::new("cmd.exe").arg("/c").arg("echo").arg("hello");
        let out = host.run_to_string(cmd).await.unwrap();
        assert!(out.success());
        assert!(out.stdout.contains("hello"));
    }

    #[tokio::test]
    async fn run_to_string_captures_nonzero_exit() {
        let host = LocalWindowsHost::new();
        let cmd = HostCommand::new("cmd.exe").arg("/c").arg("exit").arg("7");
        let out = host.run_to_string(cmd).await.unwrap();
        assert_eq!(out.exit_code, Some(7));
        assert!(!out.success());
    }

    #[tokio::test]
    async fn run_to_string_respects_timeout() {
        let host = LocalWindowsHost::new();
        let cmd = HostCommand::new("cmd.exe")
            .arg("/c")
            .arg("ping")
            .arg("-n")
            .arg("60")
            .arg("127.0.0.1")
            .timeout(Duration::from_millis(300));
        let err = host.run_to_string(cmd).await.unwrap_err();
        assert!(matches!(err, HostError::Timeout { .. }));
    }

    #[tokio::test]
    async fn run_to_string_passes_env_vars() {
        let host = LocalWindowsHost::new();
        let cmd = HostCommand::new("cmd.exe")
            .arg("/c")
            .arg("echo")
            .arg("%NCD_TEST_VAR%")
            .env("NCD_TEST_VAR", "secret_payload");
        let out = host.run_to_string(cmd).await.unwrap();
        assert!(out.stdout.contains("secret_payload"));
    }

    #[tokio::test]
    async fn spawn_returns_running_process() {
        let host = LocalWindowsHost::new();
        let cmd = HostCommand::new("cmd.exe").arg("/c").arg("echo").arg("ok");
        let process = host.spawn(cmd).await.unwrap();
        let out = process.wait().await.unwrap();
        assert!(out.success());
        assert!(out.stdout.contains("ok"));
    }

    #[tokio::test]
    async fn spawn_with_stdin_pipe() {
        let host = LocalWindowsHost::new();
        let cmd = HostCommand::new("cmd.exe")
            .arg("/c")
            .arg("findstr")
            .arg("hello")
            .stdin("hello\r\nworld\r\n");
        let process = host.spawn(cmd).await.unwrap();
        let out = process.wait().await.unwrap();
        assert!(out.success());
        assert!(out.stdout.contains("hello"));
    }

    #[tokio::test]
    async fn elevated_command_returns_unsupported_for_now() {
        let host = LocalWindowsHost::new();
        let cmd = HostCommand::new("cmd.exe").arg("/c").arg("echo").elevated();
        let err = host.run_to_string(cmd).await.unwrap_err();
        assert!(matches!(err, HostError::ElevationFailed { .. }));
    }

    #[tokio::test]
    async fn empty_program_rejected() {
        let host = LocalWindowsHost::new();
        let cmd = HostCommand::new("");
        let err = host.run_to_string(cmd).await.unwrap_err();
        assert!(matches!(err, HostError::InvalidArgument { .. }));
    }

    #[tokio::test]
    async fn extract_zip_unpacks_files() {
        let host = LocalWindowsHost::new();
        let ws = tempdir().unwrap();

        // 制作一个测试 zip(同步,在测试里直接用 std)
        let zip_path = ws.path().join("test.zip");
        {
            let file = std::fs::File::create(&zip_path).unwrap();
            let mut writer = zip::ZipWriter::new(file);
            let opts: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default();
            writer.start_file("a.txt", opts).unwrap();
            std::io::Write::write_all(&mut writer, b"alpha").unwrap();
            writer.start_file("dir/b.txt", opts).unwrap();
            std::io::Write::write_all(&mut writer, b"beta").unwrap();
            writer.finish().unwrap();
        }

        let archive = HostPath::from_windows(zip_path.to_str().unwrap());
        let dest = temp_host_path(&ws, "extracted");
        host.extract_archive(&archive, &dest, ArchiveKind::Zip)
            .await
            .unwrap();

        let a = host
            .read_file(&temp_host_path(&ws, "extracted/a.txt"))
            .await
            .unwrap();
        assert_eq!(a.as_ref(), b"alpha");
        let b = host
            .read_file(&temp_host_path(&ws, "extracted/dir/b.txt"))
            .await
            .unwrap();
        assert_eq!(b.as_ref(), b"beta");
    }

    #[tokio::test]
    async fn extract_tar_xz_returns_unsupported() {
        let host = LocalWindowsHost::new();
        let ws = tempdir().unwrap();
        let archive = temp_host_path(&ws, "fake.tar.xz");
        host.write_file(&archive, b"dummy").await.unwrap();
        let dest = temp_host_path(&ws, "out");
        let err = host
            .extract_archive(&archive, &dest, ArchiveKind::TarXz)
            .await
            .unwrap_err();
        assert!(matches!(err, HostError::Unsupported { .. }));
    }
}
