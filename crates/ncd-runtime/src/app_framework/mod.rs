//! 应用端框架（AppFramework）运行时：实例表 / 进程骨架 / 对接编排 / OneBot 导出。
//!
//! 框架差异（Karin / NoneBot2 …）在 `ncd-appframework`；这里只认 `AppFrameworkAdapter`。

mod download;
mod export;
mod instances;
mod listen_port;
mod manager;
mod native_runtime;
mod plugin_market;
mod plugin_task;
mod resident_link;

pub use export::{OneBotExportError, export_onebot_endpoint};
pub use instances::{APP_INSTANCES_FILE, AppInstanceStore};
pub use manager::{AppManager, BotConfigPort, app_link_connections, upsert_ws_client};
pub use native_runtime::{APP_PID_FILE, AppLaunchSpec, NativeAppRuntime};
pub use plugin_market::{KARIN_PLUGINS_LIST_URL, fetch_karin_plugin_market};
pub use plugin_task::run_app_plugin_task;
