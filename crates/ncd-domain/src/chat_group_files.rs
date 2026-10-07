//! 群文件浏览与传输的界面契约。NapCat / SnowLuma 的字段和参数差异在 runtime 归一，
//! 这里只描述归一之后的形状。
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// 一个群文件。`file_id` 只在拿到它的那次列表之后短期有效（NapCat 的是内存里的临时编号，
/// 重启或 24 小时后失效），操作前用最新列表里的值
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/chat/")]
pub struct GroupFile {
    pub file_id: String,
    pub name: String,
    #[ts(type = "number")]
    pub busid: i64,
    #[ts(type = "number")]
    pub size: u64,
    /// 秒级时间戳，0 表示上游没给
    #[ts(type = "number")]
    pub upload_time: i64,
    /// 临时文件的过期时间，永久文件为 0
    #[ts(type = "number")]
    pub dead_time: i64,
    #[ts(type = "number")]
    pub modify_time: i64,
    pub download_times: u32,
    pub uploader: String,
    pub uploader_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/chat/")]
pub struct GroupFolder {
    pub folder_id: String,
    pub name: String,
    #[ts(type = "number")]
    pub create_time: i64,
    pub creator: String,
    pub creator_name: String,
    pub file_count: u32,
}

/// 一个目录的内容。`more` 为真表示上游按条数截断了（NapCat 只能加大条数重拉）
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/chat/")]
pub struct GroupFileListing {
    pub folders: Vec<GroupFolder>,
    pub files: Vec<GroupFile>,
    pub more: bool,
    /// 这次请求的条数，下一页拿它翻倍
    pub limit: u32,
    pub features: GroupFileFeatures,
}

/// 后端能做的操作；做不了的界面不给入口
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/chat/")]
pub struct GroupFileFeatures {
    pub rename_folder: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/chat/")]
pub struct GroupFileSpace {
    pub file_count: u32,
    pub limit_count: u32,
    #[ts(type = "number")]
    pub used_space: u64,
    #[ts(type = "number")]
    pub total_space: u64,
}

/// 改动群文件的操作。目录用上游的文件夹 id，根目录是 `/`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/chat/")]
pub enum GroupFileAction {
    DeleteFile {
        file_id: String,
        #[ts(type = "number")]
        busid: i64,
    },
    RenameFile {
        file_id: String,
        parent: String,
        name: String,
    },
    MoveFile {
        file_id: String,
        name: String,
        from: String,
        to: String,
    },
    /// 临时文件转永久
    Persist {
        file_id: String,
    },
    CreateFolder {
        name: String,
    },
    DeleteFolder {
        folder_id: String,
    },
    RenameFolder {
        folder_id: String,
        name: String,
    },
}

/// 要下载的文件在哪个会话里。私聊文件 SnowLuma 还要文件段里的 `file_hash`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/chat/")]
pub enum ChatFileSource {
    Group {
        group_id: String,
        file_id: String,
        #[ts(type = "number | null")]
        busid: Option<i64>,
    },
    Private {
        user_id: String,
        file_id: String,
        file_hash: Option<String>,
    },
}

/// 一次下载：上游给出直链后由桌面端直接下到 `dest`（另存为对话框选的完整路径）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/chat/")]
pub struct ChatFileDownloadRequest {
    pub request_id: String,
    pub bot_id: String,
    pub source: ChatFileSource,
    pub name: String,
    pub dest: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_wire_shape_is_camel_case_with_kind_tag() {
        let value = serde_json::to_value(GroupFileAction::MoveFile {
            file_id: "a".into(),
            name: "n".into(),
            from: "/".into(),
            to: "/x".into(),
        })
        .unwrap();
        assert_eq!(
            value,
            serde_json::json!({ "kind": "moveFile", "fileId": "a", "name": "n", "from": "/", "to": "/x" })
        );
        let source: ChatFileSource = serde_json::from_value(serde_json::json!({ "kind": "private", "userId": "1", "fileId": "f", "fileHash": null })).unwrap();
        assert_eq!(
            source,
            ChatFileSource::Private {
                user_id: "1".into(),
                file_id: "f".into(),
                file_hash: None
            }
        );
    }
}
