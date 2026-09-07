//! 应用端框架（AppFramework）运行时：实例表 / 进程骨架 / 对接编排 / OneBot 导出。
//!
//! 框架差异（Karin / NoneBot2 …）在 `ncd-appframework`；这里只认 `AppFrameworkAdapter`。

mod export;
mod instances;
mod manager;
mod native_runtime;

pub use export::{OneBotExportError, export_onebot_endpoint};
pub use instances::{APP_INSTANCES_FILE, AppInstanceStore};
pub use manager::{AppManager, BotConfigPort, app_link_connections, upsert_ws_client};
pub use native_runtime::{APP_PID_FILE, AppLaunchSpec, NativeAppRuntime};
