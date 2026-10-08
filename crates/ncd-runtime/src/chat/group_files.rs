//! 群文件：按后端拼参数，把 NapCat / SnowLuma 的回包归一成同一形状。下载不经过 Bot 主机，
//! 拿到上游给的直链后由桌面端直接下到用户选的位置，远端 Bot 也不用再绕一次 SSH。
//!
//! 这里的调用不走聊天动作白名单：参数由本模块拼好，前端碰不到任意动作名。
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use ncd_domain::bot_config::BackendType;
use ncd_domain::chat_group_files::{
    ChatFileDownloadRequest, ChatFileSource, GroupFile, GroupFileAction, GroupFileFeatures,
    GroupFileListing, GroupFileSpace, GroupFolder,
};
use ncd_domain::onebot_debug::{
    DEBUG_STREAM_VERSION, DebugCallOrigin, DebugCallRequest, DebugCallResult, DebugChannelId,
    DebugStreamProgress, DebugStreamStage,
};
use ncd_network::{DownloadConfig, DownloadProgressSink, NetworkError, ProgressUpdate};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use super::ChatManager;
use crate::DebugStreamSink;

/// NapCat 没有 offset，只能加大条数从头再拉；SnowLuma 不认这个参数，一次给全
pub const FIRST_PAGE: u32 = 200;
pub const MAX_PAGE: u32 = 5000;
// NapCat 内核每拉一批限 5 秒，5000 条要翻好几批
const CALL_TIMEOUT_MS: u32 = 60_000;
const KEPT_DOWNLOADS: usize = 64;
const ROOT: &str = "/";

#[derive(Default)]
pub(super) struct FileState {
    downloads: Mutex<HashMap<String, CancellationToken>>,
    /// 本次运行里下完的文件，只有这些能经界面打开 / 定位，不给页面开任意路径
    finished: Mutex<VecDeque<PathBuf>>,
}

impl FileState {
    pub(super) fn cancel_all(&self) {
        for token in lock(&self.downloads).values() {
            token.cancel();
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|p| p.into_inner())
}

impl ChatManager {
    async fn file_backend(&self, bot_id: &str) -> Result<BackendType, String> {
        self.targets()
            .await
            .into_iter()
            .find(|t| t.bot_id == bot_id)
            .map(|t| t.backend)
            .ok_or_else(|| "聊天账号不存在".to_owned())
    }

    async fn file_call(&self, bot_id: &str, action: &str, params: Value) -> Result<Value, String> {
        let request = DebugCallRequest {
            request_id: uuid::Uuid::new_v4().to_string(),
            bot_id: bot_id.to_owned(),
            channel: DebugChannelId::Auto,
            action: action.to_owned(),
            params,
            timeout_ms: Some(CALL_TIMEOUT_MS),
            origin: DebugCallOrigin::Other,
        };
        match self.transport.call(request).await.result {
            DebugCallResult::Err { error } => Err(crate::onebot_debug::error_text(&error)),
            DebugCallResult::Ok { outcome } if !outcome.ok => Err(upstream_error(
                &outcome.wording,
                &outcome.message,
                outcome.retcode,
            )),
            DebugCallResult::Ok { outcome } if outcome.truncated => {
                Err("群文件回包过大".to_owned())
            }
            DebugCallResult::Ok { outcome } => Ok(outcome.data),
        }
    }

    pub async fn group_files(
        &self,
        bot_id: &str,
        group_id: &str,
        folder_id: Option<&str>,
        limit: Option<u32>,
    ) -> Result<GroupFileListing, String> {
        let backend = self.file_backend(bot_id).await?;
        let limit = limit.unwrap_or(FIRST_PAGE).clamp(FIRST_PAGE, MAX_PAGE);
        let data = self
            .list_raw(bot_id, group_number(group_id)?, folder_id, limit)
            .await?;
        Ok(listing(backend, &data, limit))
    }

    async fn list_raw(
        &self,
        bot_id: &str,
        group: u64,
        folder_id: Option<&str>,
        limit: u32,
    ) -> Result<Value, String> {
        match folder_id.filter(|f| !f.is_empty() && *f != ROOT) {
            None => {
                self.file_call(
                    bot_id,
                    "get_group_root_files",
                    json!({ "group_id": group, "file_count": limit }),
                )
                .await
            }
            Some(folder) => {
                self.file_call(
                    bot_id,
                    "get_group_files_by_folder",
                    json!({ "group_id": group, "folder_id": folder, "file_count": limit }),
                )
                .await
            }
        }
    }

