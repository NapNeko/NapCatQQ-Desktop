//! ncd-traits:NapCatQQ-Desktop 的 Layer 2 接口契约。
//!
//! 本 crate 只定义 trait + 必要的关联类型,不实装。具体实装(LocalConfigStore /
//! SecretStoreImpl 等)在下游 crate 里完成。
//!
//! 跨 crate 数据类型来自 [ncd_domain](::ncd_domain),通过 use ncd_domain::...
//! 引入,避免反向依赖。

pub mod backend_config_renderer;
pub mod bot_config_repo;
pub mod config_store;
pub mod migration_step;
pub mod path_probe;
pub mod secret_store;

pub use backend_config_renderer::{BackendConfigRenderer, RenderError};
pub use bot_config_repo::BotConfigRepo;
pub use config_store::{ConfigStore, JsonTransaction, JsonWrite, TransactionReport};
pub use migration_step::MigrationStep;
pub use path_probe::PathProbe;
pub use secret_store::SecretStore;
