//! 文件栏：列目录、上传下载、新建改名删除、读写小文本
//!
//! 路径全按会话所在主机的本地风格进出（本机 `C:\…`，远端 `/…`），前端直接拿来显示、拼接，
//! 也能和 shell 上报的当前目录对上。

use std::path::{Path, PathBuf};

use ncd_domain::{TerminalDirListing, TerminalFileEntry, TerminalHostOs, TerminalTextFile};
use ncd_host::{DirEntry, Host, HostError, HostPath, PathStyle};

use super::TerminalError;
use super::manager::TerminalManager;

/// 超过这个大小不在文件栏里打开编辑
const TEXT_EDIT_LIMIT: usize = 2 * 1024 * 1024;

fn style(os: TerminalHostOs) -> PathStyle {
    match os {
        TerminalHostOs::Windows => PathStyle::Windows,
        TerminalHostOs::Linux => PathStyle::Posix,
    }
}

fn to_host_path(os: TerminalHostOs, path: &str) -> Result<HostPath, TerminalError> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(TerminalError::Invalid("路径是空的".into()));
    }
    Ok(match os {
        TerminalHostOs::Windows => HostPath::from_windows(trimmed),
        TerminalHostOs::Linux => HostPath::from_posix(trimmed),
    })
}

/// 规整成主机本地写法：Windows 盘根留尾部 `\`，其余去掉尾部分隔符
fn normalize(os: TerminalHostOs, path: &HostPath) -> String {
    let rendered = path.render(style(os));
    match os {
        TerminalHostOs::Linux => {
            let trimmed = rendered.trim_end_matches('/');
            if trimmed.is_empty() { "/".to_string() } else { trimmed.to_string() }
        }
        TerminalHostOs::Windows => {
            let trimmed = rendered.trim_end_matches('\\');
            if trimmed.len() == 2 && trimmed.ends_with(':') {
                format!("{trimmed}\\")
            } else {
                trimmed.to_string()
            }
        }
    }
}

pub(crate) fn parent_of(os: TerminalHostOs, path: &str) -> Option<String> {
    match os {
        TerminalHostOs::Linux => {
            let trimmed = path.trim_end_matches('/');
            if trimmed.is_empty() {
                return None;
            }
            let (dir, _) = trimmed.rsplit_once('/')?;
            Some(if dir.is_empty() { "/".to_string() } else { dir.to_string() })
        }
        TerminalHostOs::Windows => {
            let trimmed = path.trim_end_matches('\\');
            if trimmed.len() <= 2 {
                return None;
            }
            let (dir, _) = trimmed.rsplit_once('\\')?;
            Some(if dir.len() == 2 && dir.ends_with(':') {
                format!("{dir}\\")
            } else {
                dir.to_string()
            })
        }
    }
}

pub(crate) fn join_child(os: TerminalHostOs, dir: &str, name: &str) -> String {
    let sep = match os {
        TerminalHostOs::Windows => '\\',
        TerminalHostOs::Linux => '/',
    };
    if dir.ends_with(sep) {
        format!("{dir}{name}")
    } else {
        format!("{dir}{sep}{name}")
    }
}

/// 权限位写成 `rwxr-xr-x`
pub(crate) fn mode_string(mode: u32) -> String {
    let mut out = String::with_capacity(9);
    for shift in [6u32, 3, 0] {
        let bits = (mode >> shift) & 0o7;
        out.push(if bits & 0o4 != 0 { 'r' } else { '-' });
        out.push(if bits & 0o2 != 0 { 'w' } else { '-' });
        out.push(if bits & 0o1 != 0 { 'x' } else { '-' });
    }
    out
}

fn entry_of(os: TerminalHostOs, dir: &str, entry: DirEntry) -> TerminalFileEntry {
    TerminalFileEntry {
        path: join_child(os, dir, &entry.name),
        name: entry.name,
        is_dir: entry.is_dir,
        is_symlink: entry.is_symlink,
        size: entry.size,
        modified: entry.modified,
        mode: entry.mode.map(mode_string),
    }
}

