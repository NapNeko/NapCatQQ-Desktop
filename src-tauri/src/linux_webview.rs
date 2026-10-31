//! WebKitGTK 在部分 Wayland / 显卡组合下会在焦点、浮层或重绘时停止提交画面。
//! 在 GTK/WebKit 初始化前选择兼容渲染，覆盖主窗口和后续创建的 WebView。

/// # Safety
/// 只能在进程入口、创建任何其他线程及初始化 GTK/WebKit 之前调用。
pub(super) unsafe fn configure_before_threads() {
    for name in [
        "WEBKIT_DISABLE_DMABUF_RENDERER",
        "WEBKIT_DISABLE_COMPOSITING_MODE",
    ] {
        // 保留用户显式指定的渲染设置，也便于在已兼容的环境里恢复硬件合成。
        if std::env::var_os(name).is_none() {
            // SAFETY: 调用方保证尚未创建其他线程，也未进入 GTK/WebKit。
            unsafe { std::env::set_var(name, "1") };
        }
    }
}
