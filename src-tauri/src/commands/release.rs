//! GitHub releases 远端版本快照命令。Home 页 update notice 派生用。
//!
//! 薄壳层：仅做参数转换 + 调 ncd-runtime release fetcher。任何错误都被
//! fetcher 内部消化为 None 字段，命令永远 Ok。

use ncd_runtime::release::fetch_release_snapshot;
use ncd_runtime::ReleaseSnapshot;
use tauri::State;

use crate::AppState;

/// 拉一次远端版本快照。
///
/// 返回值字段都是 Option：网络 / 解析失败一律降级到 None；前端按字段
/// 分别决定是否显示对应 update notice。
#[tauri::command]
pub async fn get_release_snapshot(
    state: State<'_, AppState>,
) -> Result<ReleaseSnapshot, String> {
    Ok(fetch_release_snapshot(&state.data_root).await)
}
