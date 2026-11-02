//! 工作区与收藏夹的加载、导入校验和规范化；写入事务由 DebugStore 协调。

use std::collections::{HashMap, HashSet};
use std::io::ErrorKind;
use std::path::Path;

use ncd_domain::onebot_debug::{DebugCollections, DebugStorageNotice, DebugWorkspace};
use serde::de::DeserializeOwned;
use serde_json::Value;
use tokio::io::AsyncReadExt;
use uuid::Uuid;

use super::files::{notice, quarantine, strip_bom};

pub(super) const WORKSPACE_FILE: &str = "workspace.json";
pub(super) const COLLECTIONS_FILE: &str = "collections.json";
/// 手改文件或前端异常也不能无限增大工作区。
pub(super) const MAX_TABS: usize = 50;
const MAX_CLOSED_TABS: usize = 10;
const MAX_RECENT_ACTIONS: usize = 20;
/// 收藏夹导入与 MCP 配置冷读取均限制为 16 MiB。
const IMPORT_MAX_BYTES: u64 = 16 * 1024 * 1024;

pub(super) async fn load_config(
    dir: &Path,
    notices: &mut Vec<DebugStorageNotice>,
) -> (DebugWorkspace, DebugCollections) {
    let workspace = load_json_or_default(dir, WORKSPACE_FILE, notices).await;
    let collections = load_json_or_default(dir, COLLECTIONS_FILE, notices).await;
    (normalize_workspace(workspace), collections)
}

pub(super) async fn load_config_read_only(
    dir: &Path,
) -> Result<(DebugWorkspace, DebugCollections), String> {
    let workspace = read_json_read_only(&dir.join(WORKSPACE_FILE)).await?;
    let collections = read_json_read_only(&dir.join(COLLECTIONS_FILE)).await?;
    Ok((normalize_workspace(workspace), collections))
}

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

/// 导入前完成文件与结构校验；无效输入不修改当前收藏夹或用户的文件。
pub(super) async fn read_collection_import(path: &Path) -> Result<DebugCollections, String> {
    let meta = tokio::fs::metadata(path)
        .await
        .map_err(|e| format!("读取导入文件失败: {e}"))?;
    if meta.len() > IMPORT_MAX_BYTES {
        return Err(format!(
            "导入文件太大（{} MB），收藏夹文件不会超过 16 MB",
            meta.len() / (1024 * 1024)
        ));
    }
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|e| format!("读取导入文件失败: {e}"))?;
    let bytes = strip_bom(&bytes);

    // `DebugCollections` 的字段都有默认值，任何 JSON 对象（比如选成了 workspace.json）
    // 都能解析成一份空收藏夹，导入会「成功」却什么都没导。先确认长得像收藏夹
    let raw: Value =
        serde_json::from_slice(bytes).map_err(|e| format!("导入文件不是有效的 JSON: {e}"))?;
    let looks_like_collections = raw
        .as_object()
        .is_some_and(|o| o.contains_key("folders") || o.contains_key("requests"));
    if !looks_like_collections {
        return Err("导入文件不是收藏夹导出文件（缺少 folders / requests）".to_owned());
    }
    serde_json::from_value(raw).map_err(|e| format!("导入文件的收藏夹结构不对: {e}"))
}

enum Loaded<T> {
    Missing,
    Parsed(T),
    /// 文件在，但读不出来或解析失败；里面是原因
    Broken(String),
}

async fn read_json_read_only<T: DeserializeOwned + Default>(path: &Path) -> Result<T, String> {
    let file = match tokio::fs::File::open(path).await {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(T::default()),
        Err(_) => return Err("无法只读加载调试台配置".to_owned()),
    };
    let mut bytes = Vec::new();
    file.take(IMPORT_MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .await
        .map_err(|_| "无法只读加载调试台配置".to_owned())?;
    if bytes.len() as u64 > IMPORT_MAX_BYTES {
        return Err("调试台配置超过只读加载大小限制".to_owned());
    }
    serde_json::from_slice(strip_bom(&bytes))
        .map_err(|_| "调试台配置损坏，请在桌面调试台处理".to_owned())
}

async fn read_json<T: DeserializeOwned>(path: &Path) -> Loaded<T> {
    let bytes = match tokio::fs::read(path).await {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == ErrorKind::NotFound => return Loaded::Missing,
        Err(e) => return Loaded::Broken(format!("读取失败: {e}")),
    };
    match serde_json::from_slice(strip_bom(&bytes)) {
        Ok(value) => Loaded::Parsed(value),
        Err(e) => Loaded::Broken(e.to_string()),
    }
}

async fn load_json_or_default<T: DeserializeOwned + Default>(
    dir: &Path,
    file: &str,
    notices: &mut Vec<DebugStorageNotice>,
) -> T {
    let path = dir.join(file);
    match read_json(&path).await {
        Loaded::Missing => T::default(),
        Loaded::Parsed(value) => value,
        Loaded::Broken(reason) => {
            let moved = quarantine(&path).await;
            notices.push(notice(file, moved, reason));
            T::default()
        }
    }
}
