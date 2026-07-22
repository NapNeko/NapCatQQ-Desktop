//! Desktop 自更新 Tauri 薄壳：check / install。
//!
//! 业务在 ncd-update::UpdateOrchestrator + GithubMsiUpdateProvider；
//! 安装成功后主动 exit，让 msiexec 完成 MajorUpgrade。
//!
//! 安全：install 不信任前端传入的 download_url，安装前重新 check，
//! 仅使用服务端解析出的 AvailableUpdate；期望版本与 UI 展示不一致则拒绝。
//!
//! 进度：走 DomainEvent::component_action_progress + 可选 task_id，
//! 与组件页任务队列同一套 UI。

use std::sync::Arc;

use ncd_domain::{ProgressLogLevel, SchemaVersion};
use ncd_network::DownloadProgressSink;
use ncd_update::{AvailableUpdate, PrecheckReport, UpdateChannel, UpdateOrchestrator};
use tauri::{AppHandle, State};
use tokio_util::sync::CancellationToken;
use tracing::info;
use uuid::Uuid;

use crate::AppState;
use crate::commands::app_settings::read_github_pat;
use crate::desktop_update::{
    DesktopUpdateProgressSink, DesktopUpdateStartupNotice, GithubMsiUpdateProvider,
    consume_startup_update_notice, product_version, product_version_str,
    spawn_post_install_relaunch_helper,
};

fn build_check_orchestrator(state: &AppState) -> Result<UpdateOrchestrator, String> {
    let version = product_version().map_err(|e| e.to_string())?;
    let token = read_github_pat(&state.data_root);
    let provider = Arc::new(GithubMsiUpdateProvider::new(state.data_root.clone(), token));
    Ok(UpdateOrchestrator::new(
        provider,
        &state.data_root,
        version,
        SchemaVersion::CURRENT,
    ))
}

fn build_install_orchestrator(
    state: &AppState,
    progress: Arc<dyn DownloadProgressSink>,
    cancel: CancellationToken,
) -> Result<UpdateOrchestrator, String> {
    let version = product_version().map_err(|e| e.to_string())?;
    let token = read_github_pat(&state.data_root);
    let provider = Arc::new(
        GithubMsiUpdateProvider::new(state.data_root.clone(), token)
            .with_progress(progress)
            .with_cancel(cancel),
    );
    Ok(UpdateOrchestrator::new(
        provider,
        &state.data_root,
        version,
        SchemaVersion::CURRENT,
    ))
}

/// 检查是否有新的 Desktop MSI（正式 Release；渠道未开放）。
///
/// - `Ok(None)`：远端可达且已是最新（或 orchestrator 过滤了伪更新）
/// - `Err`：网络/解析/无 release 快照等检查失败（前端勿当成「无需更新」）
#[tauri::command]
pub async fn check_desktop_update(
    state: State<'_, AppState>,
) -> Result<Option<AvailableUpdate>, String> {
    let orch = build_check_orchestrator(&state)?;
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
    let orch = build_check_orchestrator(&state)?;
    orch.precheck(&update).await.map_err(|e| e.to_string())
}

