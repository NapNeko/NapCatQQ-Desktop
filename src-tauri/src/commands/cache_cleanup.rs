//! 缓存清理薄壳；路径白名单、文件操作和租约均在 runtime。
use crate::AppState;
use ncd_domain::cache_cleanup::{CacheCategory, CacheCleanupResult, CacheScan};
use tauri::{State, Webview};

#[tauri::command]
pub async fn scan_download_cache(
    webview: Webview,
    state: State<'_, AppState>,
) -> Result<CacheScan, String> {
    ensure_cache_window(webview.label())?;
    state.components.download_cache().scan().await
}

#[tauri::command]
pub async fn clean_download_cache(
    webview: Webview,
    state: State<'_, AppState>,
    categories: Vec<CacheCategory>,
) -> Result<CacheCleanupResult, String> {
    ensure_cache_window(webview.label())?;
    state.components.download_cache().clean(categories).await
}

fn ensure_cache_window(label: &str) -> Result<(), String> {
    // 使用 Tauri 注入的调用者身份，不能信任前端参数中的窗口名称。
    if label == crate::lightweight::MAIN_WINDOW_LABEL {
        Ok(())
    } else {
        Err("下载缓存管理仅允许在主窗口的设置中操作".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cache_management_is_restricted_to_main_webview() {
        assert!(ensure_cache_window("main").is_ok());
        for label in ["chat-panel", "debug-console", "", "main-child"] {
            assert!(ensure_cache_window(label).is_err());
        }
    }
}
