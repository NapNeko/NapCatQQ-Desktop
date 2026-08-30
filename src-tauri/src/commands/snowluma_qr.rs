use ncd_domain::{BotId, SnowlumaQrLoginResult};
use tauri::State;

use crate::AppState;

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
