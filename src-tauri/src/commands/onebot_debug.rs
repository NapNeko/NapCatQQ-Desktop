//! OneBot 调试台的 Tauri 命令（薄壳）。通道、调用、事件接收器、落盘都在 `ncd_runtime::onebot_debug`
//!
//! 调用没拿到回包的原因（超时、通道不可用……）是数据，随 `DebugCallResponse.result` 走 `Ok`；
//! 其余命令失败时返回一句中文，前端经 `errorText` 处理。
//! 事件批次走 `Channel<DebugEventBatch>`：有序，比全局事件轻，窗口关了 `send` 失败，接收器就把它摘掉。

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use ncd_domain::BackendType;
use ncd_domain::onebot_debug::{
    DebugActionSpec, DebugCallRequest, DebugCallResponse, DebugCatalog, DebugChannelId,
    DebugChannelInfo, DebugChannels, DebugCollections, DebugCollectionsSnapshot, DebugEvent,
    DebugEventBatch, DebugHistoryEntry, DebugHistoryPage, DebugHistoryQuery, DebugReceiverInfo,
    DebugStorageNotice, DebugStreamCallRequest, DebugStreamProgress, DebugSubscribeResponse,
    DebugTarget, DebugWorkspace, DebugWorkspaceSnapshot,
};
use ncd_runtime::onebot_debug::error_text;
use ncd_runtime::{DebugEventSink, DebugStreamSink};
use tauri::ipc::Channel;
use tauri::{State, Webview};

use crate::AppState;

/// 事件出口。记下是哪个窗口、什么时候开的：页面重载后旧的 Channel 照样 send 成功，
/// 要靠 `lib.rs` 的页面加载钩子按这两项摘掉
struct ChannelSink {
    channel: Channel<DebugEventBatch>,
    page: String,
    opened_at: Instant,
}

impl DebugEventSink for ChannelSink {
    fn send(&self, batch: &DebugEventBatch) -> bool {
        self.channel.send(batch.clone()).is_ok()
    }

    fn opened_by(&self) -> Option<(&str, Instant)> {
        Some((&self.page, self.opened_at))
    }
}

#[tauri::command]
pub async fn onebot_debug_targets(state: State<'_, AppState>) -> Result<Vec<DebugTarget>, String> {
    Ok(state.onebot_debug.list_targets().await)
}

#[tauri::command]
pub async fn onebot_debug_channels(
    state: State<'_, AppState>,
    bot_id: String,
) -> Result<DebugChannels, String> {
    state
        .onebot_debug
        .list_channels(&bot_id)
        .await
        .map_err(|e| error_text(&e))
}

#[tauri::command]
pub async fn onebot_debug_test_channel(
    state: State<'_, AppState>,
    bot_id: String,
    channel: DebugChannelId,
) -> Result<DebugChannelInfo, String> {
    state
        .onebot_debug
        .test_channel(&bot_id, channel)
        .await
        .map_err(|e| error_text(&e))
}

#[tauri::command]
pub async fn onebot_debug_catalog(
    state: State<'_, AppState>,
    bot_id: Option<String>,
    backend: BackendType,
) -> Result<DebugCatalog, String> {
    Ok(state.onebot_debug.catalog(bot_id.as_deref(), backend).await)
}

#[tauri::command]
pub async fn onebot_debug_describe(
    state: State<'_, AppState>,
    bot_id: Option<String>,
    backend: BackendType,
    action: String,
) -> Result<Option<DebugActionSpec>, String> {
    Ok(state
        .onebot_debug
        .describe(bot_id.as_deref(), backend, &action)
        .await)
}

#[tauri::command]
pub async fn onebot_debug_call(
    state: State<'_, AppState>,
    request: DebugCallRequest,
) -> Result<DebugCallResponse, String> {
    Ok(state.onebot_debug.call(request).await)
}

/// 流式调用：分块上传 / 下载的进度经 `Channel<DebugStreamProgress>` 推，结果照普通调用回来；
/// 取消复用 `onebot_debug_cancel`——inflight 登记是共享的
#[tauri::command]
pub async fn onebot_debug_call_stream(
    state: State<'_, AppState>,
    request: DebugStreamCallRequest,
    progress: Channel<DebugStreamProgress>,
) -> Result<DebugCallResponse, String> {
    struct ProgressSink {
        channel: Channel<DebugStreamProgress>,
    }
    impl DebugStreamSink for ProgressSink {
        fn send(&self, progress: &DebugStreamProgress) -> bool {
            self.channel.send(progress.clone()).is_ok()
        }
    }
    Ok(state
        .onebot_debug
        .call_stream(request, Arc::new(ProgressSink { channel: progress }))
        .await)
}

#[tauri::command]
pub async fn onebot_debug_cancel(
    state: State<'_, AppState>,
    request_id: String,
) -> Result<(), String> {
    state.onebot_debug.cancel(&request_id);
    Ok(())
}

