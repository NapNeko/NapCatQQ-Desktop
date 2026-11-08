//! Linux WebKitGTK 渲染路径：在 GTK/WebKit 初始化前决定是否注入兼容渲染变量。
//!
//! WebKitGTK 在部分 Wayland / 显卡组合下会在焦点、浮层或重绘时停止提交画面，
//! 所以默认注入 `WEBKIT_DISABLE_*` 退回软件合成。软件合成在 4K、高刷新率或
//! 分数缩放下开销明显，设置页提供开关（`linuxWebviewAcceleration`），环境变量
//! [`ACCELERATION_ENV`] 可覆盖设置，便于排查问题。
//!
//! 注意：`WEBKIT_DISABLE_COMPOSITING_MODE` 在 WebKitGTK 中只做存在性判断，
//! 用户无法靠 `=0` 恢复合成，因此必须由应用侧决定是否注入。

use std::sync::OnceLock;

/// 需要注入的 WebKitGTK 兼容渲染变量。
const COMPAT_ENV_VARS: [&str; 2] = [
    "WEBKIT_DISABLE_DMABUF_RENDERER",
    "WEBKIT_DISABLE_COMPOSITING_MODE",
];

/// 覆盖设置页开关的环境变量：`1` / `true` 强制硬件合成，`0` / `false` 强制兼容渲染。
pub const ACCELERATION_ENV: &str = "NCD_LINUX_WEBVIEW_ACCELERATION";

/// 本次进程最终的渲染决策，`configure_before_threads` 之后可读，供启动日志记录。
static EFFECTIVE_ACCELERATION: OnceLock<bool> = OnceLock::new();

/// 本次进程是否保留硬件合成；尚未初始化时返回 `None`。
pub fn effective_acceleration() -> Option<bool> {
    EFFECTIVE_ACCELERATION.get().copied()
}

/// 解析布尔开关；空值 / 未知值视为未指定，避免误伤默认行为。
fn parse_bool(value: Option<&str>) -> Option<bool> {
    match value.map(str::trim) {
        Some("1" | "true" | "TRUE" | "yes" | "on") => Some(true),
        Some("0" | "false" | "FALSE" | "no" | "off") => Some(false),
        _ => None,
    }
}

/// 环境变量优先于设置页开关；两者都未指定时用设置页值。
fn resolve_acceleration(env: Option<&str>, setting: bool) -> bool {
    parse_bool(env).unwrap_or(setting)
}

/// 已显式要求硬件合成，或用户自己指定了渲染变量时，不再注入兼容渲染。
fn should_inject(acceleration: bool, already_set: bool) -> bool {
    !acceleration && !already_set
}

/// 启动早期读取设置页开关；读不到时按默认的兼容渲染处理。
///
/// 这里直接读 `app-settings.json`，早于 GTK/WebKit 与日志初始化，因此不写日志。
fn setting_acceleration() -> bool {
    match ncd_runtime::desktop::load_app_settings_checked(&crate::bootstrap::resolve_data_root()) {
        Ok(settings) => settings.linux_webview_acceleration,
        Err(_) => false,
    }
}

/// # Safety
/// 只能在进程入口、创建任何其他线程及初始化 GTK/WebKit 之前调用。
pub unsafe fn configure_before_threads() {
    let acceleration = resolve_acceleration(
        std::env::var(ACCELERATION_ENV).ok().as_deref(),
        setting_acceleration(),
    );
    let _ = EFFECTIVE_ACCELERATION.set(acceleration);
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
    use super::{parse_bool, resolve_acceleration, should_inject};

    #[test]
    fn env_accepts_explicit_true_and_false_values() {
        for value in ["1", "true", "TRUE", "yes", "on", " 1 "] {
            assert_eq!(parse_bool(Some(value)), Some(true), "{value}");
        }
        for value in ["0", "false", "FALSE", "no", "off", " 0 "] {
            assert_eq!(parse_bool(Some(value)), Some(false), "{value}");
        }
    }

    #[test]
    fn env_ignores_empty_or_unknown_values() {
        for value in [None, Some(""), Some(" "), Some("enabled")] {
            assert_eq!(parse_bool(value), None, "{value:?}");
        }
    }

    #[test]
    fn env_overrides_setting_and_setting_is_the_default() {
        assert!(resolve_acceleration(Some("1"), false));
        assert!(!resolve_acceleration(Some("0"), true));
        assert!(resolve_acceleration(None, true));
        assert!(!resolve_acceleration(None, false));
    }

    #[test]
    fn compat_rendering_is_injected_only_without_opt_in_or_user_value() {
        assert!(should_inject(false, false));
        assert!(!should_inject(true, false));
        assert!(!should_inject(false, true));
    }
}