    pub async fn group_file_space(
        &self,
        bot_id: &str,
        group_id: &str,
    ) -> Result<GroupFileSpace, String> {
        let data = self
            .file_call(
                bot_id,
                "get_group_file_system_info",
                json!({ "group_id": group_number(group_id)? }),
            )
            .await?;
        Ok(GroupFileSpace {
            file_count: count(&data["file_count"]),
            limit_count: count(&data["limit_count"]),
            used_space: unsigned(&data["used_space"]),
            total_space: unsigned(&data["total_space"]),
        })
    }

    pub async fn group_file_act(
        &self,
        bot_id: &str,
        group_id: &str,
        action: GroupFileAction,
    ) -> Result<(), String> {
        let backend = self.file_backend(bot_id).await?;
        let group = group_number(group_id)?;
        let (name, params) = action_call(backend, group, &action)?;
        self.file_call(bot_id, name, params).await?;
        // NapCat 的重命名 / 移动丢掉了服务端返回码，被拒也回成功，只能重列一遍看结果
        if backend == BackendType::NapCat {
            let expect = match &action {
                GroupFileAction::RenameFile { parent, name, .. } => {
                    Some((parent.as_str(), name.as_str()))
                }
                GroupFileAction::MoveFile { to, name, .. } => Some((to.as_str(), name.as_str())),
                _ => None,
            };
            if let Some((folder, name)) = expect {
                let data = self.list_raw(bot_id, group, Some(folder), 1000).await?;
                let found = listing(backend, &data, 1000);
                if !found.more && !found.files.iter().any(|file| file.name == name.trim()) {
                    return Err("上游没有确认这次修改，可能没有权限".to_owned());
                }
            }
        }
        Ok(())
    }

    async fn file_url(
        &self,
        bot_id: &str,
        source: &ChatFileSource,
        name: &str,
    ) -> Result<String, String> {
        let backend = self.file_backend(bot_id).await?;
        let params = match source {
            ChatFileSource::Group {
                group_id,
                file_id,
                busid,
            } => {
                let mut params = json!({ "group_id": group_number(group_id)?, "file_id": file_id });
                if let Some(busid) = busid {
                    params["busid"] = json!(busid);
                }
                ("get_group_file_url", params)
            }
            ChatFileSource::Private {
                user_id,
                file_id,
                file_hash,
            } => {
                let mut params = json!({ "file_id": file_id });
                if backend == BackendType::SnowLuma {
                    params["user_id"] = json!(group_number(user_id)?);
                    params["file_hash"] = json!(file_hash.clone().unwrap_or_default());
                }
                ("get_private_file_url", params)
            }
        };
        let data = self.file_call(bot_id, params.0, params.1).await?;
        download_url(data["url"].as_str().unwrap_or_default(), name)
    }

    /// 下到 `dest`，成功返回落盘路径。同名 `.part` 先清掉：它可能是另一份文件没下完的残留
    pub async fn download_file(
        &self,
        request: ChatFileDownloadRequest,
        sink: Arc<dyn DebugStreamSink>,
    ) -> Result<String, String> {
        let dest = download_dest(&request.dest)?;
        let token = {
            let _lease = self.desktop.lease_gate.lock().await;
            if !self.is_enabled() {
                return Err("聊天功能已关闭".into());
            }
            let token = CancellationToken::new();
            lock(&self.files.downloads).insert(request.request_id.clone(), token.clone());
            token
        };
        let result = self.download_inner(&request, &dest, sink, token).await;
        lock(&self.files.downloads).remove(&request.request_id);
        if result.is_ok() {
            let mut finished = lock(&self.files.finished);
            finished.retain(|path| path != &dest);
            if finished.len() >= KEPT_DOWNLOADS {
                finished.pop_front();
            }
            finished.push_back(dest.clone());
        }
        result.map(|()| dest.display().to_string())
    }

