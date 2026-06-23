use std::path::{Path, PathBuf};

use tracing::info;

use ncd_domain::errors::MigrationError;
use ncd_domain::migration::MigrationWarning;
use ncd_traits::PathProbe;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacySelection {
    pub root: PathBuf,
    pub app_config: Option<PathBuf>,
    pub bot_config: Option<PathBuf>,
    pub auxiliary_files: Vec<PathBuf>,
    pub warnings: Vec<MigrationWarning>,
}

impl LegacySelection {
    pub fn has_any_config(&self) -> bool {
        self.app_config.is_some() || self.bot_config.is_some()
    }
}

pub struct LegacyDiscovery<'a> {
    probe: &'a dyn PathProbe,
}

impl<'a> LegacyDiscovery<'a> {
    pub fn new(probe: &'a dyn PathProbe) -> Self {
        Self { probe }
    }

    pub fn discover(&self) -> Result<Vec<LegacySelection>, MigrationError> {
        let mut selections = Vec::new();
        for root in self.probe.probe()? {
            if !self.probe.is_allowed(&root) {
                continue;
            }
            let selection = self.scan_root(&root);
            if selection.has_any_config() {
                selections.push(selection);
            }
        }
        selections.sort_by_key(|right| std::cmp::Reverse(selection_score(right)));
        info!(
            target: "ncd_runtime::legacy_discovery",
            candidates = selections.len(),
            "legacy config roots discovered"
        );
        Ok(selections)
    }

    fn scan_root(&self, root: &Path) -> LegacySelection {
        let mut warnings = Vec::new();
        let app_candidates = collect_candidates(root, "config.json");
        let bot_candidates = collect_candidates(root, "bot.json");
        let app_config = select_best_candidate(app_candidates, true, &mut warnings);
        let bot_config = select_best_candidate(bot_candidates, false, &mut warnings);
        let auxiliary_files = collect_auxiliary(root);

        LegacySelection {
            root: root.to_path_buf(),
            app_config,
            bot_config,
            auxiliary_files,
            warnings,
        }
    }
}

fn selection_score(selection: &LegacySelection) -> i32 {
    let mut score = 0;
    if selection.app_config.is_some() {
        score += 10;
    }
    if selection.bot_config.is_some() {
        score += 10;
    }
    score
}

fn collect_candidates(root: &Path, filename: &str) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    let direct = root.join(filename);
    if direct.is_file() {
        candidates.push(direct);
    }
    let config = root.join("config").join(filename);
    if config.is_file() {
        candidates.push(config);
    }
    let runtime_config = root.join("runtime").join("config").join(filename);
    if runtime_config.is_file() {
        candidates.push(runtime_config);
    }
    candidates.sort();
    candidates.dedup();
    candidates
}

fn select_best_candidate(
    candidates: Vec<PathBuf>,
    app: bool,
    warnings: &mut Vec<MigrationWarning>,
) -> Option<PathBuf> {
    if candidates.is_empty() {
        return None;
    }
    if candidates.len() > 1 {
        warnings.push(MigrationWarning::new(
            if app {
                "multiple_app_config_candidates"
            } else {
                "multiple_bot_config_candidates"
            },
            "检测到多个配置候选，已按目录布局选择最可信的一项",
        ));
    }

    candidates
        .into_iter()
        .max_by_key(|path| candidate_score(path))
}

fn candidate_score(path: &Path) -> i32 {
    let mut score = 0;
    let parts: Vec<String> = path
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_lowercase())
        .collect();
    if path
        .parent()
        .and_then(Path::file_name)
        .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case("config"))
    {
        score += 4;
    }
    if parts.iter().any(|part| part == "runtime") {
        score += 3;
    }
    if parts.iter().any(|part| part == "versions") {
        score -= 10;
    }
    score
}

fn collect_auxiliary(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for dir in [
        root.to_path_buf(),
        root.join("config"),
        root.join("runtime").join("config"),
    ] {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if (name.starts_with("onebot11_") || name.starts_with("napcat_"))
                && name.ends_with(".json")
            {
                files.push(path);
            }
        }
    }
    files.sort();
    files.dedup();
    files
}

#[cfg(test)]
mod tests {
    use super::*;

    struct StaticProbe {
        roots: Vec<PathBuf>,
    }

    impl PathProbe for StaticProbe {
        fn probe(&self) -> Result<Vec<PathBuf>, ncd_domain::errors::PathError> {
            Ok(self.roots.clone())
        }

        fn is_allowed(&self, _: &Path) -> bool {
            true
        }
    }

    #[test]
    fn prefers_runtime_config_layout() {
        let temp = ncd_test_support::TempWorkspace::new().unwrap();
        std::fs::create_dir_all(temp.path().join("runtime/config")).unwrap();
        std::fs::write(temp.path().join("config.json"), "{}").unwrap();
        std::fs::write(temp.path().join("runtime/config/config.json"), "{}").unwrap();

        let probe = StaticProbe {
            roots: vec![temp.path().to_path_buf()],
        };
        let selections = LegacyDiscovery::new(&probe).discover().unwrap();

        assert_eq!(
            selections[0].app_config.as_ref().unwrap(),
            &temp.path().join("runtime/config/config.json")
        );
    }
}
