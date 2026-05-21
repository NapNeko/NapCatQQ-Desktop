use std::env;
use std::path::{Path, PathBuf};

use crate::errors::PathError;
use crate::traits::PathProbe;

pub struct LocalPathProbe {
    allowed_roots: Vec<PathBuf>,
}

impl LocalPathProbe {
    pub fn new() -> Self {
        let mut allowed_roots = Vec::new();

        // 1. Current working directory
        if let Ok(cwd) = env::current_dir() {
            if let Ok(canon) = cwd.canonicalize() {
                allowed_roots.push(canon);
            } else {
                allowed_roots.push(cwd);
            }
        }

        // 2. Temp directory
        let temp = env::temp_dir();
        if let Ok(canon) = temp.canonicalize() {
            allowed_roots.push(canon);
        } else {
            allowed_roots.push(temp);
        }

        // 3. User standard data directories
        if let Some(local) = dirs::data_local_dir() {
            allowed_roots.push(local.join("NapCatQQ-Desktop"));
            allowed_roots.push(local.join("NapCatQQ Desktop"));
        }
        if let Some(config) = dirs::config_dir() {
            allowed_roots.push(config.join("NapCatQQ-Desktop"));
            allowed_roots.push(config.join("NapCatQQ Desktop"));
        }
        if let Some(home) = dirs::home_dir() {
            allowed_roots.push(home.join(".config").join("NapCatQQ-Desktop"));
            allowed_roots.push(home.join(".config").join("NapCatQQ Desktop"));
        }

        // 4. Windows ProgramData
        #[cfg(target_os = "windows")]
        {
            if let Ok(pd) = env::var("ProgramData") {
                let pd_path = PathBuf::from(pd);
                allowed_roots.push(pd_path.join("NapCatQQ-Desktop"));
                allowed_roots.push(pd_path.join("NapCatQQ Desktop"));
            } else {
                let pd_path = PathBuf::from("C:\\ProgramData");
                allowed_roots.push(pd_path.join("NapCatQQ-Desktop"));
                allowed_roots.push(pd_path.join("NapCatQQ Desktop"));
            }
        }

        // Canonicalize allowed roots where possible
        let allowed_roots = allowed_roots
            .into_iter()
            .map(|p| p.canonicalize().unwrap_or(p))
            .collect();

        Self { allowed_roots }
    }
}

impl Default for LocalPathProbe {
    fn default() -> Self {
        Self::new()
    }
}

impl PathProbe for LocalPathProbe {
    fn probe(&self) -> Result<Vec<PathBuf>, PathError> {
        let mut candidates = Vec::new();

        // 1. Current workspace configuration path (development fallback)
        if let Ok(cwd) = env::current_dir() {
            candidates.push(cwd.join("runtime"));
        }

        // 2. Windows-specific ProgramData
        #[cfg(target_os = "windows")]
        {
            if let Ok(pd) = env::var("ProgramData") {
                let pd_path = PathBuf::from(pd);
                candidates.push(pd_path.join("NapCatQQ Desktop"));
                candidates.push(pd_path.join("NapCatQQ-Desktop"));
            } else {
                let pd_path = PathBuf::from("C:\\ProgramData");
                candidates.push(pd_path.join("NapCatQQ Desktop"));
                candidates.push(pd_path.join("NapCatQQ-Desktop"));
            }
        }

        // 3. User standard local AppData or config directories
        if let Some(local) = dirs::data_local_dir() {
            candidates.push(local.join("NapCatQQ-Desktop"));
            candidates.push(local.join("NapCatQQ Desktop"));
        }
        if let Some(config) = dirs::config_dir() {
            candidates.push(config.join("NapCatQQ-Desktop"));
            candidates.push(config.join("NapCatQQ Desktop"));
        }
        if let Some(home) = dirs::home_dir() {
            candidates.push(home.join(".config").join("NapCatQQ-Desktop"));
            candidates.push(home.join(".config").join("NapCatQQ Desktop"));
        }

        // Filter directories that actually contain configuration files (bot.json or config.json)
        let mut found_paths = Vec::new();
        for path in candidates {
            if !path.exists() {
                continue;
            }

            // A valid legacy path contains config files directly or inside config/ or runtime/config/
            let paths_to_check = [
                path.join("bot.json"),
                path.join("config.json"),
                path.join("config").join("bot.json"),
                path.join("config").join("config.json"),
                path.join("runtime").join("config").join("bot.json"),
                path.join("runtime").join("config").join("config.json"),
            ];

            if paths_to_check.iter().any(|p| p.exists()) {
                if let Ok(canon) = path.canonicalize() {
                    if !found_paths.contains(&canon) {
                        found_paths.push(canon);
                    }
                } else if !found_paths.contains(&path) {
                    found_paths.push(path);
                }
            }
        }

        Ok(found_paths)
    }

    fn is_allowed(&self, path: &Path) -> bool {
        // Prevent path traversal attacks
        let path_str = path.to_string_lossy();
        if path_str.contains("..") {
            return false;
        }

        let target = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());

        for root in &self.allowed_roots {
            if target.starts_with(root) {
                return true;
            }
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_is_allowed_sandbox() {
        let probe = LocalPathProbe::new();

        // 1. Current working directory is allowed
        let cwd = env::current_dir().unwrap();
        assert!(probe.is_allowed(&cwd));
        assert!(probe.is_allowed(&cwd.join("Cargo.toml")));

        // 2. Traversal is strictly denied
        let traversal = cwd.join("..").join("something");
        assert!(!probe.is_allowed(&traversal));

        // 3. System directories outside allowed roots are denied
        #[cfg(target_os = "windows")]
        {
            let system_root = Path::new("C:\\Windows");
            assert!(!probe.is_allowed(system_root));
        }
        #[cfg(target_os = "linux")]
        {
            let system_root = Path::new("/etc/shadow");
            assert!(!probe.is_allowed(system_root));
        }
    }

    #[test]
    fn test_probe_flow() {
        let temp = tempdir().unwrap();
        let runtime_dir = temp.path().join("runtime");
        let config_dir = runtime_dir.join("config");
        fs::create_dir_all(&config_dir).unwrap();

        // Write config file
        fs::write(config_dir.join("bot.json"), "{}").unwrap();

        // Test probe manually with injected paths
        let probe = LocalPathProbe::new();
        // Probe searches standard dirs, since tempdir is not standard it won't find it directly via standard search.
        // But let's verify that we can allowed-test the tempdir
        assert!(probe.is_allowed(temp.path()));
    }
}
