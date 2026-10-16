//! Windows 目标路径与 SFTP 文件操作。
use std::path::Path;

use bytes::Bytes;
use serde::Deserialize;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::{RemoteWindowsHost, ensure_success, invalid_json, ps_literal};
use crate::{ArchiveKind, DirEntry, DriveEntry, DriveKind, Host, HostError, HostPath, Os};

impl RemoteWindowsHost {
    pub(super) fn windows_path(&self, path: &HostPath) -> Result<String, HostError> {
        target_path(path, &self.info.home_dir)
    }

    fn sftp_path(&self, path: &HostPath) -> Result<HostPath, HostError> {
        Ok(HostPath::from_posix(
            self.windows_path(path)?.replace('\\', "/"),
        ))
    }

    pub(super) async fn read(&self, path: &HostPath) -> Result<Bytes, HostError> {
        let remote = self.sftp_path(path)?;
        let session = self.transport.sftp_session().await?;
        let file = session.open(remote.as_posix()).await.map_err(|error| {
            super::super::linux::sftp_err_to_host(remote.as_posix(), "read", error)
        })?;
        let mut data = Vec::new();
        file.take(64 * 1024 * 1024 + 1)
            .read_to_end(&mut data)
            .await?;
        if data.len() > 64 * 1024 * 1024 {
            return Err(HostError::InvalidArgument {
                reason: "read_file超过64MiB，请使用download".into(),
            });
        }
        Ok(Bytes::from(data))
    }
    pub(super) async fn list(&self, path: &HostPath) -> Result<Vec<DirEntry>, HostError> {
        self.transport.list_dir(&self.sftp_path(path)?).await
    }
    pub(super) async fn path_exists(&self, path: &HostPath) -> Result<bool, HostError> {
        self.transport.exists(&self.sftp_path(path)?).await
    }
    pub(super) async fn size(&self, path: &HostPath) -> Result<Option<u64>, HostError> {
        self.transport.file_size(&self.sftp_path(path)?).await
    }
    pub(super) async fn mkdir(&self, path: &HostPath) -> Result<(), HostError> {
        let path = ps_literal(&self.windows_path(path)?);
        ensure_success(
            self.script(&format!(
                "$ErrorActionPreference='Stop';[void][IO.Directory]::CreateDirectory({path})"
            ))
            .await?,
            "create_directory",
        )
    }
    pub(super) async fn write(&self, path: &HostPath, bytes: &[u8]) -> Result<(), HostError> {
        let remote = self.sftp_path(path)?;
        if let Some(parent) = HostPath::from_windows(&self.windows_path(path)?).parent() {
            self.mkdir(&parent).await?;
        }
        let session = self.transport.sftp_session().await?;
        let mut file = session.create(remote.as_posix()).await.map_err(|error| {
            super::super::linux::sftp_err_to_host(remote.as_posix(), "write", error)
        })?;
        file.write_all(bytes)
            .await
            .map_err(|error| HostError::remote_disconnected(format!("SFTP write: {error}")))?;
        file.flush()
            .await
            .map_err(|error| HostError::remote_disconnected(format!("SFTP flush: {error}")))?;
        file.shutdown()
            .await
            .map_err(|error| HostError::remote_disconnected(format!("SFTP close: {error}")))?;
        Ok(())
    }
    pub(super) async fn put(&self, local: &Path, remote: &HostPath) -> Result<(), HostError> {
        if let Some(parent) = HostPath::from_windows(&self.windows_path(remote)?).parent() {
            self.mkdir(&parent).await?;
        }
        let remote = self.sftp_path(remote)?;
        let session = self.transport.sftp_session().await?;
        let mut input = tokio::fs::File::open(local).await?;
        let mut output = session.create(remote.as_posix()).await.map_err(|error| {
            super::super::linux::sftp_err_to_host(remote.as_posix(), "upload", error)
        })?;
        tokio::io::copy(&mut input, &mut output)
            .await
            .map_err(|error| HostError::remote_disconnected(format!("SFTP upload: {error}")))?;
        output.flush().await?;
        output.shutdown().await?;
        Ok(())
    }
    pub(super) async fn get(&self, remote: &HostPath, local: &Path) -> Result<(), HostError> {
        let remote = self.sftp_path(remote)?;
        let session = self.transport.sftp_session().await?;
        let mut input = session.open(remote.as_posix()).await.map_err(|error| {
            super::super::linux::sftp_err_to_host(remote.as_posix(), "download", error)
        })?;
        if let Some(parent) = local
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            tokio::fs::create_dir_all(parent).await?;
        }
        let mut output = tokio::fs::File::create(local).await?;
        tokio::io::copy(&mut input, &mut output).await?;
        output.flush().await?;
        Ok(())
    }
    pub(super) async fn move_path(&self, from: &HostPath, to: &HostPath) -> Result<(), HostError> {
        let script = rename_script(&self.windows_path(from)?, &self.windows_path(to)?);
        ensure_success(self.script(&script).await?, "rename")
    }
    pub(super) async fn remove(&self, path: &HostPath, recursive: bool) -> Result<(), HostError> {
        let resolved = self.windows_path(path)?;
        if !recursive {
            return self.transport.remove_file(&self.sftp_path(path)?).await;
        }
        let blocked = [&self.info.home_dir, &self.info.program_data];
        if protected_path(&resolved, &blocked) {
            return Err(HostError::InvalidArgument {
                reason: format!("拒绝递归删除受保护目录: {resolved}"),
            });
        }
        let script = remove_directory_script(&resolved);
        ensure_success(self.script(&script).await?, "remove_directory")
    }
    pub(super) async fn fetch(&self, url: &str, destination: &HostPath) -> Result<(), HostError> {
        if !(url.starts_with("https://") || url.starts_with("http://")) {
            return Err(HostError::InvalidArgument {
                reason: "下载地址仅支持HTTP/HTTPS".into(),
            });
        }
        if let Some(parent) = HostPath::from_windows(&self.windows_path(destination)?).parent() {
            self.mkdir(&parent).await?;
        }
        let script = format!(
            "$ErrorActionPreference='Stop';[Net.ServicePointManager]::SecurityProtocol=[Net.SecurityProtocolType]::Tls12;Invoke-WebRequest -UseBasicParsing -Uri {} -OutFile {}",
            ps_literal(url),
            ps_literal(&self.windows_path(destination)?)
        );
        ensure_success(self.script(&script).await?, "download_url")
    }
    pub(super) async fn extract(
        &self,
        archive: &HostPath,
        destination: &HostPath,
        kind: ArchiveKind,
    ) -> Result<(), HostError> {
        self.mkdir(destination).await?;
        let archive_literal = ps_literal(&self.windows_path(archive)?);
        let destination_literal = ps_literal(&self.windows_path(destination)?);
        if kind == ArchiveKind::Msi {
            let output = self
                .run_to_string(crate::HostCommand::new("msiexec.exe").args([
                    "/a".to_string(),
                    self.windows_path(archive)?,
                    "/qn".into(),
                    "/norestart".into(),
                    format!("TARGETDIR={}", self.windows_path(destination)?),
                ]))
                .await?;
            return if matches!(output.exit_code, Some(0 | 3010)) {
                Ok(())
            } else {
                Err(HostError::ExtractFailed {
                    archive: archive.clone(),
                    reason: output.stderr,
                })
            };
        }
        let script = match kind {
            ArchiveKind::Zip => format!(
                "$ErrorActionPreference='Stop';Expand-Archive -LiteralPath {archive_literal} -DestinationPath {destination_literal} -Force"
            ),
            ArchiveKind::TarGz | ArchiveKind::TarXz => format!(
                "$ErrorActionPreference='Stop';& tar.exe -xf {archive_literal} -C {destination_literal};if($LASTEXITCODE -ne 0){{exit $LASTEXITCODE}}"
            ),
            ArchiveKind::Msi => {
                return Err(HostError::Unsupported {
                    operation: "extract_msi",
                });
            }
        };
        let output = self.script(&script).await?;
        if output.success() {
            Ok(())
        } else {
            Err(HostError::ExtractFailed {
                archive: archive.clone(),
                reason: output.stderr,
            })
        }
    }
    pub(super) async fn drives(&self) -> Result<Vec<DriveEntry>, HostError> {
        let output = self.script("$ErrorActionPreference='Stop';[Console]::OutputEncoding=New-Object System.Text.UTF8Encoding($false);$items=@([IO.DriveInfo]::GetDrives()|ForEach-Object{@{root=$_.Name;kind=[int]$_.DriveType}});ConvertTo-Json -InputObject $items -Compress").await?;
        ensure_success(output.clone(), "list_drives")?;
        #[derive(Deserialize)]
        struct Drive {
            root: String,
            kind: u32,
        }
        let drives: Vec<Drive> =
            serde_json::from_str(output.stdout.trim().trim_start_matches('\u{feff}'))
                .map_err(invalid_json)?;
        Ok(drives
            .into_iter()
            .map(|drive| DriveEntry {
                root: drive.root,
                kind: match drive.kind {
                    2 => DriveKind::Removable,
                    3 => DriveKind::Fixed,
                    4 => DriveKind::Network,
                    5 => DriveKind::Optical,
                    _ => DriveKind::Other,
                },
            })
            .collect())
    }
}

