use std::fmt;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathSafetyError {
    Empty,
    AbsolutePath,
    PrefixPath,
    ParentComponent,
    CurrentDirComponent,
    RootNotAbsolute,
}

impl fmt::Display for PathSafetyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("path is empty"),
            Self::AbsolutePath => f.write_str("absolute paths are not allowed"),
            Self::PrefixPath => f.write_str("prefixed paths are not allowed"),
            Self::ParentComponent => f.write_str("parent directory traversal is not allowed"),
            Self::CurrentDirComponent => {
                f.write_str("current directory components are not allowed")
            }
            Self::RootNotAbsolute => f.write_str("workspace root must be absolute"),
        }
    }
}

impl std::error::Error for PathSafetyError {}

pub fn assert_safe_relative_path(path: impl AsRef<Path>) -> Result<(), PathSafetyError> {
    let path = path.as_ref();

    if path.as_os_str().is_empty() {
        return Err(PathSafetyError::Empty);
    }

    for component in path.components() {
        match component {
            Component::Normal(_) => {}
            Component::CurDir => return Err(PathSafetyError::CurrentDirComponent),
            Component::ParentDir => return Err(PathSafetyError::ParentComponent),
            Component::RootDir => return Err(PathSafetyError::AbsolutePath),
            Component::Prefix(_) => return Err(PathSafetyError::PrefixPath),
        }
    }

    Ok(())
}

pub fn assert_path_within_root(
    root: impl AsRef<Path>,
    relative: impl AsRef<Path>,
) -> Result<PathBuf, PathSafetyError> {
    let root = root.as_ref();
    if !root.is_absolute() {
        return Err(PathSafetyError::RootNotAbsolute);
    }

    assert_safe_relative_path(relative.as_ref())?;
    Ok(root.join(relative))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_safe_relative_path() {
        assert!(assert_safe_relative_path("runtime/config/bot.json").is_ok());
    }

    #[test]
    fn rejects_parent_traversal() {
        assert!(matches!(
            assert_safe_relative_path("../secret"),
            Err(PathSafetyError::ParentComponent)
        ));
    }

    #[test]
    fn joins_root_safely() {
        let root = Path::new("C:/temp/workspace");
        let joined = assert_path_within_root(root, "runtime/config/config.json").unwrap();
        assert_eq!(
            joined,
            PathBuf::from("C:/temp/workspace/runtime/config/config.json")
        );
    }
}
