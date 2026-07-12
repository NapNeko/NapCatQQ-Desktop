//! Desktop 新手引导 IPC 薄壳。

use tauri::State;

use crate::AppState;
use crate::desktop_onboarding::{self, DesktopOnboardingPayload};

#[tauri::command]
pub fn get_desktop_onboarding(
    state: State<'_, AppState>,
) -> Result<DesktopOnboardingPayload, String> {
    Ok(desktop_onboarding::load_payload(&state.data_root))
}

#[tauri::command]
pub fn start_desktop_onboarding(
    state: State<'_, AppState>,
) -> Result<DesktopOnboardingPayload, String> {
    desktop_onboarding::mark_started(&state.data_root).map_err(|e| e.to_command_string())
}

#[tauri::command]
pub fn skip_desktop_onboarding(
    state: State<'_, AppState>,
) -> Result<DesktopOnboardingPayload, String> {
    desktop_onboarding::mark_skipped(&state.data_root).map_err(|e| e.to_command_string())
}

#[tauri::command]
pub fn complete_desktop_onboarding(
    state: State<'_, AppState>,
    completed_step_ids: Option<Vec<String>>,
) -> Result<DesktopOnboardingPayload, String> {
    desktop_onboarding::mark_completed(&state.data_root, completed_step_ids)
        .map_err(|e| e.to_command_string())
}

#[tauri::command]
pub fn reopen_desktop_onboarding(
    state: State<'_, AppState>,
) -> Result<DesktopOnboardingPayload, String> {
    desktop_onboarding::mark_reopened(&state.data_root).map_err(|e| e.to_command_string())
}
