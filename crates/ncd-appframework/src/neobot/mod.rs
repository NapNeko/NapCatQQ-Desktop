//! NeoBot 适配器：manifest + Component + Integration + 类型化配置 + 导入探测。
//!
//! 分阶段落地，当前有上游事实（manifest）、安装 / 探测 / 启动（component）、
//! 对接计划（integration）、两份 TOML 的类型化配置（config）、导入探测（probe）。

pub mod component;
pub mod config;
pub mod integration;
pub mod manifest;
pub mod probe;

pub use component::NeoBotComponent;
pub use config::{
    DOC_ADAPTER, DOC_DASHBOARD, NeoBotAdapterConfig, NeoBotDashboardConfig, NeoBotInstanceConfig,
    neobot_config_documents,
};
pub use integration::{NeoBotIntegration, join_webui_url};
pub use manifest::{
    NEOBOT_DEFAULT_DASHBOARD_PORT, NEOBOT_DEFAULT_ONEBOT_PORT, NEOBOT_FRAMEWORK_ID,
    neobot_manifest,
};
pub use probe::probe_neobot;
