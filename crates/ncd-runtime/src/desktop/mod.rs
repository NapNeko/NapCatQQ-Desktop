//! 桌面日志、崩溃包与 App 设置落盘。

pub mod crash_bundle;
pub mod log;
pub mod settings;

pub use crash_bundle::{CrashBundleInput, desktop_output_dir, write_crash_bundle};
pub use settings::{
    backfill_snowluma_package, load_app_settings, read_app_settings_file, replace_app_settings_with,
    update_app_settings,
};
