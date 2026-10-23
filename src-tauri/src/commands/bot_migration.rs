// 整套 Bot 迁移的 Tauri 参数与门禁薄壳。

use ncd_domain::bot_migration::{BotMigrationJob, BotMigrationPlan, BotMigrationRequest};
use tauri::State;

use crate::AppState;
use crate::commands::bot::ensure_desktop_consent;

#[tauri::command]
pub async fn preview_bot_migration(
    state: State<'_, AppState>,
    request: BotMigrationRequest,
) -> Result<BotMigrationPlan, String> {
    state.migrate_gate.ensure_idle()?;
    state.app_manager.preview_bot_migration(request).await
}

#[tauri::command]
pub async fn start_bot_migration(
    state: State<'_, AppState>,
    request: BotMigrationRequest,
    fingerprint: String,
) -> Result<BotMigrationJob, String> {
    ensure_desktop_consent(&state)?;
    state.migrate_gate.ensure_idle()?;
    state
        .app_manager
        .start_bot_migration(request, fingerprint)
        .await
}

#[tauri::command]
pub async fn get_bot_migration(
    state: State<'_, AppState>,
    bot_id: String,
) -> Result<Option<BotMigrationJob>, String> {
    state.migrate_gate.ensure_idle()?;
    state.app_manager.get_bot_migration(bot_id).await
}

#[tauri::command]
pub async fn rollback_bot_migration(
    state: State<'_, AppState>,
    job_id: String,
    accept_data_loss: bool,
) -> Result<BotMigrationJob, String> {
    ensure_desktop_consent(&state)?;
    state.migrate_gate.ensure_idle()?;
    state
        .app_manager
        .rollback_bot_migration(job_id, accept_data_loss)
        .await
}