/// 下载 MSI 并启动 msiexec；成功后退出本进程。
///
/// `expected` 仅用于对齐 UI 展示的版本号；真实下载 URL / content_sha256 一律
/// 以服务端 `check` 结果为准。`task_id` 可选，传入则推送 component_action_progress。
///
/// 若本机仍有活跃 Bot，拒绝更新（与组件更新闸门一致）。
#[tauri::command]
pub async fn install_desktop_update(
    app: AppHandle,
    state: State<'_, AppState>,
    expected: AvailableUpdate,
    task_id: Option<String>,
) -> Result<String, String> {
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

    let task_id = task_id
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let cancel = CancellationToken::new();
    state
        .active_tasks
        .lock()
        .await
        .insert(task_id.clone(), cancel.clone());

    let progress_ui = Arc::new(DesktopUpdateProgressSink::new(
        state.event_bus.clone(),
        task_id.clone(),
        1,
    ));
    let progress: Arc<dyn DownloadProgressSink> = progress_ui.clone();

    progress_ui.emit_started(2);
    progress_ui.emit_step_begin(1, "检查 Desktop 更新…");

    let orch = match build_install_orchestrator(&state, progress, cancel.clone()) {
        Ok(o) => o,
        Err(e) => {
            progress_ui.emit_log(ProgressLogLevel::Error, e.clone());
            progress_ui.emit_step_end(1, false);
            progress_ui.emit_finished(false);
            state.active_tasks.lock().await.remove(&task_id);
            return Err(e);
        }
    };

    // 服务端重拉最新包元数据，不使用前端传入的 download_url / content_sha256
    let server_update = match orch.check(UpdateChannel::Stable).await {
        Ok(Some(u)) => u,
        Ok(None) => {
            let msg = "当前已是最新版本，无需更新";
            progress_ui.emit_log(ProgressLogLevel::Info, msg);
            progress_ui.emit_step_end(1, true);
            progress_ui.emit_finished(true);
            state.active_tasks.lock().await.remove(&task_id);
            return Err(msg.into());
        }
        Err(e) => {
            let msg = e.to_string();
            progress_ui.emit_log(ProgressLogLevel::Error, msg.clone());
            progress_ui.emit_step_end(1, false);
            progress_ui.emit_finished(false);
            state.active_tasks.lock().await.remove(&task_id);
            return Err(msg);
        }
    };

    if server_update.version != expected.version {
        let msg = format!(
            "可更新版本已变化（界面 {} → 远端 {}），请重新检查后再更新",
            expected.version, server_update.version
        );
        progress_ui.emit_log(ProgressLogLevel::Error, msg.clone());
        progress_ui.emit_step_end(1, false);
        progress_ui.emit_finished(false);
        state.active_tasks.lock().await.remove(&task_id);
        return Err(msg);
    }

    let report = match orch.precheck(&server_update).await {
        Ok(r) => r,
        Err(e) => {
            let msg = e.to_string();
            progress_ui.emit_log(ProgressLogLevel::Error, msg.clone());
            progress_ui.emit_step_end(1, false);
            progress_ui.emit_finished(false);
            state.active_tasks.lock().await.remove(&task_id);
            return Err(msg);
        }
    };
    if !report.can_upgrade {
        let reason = report
            .blocking
            .first()
            .cloned()
            .unwrap_or_else(|| "precheck blocked".into());
        let msg = format!("无法升级: {reason}");
        progress_ui.emit_log(ProgressLogLevel::Error, msg.clone());
        progress_ui.emit_step_end(1, false);
        progress_ui.emit_finished(false);
        state.active_tasks.lock().await.remove(&task_id);
        return Err(msg);
    }

    progress_ui.emit_step_end(1, true);
    progress_ui.set_step(2);
    progress_ui.emit_step_begin(
        2,
        format!(
            "下载并安装 Desktop {}（将显示安装进度）…",
            server_update.version
        ),
    );

    // running_bots 列表留给 resume；当前先停本机再装，列表可为空
    let running_bots: Vec<String> = Vec::new();
    let snowluma_running = false;
    let target_version = server_update.version.to_string();

    let install_result = orch
        .install_with_graceful_shutdown(server_update, running_bots, snowluma_running)
        .await;

    state.active_tasks.lock().await.remove(&task_id);

    match install_result {
        Ok(()) => {
            progress_ui.emit_step_end(2, true);
            progress_ui.emit_log(
                ProgressLogLevel::Info,
                "安装程序已启动（进度条窗口）。应用即将退出；安装完成后会自动重新打开。",
            );
            progress_ui.emit_finished(true);

            info!(
                target: "ncd_tauri::desktop_update",
                task_id = %task_id,
                target = %target_version,
                "desktop MSI installer launched; exiting app for upgrade"
            );

            // 先起 relaunch helper（带目标版本），再关 runtime / exit
            spawn_post_install_relaunch_helper(&target_version);

            crate::commands::ncd_watch::clear_present_on_all_remote_servers(state.inner()).await;
            state.runtime.shutdown().await;
            app.exit(0);
            Ok(task_id)
        }
        Err(e) => {
            let msg = e.to_string();
            progress_ui.emit_log(ProgressLogLevel::Error, msg.clone());
            progress_ui.emit_step_end(2, false);
            progress_ui.emit_finished(false);
            Err(msg)
        }
    }
}

/// 启动时消费一次：上次自更新成功 / 未完成 / 失败提示。
#[tauri::command]
pub async fn consume_desktop_update_startup_notice(
    state: State<'_, AppState>,
) -> Result<Option<DesktopUpdateStartupNotice>, String> {
    Ok(consume_startup_update_notice(&state.data_root, product_version_str()).await)
}
