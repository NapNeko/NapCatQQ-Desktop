use std::time::{SystemTime, UNIX_EPOCH};

use ncd_domain::{
    BotId, QrLoginSessionId, SnowlumaQrFailureCategory, SnowlumaQrLoginResult,
    SnowlumaQrLoginSession,
};
use tauri::State;

use crate::AppState;

/// The decoder gate is intentionally explicit: until a validated local decoder is
/// selected, the existing noVNC path is the only safe result.
#[tauri::command]
pub async fn start_snowluma_qr_login(
    _state: State<'_, AppState>,
    bot_id: String,
) -> Result<SnowlumaQrLoginResult, String> {
    if bot_id.trim().is_empty() {
        return Err("bot_id must not be empty".to_string());
    }
    let session_nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "system clock is before UNIX epoch".to_string())?
        .as_nanos();
    let bot_id = BotId::new(bot_id);
    let session_id = QrLoginSessionId::new(format!("qr-{session_nonce}"))
        .ok_or_else(|| "failed to allocate QR session".to_string())?;
    Ok(SnowlumaQrLoginResult::FallbackNoVnc {
        session: SnowlumaQrLoginSession {
            server_id: "unknown".to_string(),
            bot_id: bot_id.to_string(),
            session_id,
            capture_generation: 0,
        },
        reason: SnowlumaQrFailureCategory::DecoderUnavailable,
    })
}
