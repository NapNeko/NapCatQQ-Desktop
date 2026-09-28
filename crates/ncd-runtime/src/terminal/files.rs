//! 文件栏：列目录、上传下载、新建改名删除、读写小文本
//!
//! 路径全按会话所在主机的本地风格进出（本机 `C:\…`，远端 `/…`），前端直接拿来显示、拼接，
//! 也能和 shell 上报的当前目录对上。

use std::path::{Path, PathBuf};

use ncd_domain::{TerminalDirListing, TerminalFileEntry, TerminalHostOs, TerminalTextFile};
use ncd_host::{DirEntry, DriveKind, Host, HostCommand, HostError, HostPath, PathStyle};

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
            if trimmed.is_empty() {
                "/".to_string()
            } else {
                trimmed.to_string()
            }
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
            Some(if dir.is_empty() {
                "/".to_string()
            } else {
                dir.to_string()
            })
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

/// `C:\` 这种盘根
pub(crate) fn is_drive_root(os: TerminalHostOs, path: &str) -> bool {
    let b = path.as_bytes();
    os == TerminalHostOs::Windows
        && b.len() == 3
        && b[0].is_ascii_alphabetic()
        && b[1] == b':'
        && b[2] == b'\\'
}

fn drive_label(kind: DriveKind) -> &'static str {
    match kind {
        DriveKind::Fixed => "本地磁盘",
        DriveKind::Removable => "可移动磁盘",
        DriveKind::Network => "网络驱动器",
        DriveKind::Optical => "光驱",
        DriveKind::Other => "磁盘",
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
        HostError::Unsupported { .. } => {
            TerminalError::Unsupported("这台主机不支持这个操作".into())
        }
        other => TerminalError::Host(other.to_string()),
    }
}

impl TerminalManager {
    fn files_host(
        &self,
        id: &str,
    ) -> Result<(std::sync::Arc<dyn Host>, TerminalHostOs), TerminalError> {
        let (host, os, _) = self.session_host(id)?;
        Ok((host, os))
    }

    /// Windows 上空路径表示「此电脑」：列出各个盘。盘根的上一级就是它，文件栏靠这个换盘
    async fn list_drives(&self, host: &dyn Host) -> Result<TerminalDirListing, TerminalError> {
        let entries = host
            .list_drives()
            .await
            .map_err(file_error)?
            .into_iter()
            .map(|d| {
                let letter = d.root.trim_end_matches('\\').to_string();
                TerminalFileEntry {
                    name: format!("{} ({letter})", drive_label(d.kind)),
                    path: d.root,
                    is_dir: true,
                    is_symlink: false,
                    size: 0,
                    modified: None,
                    mode: None,
                }
            })
            .collect();
        Ok(TerminalDirListing {
            path: String::new(),
            parent: None,
            entries,
        })
    }

    pub async fn list_dir(
        &self,
        id: &str,
        path: &str,
    ) -> Result<TerminalDirListing, TerminalError> {
        let (host, os) = self.files_host(id)?;
        if os == TerminalHostOs::Windows && path.trim().is_empty() {
            return self.list_drives(host.as_ref()).await;
        }
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
        let parent = parent_of(os, &shown).or_else(|| is_drive_root(os, &shown).then(String::new));
        Ok(TerminalDirListing {
            parent,
            path: shown,
            entries,
        })
    }

