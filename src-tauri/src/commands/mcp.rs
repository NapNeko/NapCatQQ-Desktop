//! OneBot 调试台 MCP 服务的命令与设置热应用接线。
//! 起停逻辑在 ncd-mcp 的 `McpServer`，这里只管「设置 → 服务」+ 随机端口回填。

use std::path::Path;
use std::sync::Arc;

use ncd_domain::{AppSettings, McpServerSettings, McpServerStatus};
use tauri::State;
use tokio::sync::RwLock;

use crate::AppState;

/// 把落盘的 MCP 设置应用到服务；随机分到的端口（设置里还是 0）回填进设置，
/// 应用重启后端口固定、agent 端不用跟着改。回填失败只留日志，下次 apply 再补
pub(crate) async fn apply_mcp_settings(
    mcp: &Arc<ncd_mcp::McpServer>,
    data_root: &Path,
    app_settings: &Arc<RwLock<AppSettings>>,
    settings: &McpServerSettings,
) {
    let outcome = mcp.apply(settings).await;
    let Some(bound_port) = outcome.bound_port else {
        return;
    };
    if bound_port == settings.port {
        return;
    }
    if let Err(err) =
        ncd_runtime::desktop::update_app_settings(data_root, app_settings, move |current| {
            current.mcp.port = bound_port;
        })
        .await
    {
        tracing::warn!(
            target: "ncd_tauri::mcp",
            error = %err,
            "回填 MCP 端口到设置失败"
        );
    }
}

/// MCP 服务当前状态（开着没、在听哪个端口、起失败的原因）
#[tauri::command]
pub async fn mcp_status(state: State<'_, AppState>) -> Result<McpServerStatus, String> {
    Ok(state.mcp.status())
}