#[tauri::command]
pub async fn onebot_debug_save_response(
    state: State<'_, AppState>,
    request_id: String,
    path: String,
) -> Result<(), String> {
    state
        .onebot_debug
        .save_response(&request_id, Path::new(&path))
        .await
}

#[tauri::command]
pub async fn onebot_debug_subscribe(
    webview: Webview,
    state: State<'_, AppState>,
    bot_id: String,
    source: DebugChannelId,
    events: Channel<DebugEventBatch>,
) -> Result<DebugSubscribeResponse, String> {
    let sink = ChannelSink {
        channel: events,
        page: webview.label().to_owned(),
        opened_at: Instant::now(),
    };
    state
        .onebot_debug
        .subscribe(&bot_id, source, Arc::new(sink))
        .await
        .map_err(|e| error_text(&e))
}

#[tauri::command]
pub async fn onebot_debug_unsubscribe(
    state: State<'_, AppState>,
    subscription_id: String,
) -> Result<(), String> {
    state.onebot_debug.unsubscribe(&subscription_id).await;
    Ok(())
}

#[tauri::command]
pub async fn onebot_debug_receivers(
    state: State<'_, AppState>,
) -> Result<Vec<DebugReceiverInfo>, String> {
    Ok(state.onebot_debug.receivers().await)
}

#[tauri::command]
pub async fn onebot_debug_stop_receiver(
    state: State<'_, AppState>,
    bot_id: String,
) -> Result<(), String> {
    state.onebot_debug.stop_receiver(&bot_id).await;
    Ok(())
}

#[tauri::command]
pub async fn onebot_debug_read_events(
    state: State<'_, AppState>,
    bot_id: String,
    since_seq: u64,
    limit: u32,
) -> Result<Vec<DebugEvent>, String> {
    Ok(state
        .onebot_debug
        .read_events(&bot_id, since_seq, limit)
        .await)
}

#[tauri::command]
pub async fn onebot_debug_workspace(
    state: State<'_, AppState>,
) -> Result<DebugWorkspaceSnapshot, String> {
    Ok(state.onebot_debug.workspace_snapshot().await)
}

#[tauri::command]
pub async fn onebot_debug_save_workspace(
    state: State<'_, AppState>,
    workspace: DebugWorkspace,
    revision: u32,
) -> Result<(), String> {
    state
        .onebot_debug
        .save_workspace_checked(workspace, revision)
        .await
}

#[tauri::command]
pub async fn onebot_debug_collections(
    state: State<'_, AppState>,
) -> Result<DebugCollectionsSnapshot, String> {
    Ok(state.onebot_debug.collections_snapshot().await)
}

#[tauri::command]
pub async fn onebot_debug_save_collections(
    state: State<'_, AppState>,
    collections: DebugCollections,
    revision: u32,
) -> Result<(), String> {
    state
        .onebot_debug
        .save_collections_checked(collections, revision)
        .await
}

#[tauri::command]
pub async fn onebot_debug_export_collections(
    state: State<'_, AppState>,
    path: String,
) -> Result<(), String> {
    state
        .onebot_debug
        .export_collections(Path::new(&path))
        .await
}

#[tauri::command]
pub async fn onebot_debug_import_collections(
    state: State<'_, AppState>,
    path: String,
) -> Result<DebugCollectionsSnapshot, String> {
    state
        .onebot_debug
        .import_collections(Path::new(&path))
        .await?;
    Ok(state.onebot_debug.collections_snapshot().await)
}

#[tauri::command]
pub async fn onebot_debug_history(
    state: State<'_, AppState>,
    query: DebugHistoryQuery,
) -> Result<DebugHistoryPage, String> {
    Ok(state.onebot_debug.history(query).await)
}

#[tauri::command]
pub async fn onebot_debug_history_entry(
    state: State<'_, AppState>,
    id: String,
) -> Result<Option<DebugHistoryEntry>, String> {
    Ok(state.onebot_debug.history_entry(&id).await)
}

#[tauri::command]
pub async fn onebot_debug_clear_history(state: State<'_, AppState>) -> Result<(), String> {
    state.onebot_debug.clear_history().await
}

/// 落盘文件损坏被挪走的提示在读盘时才产生；这里会等存储加载完再取
#[tauri::command]
pub async fn onebot_debug_storage_notices(
    state: State<'_, AppState>,
) -> Result<Vec<DebugStorageNotice>, String> {
    Ok(state.onebot_debug.take_storage_notices().await)
}

/// 命令测试拼 AppState 用：不碰网络，落盘目录在测试的临时根下
#[cfg(test)]
pub(crate) fn test_onebot_debug(
    root: &Path,
    bots: Arc<dyn ncd_runtime::DebugBotPort>,
) -> Arc<ncd_runtime::DebugManager> {
    let local_host: Arc<dyn ncd_host::Host> = Arc::new(ncd_host::local::LocalWindowsHost::new());
    Arc::new(ncd_runtime::DebugManager::new(
        bots,
        Arc::new(ncd_runtime::LocalOnlyHostResolver::new(local_host)),
        root.to_path_buf(),
    ))
}
