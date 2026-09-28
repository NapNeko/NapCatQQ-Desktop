//! 麦麦的长期记忆（A_Memorix），`/api/webui/memory`：往里导资料、查和删记下的东西、看记忆图谱。
//!
//! 默认是关的（`a_memorix.plugin.enabled`）。关着、还在初始化、初始化失败时多数接口照样回 200，
//! 靠 `success` / `disabled` / `reason` 说话，所以每个调用都先拆这层壳；页面先问 [`status`] 再决定给什么。

mod graph;
mod import;
mod records;

pub(crate) use graph::{graph, graph_node, graph_search};
pub(crate) use import::{import, import_setup, task, task_action, tasks};
pub(crate) use records::{delete, delete_ops, record, records, sources};

pub use graph::{
    MaiBotMemoryGraph, MaiBotMemoryGraphEdge, MaiBotMemoryGraphHit, MaiBotMemoryGraphNode,
    MaiBotMemoryGraphParagraph, MaiBotMemoryGraphRelation, MaiBotMemoryNodeDetail,
};
pub use import::{
    MaiBotLocalTextFile, MaiBotMemoryImport, MaiBotMemoryImportKind, MaiBotMemoryImportLimits,
    MaiBotMemoryImportOptions, MaiBotMemoryImportSetup, MaiBotMemoryTask, MaiBotMemoryTaskAction,
    MaiBotMemoryTaskDetail, MaiBotMemoryTaskFile, MaiBotMemoryTaskStatus, inspect_local_texts,
};
pub use records::{
    MaiBotMemoryDeleteAction, MaiBotMemoryDeleteKind, MaiBotMemoryDeleteOp,
    MaiBotMemoryDeleteResult, MaiBotMemoryDeleteSample, MaiBotMemoryDeleteTarget,
    MaiBotMemoryKindCounts, MaiBotMemoryQuery, MaiBotMemoryRecord, MaiBotMemoryRecordDetail,
    MaiBotMemoryRecordKind, MaiBotMemoryRecordPage, MaiBotMemorySource,
};

use ncd_traits::AppFrameworkError;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use crate::maibot::webui_client::{MaiBotWebUi, Request, parse};

pub(super) const BASE: &str = "/api/webui/memory";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum MaiBotMemoryState {
    Ready,
    /// 配置里没开
    Disabled,
    /// 刚开、还在加载
    Starting,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotMemoryStatus {
    pub state: MaiBotMemoryState,
    /// 失败时是原因；其余时候空
    pub message: String,
    /// 能用但缺了点什么（嵌入模型连不上、要重建向量），给人看的句子
    pub notes: Vec<String>,
}

/// 按条数说的删除范围 / 结果
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotMemoryCounts {
    pub paragraphs: u32,
    pub entities: u32,
    pub relations: u32,
    pub sources: u32,
}

/// A_Memorix 的壳：出错多半也是 200
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Envelope {
    success: Option<bool>,
    disabled: Option<bool>,
    error: Option<String>,
    message: Option<String>,
    detail: Option<String>,
}

/// 发请求、拆壳、按 `T` 收。关着和 `success: false` 都报成错，原因用上游的原话
pub(super) async fn call<T: DeserializeOwned>(
    c: &MaiBotWebUi,
    req: Request<'_>,
) -> Result<T, AppFrameworkError> {
    let path = req.path();
    let value = c.send(req).await?;
    unwrap(value, path)
}

pub(super) fn unwrap<T: DeserializeOwned>(
    value: Value,
    path: &str,
) -> Result<T, AppFrameworkError> {
    let env: Envelope = serde_json::from_value(value.clone()).unwrap_or_default();
    if env.disabled == Some(true) {
        return Err(AppFrameworkError::Validation(
            "长期记忆没开，在「记忆」页打开后再用".into(),
        ));
    }
    if env.success == Some(false) {
        let why = env
            .error
            .or(env.message)
            .or(env.detail)
            .filter(|s| !s.trim().is_empty());
        return Err(AppFrameworkError::Validation(
            why.unwrap_or_else(|| "长期记忆没办成这件事".into()),
        ));
    }
    parse(value, path)
}

/// 上游的 id（段落 / 关系 hash、事实 claim id、导入任务 id）要拼进路径，只放行这些字符
pub(super) fn check_id<'a>(id: &'a str, what: &str) -> Result<&'a str, AppFrameworkError> {
    let ok = !id.is_empty()
        && id.len() <= 200
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | ':' | '.'));
    if ok {
        Ok(id)
    } else {
        Err(AppFrameworkError::Validation(format!(
            "{what} id 不对：{id}"
        )))
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct UpstreamRuntime {
    success: Option<bool>,
    disabled: Option<bool>,
    memory_enabled: Option<bool>,
    reason: Option<String>,
    message: Option<String>,
    error: Option<String>,
    embedding_degraded: Option<bool>,
    vector_rebuild_required: Option<bool>,
    vector_rebuild_message: Option<String>,
}

/// `/runtime/config` 关着、初始化中也回 200，是唯一一个不用拆壳就能判状态的接口
pub(crate) async fn status(c: &MaiBotWebUi) -> Result<MaiBotMemoryStatus, AppFrameworkError> {
    let v = c
        .send(Request::new(
            reqwest::Method::GET,
            &format!("{BASE}/runtime/config"),
        ))
        .await?;
    Ok(status_from(serde_json::from_value(v).unwrap_or_default()))
}

fn status_from(up: UpstreamRuntime) -> MaiBotMemoryStatus {
    let text = |s: Option<String>| s.filter(|m| !m.trim().is_empty()).unwrap_or_default();
    let (state, message) = if up.disabled == Some(true) || up.memory_enabled == Some(false) {
        (MaiBotMemoryState::Disabled, String::new())
    } else {
        match up.reason.as_deref() {
            Some("a_memorix_initializing") => (MaiBotMemoryState::Starting, String::new()),
            Some("a_memorix_initialization_failed") => {
                (MaiBotMemoryState::Failed, text(up.message))
            }
            _ if up.success == Some(false) => {
                (MaiBotMemoryState::Failed, text(up.error.or(up.message)))
            }
            _ => (MaiBotMemoryState::Ready, String::new()),
        }
    };
    let mut notes = Vec::new();
    if state == MaiBotMemoryState::Ready {
        if up.embedding_degraded == Some(true) {
            notes.push("嵌入模型现在连不上，只能按关键词找记忆".to_string());
        }
        if up.vector_rebuild_required == Some(true) {
            let m = text(up.vector_rebuild_message);
            notes.push(if m.is_empty() {
                "嵌入模型换过，向量要重建后才找得准".to_string()
            } else {
                m
            });
        }
    }
    MaiBotMemoryStatus {
        state,
        message,
        notes,
    }
}

#[cfg(test)]
mod tests;
