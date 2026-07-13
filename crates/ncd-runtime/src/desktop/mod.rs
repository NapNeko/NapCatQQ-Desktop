//! 桌面日志与崩溃包。

pub mod crash_bundle;
pub mod log;

pub use crash_bundle::{CrashBundleInput, desktop_output_dir, write_crash_bundle};
