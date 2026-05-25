use std::path::{Path, PathBuf};

use tokio::fs;
use tokio::io::AsyncWriteExt;

use crate::error::NetworkError;

pub fn part_path(dest: &Path) -> PathBuf {
    let mut p = dest.as_os_str().to_owned();
    p.push(".part");
    PathBuf::from(p)
}

pub struct PartFile {
    pub path: PathBuf,
    pub existing_bytes: u64,
    file: fs::File,
}

impl PartFile {
    pub async fn open_or_create(dest: &Path) -> Result<Self, NetworkError> {
        let path = part_path(dest);
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).await?;
            }
        }

        let existing_bytes = match fs::metadata(&path).await {
            Ok(m) => m.len(),
            Err(_) => 0,
        };

        let file = if existing_bytes > 0 {
            fs::OpenOptions::new()
                .append(true)
                .open(&path)
                .await?
        } else {
            fs::File::create(&path).await?
        };

        Ok(Self {
            path,
            existing_bytes,
            file,
        })
    }

    pub async fn truncate(&mut self) -> Result<(), NetworkError> {
        self.file.set_len(0).await?;
        self.file.flush().await?;
        self.existing_bytes = 0;
        Ok(())
    }

    pub async fn append(&mut self, data: &[u8]) -> Result<(), NetworkError> {
        self.file.write_all(data).await?;
        self.existing_bytes += data.len() as u64;
        Ok(())
    }

    pub async fn flush(&mut self) -> Result<(), NetworkError> {
        self.file.flush().await?;
        Ok(())
    }

    pub async fn finalize(mut self, dest: &Path) -> Result<(), NetworkError> {
        self.file.flush().await?;
        drop(self.file);
        fs::rename(&self.path, dest).await?;
        Ok(())
    }

    pub async fn discard(self) -> Result<(), NetworkError> {
        drop(self.file);
        let _ = fs::remove_file(&self.path).await;
        Ok(())
    }
}

pub fn range_header_value(offset: u64) -> String {
    format!("bytes={offset}-")
}

pub fn supports_resume(status: reqwest::StatusCode) -> bool {
    status == reqwest::StatusCode::PARTIAL_CONTENT
}

pub fn parse_content_length_from_range(
    response: &reqwest::Response,
    resume_offset: u64,
) -> Option<u64> {
    if let Some(cr) = response.headers().get(reqwest::header::CONTENT_RANGE) {
        if let Ok(s) = cr.to_str() {
            // "bytes 1024-9999/10000"
            if let Some(slash) = s.rfind('/') {
                if let Ok(total) = s[slash + 1..].parse::<u64>() {
                    return Some(total);
                }
            }
        }
    }
    response.content_length().map(|cl| cl + resume_offset)
}
