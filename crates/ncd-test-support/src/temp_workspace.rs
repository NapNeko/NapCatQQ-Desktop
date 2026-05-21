use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::assertions::assert_safe_relative_path;
use crate::fixtures::fixture_bytes;

static NEXT_WORKSPACE_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
pub struct TempWorkspace {
    root: PathBuf,
}

impl TempWorkspace {
    pub fn new() -> io::Result<Self> {
        let root = unique_workspace_root()?;
        Ok(Self { root })
    }

    pub fn path(&self) -> &Path {
        &self.root
    }

    pub fn join(&self, relative: impl AsRef<Path>) -> PathBuf {
        self.root.join(relative)
    }

    pub fn create_dir_all(&self, relative: impl AsRef<Path>) -> io::Result<PathBuf> {
        let relative = relative.as_ref();
        validate_relative_path(relative)?;
        let path = self.join(relative);
        fs::create_dir_all(&path)?;
        Ok(path)
    }

    pub fn write_file(
        &self,
        relative: impl AsRef<Path>,
        contents: impl AsRef<[u8]>,
    ) -> io::Result<PathBuf> {
        let relative = relative.as_ref();
        validate_relative_path(relative)?;
        let path = self.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, contents)?;
        Ok(path)
    }

    pub fn copy_fixture(
        &self,
        fixture_relative: impl AsRef<Path>,
        destination_relative: impl AsRef<Path>,
    ) -> io::Result<PathBuf> {
        let bytes = fixture_bytes(fixture_relative)?;
        self.write_file(destination_relative, bytes)
    }

    pub fn populate_legacy_samples(&self) -> io::Result<()> {
        self.copy_fixture("legacy/config.json", "runtime/config/config.json")?;
        self.copy_fixture("legacy/bot.json", "runtime/config/bot.json")?;
        self.copy_fixture("legacy/servers.json", "runtime/config/servers.json")?;
        Ok(())
    }
}

impl Drop for TempWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn validate_relative_path(path: &Path) -> io::Result<()> {
    assert_safe_relative_path(path)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))
}

fn unique_workspace_root() -> io::Result<PathBuf> {
    let base = std::env::temp_dir().join("ncd-test-support");
    fs::create_dir_all(&base)?;

    for _ in 0..128 {
        let root = base.join(format!(
            "{}-{}-{}",
            std::process::id(),
            timestamp_millis(),
            NEXT_WORKSPACE_ID.fetch_add(1, Ordering::Relaxed)
        ));

        match fs::create_dir(&root) {
            Ok(()) => return Ok(root),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }

    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "failed to create a unique temp workspace",
    ))
}

fn timestamp_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_workspace_and_writes_files() {
        let workspace = TempWorkspace::new().unwrap();
        let file = workspace
            .write_file("runtime/config/config.json", b"{}\n")
            .unwrap();
        assert!(file.exists());
        assert!(workspace.path().is_absolute());
    }

    #[test]
    fn populates_legacy_samples() {
        let workspace = TempWorkspace::new().unwrap();
        workspace.populate_legacy_samples().unwrap();
        assert!(workspace.join("runtime/config/config.json").exists());
        assert!(workspace.join("runtime/config/bot.json").exists());
        assert!(workspace.join("runtime/config/servers.json").exists());
    }
}
