//! ncd-appframework：应用端框架适配器集合。
//!
//! 每个框架一个子目录（`karin/`、`nonebot2/`），各自提供：
//! - `manifest`：静态清单（UI 直接消费）
//! - `component`：实现 `ncd_component::Component`，安装 / 探测 / 启动命令走 Component × Host × Action
//! - `integration`：实现 `ncd_traits::AppIntegration`（纯计划）+ 写应用端配置（备份 → 写 → 失败还原）
//!
//! 编排（实例表、起停、调 BotManager 热推）在 ncd-runtime；这里不碰进程生命周期。
//! 衡量标准：接第二个框架只加一个子目录 + 注册一行。

pub mod adapter;
pub mod config_doc;
pub mod env_file;
pub mod karin;
pub mod node_tooling;
pub mod nonebot2;
pub mod registry;
pub mod uv_tooling;

pub use adapter::{AppComponentSpec, AppFrameworkAdapter, apply_with_backup, restore_from_backup};
pub use config_doc::{
    AppConfigDocumentRevision, AppConfigWriteResult, AppInstanceConfig, AppInstanceConfigEnvelope,
    DocumentSnapshot, MISSING_REVISION, combined_revision, revision_of,
};
pub use env_file::{EnvEntry, EnvFile, EnvWrite};
pub use karin::config::{KarinEnv, KarinInstanceConfig};
pub use karin::{KARIN_FRAMEWORK_ID, KarinAdapter, KarinComponent, KarinIntegration, karin_manifest};
pub use nonebot2::{
    NONEBOT2_FRAMEWORK_ID, NoneBot2Adapter, NoneBot2Component, NoneBot2Integration,
    nonebot2_manifest,
};
pub use registry::AppFrameworkRegistry;
