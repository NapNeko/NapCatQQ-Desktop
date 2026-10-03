//! NeoBot 适配器：manifest + Component + Integration。
//!
//! 分阶段落地，当前只有上游事实与清单（manifest）。

pub mod manifest;

pub use manifest::{
    NEOBOT_DEFAULT_DASHBOARD_PORT, NEOBOT_DEFAULT_ONEBOT_PORT, NEOBOT_FRAMEWORK_ID,
    neobot_manifest,
};
