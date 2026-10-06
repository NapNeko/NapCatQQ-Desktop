//! 应用端框架（AppFramework）运行时：实例表 / 进程骨架 / 对接编排 / OneBot 导出。
//!
//! 框架差异（Karin / NoneBot2 …）在 `ncd-appframework`；这里只认 `AppFrameworkAdapter`。

mod adopt;
mod download;
mod existing_link;
mod export;
mod instances;
mod listen_port;
mod log_tail;
mod manager;
mod native_runtime;
mod plugin_market;
mod plugin_task;
mod resident_link;
mod supervisor;

pub use adopt::AdoptSnapshot;
pub use export::{OneBotExportError, export_onebot_endpoint};
pub use instances::{APP_INSTANCES_FILE, AppInstanceStore};
pub use manager::{
    AppManager, AppTerminalContext, BotConfigPort, app_link_connections, upsert_ws_client,
};
pub use native_runtime::{APP_PID_FILE, AppLaunchSpec, NativeAppRuntime};
pub use ncd_appframework::config_backup::{
    FrameworkConfigBackup, FrameworkConfigFile, FrameworkConfigRecovery,
};
pub use plugin_market::KARIN_PLUGINS_LIST_URL;