    pub async fn read_text(&self, id: &str, path: &str) -> Result<TerminalTextFile, TerminalError> {
        let (host, os) = self.files_host(id)?;
        let file = to_host_path(os, path)?;
        if size_before_read(host.as_ref(), os, &file)
            .await
            .is_some_and(|size| size > TEXT_EDIT_LIMIT as u64)
        {
            return Err(too_big_to_edit());
        }
        let bytes = host.read_file(&file).await.map_err(file_error)?;
        // 事先没拿到大小，或者看完大小到读完之间文件又长了
        if bytes.len() > TEXT_EDIT_LIMIT {
            return Err(too_big_to_edit());
        }
        let text = String::from_utf8(bytes.to_vec())
            .map_err(|_| TerminalError::Invalid("不是 UTF-8 文本，不能在这里改".into()))?;
        let crlf = text.contains("\r\n");
        Ok(TerminalTextFile {
            path: normalize(os, &file),
            content: if crlf {
                text.replace("\r\n", "\n")
            } else {
                text
            },
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
        host.write_file(&file, data.as_bytes())
            .await
            .map_err(file_error)
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

    pub async fn remove_path(
        &self,
        id: &str,
        path: &str,
        is_dir: bool,
    ) -> Result<(), TerminalError> {
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
                let mut children = tokio::fs::read_dir(&local).await.map_err(|e| {
                    TerminalError::Invalid(format!("读不了 {}：{e}", local.display()))
                })?;
                while let Some(child) = children.next_entry().await.map_err(|e| {
                    TerminalError::Invalid(format!("读不了 {}：{e}", local.display()))
                })? {
                    let child_path = child.path();
                    let file_type = child.file_type().await.map_err(|e| {
                        TerminalError::Invalid(format!("读不了 {}：{e}", child_path.display()))
                    })?;
                    // 文件夹里指向目录的链接（含 Windows 的 junction）不跟进去：指回上层就转不出来，
                    // pnpm 那种 junction 跟进去还会把别处整棵树再传一遍。指向文件的照常传内容
                    if file_type.is_symlink()
                        && tokio::fs::metadata(&child_path)
                            .await
                            .is_ok_and(|m| m.is_dir())
                    {
                        continue;
                    }
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

    pub async fn download(
        &self,
        id: &str,
        path: &str,
        local_dest: &str,
    ) -> Result<(), TerminalError> {
        let (host, os) = self.files_host(id)?;
        host.download(&to_host_path(os, path)?, Path::new(local_dest))
            .await
            .map_err(file_error)
    }

    /// 把终端输出存到本机。路径是用户在另存为对话框里选的，只收 .txt / .log，
    /// 免得这个口子被拿去往任意位置写任意文件
    pub async fn export_text(&self, path: &str, content: &str) -> Result<(), TerminalError> {
        let target = Path::new(path);
        if !is_export_target(target) {
            return Err(TerminalError::Invalid("只能导出成 .txt 或 .log".into()));
        }
        tokio::fs::write(target, content.as_bytes())
            .await
            .map_err(|e| TerminalError::Host(format!("写不了 {path}：{e}")))
    }
}

fn is_export_target(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("txt") || e.eq_ignore_ascii_case("log"))
}

fn too_big_to_edit() -> TerminalError {
    TerminalError::Invalid("文件超过 2 MB，下载下来用编辑器打开".into())
}

/// 读之前先看大小，免得几个 G 的日志整个拉进内存才发现打不开。远端 stat -L 一次拿到，
/// 符号链接看的是指向的文件；本机从父目录列表里找，列表给的是链接本身的大小，碰到链接
/// 就只能读完再判。拿不到给 None，是不是真打不开交给读那一步报
async fn size_before_read(host: &dyn Host, os: TerminalHostOs, file: &HostPath) -> Option<u64> {
    match os {
        TerminalHostOs::Linux => {
            let cmd = HostCommand::new("stat")
                .arg("-L")
                .arg("-c")
                .arg("%s")
                .arg("--")
                .arg(file.as_posix());
            let out = host.run_to_string(cmd).await.ok()?;
            if !out.success() {
                return None;
            }
            out.stdout.trim().parse().ok()
        }
        TerminalHostOs::Windows => {
            let shown = normalize(os, file);
            let parent = parent_of(os, &shown)?;
            let (_, name) = shown.rsplit_once('\\')?;
            host.list_dir(&HostPath::from_windows(&parent))
                .await
                .ok()?
                .into_iter()
                .find(|e| e.name == name && !e.is_symlink)
                .map(|e| e.size)
        }
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
        assert_eq!(
            parent_of(TerminalHostOs::Linux, "/home/u/app"),
            Some("/home/u".into())
        );
        assert_eq!(parent_of(TerminalHostOs::Linux, "/home"), Some("/".into()));
        assert_eq!(parent_of(TerminalHostOs::Linux, "/"), None);
        assert_eq!(
            parent_of(TerminalHostOs::Windows, r"C:\Users\x"),
            Some(r"C:\Users".into())
        );
        assert_eq!(
            parent_of(TerminalHostOs::Windows, r"C:\Users"),
            Some(r"C:\".into())
        );
        assert_eq!(parent_of(TerminalHostOs::Windows, r"C:\"), None);
    }

    #[test]
    fn drive_roots_are_recognised() {
        assert!(is_drive_root(TerminalHostOs::Windows, r"D:\"));
        assert!(!is_drive_root(TerminalHostOs::Windows, r"D:\x"));
        assert!(!is_drive_root(TerminalHostOs::Windows, r"\\"));
        assert!(!is_drive_root(TerminalHostOs::Linux, r"C:\"));
    }

    #[test]
    fn children_join_with_host_separator() {
        assert_eq!(join_child(TerminalHostOs::Linux, "/", "etc"), "/etc");
        assert_eq!(
            join_child(TerminalHostOs::Linux, "/home/u", "a b"),
            "/home/u/a b"
        );
        assert_eq!(join_child(TerminalHostOs::Windows, r"C:\", "x"), r"C:\x");
        assert_eq!(
            join_child(TerminalHostOs::Windows, r"D:\data", "y"),
            r"D:\data\y"
        );
    }

    #[test]
    fn normalizes_trailing_separators() {
        let os = TerminalHostOs::Windows;
        assert_eq!(normalize(os, &HostPath::from_windows(r"C:\")), r"C:\");
        assert_eq!(
            normalize(os, &HostPath::from_windows(r"C:\Users\")),
            r"C:\Users"
        );
        let linux = TerminalHostOs::Linux;
        assert_eq!(
            normalize(linux, &HostPath::from_posix("/srv/app/")),
            "/srv/app"
        );
        assert_eq!(normalize(linux, &HostPath::from_posix("/")), "/");
    }

    #[test]
    fn export_only_takes_text_extensions() {
        assert!(is_export_target(Path::new(r"C:\Users\u\terminal.txt")));
        assert!(is_export_target(Path::new(r"C:\Users\u\out.LOG")));
        assert!(!is_export_target(Path::new(r"C:\Users\u\run.bat")));
        assert!(!is_export_target(Path::new(r"C:\Users\u\noext")));
    }

    #[test]
    fn mode_bits_render_like_ls() {
        assert_eq!(mode_string(0o100755), "rwxr-xr-x");
        assert_eq!(mode_string(0o40700), "rwx------");
        assert_eq!(mode_string(0o644), "rw-r--r--");
    }
}
