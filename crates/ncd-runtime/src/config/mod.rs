// 配置横切：store / drift / renderer / migration 同目录收束。
// 对外仍可通过 crate 根旧路径（config_drift 等）访问，见 lib.rs re-export。

pub mod app_migration;
pub mod bot_migration;
pub mod bot_repo;
pub mod drift;
pub mod migration;
pub mod renderer;
pub mod secret_store;
pub mod store;

pub use app_migration::{
    APP_SETTINGS_FILE, AppConfigMigrationResult, AppSettingsSeedResult, LEGACY_APP_COMPAT_VERSION,
    app_settings_from_legacy_config, ensure_object_payload, looks_like_app_config,
    migrate_app_config,
};
pub use bot_migration::{BOT_CONFIG_COMPAT_VERSION, BotConfigMigrationResult, migrate_bot_config};
pub use bot_repo::LocalBotConfigRepo;
pub use drift::{ConfigDrift, DriftDecision, DriftEntry, DriftError, detect_drift};
pub use migration::{MigrationOrchestrator, migrate_payload_for_tests};
pub use renderer::{
    DispatchRenderer, NapCatConfigRenderer, SnowLumaConfigRenderer, create_renderer,
    output_paths_for_backend,
};
pub use secret_store::SecretStoreImpl;
pub use store::{LocalConfigStore, prune_json_bak_files, prune_migration_backups};
