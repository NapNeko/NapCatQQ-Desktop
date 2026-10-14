//! 清理时不跟随链接。Windows 固定祖先目录，并通过独占文件句柄删除，
//! 避免检查与删除之间目录被替换、或删除正在读写的文件。
use std::fs::{self, Metadata};
use std::io;
use std::path::Path;

/// Windows 的目录读取句柄参与共享检查；不能用 access_mode(0) 元数据句柄
/// 代替禁止 DELETE 的租约。句柄从根到叶保留，创建/删除期间不能换祖先路径。
#[cfg(any(windows, test))]
pub(super) struct PlainDirectories {
    #[cfg(windows)]
    _handles: Vec<std::fs::File>,
}

#[cfg(any(windows, test))]
impl PlainDirectories {
    #[cfg(windows)]
    pub(super) fn open(path: &Path) -> io::Result<Self> {
        Self::prepare(path, false)
    }

    pub(super) fn create(path: &Path) -> io::Result<Self> {
        Self::prepare(path, true)
    }

    #[cfg(windows)]
    pub(super) fn validate_child(&self, file: &std::fs::File) -> io::Result<()> {
        validate_child_handle(
            self._handles
                .last()
                .ok_or_else(|| io::Error::other("缓存路径无目录句柄"))?,
            file,
        )
    }

    fn prepare(path: &Path, create: bool) -> io::Result<Self> {
        if !path.is_absolute() {
            return Err(io::Error::other("缓存路径必须是绝对路径"));
        }
        #[cfg(windows)]
        let mut handles = Vec::new();
        for ancestor in path.ancestors().collect::<Vec<_>>().into_iter().rev() {
            if create {
                match fs::symlink_metadata(ancestor) {
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {
                        match fs::create_dir(ancestor) {
                            Ok(()) => {}
                            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                            Err(error) => return Err(error),
                        }
                    }
                    Err(error) => return Err(error),
                    Ok(_) => {}
                }
            }
            #[cfg(windows)]
            let (handle, metadata) = {
                use std::os::windows::fs::OpenOptionsExt;
                use windows_sys::Win32::Storage::FileSystem::{
                    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_LIST_DIRECTORY,
                    FILE_READ_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE,
                };
                let handle = fs::OpenOptions::new()
                    .access_mode(FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES)
                    .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
                    .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
                    .open(ancestor)?;
                let metadata = handle.metadata()?;
                (handle, metadata)
            };
            #[cfg(not(windows))]
            let metadata = fs::symlink_metadata(ancestor)?;
            if redirected(&metadata) || !metadata.is_dir() {
                return Err(io::Error::other("缓存目录包含链接、重解析点或非目录条目"));
            }
            #[cfg(windows)]
            {
                if let Some(parent) = handles.last() {
                    validate_child_handle(parent, &handle)?;
                }
                handles.push(handle);
            }
        }
        Ok(Self {
            #[cfg(windows)]
            _handles: handles,
        })
    }
}

#[cfg(windows)]
fn validate_child_handle(parent: &std::fs::File, child: &std::fs::File) -> io::Result<()> {
    // 禁止 DELETE 能阻止重命名，但属性访问不受共享模式限制。祖先被原地
    // 改成重解析点时，路径打开可能落在外部目录；必须核对句柄解析后的真实路径。
    if final_path(child)?.parent() != Some(final_path(parent)?.as_path()) {
        return Err(io::Error::other("缓存路径已重定向，已保留文件"));
    }
    Ok(())
}

#[cfg(windows)]
fn final_path(file: &std::fs::File) -> io::Result<std::path::PathBuf> {
    use std::os::windows::{ffi::OsStringExt, io::AsRawHandle};
    use windows_sys::Win32::Storage::FileSystem::GetFinalPathNameByHandleW;
    let mut buffer = vec![0u16; 512];
    loop {
        // FILE_NAME_NORMALIZED | VOLUME_NAME_DOS（均为 0），解析中间目录的链接。
        // SAFETY: file 和可写 UTF-16 缓冲区在调用期间存活，长度是缓冲区元素数。
        #[allow(unsafe_code)]
        let length = unsafe {
            GetFinalPathNameByHandleW(
                file.as_raw_handle(),
                buffer.as_mut_ptr(),
                buffer.len() as u32,
                0,
            )
        };
        if length == 0 {
            return Err(io::Error::last_os_error());
        }
        if (length as usize) < buffer.len() {
            return Ok(std::ffi::OsString::from_wide(&buffer[..length as usize]).into());
        }
        buffer.resize(length as usize + 1, 0);
    }
}

