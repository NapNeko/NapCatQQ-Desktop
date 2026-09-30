//! NapCat backend 子模块汇总
//!
//! NapCat 与 SnowLuma 在 ncd-runtime 内部对称:napcat/ 子目录与 snowluma/
//! 子目录平级,在同一 crate 内做物理对称;后续可拆出独立的 ncd-backend-napcat crate
//!
//! 当前包含:
//! - [webui_client]:NapCat WebUI HTTP 客户端 + payload 类型
//! - [login_poller]:NapCat 登录状态机轮询器
//! - [offline_notifier]:Bot 下线通知接口与默认实现
//! - [endpoint_table]:per-Bot WebUI 端点 (port + token) 内存表
//! - [debug_client]:调试台用的 /api/Debug/* 客户端(独立于 NapCatWebUiClient trait)

pub mod debug_client;
pub mod endpoint_table;
pub mod login_poller;
pub mod offline_notifier;
pub mod webui_client;

pub use debug_client::{NapCatDebugClient, NapCatDebugError};
