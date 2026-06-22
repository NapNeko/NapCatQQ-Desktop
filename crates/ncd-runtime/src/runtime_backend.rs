// BotBackend trait 及相关类型已下沉到 ncd-traits，此处 re-export 保持向后兼容。
// 新代码请直接 use ncd_traits::runtime_backend::*。

pub use ncd_domain::StopMode;
pub use ncd_domain::bot_status::{BotStatus, ProcessHandle};
pub use ncd_traits::runtime_backend::{
    BotBackend, BotBackendError, BotRuntimeConfig, BotStartCtx, LogSnapshot, TailOpts,
};
