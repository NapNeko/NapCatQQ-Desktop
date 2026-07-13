//! 远端主机档案 / 凭据 / SSH 密钥 / HostResolver。
//!
//! 从 ncd-runtime 抽出（波次 E），避免 server 与 bot 编排同 crate 导航。
//! 下游仍可通过 `ncd_runtime::ServerManager` 等路径使用（runtime re-export）。

pub mod credential_sync;
pub mod host_resolver;
pub mod server_manager;
pub mod server_profile_migration;
pub mod ssh_keygen;

pub use credential_sync::{CredentialSyncLayer, PasswordSlot};
pub use host_resolver::{HostResolver, LocalOnlyHostResolver};
pub use server_manager::{
    AuthMethod, ConnectionHealth, HostKeyPrompt, InMemoryCredentialStore, KeyringCredentialStore,
    ProbeReport, ServerCredentialStore, ServerManager, ServerProfile, ServerState,
};
pub use server_profile_migration::{
    SERVER_PROFILE_COMPAT_VERSION, ServerProfileMigrationResult,
    migrate_legacy_single_server_app_config, migrate_server_profiles_payload,
};
pub use ssh_keygen::{GeneratedKeyPair, generate_ed25519};
