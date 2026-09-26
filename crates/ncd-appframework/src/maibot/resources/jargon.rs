//! 学到的黑话，`/api/webui/jargon`。麦麦从聊天里认出圈内词再推它的意思；
//! 算黑话且有含义、又在相关聊天（或全局）里的，才会拿去理解消息。

use ncd_traits::AppFrameworkError;
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::json;
use ts_rs::TS;

use super::{MaiBotLearningChat, MaiBotResourceDone, UpstreamChat, UpstreamData, UpstreamMessage, UpstreamPage, text};
use crate::maibot::webui_client::{MaiBotWebUi, Request};

const BASE: &str = "/api/webui/jargon";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum MaiBotJargonFilter {
    #[default]
    All,
    /// 算黑话、含义也有了
    Confirmed,
    /// AI 判定不是，或还没判
    NotJargon,
    /// 手动建的或固定过含义的
    Pinned,
    Global,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotJargonQuery {
    pub page: u32,
    pub page_size: u32,
    pub search: String,
    /// 空串是全部聊天
    pub chat_id: String,
    pub filter: MaiBotJargonFilter,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotJargon {
    #[ts(type = "number")]
    pub id: i64,
    pub content: String,
    /// 还没推出来时是空串
    pub meaning: String,
    pub chat_ids: Vec<String>,
    pub chat_names: Vec<String>,
    /// 遇见过几次
    pub count: u32,
    /// 算黑话且有含义：这样才会用上
    pub is_jargon: bool,
    pub is_global: bool,
    /// 手动建的或固定过含义：AI 不再改它
    pub pinned: bool,
    /// 推断做完了（遇见 100 次以后不再重推）
    pub complete: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotJargonPage {
    pub total: u32,
    pub items: Vec<MaiBotJargon>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotJargonOverview {
    /// 所有见过的聊天（新建、改适用范围时挑）
    pub chats: Vec<MaiBotLearningChat>,
    /// 其中有黑话的，筛选下拉只给这些
    pub used_chat_ids: Vec<String>,
    pub total: u32,
    pub confirmed: u32,
    pub pinned: u32,
    pub global: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "op", rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum MaiBotJargonAction {
    /// 上游要求至少挑一个真实聊天，全局的也要
    Create {
        content: String,
        meaning: String,
        chat_ids: Vec<String>,
        is_global: bool,
    },
    /// chat_ids 不改就给 null：里面有已经不存在的聊天时，原样发回去会被上游拒
    Update {
        #[ts(type = "number")]
        id: i64,
        content: String,
        meaning: String,
        chat_ids: Option<Vec<String>>,
        is_global: bool,
        is_jargon: bool,
        pinned: bool,
    },
    SetJargon {
        #[ts(type = "number[]")]
        ids: Vec<i64>,
        is_jargon: bool,
    },
    Delete {
        #[ts(type = "number[]")]
        ids: Vec<i64>,
    },
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct UpstreamJargon {
    id: Option<i64>,
    content: Option<String>,
    meaning: Option<String>,
    session_ids: Option<Vec<String>>,
    chat_names: Option<Vec<String>>,
    count: Option<u32>,
    is_jargon: Option<bool>,
    is_complete: Option<bool>,
    is_global: Option<bool>,
    created_by: Option<String>,
}

impl UpstreamJargon {
    fn into_item(self) -> Option<MaiBotJargon> {
        Some(MaiBotJargon {
            id: self.id?,
            content: text(self.content),
            meaning: text(self.meaning),
            chat_ids: self.session_ids.unwrap_or_default(),
            chat_names: self.chat_names.unwrap_or_default(),
            count: self.count.unwrap_or(0),
            is_jargon: self.is_jargon.unwrap_or(false),
            is_global: self.is_global.unwrap_or(false),
            pinned: self.created_by.as_deref().is_some_and(|c| c.eq_ignore_ascii_case("manual")),
            complete: self.is_complete.unwrap_or(false),
        })
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct UpstreamStats {
    total: Option<u32>,
    confirmed_jargon: Option<u32>,
    manual_jargon: Option<u32>,
    global_count: Option<u32>,
}

pub(crate) async fn list(c: &MaiBotWebUi, q: &MaiBotJargonQuery) -> Result<MaiBotJargonPage, AppFrameworkError> {
    let page = q.page.max(1).to_string();
    let size = q.page_size.clamp(1, 100).to_string();
    let search = q.search.trim();
    let mut query = vec![("page", page.as_str()), ("page_size", size.as_str())];
    if !search.is_empty() {
        query.push(("search", search));
    }
    if !q.chat_id.is_empty() {
        query.push(("session_id", q.chat_id.as_str()));
    }
    match q.filter {
        MaiBotJargonFilter::All => {}
        MaiBotJargonFilter::Confirmed => query.push(("jargon_status", "confirmed_jargon")),
        MaiBotJargonFilter::NotJargon => query.push(("jargon_status", "confirmed_not_jargon")),
        MaiBotJargonFilter::Pinned => query.push(("jargon_status", "manual_jargon")),
        MaiBotJargonFilter::Global => query.push(("is_global", "true")),
    }
    let up: UpstreamPage<UpstreamJargon> = c.get(&format!("{BASE}/list"), &query).await?;
    Ok(MaiBotJargonPage {
        total: up.total.unwrap_or(0),
        items: up.data.unwrap_or_default().into_iter().filter_map(UpstreamJargon::into_item).collect(),
    })
}

pub(crate) async fn overview(c: &MaiBotWebUi) -> Result<MaiBotJargonOverview, AppFrameworkError> {
    let path = format!("{BASE}/chats");
    let all: UpstreamData<Vec<UpstreamChat>> = c.get(&path, &[("include_empty", "true")]).await?;
    let used: UpstreamData<Vec<UpstreamChat>> = c.get(&path, &[("include_empty", "false")]).await?;
    let stats: UpstreamData<UpstreamStats> = c.get(&format!("{BASE}/stats/summary"), &[]).await?;
    let stats = stats.data.unwrap_or_default();
    Ok(MaiBotJargonOverview {
        // include_empty 还会带上找不到的孤儿 id，没名字的挑了也会被拒，不给挑
        chats: all
            .data
            .unwrap_or_default()
            .into_iter()
            .filter(|c| c.platform.as_deref().is_some_and(|p| !p.is_empty()))
            .filter_map(UpstreamChat::into_chat)
            .collect(),
        used_chat_ids: used.data.unwrap_or_default().into_iter().filter_map(|c| c.into_chat().map(|c| c.chat_id)).collect(),
        total: stats.total.unwrap_or(0),
        confirmed: stats.confirmed_jargon.unwrap_or(0),
        pinned: stats.manual_jargon.unwrap_or(0),
        global: stats.global_count.unwrap_or(0),
    })
}

pub(crate) async fn act(c: &MaiBotWebUi, a: &MaiBotJargonAction) -> Result<MaiBotResourceDone, AppFrameworkError> {
    match a {
        MaiBotJargonAction::Create { content, meaning, chat_ids, is_global } => {
            if content.trim().is_empty() {
                return Err(AppFrameworkError::Validation("黑话内容不能空".into()));
            }
            if chat_ids.is_empty() {
                return Err(AppFrameworkError::Validation("至少挑一个聊天".into()));
            }
            let body = json!({
                "content": content.trim(),
                "meaning": meaning.trim(),
                "session_ids": chat_ids,
                "is_global": is_global,
            });
            let up: UpstreamMessage = c.call(Request::new(Method::POST, &format!("{BASE}/")).body(&body)).await?;
            Ok(done(1, up, "加好了"))
        }
        MaiBotJargonAction::Update { id, content, meaning, chat_ids, is_global, is_jargon, pinned } => {
            if content.trim().is_empty() {
                return Err(AppFrameworkError::Validation("黑话内容不能空".into()));
            }
            let mut body = json!({
                "content": content.trim(),
                "meaning": meaning.trim(),
                "is_global": is_global,
                "is_jargon": is_jargon,
                "created_by": if *pinned { "MANUAL" } else { "AI" },
            });
            if let Some(ids) = chat_ids {
                if ids.is_empty() {
                    return Err(AppFrameworkError::Validation("至少挑一个聊天".into()));
                }
                body["session_ids"] = json!(ids);
            }
            let up: UpstreamMessage =
                c.call(Request::new(Method::PATCH, &format!("{BASE}/{id}")).body(&body)).await?;
            Ok(done(1, up, "改好了"))
        }
        MaiBotJargonAction::SetJargon { ids, is_jargon } => {
            if ids.is_empty() {
                return Ok(MaiBotResourceDone { affected: 0, message: String::new() });
            }
            // 这条上游走查询参数、没有 body
            let flag = if *is_jargon { "true" } else { "false" };
            let id_strs: Vec<String> = ids.iter().map(i64::to_string).collect();
            let mut query: Vec<(&str, &str)> = id_strs.iter().map(|s| ("ids", s.as_str())).collect();
            query.push(("is_jargon", flag));
            let path = format!("{BASE}/batch/set-jargon");
            let up: UpstreamMessage = c.call(Request::new(Method::POST, &path).query(&query)).await?;
            Ok(done(ids.len() as u32, up, if *is_jargon { "标成黑话了" } else { "标成不是黑话了" }))
        }
        MaiBotJargonAction::Delete { ids } => match ids.as_slice() {
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
        let raw = r#"{"success":true,"total":1,"page":1,"page_size":20,"data":[
            {"id":3,"content":"yyds","meaning":"永远的神","session_id":"a","session_ids":["a","b"],
             "chat_name":"群一","chat_names":["群一","群二"],"count":9,"is_jargon":true,
             "is_legacy_empty_meaning":false,"is_complete":false,"is_global":false,"created_by":"MANUAL",
             "created_timestamp":"2026-09-01 10:00:00.000000","updated_timestamp":"2026-09-02T10:00:00"}
        ]}"#;
        let up: UpstreamPage<UpstreamJargon> = serde_json::from_str(raw).unwrap();
        let item = up.data.unwrap().into_iter().next().and_then(UpstreamJargon::into_item).unwrap();
        assert_eq!(item.content, "yyds");
        assert_eq!(item.chat_ids, ["a", "b"]);
        assert!(item.pinned && item.is_jargon && !item.is_global);
    }

    #[test]
    fn chats_accept_both_key_names_and_skip_blank_ids() {
        let raw = r#"{"success":true,"data":[
            {"session_id":"s1","chat_name":"群一","platform":"qq","is_group":true},
            {"chat_id":"c2","chat_name":null,"platform":null,"is_group":false},
            {"session_id":"","chat_name":"空"}
        ]}"#;
        let up: UpstreamData<Vec<UpstreamChat>> = serde_json::from_str(raw).unwrap();
        let chats: Vec<_> = up.data.unwrap().into_iter().filter_map(UpstreamChat::into_chat).collect();
        assert_eq!(chats.len(), 2);
        assert_eq!((chats[0].chat_id.as_str(), chats[0].is_group), ("s1", true));
        assert_eq!(chats[1].chat_name, "c2");
    }
}