    async fn download_inner(
        &self,
        request: &ChatFileDownloadRequest,
        dest: &Path,
        sink: Arc<dyn DebugStreamSink>,
        token: CancellationToken,
    ) -> Result<(), String> {
        let url = tokio::select! {
            url = self.file_url(&request.bot_id, &request.source, &request.name) => url?,
            () = token.cancelled() => return Err("已取消".to_owned()),
        };
        let part = ncd_network::range::part_path(dest);
        let _ = tokio::fs::remove_file(&part).await;
        let progress = Arc::new(Progress {
            request_id: request.request_id.clone(),
            name: request.name.clone(),
            sink,
            token: token.clone(),
        });
        match ncd_network::download_with_resume(
            &url,
            dest,
            progress,
            token,
            DownloadConfig::default(),
        )
        .await
        {
            Ok(_) => Ok(()),
            Err(error) => {
                let _ = tokio::fs::remove_file(&part).await;
                Err(match error {
                    NetworkError::Cancelled => "已取消".to_owned(),
                    other => format!("下载失败：{other}"),
                })
            }
        }
    }

    /// 取消聊天里的一次传输：上传走协议调用，下载走本模块
    pub fn cancel_transfer(&self, request_id: &str) {
        self.transport.cancel(request_id);
        if let Some(token) = lock(&self.files.downloads).get(request_id) {
            token.cancel();
        }
    }

    pub fn downloaded(&self, path: &Path) -> bool {
        lock(&self.files.finished).iter().any(|p| p == path)
    }
}

struct Progress {
    request_id: String,
    name: String,
    sink: Arc<dyn DebugStreamSink>,
    token: CancellationToken,
}

#[async_trait]
impl DownloadProgressSink for Progress {
    async fn tick(&self, update: ProgressUpdate) {
        let sent = self.sink.send(&DebugStreamProgress {
            v: DEBUG_STREAM_VERSION,
            request_id: self.request_id.clone(),
            stage: DebugStreamStage::Downloading,
            file_name: self.name.clone(),
            done_bytes: update.downloaded,
            total_bytes: update.total,
            done_chunks: 0,
            total_chunks: None,
        });
        // 页面已经没了，没人等这份文件
        if !sent {
            self.token.cancel();
        }
    }
}

fn upstream_error(wording: &str, message: &str, retcode: i64) -> String {
    let text = if wording.trim().is_empty() {
        message.trim()
    } else {
        wording.trim()
    };
    if text.is_empty() {
        format!("上游返回错误（{retcode}）")
    } else {
        text.to_owned()
    }
}

fn group_number(value: &str) -> Result<u64, String> {
    value
        .parse::<u64>()
        .ok()
        .filter(|n| *n > 0 && !value.starts_with('0'))
        .ok_or_else(|| "无效的群号或 QQ 号".to_owned())
}

// 两家对同一字段有时给数字有时给字符串
fn text(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => String::new(),
    }
}
fn signed(value: &Value) -> i64 {
    value
        .as_i64()
        .or_else(|| value.as_f64().map(|f| f as i64))
        .or_else(|| value.as_str().and_then(|s| s.trim().parse().ok()))
        .unwrap_or(0)
}
fn unsigned(value: &Value) -> u64 {
    u64::try_from(signed(value)).unwrap_or(0)
}
fn count(value: &Value) -> u32 {
    u32::try_from(signed(value)).unwrap_or(0)
}

fn file_entry(row: &Value) -> Option<GroupFile> {
    let file_id = text(&row["file_id"]);
    if file_id.is_empty() {
        return None;
    }
    // NapCat 两个字段都给，SnowLuma 只有 file_size
    let size = if row.get("file_size").is_some_and(|v| !v.is_null()) {
        &row["file_size"]
    } else {
        &row["size"]
    };
    Some(GroupFile {
        file_id,
        name: text(&row["file_name"]),
        busid: signed(&row["busid"]),
        size: unsigned(size),
        upload_time: signed(&row["upload_time"]),
        dead_time: signed(&row["dead_time"]),
        modify_time: signed(&row["modify_time"]),
        download_times: count(&row["download_times"]),
        uploader: text(&row["uploader"]),
        uploader_name: text(&row["uploader_name"]),
    })
}

fn folder_entry(row: &Value) -> Option<GroupFolder> {
    let folder_id = Some(text(&row["folder_id"]))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| text(&row["folder"]));
    if folder_id.is_empty() {
        return None;
    }
    // NapCat 叫 creator_name，SnowLuma 叫 create_name
    let creator_name = Some(text(&row["creator_name"]))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| text(&row["create_name"]));
    Some(GroupFolder {
        folder_id,
        name: text(&row["folder_name"]),
        create_time: signed(&row["create_time"]),
        creator: text(&row["creator"]),
        creator_name,
        file_count: count(&row["total_file_count"]),
    })
}

