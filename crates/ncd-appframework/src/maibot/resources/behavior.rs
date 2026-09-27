//! 学到的行为，`/api/webui/behavior`。麦麦从聊天里总结「在什么情境下、谁做了什么、结果怎样」，
//! 回复时按情境挑一条在用的照着做，做完看对方反应加减分；分数掉到底或长期没用会被它自己停掉。
//! 上游只给看、没有改的接口，这里也只读。

use ncd_traits::AppFrameworkError;
use serde::de::IgnoredAny;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use super::{UpstreamData, UpstreamPage, text};
use crate::maibot::webui_client::MaiBotWebUi;

const BASE: &str = "/api/webui/behavior";
/// 上游管不挂在任何聊天上的那批叫这个，筛选时原样传回去
const GLOBAL_CHAT: &str = "__global__";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum MaiBotBehaviorFilter {
    #[default]
    All,
    Enabled,
    /// 麦麦自己停掉的
    Disabled,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum MaiBotBehaviorOrigin {
    #[default]
    All,
    /// 看别人怎么做、结果怎样学到的
    Observed,
    /// 麦麦自己做了、看反应学到的
    SelfReflection,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum MaiBotBehaviorSort {
    /// 最近见到或用到的在前
    #[default]
    Recent,
    Score,
    Seen,
    Used,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotBehaviorQuery {
    pub page: u32,
    pub page_size: u32,
    pub search: String,
    /// 空串是全部聊天，`__global__` 是不挂聊天的
    pub chat_id: String,
    pub filter: MaiBotBehaviorFilter,
    pub origin: MaiBotBehaviorOrigin,
    pub sort: MaiBotBehaviorSort,
}

/// 这件事是谁做的
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum MaiBotBehaviorActor {
    /// 群友或私聊的对方
    Others,
    /// 一群人一起
    Group,
    Maibot,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum MaiBotBehaviorTagKind {
    /// 在聊什么
    Domain,
    /// 对方想要什么
    Need,
    /// 气氛、态度
    Attitude,
    Other,
}

/// 情境的一个标签；同义的词上游归成一簇，label 是簇里最常见的一两个说法
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotBehaviorTag {
    pub kind: MaiBotBehaviorTagKind,
    pub label: String,
    /// 在这类情境里占的分量，0 到 1
    pub weight: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotBehavior {
    #[ts(type = "number")]
    pub id: i64,
    /// `__global__` 是不挂聊天的
    pub chat_id: String,
    pub chat_name: String,
    /// 情境，按分量从大到小
    pub scene: Vec<MaiBotBehaviorTag>,
    pub actor: MaiBotBehaviorActor,
    /// 麦麦自己做了、看反应学到的；否则是看别人学的
    pub self_reflection: bool,
    pub action: String,
    pub outcome: String,
    /// 同样的情境、做法、结果又见到一次加一
    pub seen: u32,
    /// 麦麦挑它来照着做的次数
    pub used: u32,
    pub succeeded: u32,
    pub failed: u32,
    /// -6 到 8，每次反馈加减；低于 -4 就不会再被挑中
    pub score: f64,
    pub enabled: bool,
    /// 上次见到或用到，秒级时间戳
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_at: Option<f64>,
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_feedback_at: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotBehaviorPage {
    pub total: u32,
    pub items: Vec<MaiBotBehavior>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotBehaviorChat {
    /// `__global__` 是不挂聊天的那批
    pub chat_id: String,
    pub chat_name: String,
    pub is_group: bool,
    /// 这个聊天里学到几条
    pub count: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotBehaviorOverview {
    /// 有经验的聊天，最近活跃的在前
    pub chats: Vec<MaiBotBehaviorChat>,
    pub total: u32,
    pub enabled: u32,
    pub disabled: u32,
}

/// 学到这条的一次观察
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotBehaviorEvidence {
    pub action: String,
    pub outcome: String,
    pub actor: MaiBotBehaviorActor,
    /// 依据的消息条数
    pub messages: u32,
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum MaiBotBehaviorFeedbackKind {
    Success,
    Partial,
    Failure,
    Neutral,
    /// 长期没用上，自动扣了一点分
    Decay,
    /// 被自动停用
    Disabled,
}

/// 麦麦照着做了之后的一次反馈，或者一次自动维护
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotBehaviorFeedback {
    pub kind: MaiBotBehaviorFeedbackKind,
    /// 这次加减的分
    pub delta: f64,
    pub reason: String,
    /// 实际结果怎样（自动维护没有）
    pub outcome: String,
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotBehaviorDetail {
    pub item: MaiBotBehavior,
    /// 新的在前；上游只留最近 20 次
    pub evidence: Vec<MaiBotBehaviorEvidence>,
    /// 新的在前；上游只留最近 30 次
    pub feedback: Vec<MaiBotBehaviorFeedback>,
}

// 以下是上游 JSON 的宽松形状

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct UpstreamTag {
    tag: Option<String>,
    probability: Option<f64>,
    display: Option<String>,
}

impl UpstreamTag {
    fn into_tag(self) -> Option<MaiBotBehaviorTag> {
        let tag = self.tag.map(|t| t.trim().to_string()).filter(|t| !t.is_empty())?;
        // 标签形如 `domain:游戏`；簇的 key 也可能是随机的 tc_<uuid>，这时只能靠 display 给名字
        let (kind, key) = tag.split_once(':').unwrap_or(("", tag.as_str()));
        let display = self.display.map(|d| d.trim().to_string()).filter(|d| !d.is_empty() && *d != tag);
        let label = match display {
            Some(d) => d,
            None if key.starts_with("tc_") || key.is_empty() => return None,
            None => key.to_string(),
        };
        Some(MaiBotBehaviorTag {
            kind: tag_kind(kind),
            label,
            weight: self.probability.unwrap_or(0.0).clamp(0.0, 1.0),
        })
    }
}

fn tag_kind(raw: &str) -> MaiBotBehaviorTagKind {
    match raw {
        "domain" => MaiBotBehaviorTagKind::Domain,
        "need" => MaiBotBehaviorTagKind::Need,
        "attitude" => MaiBotBehaviorTagKind::Attitude,
        _ => MaiBotBehaviorTagKind::Other,
    }
}

fn actor(raw: Option<&str>) -> MaiBotBehaviorActor {
    match raw {
        Some("other_user") => MaiBotBehaviorActor::Others,
        Some("group_collective") => MaiBotBehaviorActor::Group,
        Some("maibot_self") => MaiBotBehaviorActor::Maibot,
        _ => MaiBotBehaviorActor::Unknown,
    }
}

/// 上游存的是麦麦那台机器的本地时间、不带时区，按本机时区折算；远端时区不同会差几个小时
fn iso_secs(raw: Option<&str>) -> Option<f64> {
    let s = raw?.trim();
    if s.is_empty() {
        return None;
    }
    if let Ok(t) = chrono::DateTime::parse_from_rfc3339(s) {
        return Some(t.timestamp_millis() as f64 / 1000.0);
    }
    let naive = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f")
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%.f"))
        .ok()?;
    let local = naive.and_local_timezone(chrono::Local).earliest()?;
    Some(local.timestamp_millis() as f64 / 1000.0)
}

fn chat_id(session_id: Option<String>) -> String {
    session_id.filter(|s| !s.is_empty()).unwrap_or_else(|| GLOBAL_CHAT.into())
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct UpstreamPath {
    id: Option<i64>,
    session_id: Option<String>,
    chat_name: Option<String>,
    scene_cluster_tags: Option<Vec<UpstreamTag>>,
    actor_type: Option<String>,
    learning_type: Option<String>,
    action: Option<String>,
    outcome: Option<String>,
    count: Option<u32>,
    activation_count: Option<u32>,
    success_count: Option<u32>,
    failure_count: Option<u32>,
    score: Option<f64>,
    enabled: Option<bool>,
    last_active_time: Option<String>,
    last_feedback_time: Option<String>,
}

impl UpstreamPath {
    fn into_item(self) -> Option<MaiBotBehavior> {
        let mut scene: Vec<MaiBotBehaviorTag> =
            self.scene_cluster_tags.unwrap_or_default().into_iter().filter_map(UpstreamTag::into_tag).collect();
        scene.sort_by(|a, b| b.weight.total_cmp(&a.weight));
        Some(MaiBotBehavior {
            id: self.id?,
            chat_id: chat_id(self.session_id),
            chat_name: text(self.chat_name),
            scene,
            actor: actor(self.actor_type.as_deref()),
            self_reflection: self.learning_type.as_deref() == Some("self_reflection"),
            action: text(self.action),
            outcome: text(self.outcome),
            seen: self.count.unwrap_or(0),
            used: self.activation_count.unwrap_or(0),
            succeeded: self.success_count.unwrap_or(0),
            failed: self.failure_count.unwrap_or(0),
            score: self.score.unwrap_or(0.0),
            enabled: self.enabled.unwrap_or(true),
            active_at: iso_secs(self.last_active_time.as_deref()),
            last_feedback_at: iso_secs(self.last_feedback_time.as_deref()),
        })
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct UpstreamChat {
    session_id: Option<String>,
    display_name: Option<String>,
    chat_type: Option<String>,
    path_count: Option<u32>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct UpstreamDetail {
    path: Option<UpstreamPath>,
    // 这两个上游是原样吐出库里的 JSON 数组，旧数据里可能混着别的形状，逐条认
    evidence: Option<Vec<Value>>,
    feedback: Option<Vec<Value>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct UpstreamEvidence {
    action: Option<String>,
    outcome: Option<String>,
    actor_type: Option<String>,
    source_ids: Option<Vec<IgnoredAny>>,
    created_at: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct UpstreamFeedback {
    score_delta: Option<f64>,
    status: Option<String>,
    reason: Option<String>,
    outcome: Option<String>,
    created_at: Option<String>,
}

fn feedback_kind(status: &str) -> MaiBotBehaviorFeedbackKind {
    match status.trim().to_ascii_lowercase().as_str() {
        "success" | "succeeded" | "completed" => MaiBotBehaviorFeedbackKind::Success,
        "partial_success" => MaiBotBehaviorFeedbackKind::Partial,
        "failed" | "blocked" | "abandoned" => MaiBotBehaviorFeedbackKind::Failure,
        "maintenance_decay" => MaiBotBehaviorFeedbackKind::Decay,
        "maintenance_disable" => MaiBotBehaviorFeedbackKind::Disabled,
        _ => MaiBotBehaviorFeedbackKind::Neutral,
    }
}

pub(crate) async fn list(c: &MaiBotWebUi, q: &MaiBotBehaviorQuery) -> Result<MaiBotBehaviorPage, AppFrameworkError> {
    let page = q.page.max(1).to_string();
    let size = q.page_size.clamp(1, 100).to_string();
    let search = q.search.trim();
    let sort_by = match q.sort {
        MaiBotBehaviorSort::Recent => "last_active_time",
        MaiBotBehaviorSort::Score => "score",
        MaiBotBehaviorSort::Seen => "count",
        MaiBotBehaviorSort::Used => "activation_count",
    };
    let mut query = vec![
        ("page", page.as_str()),
        ("page_size", size.as_str()),
        ("sort_by", sort_by),
        ("sort_order", "desc"),
    ];
    if !search.is_empty() {
        query.push(("search", search));
    }
    if !q.chat_id.is_empty() {
        query.push(("session_id", q.chat_id.as_str()));
    }
    match q.filter {
        MaiBotBehaviorFilter::All => {}
        MaiBotBehaviorFilter::Enabled => query.push(("enabled", "true")),
        MaiBotBehaviorFilter::Disabled => query.push(("enabled", "false")),
    }
    match q.origin {
        MaiBotBehaviorOrigin::All => {}
        MaiBotBehaviorOrigin::Observed => query.push(("learning_type", "observed_behavior")),
        MaiBotBehaviorOrigin::SelfReflection => query.push(("learning_type", "self_reflection")),
    }
    let up: UpstreamPage<UpstreamPath> = c.get(&format!("{BASE}/paths"), &query).await?;
    Ok(MaiBotBehaviorPage {
        total: up.total.unwrap_or(0),
        items: up.data.unwrap_or_default().into_iter().filter_map(UpstreamPath::into_item).collect(),
    })
}

/// 上游没有统计接口：在用、停用各查一页只取总数
async fn count(c: &MaiBotWebUi, enabled: &str) -> Result<u32, AppFrameworkError> {
    let up: UpstreamPage<IgnoredAny> =
        c.get(&format!("{BASE}/paths"), &[("page_size", "1"), ("enabled", enabled)]).await?;
    Ok(up.total.unwrap_or(0))
}

pub(crate) async fn overview(c: &MaiBotWebUi) -> Result<MaiBotBehaviorOverview, AppFrameworkError> {
    let chats: UpstreamData<Vec<UpstreamChat>> = c.get(&format!("{BASE}/chats"), &[]).await?;
    let enabled = count(c, "true").await?;
    let disabled = count(c, "false").await?;
    Ok(MaiBotBehaviorOverview {
        chats: chats
            .data
            .unwrap_or_default()
            .into_iter()
            .map(|ch| {
                let chat_id = chat_id(ch.session_id);
                MaiBotBehaviorChat {
                    chat_name: ch.display_name.filter(|n| !n.is_empty()).unwrap_or_else(|| chat_id.clone()),
                    chat_id,
                    is_group: ch.chat_type.as_deref() == Some("group"),
                    count: ch.path_count.unwrap_or(0),
                }
            })
            .collect(),
        total: enabled + disabled,
        enabled,
        disabled,
    })
}

pub(crate) async fn detail(c: &MaiBotWebUi, id: i64) -> Result<MaiBotBehaviorDetail, AppFrameworkError> {
    let up: UpstreamData<UpstreamDetail> = c.get(&format!("{BASE}/paths/{id}"), &[]).await?;
    let d = up.data.unwrap_or_default();
    let item = d
        .path
        .and_then(UpstreamPath::into_item)
        .ok_or_else(|| AppFrameworkError::Integration(format!("麦麦没回第 {id} 条经验")))?;
    let evidence = d
        .evidence
        .unwrap_or_default()
        .into_iter()
        .rev()
        .filter_map(|v| serde_json::from_value::<UpstreamEvidence>(v).ok())
        .map(|e| MaiBotBehaviorEvidence {
            action: text(e.action),
            outcome: text(e.outcome),
            actor: actor(e.actor_type.as_deref()),
            messages: e.source_ids.map_or(0, |ids| ids.len() as u32),
            at: iso_secs(e.created_at.as_deref()),
        })
        .collect();
    let feedback = d
        .feedback
        .unwrap_or_default()
        .into_iter()
        .rev()
        .filter_map(|v| serde_json::from_value::<UpstreamFeedback>(v).ok())
        .map(|f| MaiBotBehaviorFeedback {
            kind: feedback_kind(f.status.as_deref().unwrap_or("")),
            delta: f.score_delta.unwrap_or(0.0),
            reason: text(f.reason),
            outcome: text(f.outcome),
            at: iso_secs(f.created_at.as_deref()),
        })
        .collect();
    Ok(MaiBotBehaviorDetail { item, evidence, feedback })
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const PATH_ROW: &str = r#"{"id":12,"session_id":null,"chat_name":"全局行为","scene_cluster_id":3,
        "scene_cluster_name":"游戏 / 抽卡=0.600；tc_0f=0.100","scene_cluster_tags":[
            {"tag":"attitude:吐槽","probability":0.3,"display":"吐槽"},
            {"tag":"domain:tc_0f3a","probability":0.1,"display":"domain:tc_0f3a"},
            {"tag":"domain:游戏","probability":0.6,"display":"游戏 / 抽卡"},
            {"tag":"need:安慰","probability":0.2,"display":"need:安慰"}
        ],"scene_cluster_source_count":4,"actor_type":"maibot_self","learning_type":"self_reflection",
        "action":"接一句「又歪了是吧」","outcome":"对方笑了，接着聊","count":3,"activation_count":5,
        "success_count":2,"failure_count":1,"score":1.5,"enabled":false,
        "last_active_time":"2026-09-20T10:00:00.123456","last_feedback_time":null,"update_time":"2026-09-21T08:00:00"}"#;

    #[test]
    fn upstream_rows_map_to_items() {
        let up: UpstreamPath = serde_json::from_str(PATH_ROW).unwrap();
        let item = up.into_item().unwrap();
        assert_eq!(item.chat_id, GLOBAL_CHAT, "不挂聊天的按上游的筛选值给");
        let labels: Vec<_> = item.scene.iter().map(|t| (t.kind, t.label.as_str())).collect();
        // 按分量排；随机 key 又没解析出名字的不要，没解析出名字但 key 可读的用 key
        assert_eq!(
            labels,
            [
                (MaiBotBehaviorTagKind::Domain, "游戏 / 抽卡"),
                (MaiBotBehaviorTagKind::Attitude, "吐槽"),
                (MaiBotBehaviorTagKind::Need, "安慰"),
            ]
        );
        assert_eq!(item.actor, MaiBotBehaviorActor::Maibot);
        assert!(item.self_reflection && !item.enabled);
        assert_eq!((item.seen, item.used, item.succeeded, item.failed), (3, 5, 2, 1));
        assert!(item.active_at.is_some() && item.last_feedback_at.is_none());
    }

    #[test]
    fn naive_and_offset_times_both_parse() {
        let a = iso_secs(Some("2026-09-20T10:00:00")).unwrap();
        let b = iso_secs(Some("2026-09-20T10:00:00.500000")).unwrap();
        assert!((b - a - 0.5).abs() < 1e-6);
        assert_eq!(iso_secs(Some("2026-09-20 10:00:00")), Some(a));
        assert_eq!(iso_secs(Some("1970-01-01T00:00:10+00:00")), Some(10.0));
        assert_eq!(iso_secs(Some("昨天")), None);
        assert_eq!(iso_secs(Some("")), None);
    }

    fn client(server: &MockServer) -> MaiBotWebUi {
        MaiBotWebUi::connect(server.address().port(), "tok").unwrap()
    }

    #[tokio::test]
    async fn list_sends_filters_the_way_upstream_reads_them() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/webui/behavior/paths"))
            .and(query_param("session_id", "__global__"))
            .and(query_param("enabled", "false"))
            .and(query_param("learning_type", "self_reflection"))
            .and(query_param("sort_by", "activation_count"))
            .and(query_param("sort_order", "desc"))
            .and(query_param("page_size", "100"))
            .respond_with(ResponseTemplate::new(200).set_body_string(format!(
                r#"{{"success":true,"total":1,"page":1,"page_size":100,"data":[{PATH_ROW}]}}"#
            )))
            .expect(1)
            .mount(&server)
            .await;
        let q = MaiBotBehaviorQuery {
            page: 0,
            page_size: 500,
            search: "  ".into(),
            chat_id: GLOBAL_CHAT.into(),
            filter: MaiBotBehaviorFilter::Disabled,
            origin: MaiBotBehaviorOrigin::SelfReflection,
            sort: MaiBotBehaviorSort::Used,
        };
        let page = list(&client(&server), &q).await.unwrap();
        assert_eq!((page.total, page.items.len()), (1, 1));
    }

    #[tokio::test]
    async fn overview_counts_both_states_and_names_the_global_bucket() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/webui/behavior/chats"))
            .respond_with(ResponseTemplate::new(200).set_body_string(
                r#"{"success":true,"data":[
                    {"session_id":"s1","display_name":"麦麦测试群","chat_type":"group","path_count":7},
                    {"session_id":"","display_name":"全局行为","chat_type":"","path_count":2}
                ]}"#,
            ))
            .mount(&server)
            .await;
        for (flag, total) in [("true", 6), ("false", 3)] {
            Mock::given(method("GET"))
                .and(path("/api/webui/behavior/paths"))
                .and(query_param("enabled", flag))
                .respond_with(ResponseTemplate::new(200).set_body_string(format!(
                    r#"{{"success":true,"total":{total},"page":1,"page_size":1,"data":[{{"id":1}}]}}"#
                )))
                .mount(&server)
                .await;
        }
        let ov = overview(&client(&server)).await.unwrap();
        assert_eq!((ov.total, ov.enabled, ov.disabled), (9, 6, 3));
        assert_eq!(ov.chats[0].chat_id, "s1");
        assert!(ov.chats[0].is_group);
        assert_eq!((ov.chats[1].chat_id.as_str(), ov.chats[1].count), (GLOBAL_CHAT, 2));
    }

    #[tokio::test]
    async fn detail_puts_newest_first_and_skips_odd_entries() {
        let server = MockServer::start().await;
        let body = format!(
            r#"{{"success":true,"data":{{"path":{PATH_ROW},"scene_cluster":{{}},
                "evidence":[
                    {{"action":"a1","outcome":"o1","source_ids":["m1","m2"],"actor_type":"other_user","created_at":"2026-09-01T10:00:00"}},
                    "旧数据里的一句话",
                    {{"action":"a2","outcome":"o2","source_ids":["m3"],"actor_type":"maibot_self","created_at":"2026-09-02T10:00:00"}}
                ],
                "feedback":[
                    {{"score_delta":1.0,"status":"success","reason":"对方回了哈哈","outcome":"聊开了","created_at":"2026-09-03T10:00:00"}},
                    {{"score_delta":-0.35,"status":"maintenance_decay","reason":"长期没用","source":"behavior_pattern_maintenance"}},
                    {{"score_delta":0.0,"status":"maintenance_disable","reason":"停用"}}
                ],"nodes":[],"edges":[]}}}}"#
        );
        Mock::given(method("GET"))
            .and(path("/api/webui/behavior/paths/12"))
            .respond_with(ResponseTemplate::new(200).set_body_string(body))
            .mount(&server)
            .await;
        let d = detail(&client(&server), 12).await.unwrap();
        assert_eq!(d.item.id, 12);
        let ev: Vec<_> = d.evidence.iter().map(|e| (e.action.as_str(), e.messages, e.actor)).collect();
        assert_eq!(ev, [("a2", 1, MaiBotBehaviorActor::Maibot), ("a1", 2, MaiBotBehaviorActor::Others)]);
        let kinds: Vec<_> = d.feedback.iter().map(|f| f.kind).collect();
        assert_eq!(
            kinds,
            [
                MaiBotBehaviorFeedbackKind::Disabled,
                MaiBotBehaviorFeedbackKind::Decay,
                MaiBotBehaviorFeedbackKind::Success,
            ]
        );
        assert!(d.feedback[1].at.is_none());
    }

    #[tokio::test]
    async fn missing_path_is_an_error_not_an_empty_item() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/webui/behavior/paths/99"))
            .respond_with(ResponseTemplate::new(404).set_body_string(r#"{"detail":"行为经验路径不存在"}"#))
            .mount(&server)
            .await;
        let err = detail(&client(&server), 99).await.unwrap_err();
        assert!(err.to_string().contains("行为经验路径不存在"), "{err}");
    }
}
