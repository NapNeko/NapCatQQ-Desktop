//! NeoBot 适配器：manifest + Component + Integration。
//!
//! 分阶段落地，当前有上游事实（manifest）、安装 / 探测 / 启动（component）、
//! 对接计划（integration）。

pub mod component;
pub mod integration;
pub mod manifest;

pub use component::NeoBotComponent;
pub use integration::{NeoBotIntegration, join_webui_url};
pub use manifest::{
    NEOBOT_DEFAULT_DASHBOARD_PORT, NEOBOT_DEFAULT_ONEBOT_PORT, NEOBOT_FRAMEWORK_ID,
    neobot_manifest,
};