pub(super) fn redirected(metadata: &Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0 // FILE_ATTRIBUTE_REPARSE_POINT
    }
    #[cfg(not(windows))]
    {
        false
    }
}

pub(super) fn plain_ancestors(path: &Path) -> io::Result<()> {
    if !path.is_absolute() {
        return Err(io::Error::other("缓存路径必须是绝对路径"));
    }
    for ancestor in path.ancestors() {
        let metadata = fs::symlink_metadata(ancestor)?;
        if redirected(&metadata) || !metadata.is_dir() {
            return Err(io::Error::other("缓存目录包含链接或非目录条目"));
        }
    }
    Ok(())
}

#[cfg(not(windows))]
pub(super) fn remove_plain_file(
    path: &Path,
    bytes: u64,
    modified: Option<std::time::SystemTime>,
) -> io::Result<u64> {
    plain_ancestors(
        path.parent()
            .ok_or_else(|| io::Error::other("缓存路径无父目录"))?,
    )?;
    let metadata = fs::symlink_metadata(path)?;
    if redirected(&metadata) || !metadata.is_file() {
        return Err(io::Error::other("缓存文件已变成链接或非普通文件"));
    }
    unchanged(&metadata, bytes, modified)?;
    fs::remove_file(path)?;
    Ok(metadata.len())
}

#[cfg(windows)]
pub(super) fn remove_plain_file(
    path: &Path,
    bytes: u64,
    modified: Option<std::time::SystemTime>,
) -> io::Result<u64> {
    use std::fs::OpenOptions;
    use std::os::windows::{fs::OpenOptionsExt, io::AsRawHandle};
    use windows_sys::Win32::Storage::FileSystem::{
        DELETE, FILE_DISPOSITION_INFO, FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES,
        FileDispositionInfo, SetFileInformationByHandle,
    };
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("缓存路径无父目录"))?;
    let directories = PlainDirectories::open(parent)?;
    // DELETE | FILE_READ_ATTRIBUTES；拒绝共享，已有读写/删除句柄会导致打开失败。
    let file = OpenOptions::new()
        .access_mode(DELETE | FILE_READ_ATTRIBUTES)
        .share_mode(0)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    let metadata = file.metadata()?;
    if redirected(&metadata) || !metadata.is_file() {
        return Err(io::Error::other("缓存文件已变成链接或非普通文件"));
    }
    directories.validate_child(&file)?;
    let disposition = FILE_DISPOSITION_INFO { DeleteFile: true };
    unchanged(&metadata, bytes, modified)?;
    // https://learn.microsoft.com/windows/win32/api/fileapi/nf-fileapi-setfileinformationbyhandle
    // SAFETY: file 持有有效句柄，结构与 FileDispositionInfo 对应，调用期间均存活。
    #[allow(unsafe_code)]
    let ok = unsafe {
        SetFileInformationByHandle(
            file.as_raw_handle(),
            FileDispositionInfo,
            (&disposition as *const FILE_DISPOSITION_INFO).cast(),
            std::mem::size_of::<FILE_DISPOSITION_INFO>() as u32,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    drop(file);
    drop(directories);
    Ok(metadata.len())
}

fn unchanged(
    metadata: &Metadata,
    bytes: u64,
    modified: Option<std::time::SystemTime>,
) -> io::Result<()> {
    if metadata.len() != bytes || metadata.modified().ok() != modified {
        return Err(io::Error::new(
            io::ErrorKind::WouldBlock,
            "缓存文件在扫描后已变化",
        ));
    }
    Ok(())
}
