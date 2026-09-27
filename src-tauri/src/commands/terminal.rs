//! 内嵌终端的 Tauri 命令（薄壳）。会话、回放、流控在 `ncd_runtime::terminal`
//!
//! 终端输出走 `Channel<InvokeResponseBody>` 原始字节：比事件快、有序，大块走 fetch 不转 JSON。
//! 状态事件走另一条 JSON 通道，带 `v` 信封。

use std::path::Path;
use std::sync::Arc;

use ncd_domain::{
    LocalShellOption, ServerStats, TerminalDirListing, TerminalEvent, TerminalEventEnvelope,
    TerminalOpenRequest, TerminalSessionInfo, TerminalTextFile,
};
use ncd_runtime::terminal::TerminalSink;
use tauri::State;
use tauri::ipc::{Channel, InvokeResponseBody};

use crate::AppState;

struct ChannelSink {
    output: Channel<InvokeResponseBody>,
    events: Channel<TerminalEventEnvelope>,
}

impl TerminalSink for ChannelSink {
    fn output(&self, bytes: &[u8]) -> bool {
        self.output
            .send(InvokeResponseBody::Raw(bytes.to_vec()))
            .is_ok()
    }

    fn event(&self, event: &TerminalEvent) -> bool {
        self.events
            .send(TerminalEventEnvelope::new(event.clone()))
            .is_ok()
    }
}

#[tauri::command]
pub async fn terminal_local_shells(
    state: State<'_, AppState>,
) -> Result<Vec<LocalShellOption>, String> {
    let terminals = Arc::clone(&state.terminals);
    tokio::task::spawn_blocking(move || terminals.local_shells())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn terminal_list(state: State<'_, AppState>) -> Result<Vec<TerminalSessionInfo>, String> {
    Ok(state.terminals.list())
}

#[tauri::command]
pub async fn terminal_open(
    state: State<'_, AppState>,
    request: TerminalOpenRequest,
) -> Result<TerminalSessionInfo, String> {
    state.terminals.open(request).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn terminal_attach(
    state: State<'_, AppState>,
    id: String,
    output: Channel<InvokeResponseBody>,
    events: Channel<TerminalEventEnvelope>,
) -> Result<TerminalSessionInfo, String> {
    state
        .terminals
        .attach(&id, Arc::new(ChannelSink { output, events }))
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn terminal_write(
    state: State<'_, AppState>,
    id: String,
    data: String,
) -> Result<(), String> {
    state.terminals.write(&id, &data).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn terminal_resize(
    state: State<'_, AppState>,
    id: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    state
        .terminals
        .resize(&id, cols, rows)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn terminal_ack(state: State<'_, AppState>, id: String, bytes: u64) -> Result<(), String> {
    state.terminals.ack(&id, bytes);
    Ok(())
}

#[tauri::command]
pub async fn terminal_restart(
    state: State<'_, AppState>,
    id: String,
) -> Result<TerminalSessionInfo, String> {
    state.terminals.restart(&id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn terminal_close(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.terminals.close(&id);
    Ok(())
}

#[tauri::command]
pub async fn terminal_clear_history(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state
        .terminals
        .clear_history(&id)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn terminal_fill_sudo(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.terminals.fill_sudo(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn terminal_open_external(
    state: State<'_, AppState>,
    request: TerminalOpenRequest,
) -> Result<(), String> {
    state
        .terminals
        .open_external(&request)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn terminal_stats(state: State<'_, AppState>, id: String) -> Result<ServerStats, String> {
    state
        .terminals
        .host_stats(&id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn terminal_list_dir(
    state: State<'_, AppState>,
    id: String,
    path: String,
) -> Result<TerminalDirListing, String> {
    state
        .terminals
        .list_dir(&id, &path)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn terminal_read_text(
    state: State<'_, AppState>,
    id: String,
    path: String,
) -> Result<TerminalTextFile, String> {
    state
        .terminals
        .read_text(&id, &path)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn terminal_write_text(
    state: State<'_, AppState>,
    id: String,
    path: String,
    content: String,
    crlf: bool,
) -> Result<(), String> {
    state
        .terminals
        .write_text(&id, &path, &content, crlf)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn terminal_make_dir(
    state: State<'_, AppState>,
    id: String,
    path: String,
) -> Result<(), String> {
    state
        .terminals
        .make_dir(&id, &path)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn terminal_rename(
    state: State<'_, AppState>,
    id: String,
    from: String,
    to: String,
) -> Result<(), String> {
    state
        .terminals
        .rename_path(&id, &from, &to)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn terminal_remove(
    state: State<'_, AppState>,
    id: String,
    path: String,
    is_dir: bool,
) -> Result<(), String> {
    state
        .terminals
        .remove_path(&id, &path, is_dir)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn terminal_upload(
    state: State<'_, AppState>,
    id: String,
    local_paths: Vec<String>,
    dest_dir: String,
) -> Result<u32, String> {
    state
        .terminals
        .upload(&id, &local_paths, &dest_dir)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn terminal_download(
    state: State<'_, AppState>,
    id: String,
    path: String,
    local_dest: String,
) -> Result<(), String> {
    state
        .terminals
        .download(&id, &path, &local_dest)
        .await
        .map_err(|e| e.to_string())
}

/// 导出终端输出。路径来自用户在另存为对话框里选的，只收 .txt / .log，不当成通用写文件口子用
#[tauri::command]
pub async fn terminal_export_text(path: String, content: String) -> Result<(), String> {
    let ext = Path::new(&path)
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    if !matches!(ext.as_deref(), Some("txt" | "log")) {
        return Err("只能导出成 .txt 或 .log".into());
    }
    tokio::fs::write(&path, content.as_bytes())
        .await
        .map_err(|e| format!("写不了 {path}：{e}"))
}

#[tauri::command]
pub async fn read_clipboard_text() -> Result<String, String> {
    tokio::task::spawn_blocking(crate::clipboard::read_text)
        .await
        .map_err(|e| e.to_string())?
}

/// 命令测试拼 AppState 用：不开终端
#[cfg(test)]
pub(crate) fn test_terminals() -> Arc<ncd_runtime::terminal::TerminalManager> {
    use ncd_runtime::terminal::{TerminalError, TerminalLaunchPlan, TerminalPlanner};

    struct NoTerminals;

    #[async_trait::async_trait]
    impl TerminalPlanner for NoTerminals {
        async fn plan(
            &self,
            _request: &TerminalOpenRequest,
        ) -> Result<TerminalLaunchPlan, TerminalError> {
            Err(TerminalError::Unsupported("测试里不开终端".into()))
        }

        fn sudo_password(&self, _host_id: &str) -> Option<String> {
            None
        }

        fn local_shells(&self) -> Vec<LocalShellOption> {
            Vec::new()
        }
    }

    Arc::new(ncd_runtime::terminal::TerminalManager::new(Arc::new(
        NoTerminals,
    )))
}
