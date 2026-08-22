//! Desktop 用户协议 IPC 薄壳。

use tauri::State;

use crate::AppState;
use crate::desktop_consent::{self, DesktopAgreementsPayload};

#[tauri::command]
pub fn get_desktop_agreements(
    state: State<'_, AppState>,
) -> Result<DesktopAgreementsPayload, String> {
    Ok(desktop_consent::load_payload(&state.data_root))
}

#[tauri::command]
pub fn accept_desktop_agreements(
    state: State<'_, AppState>,
    version: String,
) -> Result<DesktopAgreementsPayload, String> {
    desktop_consent::record_consent(&state.data_root, &version)
        .map_err(|e| e.to_command_string())?;
    // 写入后只再读 consent 文件状态；正文走进程缓存，不再重建 Markdown
    Ok(desktop_consent::load_payload(&state.data_root))
}
