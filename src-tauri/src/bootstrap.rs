use std::path::{Path, PathBuf};

use ncd_core::{
    BootstrapSnapshot, LocalConfigStore, LocalPathProbe, MigrationOrchestrator, SecretStoreImpl,
};

const APP_DATA_DIR_NAME: &str = "NapCatQQ Desktop";
const LEGACY_APP_DATA_DIR_NAME: &str = "NapCatQQ-Desktop";

pub(crate) fn resolve_data_root() -> PathBuf {
    #[cfg(windows)]
    let program_data = std::env::var_os("ProgramData").map(PathBuf::from);
    #[cfg(not(windows))]
    let program_data = None;
    let local_data = dirs::data_local_dir();
    resolve_data_root_from_candidates(program_data, local_data)
}

pub(crate) fn resolve_data_root_from_candidates(
    program_data: Option<PathBuf>,
    local_data: Option<PathBuf>,
) -> PathBuf {
    if let Some(program_data) = program_data {
        return choose_programdata_root(program_data);
    }

    local_data
        .unwrap_or_else(default_base_without_system_dirs)
        .join(APP_DATA_DIR_NAME)
}

fn choose_programdata_root(program_data: PathBuf) -> PathBuf {
    let primary = program_data.join(APP_DATA_DIR_NAME);
    let legacy = program_data.join(LEGACY_APP_DATA_DIR_NAME);
    let candidates = dedupe_paths(vec![primary.clone(), legacy.clone()]);

    if let Some(path) = candidates.iter().find(|path| has_napcat_runtime(path)) {
        return path.clone();
    }

    if primary.exists() {
        return primary;
    }

    if legacy.exists() {
        return legacy;
    }

    primary
}

fn dedupe_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut deduped = Vec::new();
    for path in paths {
        if !deduped.iter().any(|existing| existing == &path) {
            deduped.push(path);
        }
    }
    deduped
}

fn has_napcat_runtime(data_root: &Path) -> bool {
    data_root
        .join("runtime")
        .join("NapCatQQ")
        .join("NapCatWinBootMain.exe")
        .exists()
}

fn default_base_without_system_dirs() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| std::env::temp_dir())
}

pub fn build_snapshot() -> BootstrapSnapshot {
    let data_root = resolve_data_root();
    build_snapshot_for_data_root(&data_root)
}

pub fn build_snapshot_for_data_root(data_root: &Path) -> BootstrapSnapshot {
    let store = LocalConfigStore::new(data_root);
    let probe = LocalPathProbe::new();
    let secrets = SecretStoreImpl::new(data_root.join("secrets"));

    MigrationOrchestrator::new(&store, &probe, &secrets).bootstrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"").unwrap();
    }

    #[test]
    fn data_root_prefers_programdata_primary_name() {
        let temp = tempfile::tempdir().unwrap();
        let program_data = temp.path().join("ProgramData");
        let local_data = temp.path().join("LocalData");
        touch(
            &local_data
                .join("NapCatQQ-Desktop")
                .join("runtime")
                .join("config")
                .join("bot.json"),
        );

        let resolved =
            resolve_data_root_from_candidates(Some(program_data.clone()), Some(local_data));

        assert_eq!(resolved, program_data.join(APP_DATA_DIR_NAME));
    }

    #[test]
    fn data_root_falls_back_to_legacy_programdata_name_when_primary_missing() {
        let temp = tempfile::tempdir().unwrap();
        let program_data = temp.path().join("ProgramData");
        let legacy = program_data.join(LEGACY_APP_DATA_DIR_NAME);
        std::fs::create_dir_all(&legacy).unwrap();

        let resolved = resolve_data_root_from_candidates(
            Some(program_data.clone()),
            Some(temp.path().join("LocalData")),
        );

        assert_eq!(resolved, legacy);
    }

    #[test]
    fn data_root_prefers_programdata_primary_with_napcat_runtime() {
        let temp = tempfile::tempdir().unwrap();
        let program_data = temp.path().join("ProgramData");
        let primary = program_data.join(APP_DATA_DIR_NAME);
        let legacy = program_data.join(LEGACY_APP_DATA_DIR_NAME);
        touch(
            &primary
                .join("runtime")
                .join("NapCatQQ")
                .join("NapCatWinBootMain.exe"),
        );
        std::fs::create_dir_all(&legacy).unwrap();

        let resolved = resolve_data_root_from_candidates(
            Some(program_data.clone()),
            Some(temp.path().join("LocalData")),
        );

        assert_eq!(resolved, primary);
    }

    #[test]
    fn data_root_prefers_programdata_runtime_candidate_even_when_legacy() {
        let temp = tempfile::tempdir().unwrap();
        let program_data = temp.path().join("ProgramData");
        let primary = program_data.join(APP_DATA_DIR_NAME);
        let legacy = program_data.join(LEGACY_APP_DATA_DIR_NAME);
        std::fs::create_dir_all(&primary).unwrap();
        touch(
            &legacy
                .join("runtime")
                .join("NapCatQQ")
                .join("NapCatWinBootMain.exe"),
        );

        let resolved = resolve_data_root_from_candidates(
            Some(program_data.clone()),
            Some(temp.path().join("LocalData")),
        );

        assert_eq!(resolved, legacy);
    }

    #[test]
    fn data_root_prefers_existing_programdata_primary_over_legacy_config() {
        let temp = tempfile::tempdir().unwrap();
        let program_data = temp.path().join("ProgramData");
        let primary = program_data.join(APP_DATA_DIR_NAME);
        let legacy = program_data.join(LEGACY_APP_DATA_DIR_NAME);
        std::fs::create_dir_all(&primary).unwrap();
        touch(&legacy.join("runtime").join("config").join("bot.json"));

        let resolved = resolve_data_root_from_candidates(
            Some(program_data.clone()),
            Some(temp.path().join("LocalData")),
        );

        assert_eq!(resolved, primary);
    }

    #[test]
    fn data_root_ignores_local_data_when_programdata_exists() {
        let temp = tempfile::tempdir().unwrap();
        let program_data = temp.path().join("ProgramData");
        let local_data = temp.path().join("LocalData");
        touch(
            &local_data
                .join(APP_DATA_DIR_NAME)
                .join("runtime")
                .join("NapCatQQ")
                .join("NapCatWinBootMain.exe"),
        );

        let resolved =
            resolve_data_root_from_candidates(Some(program_data.clone()), Some(local_data));

        assert_eq!(resolved, program_data.join(APP_DATA_DIR_NAME));
    }

    #[test]
    fn data_root_defaults_to_programdata_primary_when_candidates_are_missing() {
        let temp = tempfile::tempdir().unwrap();
        let program_data = temp.path().join("ProgramData");

        let resolved = resolve_data_root_from_candidates(
            Some(program_data.clone()),
            Some(temp.path().join("LocalData")),
        );

        assert_eq!(resolved, program_data.join(APP_DATA_DIR_NAME));
    }

    #[test]
    fn data_root_falls_back_without_programdata() {
        let temp = tempfile::tempdir().unwrap();
        let local_data = temp.path().join("LocalData");
        let legacy = local_data.join(LEGACY_APP_DATA_DIR_NAME);
        std::fs::create_dir_all(&legacy).unwrap();

        let resolved = resolve_data_root_from_candidates(None, Some(local_data.clone()));

        assert_eq!(resolved, local_data.join(APP_DATA_DIR_NAME));
    }
}
