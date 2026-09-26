//! 麦麦 WebUI 里两份主配置之外的东西：提示词、表情包、学到的表达 / 黑话 / 行为、人物、长期记忆、试聊。
//!
//! 每块一个子模块：对外的强类型（ts-rs）、宽松收上游 JSON 的 `Upstream*`、这一块的 WebUI 调用。
//! 上游跨版本会加字段、在字符串位置给 null，所以先宽松收再换成对外类型，对外的不带 null。

pub mod expression;
pub mod jargon;
pub mod prompts;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// 改 / 删这类操作的结果。页面做完操作会重新拉列表，这里只报动了几条、上游怎么说
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotResourceDone {
    pub affected: u32,
    pub message: String,
}

/// 麦麦见过的一个聊天，学习类数据按它归
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotLearningChat {
    /// 上游的会话 id：32 位 md5，只能靠它换名字
    pub chat_id: String,
    pub chat_name: String,
    pub platform: String,
    pub is_group: bool,
}

/// 上游的列表都是 `{success, total, page, page_size, data: [...]}`
#[derive(Debug, Deserialize)]
#[serde(default)]
pub(crate) struct UpstreamPage<T> {
    pub total: Option<u32>,
    pub data: Option<Vec<T>>,
}

impl<T> Default for UpstreamPage<T> {
    fn default() -> Self {
        Self { total: None, data: None }
    }
}

/// `{success, data: T}` 这层壳
#[derive(Debug, Deserialize)]
#[serde(default)]
pub(crate) struct UpstreamData<T> {
    pub data: Option<T>,
}

impl<T> Default for UpstreamData<T> {
    fn default() -> Self {
        Self { data: None }
    }
}

/// 上游删除、更新类回包里的 message；没有就用给的默认话
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub(crate) struct UpstreamMessage {
    pub message: Option<String>,
    pub deleted_count: Option<u32>,
}

pub(crate) fn text(v: Option<String>) -> String {
    v.unwrap_or_default()
}

/// 表达方式那边叫 chat_id，黑话那边叫 session_id，是同一个东西
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub(crate) struct UpstreamChat {
    pub chat_id: Option<String>,
    pub session_id: Option<String>,
    pub chat_name: Option<String>,
    pub platform: Option<String>,
    pub is_group: Option<bool>,
}

impl UpstreamChat {
    pub(crate) fn into_chat(self) -> Option<MaiBotLearningChat> {
        let chat_id = self.chat_id.or(self.session_id).filter(|s| !s.is_empty())?;
        Some(MaiBotLearningChat {
            chat_name: self.chat_name.filter(|n| !n.is_empty()).unwrap_or_else(|| chat_id.clone()),
            chat_id,
            platform: text(self.platform),
            is_group: self.is_group.unwrap_or(false),
        })
    }
}