pub(super) fn rename_script(from: &str, to: &str) -> String {
    // File.Replace 保留已有指针直到新文件原子发布；目录切换由调用方的 journal 管理。
    format!(
        "$ErrorActionPreference='Stop';$from={};$to={};if([IO.Directory]::Exists($from)){{if(Test-Path -LiteralPath $to){{throw 'Cannot overwrite a directory'}};[IO.Directory]::Move($from,$to)}}else{{if([IO.Directory]::Exists($to)){{throw 'Cannot overwrite a directory'}};if([IO.File]::Exists($to)){{[IO.File]::Replace($from,$to,$null)}}else{{[IO.File]::Move($from,$to)}}}}",
        ps_literal(from),
        ps_literal(to)
    )
}

pub(super) fn remove_directory_script(path: &str) -> String {
    // 不使用 PS5.1 的递归 provider；链接只删链接，不遍历其目标。
    format!(
        r#"$ErrorActionPreference='Stop';$p=[IO.Path]::GetFullPath({});if($p.TrimEnd('\') -eq [IO.Path]::GetPathRoot($p).TrimEnd('\')){{throw 'Protected root'}};$parent=[IO.Directory]::GetParent($p);while($null -ne $parent){{if($parent.Attributes -band [IO.FileAttributes]::ReparsePoint){{throw 'Refusing to delete through a reparse ancestor'}};$parent=$parent.Parent}};function Remove-NcdTree([string]$item){{if(!(Test-Path -LiteralPath $item)){{return}};$attributes=[IO.File]::GetAttributes($item);if($attributes -band [IO.FileAttributes]::ReparsePoint){{if($attributes -band [IO.FileAttributes]::Directory){{[IO.Directory]::Delete($item,$false)}}else{{[IO.File]::Delete($item)}};return}};if($attributes -band [IO.FileAttributes]::Directory){{foreach($child in [IO.Directory]::EnumerateFileSystemEntries($item)){{Remove-NcdTree $child}};[IO.File]::SetAttributes($item,($attributes -band (-bnot [IO.FileAttributes]::ReadOnly)));[IO.Directory]::Delete($item,$false)}}else{{[IO.File]::SetAttributes($item,($attributes -band (-bnot [IO.FileAttributes]::ReadOnly)));[IO.File]::Delete($item)}}}};Remove-NcdTree $p"#,
        ps_literal(path)
    )
}

