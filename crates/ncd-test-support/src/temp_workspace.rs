use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

#[derive(Debug)]
pub struct TempWorkspace {
    root: tempfile::TempDir,
}

impl TempWorkspace {
    pub fn new() -> io::Result<Self> {
        Ok(Self {
            root: tempfile::tempdir()?,
        })
    }

    pub fn path(&self) -> &Path {
        self.root.path()
    }

    pub fn join(&self, relative: impl AsRef<Path>) -> PathBuf {
        self.path().join(relative)
    }

    pub fn create_dir_all(&self, relative: impl AsRef<Path>) -> io::Result<PathBuf> {
        let path = self.writable_path(relative.as_ref())?;
        fs::create_dir_all(&path)?;
        Ok(path)
    }

    pub fn write_file(
        &self,
        relative: impl AsRef<Path>,
        contents: impl AsRef<[u8]>,
    ) -> io::Result<PathBuf> {
        let path = self.writable_path(relative.as_ref())?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, contents)?;
        Ok(path)
    }

    fn writable_path(&self, relative: &Path) -> io::Result<PathBuf> {
        if relative.as_os_str().is_empty()
            || relative
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "test workspace writes require a relative path without traversal",
            ));
        }
        Ok(self.join(relative))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_nested_files_but_rejects_paths_outside_the_workspace() {
        let workspace = TempWorkspace::new().unwrap();
        let file = workspace.write_file("config/bot.json", b"{}\n").unwrap();
        assert_eq!(fs::read(file).unwrap(), b"{}\n");

        for path in [Path::new(""), Path::new("../outside"), workspace.path()] {
            assert_eq!(
                workspace.write_file(path, b"bad").unwrap_err().kind(),
                io::ErrorKind::InvalidInput
            );
            assert_eq!(
                workspace.create_dir_all(path).unwrap_err().kind(),
                io::ErrorKind::InvalidInput
            );
        }
    }
}
