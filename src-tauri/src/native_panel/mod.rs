//! 托盘面板的原生绘制层：弹出窗、主题、文字排版、绘制原语。
//!
//! Windows 使用 Direct2D / DirectWrite，Linux 使用 GTK / Cairo / Pango；
//! 不 import Tauri 类型，挂在主事件循环上跑；
//! 上面接业务的是 [`tray_panel_native`]（主托盘）和 [`chat_tray_panel_native`]（聊天托盘）。

#[cfg(windows)]
pub mod gfx;
#[cfg(target_os = "linux")]
#[path = "gfx_linux.rs"]
pub mod gfx;
pub mod icons;
pub mod parts;
#[cfg(all(test, windows))]
pub mod shots;
#[cfg(windows)]
pub(crate) mod sys;
pub mod theme;
#[cfg(windows)]
pub mod window;
#[cfg(target_os = "linux")]
#[path = "window_linux.rs"]
pub mod window;

pub use theme::{Rgba, Theme};
