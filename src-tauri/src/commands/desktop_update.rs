//! Desktop 自更新 Tauri 薄壳：check / install。
//!
//! 业务在 ncd-update::UpdateOrchestrator + GithubMsiUpdateProvider；
//! 安装成功后主动 exit，让 msiexec 完成 MajorUpgrade。

use std::sync::Arc;

use ncd_domain::SchemaVersion;
use ncd_update::{AvailableUpdate, PrecheckReport, UpdateChannel, UpdateOrchestrator};
use tauri::{AppHandle, State};
use tracing::info;

use crate::commands::app_settings::read_github_pat;
use crate::desktop_update::{product_version, GithubMsiUpdateProvider};
use crate::AppState;

fn build_orchestrator(state: &AppState) -> Result<UpdateOrchestrator, String> {
    let version = product_version().map_err(|e| e.to_string())?;
    let token = read_github_pat(&state.data_root);
    let provider = Arc::new(GithubMsiUpdateProvider::new(
        state.data_root.clone(),
        token,
    ));
    Ok(UpdateOrchestrator::new(
        provider,
        &state.data_root,
        version,
        SchemaVersion::CURRENT,
    ))
}

/// 检查是否有新的 Desktop MSI（Stable 通道）。
#[tauri::command]
pub async fn check_desktop_update(
    state: State<'_, AppState>,
) -> Result<Option<AvailableUpdate>, String> {
    let orch = build_orchestrator(&state)?;
    orch.check(UpdateChannel::Stable)
        .await
        .map_err(|e| e.to_string())
}

/// schema 预检（当前 MSI 路径固定 CURRENT schema，跨度检查仍可用）。
#[tauri::command]
pub async fn precheck_desktop_update(
    state: State<'_, AppState>,
    update: AvailableUpdate,
) -> Result<PrecheckReport, String> {
    let orch = build_orchestrator(&state)?;
    orch.precheck(&update).await.map_err(|e| e.to_string())
}

/// 下载 MSI 并启动 msiexec；成功后退出本进程。
///
/// 若本机仍有活跃 Bot，拒绝更新（与组件更新闸门一致）。
#[tauri::command]
pub async fn install_desktop_update(
    app: AppHandle,
    state: State<'_, AppState>,
    update: AvailableUpdate,
) -> Result<(), String> {
    let local_active = state
        .bot_manager
        .count_local_active_bots()
        .await
        .map_err(|e| e.to_string())?;
    if local_active > 0 {
        return Err(format!(
            "有 {local_active} 个本机 Bot 正在运行，请先全部停止后再更新 Desktop"
        ));
    }

    let report = {
        let orch = build_orchestrator(&state)?;
        orch.precheck(&update).await.map_err(|e| e.to_string())?
    };
    if !report.can_upgrade {
        let reason = report
            .blocking
            .first()
            .cloned()
            .unwrap_or_else(|| "precheck blocked".into());
        return Err(format!("无法升级: {reason}"));
    }

    // running_bots 列表留给 resume；当前先停本机再装，列表可为空
    let running_bots: Vec<String> = Vec::new();
    let snowluma_running = false;

    let orch = build_orchestrator(&state)?;
    orch.install_with_graceful_shutdown(update, running_bots, snowluma_running)
        .await
        .map_err(|e| e.to_string())?;

    info!(
        target: "ncd_tauri::desktop_update",
        "desktop MSI installer launched; exiting app for upgrade"
    );

    // 清理远端 present 标记，避免 watch 误判在线
    crate::commands::ncd_watch::clear_present_on_all_remote_servers(state.inner()).await;
    state.runtime.shutdown().await;
    app.exit(0);
    Ok(())
}
