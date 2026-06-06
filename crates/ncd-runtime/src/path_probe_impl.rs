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

fn current_dir() -> Option<PathBuf> {
    env::current_dir().ok()
}

#[cfg(target_os = "windows")]
fn program_data_dir() -> Option<PathBuf> {
    env::var_os("ProgramData").map(PathBuf::from)
}

#[cfg(not(target_os = "windows"))]
fn program_data_dir() -> Option<PathBuf> {
    None
}

fn probe_candidates(
    cwd: Option<PathBuf>,
    program_data: Option<PathBuf>,
    local_data: Option<PathBuf>,
    config_dir: Option<PathBuf>,
    home_dir: Option<PathBuf>,
) -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Some(pd) = program_data {
        candidates.push(pd.join("NapCatQQ Desktop"));
        candidates.push(pd.join("NapCatQQ-Desktop"));
    }

    if let Some(local) = local_data {
        candidates.push(local.join("NapCatQQ-Desktop"));
        candidates.push(local.join("NapCatQQ Desktop"));
    }
    if let Some(config) = config_dir {
        candidates.push(config.join("NapCatQQ-Desktop"));
        candidates.push(config.join("NapCatQQ Desktop"));
    }
    if let Some(home) = home_dir {
        candidates.push(home.join(".config").join("NapCatQQ-Desktop"));
        candidates.push(home.join(".config").join("NapCatQQ Desktop"));
    }

    if let Some(cwd) = cwd {
        candidates.push(cwd.join("runtime"));
    }

    candidates
}

fn filter_existing_config_roots(candidates: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut found_paths = Vec::new();
    for path in candidates {
        if !path.exists() {
            continue;
        }

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

    found_paths
}
impl PathProbe for LocalPathProbe {
    fn probe(&self) -> Result<Vec<PathBuf>, PathError> {
        Ok(filter_existing_config_roots(probe_candidates(
            current_dir(),
            program_data_dir(),
            dirs::data_local_dir(),
            dirs::config_dir(),
            dirs::home_dir(),
        )))
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

        fs::write(config_dir.join("bot.json"), "{}").unwrap();

        let probe = LocalPathProbe::new();
        assert!(probe.is_allowed(temp.path()));
    }

    #[test]
    fn programdata_sources_are_before_cwd_runtime() {
        let temp = tempdir().unwrap();
        let cwd = temp.path().join("cwd");
        let program_data = temp.path().join("ProgramData");
        let local = temp.path().join("LocalData");

        let candidates = probe_candidates(
            Some(cwd.clone()),
            Some(program_data.clone()),
            Some(local.clone()),
            None,
            None,
        );

        assert_eq!(candidates[0], program_data.join("NapCatQQ Desktop"));
        assert_eq!(candidates[1], program_data.join("NapCatQQ-Desktop"));
        assert_eq!(candidates.last().unwrap(), &cwd.join("runtime"));
    }

    #[test]
    fn filtered_probe_preserves_programdata_before_cwd_source() {
        let temp = tempdir().unwrap();
        let program_data = temp.path().join("ProgramData");
        let legacy = program_data.join("NapCatQQ-Desktop");
        let cwd_runtime = temp.path().join("cwd/runtime");
        fs::create_dir_all(legacy.join("runtime/config")).unwrap();
        fs::create_dir_all(cwd_runtime.join("config")).unwrap();
        fs::write(legacy.join("runtime/config/bot.json"), "{}").unwrap();
        fs::write(cwd_runtime.join("config/bot.json"), "{}").unwrap();

        let found = filter_existing_config_roots(probe_candidates(
            Some(temp.path().join("cwd")),
            Some(program_data),
            None,
            None,
            None,
        ));

        assert_eq!(found[0], legacy.canonicalize().unwrap());
        assert_eq!(found[1], cwd_runtime.canonicalize().unwrap());
    }
}
