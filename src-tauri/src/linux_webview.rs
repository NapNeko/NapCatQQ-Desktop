//! WebKitGTK 在部分 Wayland / 显卡组合下会在焦点、浮层或重绘时停止提交画面。
//! 在 GTK/WebKit 初始化前选择兼容渲染，覆盖主窗口和后续创建的 WebView。
//!
//! 兼容渲染会让 WebKitGTK 退回软件合成，4K、高刷新率或分数缩放下 CPU 开销明显。
//! 其中 `WEBKIT_DISABLE_COMPOSITING_MODE` 在 WebKitGTK 里只做存在性判断，用户
//! 无法用 `=0` 恢复，因此这里提供 [`ACCELERATION_ENV`] 供支持良好的环境让开。

/// 需要注入的 WebKitGTK 兼容渲染变量。
const COMPAT_ENV_VARS: [&str; 2] = [
    "WEBKIT_DISABLE_DMABUF_RENDERER",
    "WEBKIT_DISABLE_COMPOSITING_MODE",
];

/// 显式请求 WebKitGTK 硬件合成；设为 `1` / `true` 等真值时不再注入兼容渲染变量。
///
/// 仅在本机显卡驱动与桌面环境确认正常时使用：兼容渲染是画面卡死、闪烁的兜底。
pub const ACCELERATION_ENV: &str = "NCD_LINUX_WEBVIEW_ACCELERATION";

fn wants_acceleration(value: Option<&str>) -> bool {
    matches!(
        value.map(str::trim),
        Some("1" | "true" | "TRUE" | "yes" | "on")
    )
}

fn should_inject(acceleration: bool, already_set: bool) -> bool {
    !acceleration && !already_set
}

/// # Safety
/// 只能在进程入口、创建任何其他线程及初始化 GTK/WebKit 之前调用。
pub(super) unsafe fn configure_before_threads() {
    let acceleration = wants_acceleration(std::env::var(ACCELERATION_ENV).ok().as_deref());
    for name in COMPAT_ENV_VARS {
        // 保留用户显式指定的渲染设置，也便于在已兼容的环境里恢复硬件合成。
        if !should_inject(acceleration, std::env::var_os(name).is_some()) {
            continue;
        }
        // SAFETY: 调用方保证尚未创建其他线程，也未进入 GTK/WebKit。
        unsafe { std::env::set_var(name, "1") };
    }
}

#[cfg(test)]
mod tests {
    use super::{should_inject, wants_acceleration};

    #[test]
    fn acceleration_env_accepts_true_values() {
        for value in ["1", "true", "TRUE", "yes", "on", " 1 "] {
            assert!(wants_acceleration(Some(value)), "{value}");
        }
    }

    #[test]
    fn acceleration_env_ignores_false_values() {
        for value in [None, Some(""), Some("0"), Some("false"), Some("no")] {
            assert!(!wants_acceleration(value), "{value:?}");
        }
    }

    #[test]
    fn compat_rendering_is_the_default_and_can_be_disabled() {
        assert!(should_inject(false, false));
        assert!(!should_inject(true, false));
        assert!(!should_inject(false, true));
    }
}
