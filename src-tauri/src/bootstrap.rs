use ncd_core::{BootstrapSnapshot, LocalConfigStore, LocalPathProbe, MigrationOrchestrator, SecretStoreImpl};

pub(crate) fn resolve_data_root() -> std::path::PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| std::env::temp_dir()))
        .join("NapCatQQ-Desktop")
}

pub fn build_snapshot() -> BootstrapSnapshot {
    let data_root = resolve_data_root();
    let store = LocalConfigStore::new(&data_root);
    let probe = LocalPathProbe::new();
    let secrets = SecretStoreImpl::new(data_root.join("secrets"));

    MigrationOrchestrator::new(&store, &probe, &secrets).bootstrap()
}
