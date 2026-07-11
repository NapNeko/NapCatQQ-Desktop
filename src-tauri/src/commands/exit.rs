// 桌面退出闸门:本机 Bot 须先停;允许退出时只停本机,远端 Docker 保持运行

use serde::Serialize;
use tauri::AppHandle;

use crate::AppState;

#[derive(Debug, Clone, Serialize)]
pub struct PrepareExitDesktopResponse {
    pub local_active: usize,
    pub remote_active: usize,
    pub can_exit: bool,
}

#[tauri::command]
pub async fn prepare_exit_desktop(
    state: tauri::State<'_, AppState>,
) -> Result<PrepareExitDesktopResponse, String> {
    let local_active = state
        .bot_manager
        .count_local_active_bots()
        .await
        .map_err(|e| e.to_string())?;
    let remote_active = state
        .bot_manager
        .count_remote_active_bots()
        .await
        .map_err(|e| e.to_string())?;
    Ok(PrepareExitDesktopResponse {
        local_active,
        remote_active,
        can_exit: local_active == 0,
    })
}

/// 本机已无活跃 Bot 时退出进程;远端 Bot 不 stop,仅 detach 本机会话
#[tauri::command]
pub async fn request_exit_app(
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    let local_active = state
        .bot_manager
        .count_local_active_bots()
        .await
        .map_err(|e| e.to_string())?;
    if local_active > 0 {
        return Err(format!(
            "有 {local_active} 个本机 Bot 正在运行，请先停止后再退出"
        ));
    }

    let result = state.bot_manager.exit_desktop().await;
    if !result.failed.is_empty() {
        eprintln!(
            "[bot_manager] request_exit_app: {} local bot(s) failed to stop cleanly",
            result.failed.len()
        );
    }
    // 删远端 desktop_present,ncd-watch 立刻可告警(不必干等 90s TTL)
    crate::commands::ncd_watch::clear_present_on_all_remote_servers(state.inner()).await;
    state.runtime.shutdown().await;
    app.exit(0);
    Ok(())
}