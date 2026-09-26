//! 记忆图谱：实体是点，关系是边。上游 `/graph?limit=N` 按插入顺序取最早的 N 个点，
//! 大一点的库这样看到的全是最早那批边角料；这里一次多要一些，按连着的边数挑最核心的给页面画。

use std::collections::{HashMap, HashSet};

use ncd_traits::AppFrameworkError;
use reqwest::Method;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{BASE, call};
use crate::maibot::resources::text;
use crate::maibot::webui_client::{MaiBotWebUi, Request};

/// 上游单次最多给这么多点
const FETCH_LIMIT: &str = "5000";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotMemoryGraphNode {
    /// 实体名，也是上游认的点 id
    pub id: String,
    /// 在整张图里连着几条边（挑点、定大小用）
    pub degree: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotMemoryGraphEdge {
    pub source: String,
    pub target: String,
    /// 关系词，几种关系时连起来
    pub label: String,
    /// 这两点之间有几条关系
    pub relations: u32,
    /// 几段原文撑着
    pub evidence: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotMemoryGraph {
    pub nodes: Vec<MaiBotMemoryGraphNode>,
    pub edges: Vec<MaiBotMemoryGraphEdge>,
    pub total_nodes: u32,
    pub total_edges: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotMemoryGraphRelation {
    pub hash: String,
    pub subject: String,
    pub predicate: String,
    pub object: String,
    pub confidence: f64,
    pub paragraphs: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotMemoryGraphParagraph {
    pub hash: String,
    pub preview: String,
    pub source: String,
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotMemoryNodeDetail {
    pub id: String,
    /// 实体 hash，删这个实体用
    pub hash: String,
    /// 在多少段原文里出现过
    pub mentions: u32,
    pub relations: Vec<MaiBotMemoryGraphRelation>,
    pub paragraphs: Vec<MaiBotMemoryGraphParagraph>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotMemoryGraphHit {
    /// entity / relation
    pub kind: String,
    pub title: String,
    /// 点开时看哪个点：实体本身，或关系的主语
    pub node: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct UpstreamGraph {
    nodes: Option<Vec<UpstreamNode>>,
    edges: Option<Vec<UpstreamEdge>>,
    total_nodes: Option<u32>,
    total_edges: Option<u32>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct UpstreamNode {
    id: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct UpstreamEdge {
    source: Option<String>,
    target: Option<String>,
    label: Option<String>,
    predicates: Option<Vec<String>>,
    relation_count: Option<u32>,
    evidence_count: Option<u32>,
}

pub(crate) async fn graph(c: &MaiBotWebUi, max_nodes: u32) -> Result<MaiBotMemoryGraph, AppFrameworkError> {
    let path = format!("{BASE}/graph");
    let up: UpstreamGraph = call(c, Request::new(Method::GET, &path).query(&[("limit", FETCH_LIMIT)]).slow()).await?;
    Ok(pick_core(up, max_nodes.clamp(10, 500) as usize))
}

/// 按度数挑前 `keep` 个点，只留两头都在里面的边；度数一样按名字排，每次画出来一样
fn pick_core(up: UpstreamGraph, keep: usize) -> MaiBotMemoryGraph {
    let edges: Vec<MaiBotMemoryGraphEdge> = up
        .edges
        .unwrap_or_default()
        .into_iter()
        .filter_map(|e| {
            let label = e.label.filter(|l| !l.trim().is_empty()).unwrap_or_else(|| e.predicates.unwrap_or_default().join("、"));
            Some(MaiBotMemoryGraphEdge {
                source: e.source.filter(|s| !s.is_empty())?,
                target: e.target.filter(|s| !s.is_empty())?,
                label,
                relations: e.relation_count.unwrap_or(1),
                evidence: e.evidence_count.unwrap_or(0),
            })
        })
        .filter(|e| e.source != e.target)
        .collect();
    let mut degree: HashMap<&str, u32> = HashMap::new();
    for e in &edges {
        *degree.entry(e.source.as_str()).or_default() += 1;
        *degree.entry(e.target.as_str()).or_default() += 1;
    }
    let mut nodes: Vec<MaiBotMemoryGraphNode> = up
        .nodes
        .unwrap_or_default()
        .into_iter()
        .filter_map(|n| n.id.filter(|s| !s.is_empty()))
        .map(|id| MaiBotMemoryGraphNode { degree: degree.get(id.as_str()).copied().unwrap_or(0), id })
        .collect();
    let total_nodes = up.total_nodes.unwrap_or(nodes.len() as u32);
    let total_edges = up.total_edges.unwrap_or(edges.len() as u32);
    nodes.sort_by(|a, b| b.degree.cmp(&a.degree).then_with(|| a.id.cmp(&b.id)));
    nodes.truncate(keep);
    let kept: HashSet<&str> = nodes.iter().map(|n| n.id.as_str()).collect();
    let edges = edges
        .iter()
        .filter(|e| kept.contains(e.source.as_str()) && kept.contains(e.target.as_str()))
        .cloned()
        .collect();
    MaiBotMemoryGraph { nodes, edges, total_nodes, total_edges }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct UpstreamNodeDetail {
    node: Option<UpstreamNodeInfo>,
    relations: Option<Vec<UpstreamRelation>>,
    paragraphs: Option<Vec<UpstreamParagraph>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct UpstreamNodeInfo {
    id: Option<String>,
    hash: Option<String>,
    appearance_count: Option<u32>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct UpstreamRelation {
    hash: Option<String>,
    subject: Option<String>,
    predicate: Option<String>,
    object: Option<String>,
    confidence: Option<f64>,
    paragraph_count: Option<u32>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct UpstreamParagraph {
    hash: Option<String>,
    preview: Option<String>,
    source: Option<String>,
    created_at: Option<f64>,
}

pub(crate) async fn graph_node(c: &MaiBotWebUi, node_id: &str) -> Result<MaiBotMemoryNodeDetail, AppFrameworkError> {
    let id = node_id.trim();
    if id.is_empty() {
        return Err(AppFrameworkError::Validation("没指定看哪个点".into()));
    }
    let query = [("node_id", id), ("relation_limit", "40"), ("paragraph_limit", "12")];
    let up: UpstreamNodeDetail =
        call(c, Request::new(Method::GET, &format!("{BASE}/graph/node-detail")).query(&query)).await?;
    let node = up.node.unwrap_or_default();
    Ok(MaiBotMemoryNodeDetail {
        id: node.id.filter(|s| !s.is_empty()).unwrap_or_else(|| id.to_string()),
        hash: text(node.hash),
        mentions: node.appearance_count.unwrap_or(0),
        relations: up
            .relations
            .unwrap_or_default()
            .into_iter()
            .filter_map(|r| {
                Some(MaiBotMemoryGraphRelation {
                    hash: r.hash.filter(|s| !s.is_empty())?,
                    subject: text(r.subject),
                    predicate: text(r.predicate),
                    object: text(r.object),
                    confidence: r.confidence.unwrap_or(0.0),
                    paragraphs: r.paragraph_count.unwrap_or(0),
                })
            })
            .collect(),
        paragraphs: up
            .paragraphs
            .unwrap_or_default()
            .into_iter()
            .filter_map(|p| {
                Some(MaiBotMemoryGraphParagraph {
                    hash: p.hash.filter(|s| !s.is_empty())?,
                    preview: text(p.preview),
                    source: text(p.source),
                    created_at: p.created_at,
                })
            })
            .collect(),
    })
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct UpstreamHits {
    items: Option<Vec<UpstreamHit>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct UpstreamHit {
    #[serde(rename = "type")]
    kind: Option<String>,
    title: Option<String>,
    entity_name: Option<String>,
    subject: Option<String>,
}

pub(crate) async fn graph_search(c: &MaiBotWebUi, query: &str) -> Result<Vec<MaiBotMemoryGraphHit>, AppFrameworkError> {
    let q = query.trim();
    if q.is_empty() {
        return Ok(Vec::new());
    }
    let path = format!("{BASE}/graph/search");
    let up: UpstreamHits = call(c, Request::new(Method::GET, &path).query(&[("query", q), ("limit", "20")])).await?;
    Ok(up
        .items
        .unwrap_or_default()
        .into_iter()
        .filter_map(|h| {
            let kind = text(h.kind);
            let node = if kind == "relation" { h.subject } else { h.entity_name.or(h.title.clone()) };
            Some(MaiBotMemoryGraphHit { title: text(h.title), node: node.filter(|s| !s.is_empty())?, kind })
        })
        .collect())
}

#[cfg(test)]
pub(super) fn pick_core_for_test(raw: &str, keep: usize) -> MaiBotMemoryGraph {
    pick_core(serde_json::from_str(raw).unwrap(), keep)
}