fn listing(backend: BackendType, data: &Value, limit: u32) -> GroupFileListing {
    let rows = |key: &str| data[key].as_array().cloned().unwrap_or_default();
    let folders: Vec<_> = rows("folders").iter().filter_map(folder_entry).collect();
    let files: Vec<_> = rows("files").iter().filter_map(file_entry).collect();
    // NapCat 把文件和文件夹合起来数，拉满了就说明后面可能还有
    let more = backend == BackendType::NapCat && folders.len() + files.len() >= limit as usize;
    GroupFileListing {
        folders,
        files,
        more,
        limit,
        features: GroupFileFeatures {
            rename_folder: backend == BackendType::SnowLuma,
        },
    }
}

fn entry_name(name: &str) -> Result<&str, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("名称不能为空".to_owned());
    }
    if name.chars().count() > 255 {
        return Err("名称太长".to_owned());
    }
    if name
        .chars()
        .any(|c| c.is_control() || r#"\/:*?"<>|"#.contains(c))
    {
        return Err(r#"名称不能包含 \ / : * ? " < > |"#.to_owned());
    }
    Ok(name)
}

fn action_call(
    backend: BackendType,
    group: u64,
    action: &GroupFileAction,
) -> Result<(&'static str, Value), String> {
    Ok(match action {
        GroupFileAction::DeleteFile { file_id, busid } => (
            "delete_group_file",
            json!({ "group_id": group, "file_id": file_id, "busid": busid }),
        ),
        GroupFileAction::RenameFile {
            file_id,
            parent,
            name,
        } => (
            "rename_group_file",
            json!({ "group_id": group, "file_id": file_id, "current_parent_directory": parent, "new_name": entry_name(name)? }),
        ),
        // 两家参数名不同，SnowLuma 不认的键会忽略，一起发省得按版本猜
        GroupFileAction::MoveFile {
            file_id, from, to, ..
        } => {
            if from == to {
                return Err("已在这个文件夹里".to_owned());
            }
            (
                "move_group_file",
                json!({ "group_id": group, "file_id": file_id, "current_parent_directory": from, "target_parent_directory": to, "parent_directory": from, "target_directory": to }),
            )
        }
        GroupFileAction::Persist { file_id } => (
            "trans_group_file",
            json!({ "group_id": group, "file_id": file_id }),
        ),
        GroupFileAction::CreateFolder { name } => (
            "create_group_file_folder",
            json!({ "group_id": group, "name": entry_name(name)?, "parent_id": ROOT }),
        ),
        GroupFileAction::DeleteFolder { folder_id } => (
            "delete_group_folder",
            json!({ "group_id": group, "folder_id": folder_id }),
        ),
        GroupFileAction::RenameFolder { folder_id, name } => {
            if backend != BackendType::SnowLuma {
                return Err("NapCat 不支持重命名文件夹".to_owned());
            }
            (
                "rename_group_file_folder",
                json!({ "group_id": group, "folder_id": folder_id, "new_folder_name": entry_name(name)? }),
            )
        }
    })
}

/// 只认 http(s)。NapCat 给的链接 `fname=` 是空的，补上文件名，有的节点不带名字会拒
fn download_url(raw: &str, name: &str) -> Result<String, String> {
    let mut url = reqwest::Url::parse(raw.trim()).map_err(|_| "上游没有给出下载链接".to_owned())?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("上游给出的不是网页下载链接".to_owned());
    }
    let pairs: Vec<(String, String)> = url
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    if pairs.iter().any(|(k, v)| k == "fname" && v.is_empty()) {
        url.query_pairs_mut().clear().extend_pairs(
            pairs
                .iter()
                .map(|(k, v)| (k.as_str(), if k == "fname" { name } else { v.as_str() })),
        );
    }
    Ok(url.into())
}

