//! 查长期记忆里记下的东西：段落（原文片段）、实体（人和物）、关系（谁和谁怎样）、事实；
//! 按来源（导入的文件、聊天）看。删都是先预览再执行：上游会连带删掉只靠它撑着的关系，
//! 预览把连带的也算进去；段落、实体是软删，大约一天内能从最近删除里恢复。

use ncd_traits::AppFrameworkError;
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::json;
use ts_rs::TS;

use super::{BASE, MaiBotMemoryCounts, call, check_id};
use crate::maibot::resources::text;
use crate::maibot::webui_client::{MaiBotWebUi, Request};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum MaiBotMemoryRecordKind {
    Paragraph,
    Entity,
    Relation,
    Fact,
}

impl MaiBotMemoryRecordKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Paragraph => "paragraph",
            Self::Entity => "entity",
            Self::Relation => "relation",
            Self::Fact => "fact",
        }
    }

    fn from_upstream(s: &str) -> Option<Self> {
        Some(match s {
            "paragraph" => Self::Paragraph,
            "entity" => Self::Entity,
            "relation" => Self::Relation,
            "fact" => Self::Fact,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotMemoryQuery {
    pub search: String,
    /// 空是全部四种
    pub kinds: Vec<MaiBotMemoryRecordKind>,
    /// 连已删、已失效的也看
    pub include_inactive: bool,
    pub limit: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotMemoryRecord {
    pub kind: MaiBotMemoryRecordKind,
    pub id: String,
    pub title: String,
    pub summary: String,
    pub source: String,
    /// active / deleted / inactive / retracted …，上游原样
    pub status: String,
    pub active: bool,
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<f64>,
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<f64>,
    /// 段落：narrative / factual / quote / mixed
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub knowledge_type: Option<String>,
    /// 实体：在多少段里出现过
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mentions: Option<u32>,
    /// 关系：0..1
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotMemoryKindCounts {
    pub paragraph: u32,
    pub entity: u32,
    pub relation: u32,
    pub fact: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotMemoryRecordPage {
    pub items: Vec<MaiBotMemoryRecord>,
    pub counts: MaiBotMemoryKindCounts,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotMemoryRecordDetail {
    pub record: MaiBotMemoryRecord,
    /// 撑着它的原文片段
    pub paragraphs: Vec<MaiBotMemoryRecord>,
    pub entities: Vec<MaiBotMemoryRecord>,
    pub relations: Vec<MaiBotMemoryRecord>,
    pub facts: Vec<MaiBotMemoryRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotMemorySource {
    /// 导入的文件名、聊天这类来源名
    pub source: String,
    /// 还在的段落数
    pub paragraphs: u32,
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_updated: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum MaiBotMemoryDeleteKind {
    Paragraph,
    Entity,
    Relation,
    /// 一个来源下的所有段落（撤销一次导入）
    Source,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotMemoryDeleteTarget {
    pub kind: MaiBotMemoryDeleteKind,
    /// hash，或来源名
    pub ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "op", rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum MaiBotMemoryDeleteAction {
    /// 只算会删掉哪些，不动
    Preview {
        target: MaiBotMemoryDeleteTarget,
    },
    Execute {
        target: MaiBotMemoryDeleteTarget,
    },
    Restore {
        operation_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotMemoryDeleteSample {
    /// paragraph / entity / relation
    pub kind: String,
    pub label: String,
    pub preview: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotMemoryDeleteResult {
    pub counts: MaiBotMemoryCounts,
    /// 预览时给前 100 条长什么样
    pub samples: Vec<MaiBotMemoryDeleteSample>,
    /// 执行后给，恢复要用
    pub operation_id: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotMemoryDeleteOp {
    pub id: String,
    /// paragraph / entity / relation / source / mixed
    pub mode: String,
    /// 上游原样：executed / restored …
    pub status: String,
    pub reason: String,
    pub created_at: f64,
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restored_at: Option<f64>,
    pub counts: MaiBotMemoryCounts,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct UpstreamRecord {
    #[serde(rename = "type")]
    kind: Option<String>,
    id: Option<String>,
    title: Option<String>,
    summary: Option<String>,
    source: Option<String>,
    status: Option<String>,
    created_at: Option<f64>,
    updated_at: Option<f64>,
    metadata: Option<UpstreamRecordMeta>,
}

/// metadata 里还有一整份 raw，只取这几项
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct UpstreamRecordMeta {
    knowledge_type: Option<String>,
    appearance_count: Option<u32>,
    confidence: Option<f64>,
}

impl UpstreamRecord {
    fn into_record(self) -> Option<MaiBotMemoryRecord> {
        let kind = MaiBotMemoryRecordKind::from_upstream(self.kind.as_deref()?)?;
        let status = text(self.status);
        let meta = self.metadata.unwrap_or_default();
        Some(MaiBotMemoryRecord {
            kind,
            id: self.id.filter(|s| !s.is_empty())?,
            title: text(self.title),
            summary: text(self.summary),
            source: text(self.source),
            active: status.is_empty() || status == "active",
            status,
            created_at: self.created_at,
            updated_at: self.updated_at,
            knowledge_type: meta
                .knowledge_type
                .filter(|_| kind == MaiBotMemoryRecordKind::Paragraph),
            mentions: meta
                .appearance_count
                .filter(|_| kind == MaiBotMemoryRecordKind::Entity),
            confidence: meta
                .confidence
                .filter(|_| kind == MaiBotMemoryRecordKind::Relation),
        })
    }
}

fn records_of(list: Option<Vec<UpstreamRecord>>) -> Vec<MaiBotMemoryRecord> {
    list.unwrap_or_default()
        .into_iter()
        .filter_map(UpstreamRecord::into_record)
        .collect()
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct UpstreamSearch {
    items: Option<Vec<UpstreamRecord>>,
    counts: Option<MaiBotMemoryKindCountsIn>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct MaiBotMemoryKindCountsIn {
    paragraph: Option<u32>,
    entity: Option<u32>,
    relation: Option<u32>,
    fact: Option<u32>,
}

pub(crate) async fn records(
    c: &MaiBotWebUi,
    q: &MaiBotMemoryQuery,
) -> Result<MaiBotMemoryRecordPage, AppFrameworkError> {
    let limit = q.limit.clamp(1, 200).to_string();
    let types = q
        .kinds
        .iter()
        .map(|k| k.as_str())
        .collect::<Vec<_>>()
        .join(",");
    let search = q.search.trim();
    let mut query = vec![
        ("limit", limit.as_str()),
        (
            "include_inactive",
            if q.include_inactive { "true" } else { "false" },
        ),
    ];
    if !search.is_empty() {
        query.push(("query", search));
    }
    if !types.is_empty() {
        query.push(("types", types.as_str()));
    }
    let up: UpstreamSearch = call(
        c,
        Request::new(Method::GET, &format!("{BASE}/records/search")).query(&query),
    )
    .await?;
    let n = up.counts.unwrap_or_default();
    Ok(MaiBotMemoryRecordPage {
        items: records_of(up.items),
        counts: MaiBotMemoryKindCounts {
            paragraph: n.paragraph.unwrap_or(0),
            entity: n.entity.unwrap_or(0),
            relation: n.relation.unwrap_or(0),
            fact: n.fact.unwrap_or(0),
        },
    })
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct UpstreamDetail {
    record: Option<UpstreamRecord>,
    related: Option<UpstreamRelated>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct UpstreamRelated {
    paragraphs: Option<Vec<UpstreamRecord>>,
    entities: Option<Vec<UpstreamRecord>>,
    relations: Option<Vec<UpstreamRecord>>,
    facts: Option<Vec<UpstreamRecord>>,
}

pub(crate) async fn record(
    c: &MaiBotWebUi,
    kind: MaiBotMemoryRecordKind,
    id: &str,
) -> Result<MaiBotMemoryRecordDetail, AppFrameworkError> {
    let id = check_id(id, "记忆")?;
    let path = format!("{BASE}/records/{}/{id}", kind.as_str());
    let up: UpstreamDetail = call(
        c,
        Request::new(Method::GET, &path).query(&[("limit", "30")]),
    )
    .await?;
    let record = up
        .record
        .and_then(UpstreamRecord::into_record)
        .ok_or_else(|| AppFrameworkError::Validation("这条记忆没找到".into()))?;
    let rel = up.related.unwrap_or_default();
    // 上游把自己也放进了同类的相关列表里，去掉免得详情里重复出现
    let others = |list: Option<Vec<UpstreamRecord>>| -> Vec<MaiBotMemoryRecord> {
        records_of(list)
            .into_iter()
            .filter(|r| r.id != record.id)
            .collect()
    };
    Ok(MaiBotMemoryRecordDetail {
        paragraphs: others(rel.paragraphs),
        entities: others(rel.entities),
        relations: others(rel.relations),
        facts: others(rel.facts),
        record,
    })
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct UpstreamSources {
    items: Option<Vec<UpstreamSource>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct UpstreamSource {
    source: Option<String>,
    count: Option<u32>,
    last_updated: Option<f64>,
}

pub(crate) async fn sources(c: &MaiBotWebUi) -> Result<Vec<MaiBotMemorySource>, AppFrameworkError> {
    let up: UpstreamSources =
        call(c, Request::new(Method::GET, &format!("{BASE}/sources"))).await?;
    Ok(up
        .items
        .unwrap_or_default()
        .into_iter()
        .filter_map(|s| {
            Some(MaiBotMemorySource {
                source: s.source.filter(|x| !x.trim().is_empty())?,
                paragraphs: s.count.unwrap_or(0),
                last_updated: s.last_updated,
            })
        })
        .collect())
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct UpstreamCounts {
    paragraphs: Option<u32>,
    entities: Option<u32>,
    relations: Option<u32>,
    sources: Option<u32>,
}

impl From<UpstreamCounts> for MaiBotMemoryCounts {
    fn from(c: UpstreamCounts) -> Self {
        Self {
            paragraphs: c.paragraphs.unwrap_or(0),
            entities: c.entities.unwrap_or(0),
            relations: c.relations.unwrap_or(0),
            sources: c.sources.unwrap_or(0),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct UpstreamPreview {
    counts: Option<UpstreamCounts>,
    items: Option<Vec<UpstreamPreviewItem>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct UpstreamPreviewItem {
    item_type: Option<String>,
    label: Option<String>,
    preview: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct UpstreamExecuted {
    operation_id: Option<String>,
    counts: Option<UpstreamCounts>,
    deleted_paragraph_count: Option<u32>,
    deleted_entity_count: Option<u32>,
    deleted_relation_count: Option<u32>,
    deleted_source_count: Option<u32>,
}

/// 上游各种删都是 mode + selector；来源按名字，其余按 hash
fn target_body(t: &MaiBotMemoryDeleteTarget) -> Result<serde_json::Value, AppFrameworkError> {
    if t.ids.iter().all(|s| s.trim().is_empty()) {
        return Err(AppFrameworkError::Validation("没挑要删的".into()));
    }
    let ids: Vec<&str> = t
        .ids
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    let (mode, key) = match t.kind {
        MaiBotMemoryDeleteKind::Paragraph => ("paragraph", "hashes"),
        MaiBotMemoryDeleteKind::Entity => ("entity", "hashes"),
        MaiBotMemoryDeleteKind::Relation => ("relation", "hashes"),
        MaiBotMemoryDeleteKind::Source => ("source", "sources"),
    };
    if t.kind != MaiBotMemoryDeleteKind::Source {
        for id in &ids {
            check_id(id, "记忆")?;
        }
    }
    let mut selector = serde_json::Map::new();
    selector.insert(key.into(), json!(ids));
    Ok(json!({ "mode": mode, "selector": selector }))
}

pub(crate) async fn delete(
    c: &MaiBotWebUi,
    a: &MaiBotMemoryDeleteAction,
) -> Result<MaiBotMemoryDeleteResult, AppFrameworkError> {
    match a {
        MaiBotMemoryDeleteAction::Preview { target } => {
            let body = target_body(target)?;
            let up: UpstreamPreview = call(
                c,
                Request::new(Method::POST, &format!("{BASE}/delete/preview"))
                    .body(&body)
                    .slow(),
            )
            .await?;
            Ok(MaiBotMemoryDeleteResult {
                counts: up.counts.unwrap_or_default().into(),
                samples: up
                    .items
                    .unwrap_or_default()
                    .into_iter()
                    .map(|i| MaiBotMemoryDeleteSample {
                        kind: text(i.item_type),
                        label: text(i.label),
                        preview: text(i.preview),
                    })
                    .collect(),
                operation_id: String::new(),
                message: String::new(),
            })
        }
        MaiBotMemoryDeleteAction::Execute { target } => {
            let mut body = target_body(target)?;
            body["reason"] = json!("desktop");
            body["requested_by"] = json!("desktop");
            let up: UpstreamExecuted = call(
                c,
                Request::new(Method::POST, &format!("{BASE}/delete/execute"))
                    .body(&body)
                    .slow(),
            )
            .await?;
            let fallback: MaiBotMemoryCounts = up.counts.unwrap_or_default().into();
            let counts = MaiBotMemoryCounts {
                paragraphs: up.deleted_paragraph_count.unwrap_or(fallback.paragraphs),
                entities: up.deleted_entity_count.unwrap_or(fallback.entities),
                relations: up.deleted_relation_count.unwrap_or(fallback.relations),
                sources: up.deleted_source_count.unwrap_or(fallback.sources),
            };
            Ok(MaiBotMemoryDeleteResult {
                message: describe(&counts, "删掉了"),
                counts,
                samples: Vec::new(),
                operation_id: text(up.operation_id),
            })
        }
        MaiBotMemoryDeleteAction::Restore { operation_id } => {
            let id = check_id(operation_id, "删除记录")?;
            let body = json!({ "operation_id": id, "requested_by": "desktop", "reason": "desktop_restore" });
            let _: serde_json::Value = call(
                c,
                Request::new(Method::POST, &format!("{BASE}/delete/restore"))
                    .body(&body)
                    .slow(),
            )
            .await?;
            Ok(MaiBotMemoryDeleteResult {
                counts: MaiBotMemoryCounts::default(),
                samples: Vec::new(),
                operation_id: id.to_string(),
                message: "恢复了".into(),
            })
        }
    }
}

/// 「删掉了 3 段、2 条关系」这种
fn describe(n: &MaiBotMemoryCounts, verb: &str) -> String {
    let parts: Vec<String> = [
        (n.paragraphs, "段"),
        (n.entities, "个实体"),
        (n.relations, "条关系"),
    ]
    .into_iter()
    .filter(|(k, _)| *k > 0)
    .map(|(k, unit)| format!("{k} {unit}"))
    .collect();
    if parts.is_empty() {
        verb.to_string()
    } else {
        format!("{verb} {}", parts.join("、"))
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct UpstreamOps {
    items: Option<Vec<UpstreamOp>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct UpstreamOp {
    operation_id: Option<String>,
    mode: Option<String>,
    status: Option<String>,
    reason: Option<String>,
    created_at: Option<f64>,
    restored_at: Option<f64>,
    summary: Option<UpstreamOpSummary>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct UpstreamOpSummary {
    counts: Option<UpstreamCounts>,
}

pub(crate) async fn delete_ops(
    c: &MaiBotWebUi,
) -> Result<Vec<MaiBotMemoryDeleteOp>, AppFrameworkError> {
    let up: UpstreamOps = call(
        c,
        Request::new(Method::GET, &format!("{BASE}/delete/operations")).query(&[("limit", "30")]),
    )
    .await?;
    Ok(up
        .items
        .unwrap_or_default()
        .into_iter()
        .filter_map(|o| {
            Some(MaiBotMemoryDeleteOp {
                id: o.operation_id.filter(|s| !s.is_empty())?,
                mode: text(o.mode),
                status: text(o.status),
                reason: text(o.reason),
                created_at: o.created_at.unwrap_or(0.0),
                restored_at: o.restored_at,
                counts: o.summary.and_then(|s| s.counts).unwrap_or_default().into(),
            })
        })
        .collect())
}
