// 二次启动时唤起已有实例(须为 Builder 上第一个 plugin)

#[cfg(desktop)]
pub fn plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri_plugin_single_instance::init(|app, _args, _cwd| {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            let _ = crate::commands::tray::window_show(app).await;
        });
    })
}