pub(super) fn target_path(path: &HostPath, home: &str) -> Result<String, HostError> {
    // HostPath 的盘符转换可能把设备前缀变成 ///?/c/...；先按两种分隔符识别命名空间。
    let namespace_path = path.as_posix().replace('\\', "/");
    if namespace_path.starts_with('/')
        && matches!(
            namespace_path.trim_start_matches('/').split('/').next(),
            Some("?" | "." | "??")
        )
    {
        return Err(HostError::InvalidArgument {
            reason: "不支持Windows设备命名空间路径".into(),
        });
    }
    let rendered = path.render_for(Os::Windows);
    if rendered.contains('\0') {
        return Err(HostError::InvalidArgument {
            reason: "Windows路径含NUL字符".into(),
        });
    }
    let absolute = if rendered.as_bytes().get(1) == Some(&b':') || rendered.starts_with("\\\\") {
        rendered
    } else if rendered.starts_with('\\') {
        return Err(HostError::InvalidArgument {
            reason: format!("Windows目标路径必须包含盘符: {path}"),
        });
    } else {
        format!("{}\\{rendered}", home.trim_end_matches('\\'))
    };
    if absolute.starts_with("\\\\?\\")
        || absolute.starts_with("\\\\.\\")
        || (!absolute.starts_with("\\\\")
            && (!absolute.as_bytes()[0].is_ascii_alphabetic()
                || absolute.as_bytes().get(2) != Some(&b'\\')))
    {
        return Err(HostError::InvalidArgument {
            reason: "不支持Windows设备路径或盘符相对路径".into(),
        });
    }
    let root_components = if absolute.starts_with("\\\\") { 2 } else { 1 };
    let mut parts = Vec::new();
    for part in absolute.split('\\').filter(|part| !part.is_empty()) {
        match part {
            "." => {}
            ".." if parts.len() > root_components => {
                parts.pop();
            }
            ".." => {
                return Err(HostError::InvalidArgument {
                    reason: "Windows路径越过盘根".into(),
                });
            }
            _ if part.ends_with('.')
                || part.ends_with(' ')
                || (part.contains(':') && !parts.is_empty()) =>
            {
                return Err(HostError::InvalidArgument {
                    reason: "Windows路径包含有歧义的片段".into(),
                });
            }
            _ => parts.push(part),
        }
    }
    if parts.len() < root_components {
        return Err(HostError::InvalidArgument {
            reason: "Windows路径根无效".into(),
        });
    }
    let mut result = parts.join("\\");
    if absolute.starts_with("\\\\") {
        result = format!("\\\\{result}");
    }
    if !absolute.starts_with("\\\\") && parts.len() == 1 {
        result.push('\\');
    }
    Ok(result)
}

pub(super) fn protected_path(path: &str, blocked: &[&String]) -> bool {
    let normalized = path.trim_end_matches('\\').to_ascii_lowercase();
    let components: Vec<_> = normalized
        .split('\\')
        .filter(|part| !part.is_empty())
        .collect();
    normalized.len() <= 2
        || (normalized.starts_with("\\\\") && components.len() <= 2)
        || blocked
            .iter()
            .any(|path| normalized == path.trim_end_matches('\\').to_ascii_lowercase())
        || components.last().is_some_and(|name| {
            matches!(
                *name,
                "windows" | "system32" | "program files" | "program files (x86)" | "users"
            )
        })
}
