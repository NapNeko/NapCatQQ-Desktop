//! 托盘面板的原生绘制层：弹出窗、主题、文字排版、绘制原语。
//!
//! 这个模块只依赖 windows crate，不 import 任何 Tauri 类型，挂在主事件循环上跑；
//! 上面接业务的是 [`tray_panel_native`]（主托盘）和 [`chat_tray_panel_native`]（聊天托盘）。

pub mod gfx;
pub mod icons;
pub mod theme;
pub mod window;

pub use theme::{Rgba, Theme};
