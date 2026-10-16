//! 每个实例独立的持久状态；手动停止先落盘，再通知进程所有者。
use ncd_domain::windows_remote::{WindowsWorkloadSpec, WindowsWorkloadStatus};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entry {
    pub spec: WindowsWorkloadSpec,
    pub status: WindowsWorkloadStatus,
    pub desired_running: bool,
    pub owner_sid: String,
    pub runner_pid: Option<u32>,
    pub runner_created: Option<u64>,
    #[serde(default)]
    pub process_created: Option<u64>,
}

pub fn key(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}
pub fn instance_dir(root: &Path, id: &str) -> PathBuf {
    root.join("state").join("workloads").join(key(id))
}
pub fn entry_path(root: &Path, id: &str) -> PathBuf {
    instance_dir(root, id).join("entry.json")
}

pub fn read<T: DeserializeOwned>(path: &Path) -> Result<T, String> {
    let mut bytes = Vec::new();
    File::open(path)
        .map_err(|e| format!("read {}: {e}", path.display()))?
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 1024 * 1024 {
        return Err("state file exceeds limit".into());
    }
    serde_json::from_slice(&bytes).map_err(|e| format!("invalid state {}: {e}", path.display()))
}

pub fn write(path: &Path, value: &impl Serialize) -> Result<(), String> {
    let directory = path.parent().ok_or("state path has no parent")?;
    fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    let staging = directory.join(format!(".{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&staging)
            .map_err(|e| e.to_string())?;
        serde_json::to_writer(&mut file, value).map_err(|e| e.to_string())?;
        file.flush()
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())?;
        drop(file);
        #[cfg(windows)]
        crate::native::replace_file(&staging, path)?;
        #[cfg(not(windows))]
        fs::rename(&staging, path).map_err(|e| e.to_string())?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(staging);
    }
    result
}

pub struct FileLock {
    _file: File,
}
impl FileLock {
    pub fn acquire(path: &Path) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(0);
        }
        for attempt in 0..10 {
            match options.open(path) {
                Ok(file) => return Ok(Self { _file: file }),
                Err(error) if error.raw_os_error() == Some(32) && attempt < 9 => {
                    std::thread::sleep(std::time::Duration::from_millis(20))
                }
                Err(error) => return Err(format!("state is busy: {error}")),
            }
        }
        Err("state lock retry limit reached".into())
    }
}

pub fn entries(root: &Path) -> Result<Vec<Entry>, String> {
    let directory = root.join("state/workloads");
    if !directory.exists() {
        return Ok(Vec::new());
    }
    let mut output = Vec::new();
    for item in fs::read_dir(directory).map_err(|e| e.to_string())? {
        let path = item.map_err(|e| e.to_string())?.path().join("entry.json");
        if path.is_file() {
            output.push(read(&path)?);
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn instance_names_cannot_escape_managed_directory() {
        let path = instance_dir(Path::new("root"), "../../Windows/System32");
        assert_eq!(path.components().count(), 4);
        assert!(!path.to_string_lossy().contains("System32"));
    }
    #[test]
    fn atomic_updates_replace_existing_state() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("state.json");
        write(&path, &1u32).unwrap();
        write(&path, &2u32).unwrap();
        assert_eq!(read::<u32>(&path).unwrap(), 2);
    }
}
