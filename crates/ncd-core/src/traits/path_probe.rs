use std::path::{Path, PathBuf};

use crate::errors::PathError;

pub trait PathProbe: Send + Sync {
    fn probe(&self) -> Result<Vec<PathBuf>, PathError>;
    fn is_allowed(&self, path: &Path) -> bool;
}
