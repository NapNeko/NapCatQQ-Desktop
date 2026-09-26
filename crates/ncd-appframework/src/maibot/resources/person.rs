//! 麦麦认识的人，`/api/webui/person`。跟麦麦说过话的账号一人一条：麦麦怎么称呼 TA、为什么这么叫。
//! 删掉或设成不认识都留不住：对方下次说话会重新登记，称呼也会按昵称重来。

use ncd_traits::AppFrameworkError;
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use ts_rs::TS;

use super::{MaiBotResourceDone, UpstreamData, UpstreamMessage, UpstreamPage, text};
use crate::maibot::webui_client::{MaiBotWebUi, Request};

const BASE: &str = "/api/webui/person";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum MaiBotPersonFilter {
    #[default]
    All,
    Known,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotPersonQuery {
    pub page: u32,
    pub page_size: u32,
    /// 搜称呼、昵称、账号
    pub search: String,
    pub filter: MaiBotPersonFilter,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotGroupCard {
    pub group_id: String,
    pub card: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotPerson {
    /// 上游的人物 id（平台加账号的 md5），改、删都认它
    pub person_id: String,
    /// 麦麦怎么称呼 TA；空串时麦麦用昵称
    pub name: String,
    pub name_reason: String,
    pub platform: String,
    pub user_id: String,
    /// 平台上的昵称，对方改了麦麦会跟着记
    pub nickname: String,
    pub group_cards: Vec<MaiBotGroupCard>,
    pub is_known: bool,
    /// 秒级时间戳
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_seen: Option<f64>,
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_seen: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotPersonPage {
    pub total: u32,
    pub items: Vec<MaiBotPerson>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotPersonOverview {
    pub total: u32,
    pub known: u32,
    pub unknown: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "op", rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum MaiBotPersonAction {
    /// 三项一起发：上游只改带了的键，但 is_known 发 null 会 500，所以一律给值
    Update {
        person_id: String,
        name: String,
        name_reason: String,
        is_known: bool,
    },
    Delete { person_ids: Vec<String> },
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct UpstreamPerson {
    person_id: Option<String>,
    person_name: Option<String>,
    name_reason: Option<String>,
    platform: Option<String>,
    user_id: Option<String>,
    nickname: Option<String>,
    group_nick_name: Option<Vec<UpstreamGroupNick>>,
    is_known: Option<bool>,
    know_since: Option<f64>,
    last_know: Option<f64>,
}

/// 群号在旧数据里可能是数字，照样收
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct UpstreamGroupNick {
    group_id: Option<Value>,
    group_cardname: Option<String>,
}

impl UpstreamPerson {
    fn into_item(self) -> Option<MaiBotPerson> {
        let person_id = self.person_id.filter(|s| !s.is_empty())?;
        let group_cards = self
            .group_nick_name
            .unwrap_or_default()
            .into_iter()
            .filter_map(|g| {
                let group_id = match g.group_id? {
                    Value::String(s) => s,
                    Value::Null => return None,
                    other => other.to_string(),
                };
                let card = g.group_cardname.filter(|c| !c.trim().is_empty())?;
                Some(MaiBotGroupCard { group_id, card })
            })
            .collect();
        Some(MaiBotPerson {
            person_id,
            name: text(self.person_name),
            name_reason: text(self.name_reason),
            platform: text(self.platform),
            user_id: text(self.user_id),
            nickname: text(self.nickname),
            group_cards,
            is_known: self.is_known.unwrap_or(false),
            first_seen: self.know_since,
            last_seen: self.last_know,
        })
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct UpstreamStats {
    total: Option<u32>,
    known: Option<u32>,
    unknown: Option<u32>,
}

/// 人物 id 要拼进路径，只放行字母数字（上游是 32 位 hex）
fn check_person_id(id: &str) -> Result<&str, AppFrameworkError> {
    let ok = !id.is_empty() && id.len() <= 128 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if ok { Ok(id) } else { Err(AppFrameworkError::Validation(format!("人物 id 不对：{id}"))) }
}

pub(crate) async fn list(c: &MaiBotWebUi, q: &MaiBotPersonQuery) -> Result<MaiBotPersonPage, AppFrameworkError> {
    let page = q.page.max(1).to_string();
    let size = q.page_size.clamp(1, 100).to_string();
    let search = q.search.trim();
    let mut query = vec![("page", page.as_str()), ("page_size", size.as_str())];
    if !search.is_empty() {
        query.push(("search", search));
    }
    match q.filter {
        MaiBotPersonFilter::All => {}
        MaiBotPersonFilter::Known => query.push(("is_known", "true")),
        MaiBotPersonFilter::Unknown => query.push(("is_known", "false")),
    }
    let up: UpstreamPage<UpstreamPerson> = c.get(&format!("{BASE}/list"), &query).await?;
    Ok(MaiBotPersonPage {
        total: up.total.unwrap_or(0),
        items: up.data.unwrap_or_default().into_iter().filter_map(UpstreamPerson::into_item).collect(),
    })
}

pub(crate) async fn overview(c: &MaiBotWebUi) -> Result<MaiBotPersonOverview, AppFrameworkError> {
    let stats: UpstreamData<UpstreamStats> = c.get(&format!("{BASE}/stats/summary"), &[]).await?;
    let stats = stats.data.unwrap_or_default();
    let total = stats.total.unwrap_or(0);
    let known = stats.known.unwrap_or(0);
    Ok(MaiBotPersonOverview { total, known, unknown: stats.unknown.unwrap_or(total.saturating_sub(known)) })
}

pub(crate) async fn act(c: &MaiBotWebUi, a: &MaiBotPersonAction) -> Result<MaiBotResourceDone, AppFrameworkError> {
    match a {
        MaiBotPersonAction::Update { person_id, name, name_reason, is_known } => {
            let id = check_person_id(person_id)?;
            let body = json!({
                "person_name": name.trim(),
                "name_reason": name_reason.trim(),
                "is_known": is_known,
            });
            let _: UpstreamMessage = c.call(Request::new(Method::PATCH, &format!("{BASE}/{id}")).body(&body)).await?;
            Ok(MaiBotResourceDone { affected: 1, message: "改好了".into() })
        }
        MaiBotPersonAction::Delete { person_ids } => {
            for id in person_ids {
                check_person_id(id)?;
            }
            match person_ids.as_slice() {
                [] => Ok(MaiBotResourceDone { affected: 0, message: String::new() }),
                [id] => {
                    let _: UpstreamMessage = c.call(Request::new(Method::DELETE, &format!("{BASE}/{id}"))).await?;
                    Ok(MaiBotResourceDone { affected: 1, message: "删掉了".into() })
                }
                _ => {
                    let body = json!({ "person_ids": person_ids });
                    let up: UpstreamMessage =
                        c.call(Request::new(Method::POST, &format!("{BASE}/batch/delete")).body(&body)).await?;
                    let n = up.deleted_count.unwrap_or(person_ids.len() as u32);
                    Ok(MaiBotResourceDone { affected: n, message: format!("删掉了 {n} 个人") })
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upstream_rows_map_to_items() {
        let raw = r#"{"success":true,"total":2,"page":1,"page_size":20,"data":[
            {"id":1,"is_known":true,"person_id":"a1b2","person_name":"小林","name_reason":"群里都这么叫",
             "platform":"qq","user_id":"10001","nickname":"林林","memory_points":null,
             "group_nick_name":[{"group_id":"123","group_cardname":"林·策划"},{"group_id":456,"group_cardname":"林"},
                                {"group_id":"789","group_cardname":" "}],
             "know_times":1,"know_since":1790000000.5,"last_know":null},
            {"id":2,"is_known":false,"person_id":"","person_name":null,"platform":"qq","user_id":"2"}
        ]}"#;
        let up: UpstreamPage<UpstreamPerson> = serde_json::from_str(raw).unwrap();
        let items: Vec<_> = up.data.unwrap().into_iter().filter_map(UpstreamPerson::into_item).collect();
        assert_eq!(items.len(), 1, "没有 person_id 的行改不了也删不了，不给");
        let p = &items[0];
        assert_eq!((p.name.as_str(), p.nickname.as_str()), ("小林", "林林"));
        assert_eq!(p.group_cards, [
            MaiBotGroupCard { group_id: "123".into(), card: "林·策划".into() },
            MaiBotGroupCard { group_id: "456".into(), card: "林".into() },
        ]);
        assert_eq!((p.first_seen, p.last_seen), (Some(1790000000.5), None));
    }

    #[test]
    fn person_ids_are_checked_before_going_into_the_path() {
        assert!(check_person_id("0f3a9c").is_ok());
        assert!(check_person_id("../config").is_err());
        assert!(check_person_id("a/b").is_err());
        assert!(check_person_id("").is_err());
    }
}
