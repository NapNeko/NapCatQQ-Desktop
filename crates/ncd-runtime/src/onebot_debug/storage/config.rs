//! 工作区上限和收藏夹导入合并；不执行 I/O。

use std::collections::{HashMap, HashSet};

use ncd_domain::onebot_debug::{DebugCollections, DebugWorkspace};
use uuid::Uuid;

use super::{MAX_CLOSED_TABS, MAX_RECENT_ACTIONS, MAX_TABS};

/// 把工作区收进上限内。约定（与前端 store 一致）：
/// `tabs` 老的在前；`closed_tabs`、`recent_actions` 新的在前，所以后两个截尾、`tabs` 从头丢
pub(super) fn normalize_workspace(mut ws: DebugWorkspace) -> DebugWorkspace {
    ws.closed_tabs.truncate(MAX_CLOSED_TABS);
    ws.recent_actions.truncate(MAX_RECENT_ACTIONS);

    if ws.tabs.len() > MAX_TABS {
        let mut excess = ws.tabs.len() - MAX_TABS;
        let active = ws.active_tab.clone();
        ws.tabs.retain(|tab| {
            if excess > 0 && active.as_deref() != Some(tab.id.as_str()) {
                excess -= 1;
                false
            } else {
                true
            }
        });
        // 只有多个标签共用了激活 id 这种畸形数据才会还超，硬截掉兜底
        ws.tabs.truncate(MAX_TABS);
    }
    ws
}

/// 把导入的收藏夹并进现有的：导入的排在现有的后面，id 撞了就换新 id，
/// 请求原来挂的文件夹跟着换后的 id 走；挂在导入文件里根本没有的文件夹上的，落到根
pub(super) fn merge_collections(
    mut base: DebugCollections,
    imported: DebugCollections,
) -> DebugCollections {
    // 文件夹和请求共用一个集合查重，比只查各自的类别更保守，代价可以忽略
    let mut used: HashSet<String> = base
        .folders
        .iter()
        .map(|f| f.id.clone())
        .chain(base.requests.iter().map(|r| r.id.clone()))
        .collect();
    let mut fresh_id = |old: &str| -> String {
        if used.insert(old.to_owned()) {
            old.to_owned()
        } else {
            loop {
                let id = Uuid::new_v4().to_string();
                if used.insert(id.clone()) {
                    break id;
                }
            }
        }
    };

    // 导入项的 order 整体平移到现有最大值之后，彼此的先后不变
    let folder_base = base
        .folders
        .iter()
        .map(|f| f.order)
        .max()
        .map_or(0, |m| m.saturating_add(1));
    let folder_min = imported.folders.iter().map(|f| f.order).min().unwrap_or(0);
    let request_base = base
        .requests
        .iter()
        .map(|r| r.order)
        .max()
        .map_or(0, |m| m.saturating_add(1));
    let request_min = imported.requests.iter().map(|r| r.order).min().unwrap_or(0);

    let mut folder_ids: HashMap<String, String> = HashMap::new();
    for mut folder in imported.folders {
        let old = folder.id.clone();
        folder.id = fresh_id(&old);
        folder.order = folder_base.saturating_add(folder.order.saturating_sub(folder_min));
        // 导入文件里同一个 id 出现两次时，请求归第一个
        folder_ids.entry(old).or_insert_with(|| folder.id.clone());
        base.folders.push(folder);
    }
    for mut request in imported.requests {
        request.id = fresh_id(&request.id);
        request.order = request_base.saturating_add(request.order.saturating_sub(request_min));
        request.folder_id = request
            .folder_id
            .and_then(|old| folder_ids.get(&old).cloned());
        base.requests.push(request);
    }
    base
}
