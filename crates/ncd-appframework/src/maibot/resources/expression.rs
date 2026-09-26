//! 学到的表达方式，`/api/webui/expression`。麦麦从聊天里学「什么情况下可以怎么说」；
//! 默认配置（`expression.expression_checked_only`）下只有人工精选过的才会用在回复里。

use ncd_traits::AppFrameworkError;
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::json;
use ts_rs::TS;

use super::{MaiBotLearningChat, MaiBotResourceDone, UpstreamChat, UpstreamData, UpstreamMessage, UpstreamPage, text};
use crate::maibot::webui_client::{MaiBotWebUi, Request};

const BASE: &str = "/api/webui/expression";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum MaiBotExpressionFilter {
    #[default]
    All,
    Curated,
    Uncurated,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotExpressionQuery {
    pub page: u32,
    pub page_size: u32,
    pub search: String,
    /// 空串是全部聊天
    pub chat_id: String,
    pub filter: MaiBotExpressionFilter,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotExpression {
    #[ts(type = "number")]
    pub id: i64,
    /// 什么情况下
    pub situation: String,
    /// 可以怎么说
    pub style: String,
    /// 空串是旧数据里的全局表达
    pub chat_id: String,
    pub chat_name: String,
    /// 人工精选过（上游 checked 且 modified_by=user）
    pub curated: bool,
    /// 秒级时间戳：上次用到、学到或改过
    pub last_active: f64,
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotExpressionPage {
    pub total: u32,
    pub items: Vec<MaiBotExpression>,
}

/// 页面开着要的其余东西：聊天列表、各筛选的条数
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotExpressionOverview {
    /// 麦麦见过的所有聊天，新建时从这里挑
    pub chats: Vec<MaiBotLearningChat>,
    /// 其中学到过表达的，筛选下拉只给这些
    pub used_chat_ids: Vec<String>,
    pub total: u32,
    pub curated: u32,
    pub uncurated: u32,
    pub recent_7days: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "op", rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum MaiBotExpressionAction {
    Create {
        situation: String,
        style: String,
        chat_id: String,
    },
    /// chat_id 不改就给 null：上游改聊天要求聊天还在，全局的旧数据发空串会被拒
    Update {
        #[ts(type = "number")]
        id: i64,
        situation: String,
        style: String,
        chat_id: Option<String>,
    },
    Curate {
        #[ts(type = "number[]")]
        ids: Vec<i64>,
        curated: bool,
    },
    Delete {
        #[ts(type = "number[]")]
        ids: Vec<i64>,
    },
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct UpstreamExpression {
    id: Option<i64>,
    situation: Option<String>,
    style: Option<String>,
    last_active_time: Option<f64>,
    chat_id: Option<String>,
    chat_name: Option<String>,
    create_date: Option<f64>,
    checked: Option<bool>,
    modified_by: Option<String>,
}

impl UpstreamExpression {
    fn into_item(self) -> Option<MaiBotExpression> {
        let curated = self.checked.unwrap_or(false)
            && self.modified_by.as_deref().is_some_and(|m| m.eq_ignore_ascii_case("user"));
        let chat_id = text(self.chat_id);
        Some(MaiBotExpression {
            id: self.id?,
            situation: text(self.situation),
            style: text(self.style),
            chat_name: self.chat_name.filter(|n| !n.is_empty()).unwrap_or_else(|| {
                if chat_id.is_empty() { "全局".into() } else { chat_id.clone() }
            }),
            chat_id,
            curated,
            last_active: self.last_active_time.unwrap_or(0.0),
            created: self.create_date,
        })
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct UpstreamSummary {
    total: Option<u32>,
    recent_7days: Option<u32>,
}

fn review_filter(f: MaiBotExpressionFilter) -> &'static str {
    match f {
        MaiBotExpressionFilter::All => "all",
        MaiBotExpressionFilter::Curated => "user_checked",
        MaiBotExpressionFilter::Uncurated => "unchecked",
    }
}

async fn page(
    c: &MaiBotWebUi,
    q: &MaiBotExpressionQuery,
) -> Result<UpstreamPage<UpstreamExpression>, AppFrameworkError> {
    let page = q.page.max(1).to_string();
    let size = q.page_size.clamp(1, 100).to_string();
    let search = q.search.trim();
    let mut query = vec![("page", page.as_str()), ("page_size", size.as_str()), ("review_filter", review_filter(q.filter))];
    if !search.is_empty() {
        query.push(("search", search));
    }
    if !q.chat_id.is_empty() {
        query.push(("chat_id", q.chat_id.as_str()));
    }
    c.get(&format!("{BASE}/list"), &query).await
}

pub(crate) async fn list(c: &MaiBotWebUi, q: &MaiBotExpressionQuery) -> Result<MaiBotExpressionPage, AppFrameworkError> {
    let up = page(c, q).await?;
    Ok(MaiBotExpressionPage {
        total: up.total.unwrap_or(0),
        items: up.data.unwrap_or_default().into_iter().filter_map(UpstreamExpression::into_item).collect(),
    })
}

pub(crate) async fn overview(c: &MaiBotWebUi) -> Result<MaiBotExpressionOverview, AppFrameworkError> {
    let all: UpstreamData<Vec<UpstreamChat>> = c.get(&format!("{BASE}/chat-targets"), &[]).await?;
    let used: UpstreamData<Vec<UpstreamChat>> = c.get(&format!("{BASE}/chats"), &[]).await?;
    let summary: UpstreamData<UpstreamSummary> = c.get(&format!("{BASE}/stats/summary"), &[]).await?;
    // 精选 / 未精选的条数按列表同一套范围数：上游 /review/stats 不按账号过滤，和列表对不上
    let count = |filter| MaiBotExpressionQuery { page: 1, page_size: 1, search: String::new(), chat_id: String::new(), filter };
    let curated = page(c, &count(MaiBotExpressionFilter::Curated)).await?.total.unwrap_or(0);
    let uncurated = page(c, &count(MaiBotExpressionFilter::Uncurated)).await?.total.unwrap_or(0);
    let summary = summary.data.unwrap_or_default();
    Ok(MaiBotExpressionOverview {
        chats: all.data.unwrap_or_default().into_iter().filter_map(UpstreamChat::into_chat).collect(),
        used_chat_ids: used
            .data
            .unwrap_or_default()
            .into_iter()
            .filter_map(|c| c.into_chat().map(|c| c.chat_id))
            .collect(),
        total: summary.total.unwrap_or(0),
        curated,
        uncurated,
        recent_7days: summary.recent_7days.unwrap_or(0),
    })
}

fn required(situation: &str, style: &str) -> Result<(), AppFrameworkError> {
    if situation.trim().is_empty() || style.trim().is_empty() {
        return Err(AppFrameworkError::Validation("情境和说法都要填".into()));
    }
    Ok(())
}

pub(crate) async fn act(c: &MaiBotWebUi, a: &MaiBotExpressionAction) -> Result<MaiBotResourceDone, AppFrameworkError> {
    match a {
        MaiBotExpressionAction::Create { situation, style, chat_id } => {
            required(situation, style)?;
            let body = json!({ "situation": situation.trim(), "style": style.trim(), "chat_id": chat_id });
            // 上游这条要带尾斜杠，不带会落到网页的兜底路由上回 405
            let up: UpstreamMessage = c.call(Request::new(Method::POST, &format!("{BASE}/")).body(&body)).await?;
            Ok(done(1, up, "加好了"))
        }
        MaiBotExpressionAction::Update { id, situation, style, chat_id } => {
            required(situation, style)?;
            let mut body = json!({ "situation": situation.trim(), "style": style.trim() });
            if let Some(chat_id) = chat_id {
                body["chat_id"] = json!(chat_id);
            }
            let up: UpstreamMessage =
                c.call(Request::new(Method::PATCH, &format!("{BASE}/{id}")).body(&body)).await?;
            Ok(done(1, up, "改好了"))
        }
        MaiBotExpressionAction::Curate { ids, curated } => {
            // 上游只有逐条改精选；/review/batch 里「不通过」会把那条删掉，不能拿来取消精选
            let body = json!({ "approved": curated });
            for id in ids {
                let path = format!("{BASE}/{id}/review-status");
                c.send(Request::new(Method::PATCH, &path).body(&body)).await?;
            }
            let message = if *curated { "已精选" } else { "已取消精选" };
            Ok(MaiBotResourceDone { affected: ids.len() as u32, message: message.into() })
        }
        MaiBotExpressionAction::Delete { ids } => match ids.as_slice() {
            [] => Ok(MaiBotResourceDone { affected: 0, message: String::new() }),
            [id] => {
                let up: UpstreamMessage = c.call(Request::new(Method::DELETE, &format!("{BASE}/{id}"))).await?;
                Ok(done(1, up, "删掉了"))
            }
            _ => {
                let body = json!({ "ids": ids });
                let up: UpstreamMessage =
                    c.call(Request::new(Method::POST, &format!("{BASE}/batch/delete")).body(&body)).await?;
                Ok(done(ids.len() as u32, up, "删掉了"))
            }
        },
    }
}

fn done(affected: u32, up: UpstreamMessage, fallback: &str) -> MaiBotResourceDone {
    MaiBotResourceDone {
        affected: up.deleted_count.unwrap_or(affected),
        message: up.message.filter(|m| !m.is_empty()).unwrap_or_else(|| fallback.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upstream_rows_map_to_items() {
        let raw = r#"{"success":true,"total":2,"page":1,"page_size":20,"data":[
            {"id":7,"situation":"有人夸你","style":"嘿嘿谢谢","last_active_time":1790000000.5,"chat_id":"ab","chat_name":"麦麦测试群","create_date":null,"checked":true,"modified_by":"user"},
            {"id":8,"situation":"被问到不会的","style":"这个我不太懂","last_active_time":null,"chat_id":"","chat_name":null,"checked":true,"modified_by":"ai"}
        ]}"#;
        let up: UpstreamPage<UpstreamExpression> = serde_json::from_str(raw).unwrap();
        let items: Vec<_> = up.data.unwrap().into_iter().filter_map(UpstreamExpression::into_item).collect();
        assert_eq!(items.len(), 2);
        assert!(items[0].curated);
        assert_eq!(items[0].chat_name, "麦麦测试群");
        // AI 过审的不算精选；全局的旧数据给个名字
        assert!(!items[1].curated);
        assert_eq!(items[1].chat_name, "全局");
        assert_eq!(items[1].last_active, 0.0);
    }

    #[test]
    fn filters_map_to_upstream_values() {
        assert_eq!(review_filter(MaiBotExpressionFilter::All), "all");
        assert_eq!(review_filter(MaiBotExpressionFilter::Curated), "user_checked");
        assert_eq!(review_filter(MaiBotExpressionFilter::Uncurated), "unchecked");
    }
}
