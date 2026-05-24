//! NapCat backend 子模块汇总。
//!
//! 蓝图 §4.4 规定 NapCat 与 SnowLuma 在 `ncd-core` 内部对称:`napcat/` 子目录
//! 与 `snowluma/` 子目录平级,M1 阶段先在同一 crate 内做物理对称,M6 阶段拆出
//! 独立的 `ncd-backend-napcat` crate。
//!
//! 当前包含:
//! - [`webui_client`]:NapCat WebUI HTTP 客户端 + payload 类型
//! - [`login_poller`]:NapCat 登录状态机轮询器
//! - [`offline_notifier`]:Bot 下线通知接口与默认实现

pub mod login_poller;
pub mod offline_notifier;
pub mod webui_client;
