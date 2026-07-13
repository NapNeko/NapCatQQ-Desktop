//! 应用端编排入口（Phase1：OneBot 导出 + 桩 Integration/Runtime）。
//!
//! 真框架对接（NoneBot2 / AstrBot）后补 manifest + 写盘 Integration；
//! 此处禁止把应用实例塞进协议 Bot 列表或 BackendType。

mod export;
mod stub;

pub use export::{OneBotExportError, export_onebot_endpoint};
pub use stub::{StubAppIntegration, StubAppRuntime};
