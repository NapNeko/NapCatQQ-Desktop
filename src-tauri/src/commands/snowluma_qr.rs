use ncd_domain::{BotId, SnowlumaQrLoginResult};
use tauri::State;

use crate::AppState;

/// Starts the bounded QR extraction operation and returns its volatile result.
///
/// This one-shot command intentionally exposes no cancellation token. Aborting the
/// caller task is not a cancellation guarantee for a remote command; handled service
/// errors still perform best-effort local and remote temporary-file cleanup.
#[tauri::command]
pub async fn start_snowluma_qr_login(
    state: State<'_, AppState>,
    bot_id: String,
) -> Result<SnowlumaQrLoginResult, String> {
    if bot_id.trim().is_empty() {
        return Err("bot_id must not be empty".to_string());
    }
    state
        .bot_manager
        .start_snowluma_qr_login(&BotId::new(bot_id))
        .await
        .map_err(|err| err.to_string())
}