fn download_dest(raw: &str) -> Result<PathBuf, String> {
    let dest = PathBuf::from(raw);
    if !dest.is_absolute() || dest.file_name().is_none() {
        return Err("保存位置无效".to_owned());
    }
    if dest.is_dir() {
        return Err("保存位置是一个文件夹".to_owned());
    }
    if !dest.parent().is_some_and(Path::is_dir) {
        return Err("保存位置所在的文件夹不存在".to_owned());
    }
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn napcat_listing_reads_both_size_fields_and_reports_more_when_full() {
        let data = json!({
            "files": [{ "file_id": "uuid-1", "file_name": "a.zip", "busid": 102, "size": 10, "file_size": 10, "upload_time": 1, "dead_time": 0, "modify_time": 2, "download_times": 3, "uploader": 123, "uploader_name": "甲" }],
            "folders": [{ "folder_id": "/f1", "folder": "/f1", "folder_name": "资料", "create_time": 5, "creator": 456, "creator_name": "乙", "total_file_count": 7 }]
        });
        let result = listing(BackendType::NapCat, &data, 2);
        assert!(result.more);
        assert!(!result.features.rename_folder);
        assert_eq!(result.files[0].size, 10);
        assert_eq!(result.files[0].uploader, "123");
        assert_eq!(result.folders[0].creator_name, "乙");
        assert_eq!(result.folders[0].file_count, 7);
    }

    #[test]
    fn snowluma_listing_uses_its_own_field_names_and_is_never_paged() {
        let data = json!({
            "files": [{ "file_id": "/abc", "file_name": "b.txt", "busid": 102, "file_size": 99, "uploader": 1 }, { "file_name": "无 id 的行" }],
            "folders": [{ "folder_id": "/f2", "folder_name": "图", "create_name": "丙", "total_file_count": "4" }]
        });
        let result = listing(BackendType::SnowLuma, &data, 1);
        assert!(!result.more);
        assert!(result.features.rename_folder);
        assert_eq!(result.files.len(), 1);
        assert_eq!(result.files[0].size, 99);
        assert_eq!(result.folders[0].creator_name, "丙");
        assert_eq!(result.folders[0].file_count, 4);
    }

    #[test]
    fn move_sends_both_backends_parameter_names() {
        let (name, params) = action_call(
            BackendType::NapCat,
            1,
            &GroupFileAction::MoveFile {
                file_id: "x".into(),
                name: "a".into(),
                from: "/".into(),
                to: "/f".into(),
            },
        )
        .unwrap();
        assert_eq!(name, "move_group_file");
        assert_eq!(params["current_parent_directory"], "/");
        assert_eq!(params["parent_directory"], "/");
        assert_eq!(params["target_parent_directory"], "/f");
        assert_eq!(params["target_directory"], "/f");
    }

    #[test]
    fn folder_rename_is_snowluma_only_and_names_are_checked() {
        let rename = GroupFileAction::RenameFolder {
            folder_id: "/f".into(),
            name: "新".into(),
        };
        assert!(action_call(BackendType::NapCat, 1, &rename).is_err());
        assert_eq!(
            action_call(BackendType::SnowLuma, 1, &rename).unwrap().1["new_folder_name"],
            "新"
        );
        assert!(
            action_call(
                BackendType::SnowLuma,
                1,
                &GroupFileAction::CreateFolder { name: "a/b".into() }
            )
            .is_err()
        );
        assert!(
            action_call(
                BackendType::SnowLuma,
                1,
                &GroupFileAction::CreateFolder { name: "  ".into() }
            )
            .is_err()
        );
        assert_eq!(
            action_call(
                BackendType::NapCat,
                1,
                &GroupFileAction::CreateFolder {
                    name: " 资料 ".into()
                }
            )
            .unwrap()
            .1["name"],
            "资料"
        );
    }

    #[test]
    fn download_url_fills_empty_fname_and_rejects_other_schemes() {
        let url = download_url("https://x.qq.com/ftn_handler/ABC/?fname=", "报告 1.pdf").unwrap();
        assert!(
            url.starts_with("https://x.qq.com/ftn_handler/ABC/?fname="),
            "{url}"
        );
        assert!(!url.ends_with("fname="), "{url}");
        assert_eq!(
            download_url("https://x.qq.com/f/?fname=keep", "n").unwrap(),
            "https://x.qq.com/f/?fname=keep"
        );
        assert!(download_url("file:///etc/passwd", "n").is_err());
        assert!(download_url("", "n").is_err());
    }

    #[test]
    fn download_destination_must_be_a_file_in_an_existing_folder() {
        let dir = tempfile::tempdir().unwrap();
        assert!(download_dest(dir.path().join("a.zip").to_str().unwrap()).is_ok());
        assert!(download_dest(dir.path().to_str().unwrap()).is_err());
        assert!(download_dest(dir.path().join("missing").join("a.zip").to_str().unwrap()).is_err());
        assert!(download_dest("relative.zip").is_err());
    }

    #[test]
    fn group_numbers_must_be_positive_integers() {
        assert_eq!(group_number("123").unwrap(), 123);
        for bad in ["", "0", "012", "-1", "1e3", "abc"] {
            assert!(group_number(bad).is_err(), "{bad}");
        }
    }
}
