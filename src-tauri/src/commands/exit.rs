// 桌面退出闸门:本机 Bot 须先停;允许退出时只停本机,远端 Docker 保持运行

use serde::Serialize;
use tauri::AppHandle;
use ts_rs::TS;

use crate::AppState;

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../src-ui/core/ipc/generated/")]
pub struct PrepareExitDesktopResponse {
    pub local_active: usize,
    pub remote_active: usize,
    pub can_exit: bool,
}

#[tauri::command]
pub async fn prepare_exit_desktop(
    state: tauri::State<'_, AppState>,
) -> Result<PrepareExitDesktopResponse, String> {
    let local_active = local_active_bots(&state).await?;
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
    let local_active = local_active_bots(&state).await?;
    if local_active > 0 {
        return Err(exit_blocked_message(local_active));
    }
    shutdown_and_exit(&app, &state, "request_exit_app").await;
    Ok(())
}

pub(crate) async fn local_active_bots(state: &AppState) -> Result<usize, String> {
    state
        .bot_manager
        .count_local_active_bots()
        .await
        .map_err(|e| e.to_string())
}

pub(crate) fn exit_blocked_message(local_active: usize) -> String {
    format!("有 {local_active} 个本机 Bot 正在运行，请先停止后再退出")
}

/// 闸门放行之后的收尾。主菜单退出和托盘退出都走这一份：之前两边各抄一遍，托盘那份
/// 漏了停应用端实例，退出后 Karin / AstrBot / MaiBot 进程就成了没人管的孤儿
pub(crate) async fn shutdown_and_exit(app: &AppHandle, state: &AppState, origin: &str) {
    let result = state.bot_manager.exit_desktop().await;
    if !result.failed.is_empty() {
        tracing::warn!(
            origin,
            failed = result.failed.len(),
            "local bot(s) failed to stop cleanly on exit"
        );
    }
    // 本机应用端实例随 Desktop 退出停止（有退出入口的先请它自己退）；远端实例脱管（与协议 Bot 同语义）
    state.app_manager.shutdown_local().await;
    state.terminals.close_all();
    // 删远端 desktop_present,ncd-watch 立刻可告警(不必干等 90s TTL)
    crate::commands::ncd_watch::clear_present_on_all_remote_servers(state).await;
    state.runtime.shutdown().await;
    app.exit(0);
}
