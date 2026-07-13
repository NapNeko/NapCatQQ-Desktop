//! 配置横切：store / drift / renderer / migration / 路径 / 旧目录发现。
//!
//! 从 ncd-runtime 抽出（波次 E2），避免配置与 bot 编排同 crate 导航。
//! 下游仍可通过 `ncd_runtime::{LocalConfigStore, DataPaths, ...}` 使用（runtime re-export）。

pub mod app_migration;
pub mod bot_migration;
pub mod bot_repo;
pub mod data_paths;
pub mod drift;
pub mod legacy_discovery;
pub mod migration;
pub mod path_probe;
pub mod renderer;
pub mod secret_store;
pub mod store;

pub use app_migration::{
    APP_SETTINGS_FILE, AppConfigMigrationResult, AppSettingsSeedResult, LEGACY_APP_COMPAT_VERSION,
    app_settings_from_legacy_config, ensure_object_payload, looks_like_app_config, migrate_app_config,
};
pub use bot_migration::{BOT_CONFIG_COMPAT_VERSION, BotConfigMigrationResult, migrate_bot_config};
pub use bot_repo::LocalBotConfigRepo;
pub use data_paths::{
    DataPaths, LAYOUT_VERSION, MAX_DESKTOP_LOG_FILES, MAX_JSON_BAK_FILES, MAX_MIGRATION_BACKUPS,
    read_layout_version, write_layout_version,
};
pub use drift::{ConfigDrift, DriftDecision, DriftEntry, DriftError, detect_drift};
pub use legacy_discovery::{LegacyDiscovery, LegacySelection};
pub use migration::{MigrationOrchestrator, migrate_payload_for_tests};
pub use path_probe::LocalPathProbe;
pub use renderer::{
    DispatchRenderer, NapCatConfigRenderer, SnowLumaConfigRenderer, create_renderer,
    output_paths_for_backend,
};
pub use secret_store::SecretStoreImpl;
pub use store::{LocalConfigStore, prune_json_bak_files, prune_migration_backups};
