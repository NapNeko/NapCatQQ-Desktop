//! 每次下载独占临时目录，校验成功后才替换目标文件。

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use tokio::fs;

use crate::NetworkError;

static NEXT_DOWNLOAD: AtomicU64 = AtomicU64::new(0);

pub(crate) struct StagedDownload {
    directory: PathBuf,
    pub file: PathBuf,
}

impl StagedDownload {
    pub async fn create(destination: &Path) -> Result<Arc<Self>, NetworkError> {
        let parent = destination
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        fs::create_dir_all(parent).await?;
        loop {
            let id = NEXT_DOWNLOAD.fetch_add(1, Ordering::Relaxed);
            let directory = parent.join(format!(".ncd-download-{}-{id}", std::process::id()));
            // 创建目录和建立清理所有权之间不能让出执行权，否则 abort 可留下无主目录。
            match std::fs::create_dir(&directory) {
                Ok(()) => {
                    return Ok(Arc::new(Self {
                        file: directory.join("payload"),
                        directory,
                    }));
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.into()),
            }
        }
    }

    pub async fn commit(&self, destination: &Path) -> Result<(), NetworkError> {
        fs::rename(&self.file, destination).await?;
        Ok(())
    }
}

impl Drop for StagedDownload {
    fn drop(&mut self) {
        // worker 也持有此所有权，取消父 future 时等最后一个 writer 关闭后再清理。
        let _ = std::fs::remove_file(&self.file);
        let _ = std::fs::remove_file(crate::range::part_path(&self.file));
        let _ = std::fs::remove_dir(&self.directory);
    }
}