fn file_error(err: HostError) -> TerminalError {
    match err {
        HostError::PathNotFound { path } => {
            TerminalError::Host(format!("找不到 {}", path.as_posix()))
        }
        HostError::PermissionDenied { path, .. } => {
            TerminalError::Host(format!("没有权限：{}", path.as_posix()))
        }
        HostError::Unsupported { .. } => TerminalError::Unsupported("这台主机不支持这个操作".into()),
        other => TerminalError::Host(other.to_string()),
    }
}

impl TerminalManager {
    fn files_host(&self, id: &str) -> Result<(std::sync::Arc<dyn Host>, TerminalHostOs), TerminalError> {
        let (host, os, _) = self.session_host(id)?;
        Ok((host, os))
    }

    pub async fn list_dir(&self, id: &str, path: &str) -> Result<TerminalDirListing, TerminalError> {
        let (host, os) = self.files_host(id)?;
        let dir = to_host_path(os, path)?;
        let shown = normalize(os, &dir);
        let mut entries: Vec<TerminalFileEntry> = host
            .list_dir(&dir)
            .await
            .map_err(file_error)?
            .into_iter()
            .map(|e| entry_of(os, &shown, e))
            .collect();
        entries.sort_by(|a, b| {
            b.is_dir
                .cmp(&a.is_dir)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        Ok(TerminalDirListing {
            parent: parent_of(os, &shown),
            path: shown,
            entries,
        })
    }

    pub async fn read_text(&self, id: &str, path: &str) -> Result<TerminalTextFile, TerminalError> {
        let (host, os) = self.files_host(id)?;
        let file = to_host_path(os, path)?;
        let bytes = host.read_file(&file).await.map_err(file_error)?;
        if bytes.len() > TEXT_EDIT_LIMIT {
            return Err(TerminalError::Invalid(
                "文件超过 2 MB，下载下来用编辑器打开".into(),
            ));
        }
        let text = String::from_utf8(bytes.to_vec())
            .map_err(|_| TerminalError::Invalid("不是 UTF-8 文本，不能在这里改".into()))?;
        let crlf = text.contains("\r\n");
        Ok(TerminalTextFile {
            path: normalize(os, &file),
            content: if crlf { text.replace("\r\n", "\n") } else { text },
            crlf,
        })
    }

    pub async fn write_text(
        &self,
        id: &str,
        path: &str,
        content: &str,
        crlf: bool,
    ) -> Result<(), TerminalError> {
        let (host, os) = self.files_host(id)?;
        let file = to_host_path(os, path)?;
        let data = if crlf {
            content.replace("\r\n", "\n").replace('\n', "\r\n")
        } else {
            content.to_string()
        };
        host.write_file(&file, data.as_bytes()).await.map_err(file_error)
    }

    pub async fn make_dir(&self, id: &str, path: &str) -> Result<(), TerminalError> {
        let (host, os) = self.files_host(id)?;
        host.create_dir_all(&to_host_path(os, path)?)
            .await
            .map_err(file_error)
    }

    pub async fn rename_path(&self, id: &str, from: &str, to: &str) -> Result<(), TerminalError> {
        let (host, os) = self.files_host(id)?;
        let to = to_host_path(os, to)?;
        if host.exists(&to).await.map_err(file_error)? {
            return Err(TerminalError::Invalid("目标已经存在".into()));
        }
        host.rename(&to_host_path(os, from)?, &to)
            .await
            .map_err(file_error)
    }

    pub async fn remove_path(&self, id: &str, path: &str, is_dir: bool) -> Result<(), TerminalError> {
        let (host, os) = self.files_host(id)?;
        let target = to_host_path(os, path)?;
        let result = if is_dir {
            host.remove_dir_all(&target).await
        } else {
            host.remove_file(&target).await
        };
        result.map_err(file_error)
    }

    /// 把本机文件 / 文件夹传到 `dest_dir` 下；返回传了几个文件
    pub async fn upload(
        &self,
        id: &str,
        local_paths: &[String],
        dest_dir: &str,
    ) -> Result<u32, TerminalError> {
        let (host, os) = self.files_host(id)?;
        let dest = normalize(os, &to_host_path(os, dest_dir)?);
        let mut count = 0u32;
        // 文件夹逐层展开：（本机路径，目标路径）
        let mut stack: Vec<(PathBuf, String)> = Vec::new();
        for local in local_paths {
            let path = PathBuf::from(local);
            let name = file_name(&path)?;
            stack.push((path, join_child(os, &dest, &name)));
        }
        while let Some((local, remote)) = stack.pop() {
            let meta = tokio::fs::metadata(&local)
                .await
                .map_err(|e| TerminalError::Invalid(format!("读不了 {}：{e}", local.display())))?;
            if meta.is_dir() {
                host.create_dir_all(&to_host_path(os, &remote)?)
                    .await
                    .map_err(file_error)?;
                let mut children = tokio::fs::read_dir(&local)
                    .await
                    .map_err(|e| TerminalError::Invalid(format!("读不了 {}：{e}", local.display())))?;
                while let Ok(Some(child)) = children.next_entry().await {
                    let child_path = child.path();
                    let name = file_name(&child_path)?;
                    stack.push((child_path, join_child(os, &remote, &name)));
                }
            } else {
                host.upload(&local, &to_host_path(os, &remote)?)
                    .await
                    .map_err(file_error)?;
                count += 1;
            }
        }
        Ok(count)
    }

    pub async fn download(&self, id: &str, path: &str, local_dest: &str) -> Result<(), TerminalError> {
        let (host, os) = self.files_host(id)?;
        host.download(&to_host_path(os, path)?, Path::new(local_dest))
            .await
            .map_err(file_error)
    }
}

fn file_name(path: &Path) -> Result<String, TerminalError> {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or_else(|| TerminalError::Invalid(format!("认不出文件名：{}", path.display())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parents_stop_at_roots() {
        assert_eq!(parent_of(TerminalHostOs::Linux, "/home/u/app"), Some("/home/u".into()));
        assert_eq!(parent_of(TerminalHostOs::Linux, "/home"), Some("/".into()));
        assert_eq!(parent_of(TerminalHostOs::Linux, "/"), None);
        assert_eq!(parent_of(TerminalHostOs::Windows, r"C:\Users\x"), Some(r"C:\Users".into()));
        assert_eq!(parent_of(TerminalHostOs::Windows, r"C:\Users"), Some(r"C:\".into()));
        assert_eq!(parent_of(TerminalHostOs::Windows, r"C:\"), None);
    }

    #[test]
    fn children_join_with_host_separator() {
        assert_eq!(join_child(TerminalHostOs::Linux, "/", "etc"), "/etc");
        assert_eq!(join_child(TerminalHostOs::Linux, "/home/u", "a b"), "/home/u/a b");
        assert_eq!(join_child(TerminalHostOs::Windows, r"C:\", "x"), r"C:\x");
        assert_eq!(join_child(TerminalHostOs::Windows, r"D:\data", "y"), r"D:\data\y");
    }

    #[test]
    fn normalizes_trailing_separators() {
        let os = TerminalHostOs::Windows;
        assert_eq!(normalize(os, &HostPath::from_windows(r"C:\")), r"C:\");
        assert_eq!(normalize(os, &HostPath::from_windows(r"C:\Users\")), r"C:\Users");
        let linux = TerminalHostOs::Linux;
        assert_eq!(normalize(linux, &HostPath::from_posix("/srv/app/")), "/srv/app");
        assert_eq!(normalize(linux, &HostPath::from_posix("/")), "/");
    }

    #[test]
    fn mode_bits_render_like_ls() {
        assert_eq!(mode_string(0o100755), "rwxr-xr-x");
        assert_eq!(mode_string(0o40700), "rwx------");
        assert_eq!(mode_string(0o644), "rw-r--r--");
    }
}
