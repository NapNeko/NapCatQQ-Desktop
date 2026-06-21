//! GitHub releases 远端版本快照命令Home 页 update notice 派生用
//!
//! 薄壳层:仅做参数转换 + 调 ncd-runtime release fetcher任何错误都被
//! fetcher 内部消化为 None 字段,命令永远 Ok

use ncd_runtime::release::fetch_release_snapshot;
use ncd_runtime::ReleaseSnapshot;
use tauri::State;

use crate::AppState;
use crate::commands::app_settings::read_github_pat;

/// 拉一次远端版本快照
///
/// 返回值字段都是 Option:网络 / 解析失败一律降级到 None;前端按字段
/// 分别决定是否显示对应 update notice
///
/// 若用户在设置页配过 GitHub PAT,这里读出来带上认证头,把匿名速率限制
/// (60 次/小时)提升到认证额度(5000 次/小时)
#[tauri::command]
pub async fn get_release_snapshot(
    state: State<'_, AppState>,
) -> Result<ReleaseSnapshot, String> {
    let token = read_github_pat(&state.data_root);
    Ok(fetch_release_snapshot(&state.data_root, token.as_deref()).await)
}
