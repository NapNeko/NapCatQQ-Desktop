//! 运行期资源：人格 / 知识库 / 会话规则 / 多配置。只走 Dashboard API。

use ncd_traits::AppFrameworkError;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use ts_rs::TS;

use super::dashboard_client::DashboardClient;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum AstrBotDashboardGate {
    Ok,
    NotRunning,
    Auth,
    Unreachable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AstrBotDashboardStatus {
    pub running: bool,
    pub has_password: bool,
    pub reachable: bool,
    pub authenticated: bool,
    pub gate: AstrBotDashboardGate,
    pub message: String,
}

impl AstrBotDashboardStatus {
    pub fn not_running() -> Self {
        Self {
            running: false,
            has_password: false,
            reachable: false,
            authenticated: false,
            gate: AstrBotDashboardGate::NotRunning,
            message: "启动实例后才能改人格、知识库和会话规则".into(),
        }
    }

    pub fn auth(has_password: bool, message: impl Into<String>) -> Self {
        Self {
            running: true,
            has_password,
            reachable: true,
            authenticated: false,
            gate: AstrBotDashboardGate::Auth,
            message: message.into(),
        }
    }

    pub fn unreachable(has_password: bool, message: impl Into<String>) -> Self {
        Self {
            running: true,
            has_password,
            reachable: false,
            authenticated: false,
            gate: AstrBotDashboardGate::Unreachable,
            message: message.into(),
        }
    }

    pub fn ok() -> Self {
        Self {
            running: true,
            has_password: true,
            reachable: true,
            authenticated: true,
            gate: AstrBotDashboardGate::Ok,
            message: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AstrBotPersona {
    pub persona_id: String,
    pub system_prompt: String,
    pub begin_dialogs: Vec<String>,
    pub folder_id: String,
}

impl Default for AstrBotPersona {
    fn default() -> Self {
        Self {
            persona_id: String::new(),
            system_prompt: String::new(),
            begin_dialogs: Vec::new(),
            folder_id: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AstrBotKnowledgeBase {
    pub kb_id: String,
    pub kb_name: String,
    pub description: String,
    pub embedding_provider_id: String,
}

impl Default for AstrBotKnowledgeBase {
    fn default() -> Self {
        Self {
            kb_id: String::new(),
            kb_name: String::new(),
            description: String::new(),
            embedding_provider_id: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AstrBotKbCreate {
    pub kb_name: String,
    pub description: String,
    pub embedding_provider_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AstrBotSessionRule {
    pub umo: String,
    pub rule_key: String,
    pub rule_json: String,
}

impl Default for AstrBotSessionRule {
    fn default() -> Self {
        Self {
            umo: String::new(),
            rule_key: String::new(),
            rule_json: "{}".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AstrBotAbconfInfo {
    pub id: String,
    pub name: String,
}

impl Default for AstrBotAbconfInfo {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
        }
    }
}

pub async fn login_client(
    instance_id: &str,
    port: u16,
    username: &str,
    password: &str,
) -> Result<DashboardClient, AppFrameworkError> {
    let client = DashboardClient::connect(instance_id, "127.0.0.1", port)?;
    client.login(username, password).await?;
    Ok(client)
}

pub async fn probe_status(
    instance_id: &str,
    port: u16,
    username: &str,
    password: Option<&str>,
) -> AstrBotDashboardStatus {
    let Some(password) = password.filter(|s| !s.is_empty()) else {
        return AstrBotDashboardStatus::auth(false, "没有可用的 WebUI 密码。到连接页写下密码");
    };
    let client = match DashboardClient::connect(instance_id, "127.0.0.1", port) {
        Ok(c) => c,
        Err(e) => return AstrBotDashboardStatus::unreachable(true, e.to_string()),
    };
    if let Err(e) = client.login(username, password).await {
        return match e {
            AppFrameworkError::DashboardAuth(m) => AstrBotDashboardStatus::auth(true, m),
            other => AstrBotDashboardStatus::unreachable(true, other.to_string()),
        };
    }
    // login 可能只命中了进程内缓存；门控必须靠一次真请求确认对面还在
    match client.verify(username, password).await {
        Ok(()) => AstrBotDashboardStatus::ok(),
        Err(AppFrameworkError::DashboardAuth(m)) => AstrBotDashboardStatus::auth(true, m),
        Err(e) => AstrBotDashboardStatus::unreachable(true, e.to_string()),
    }
}

pub async fn list_personas(client: &DashboardClient) -> Result<Vec<AstrBotPersona>, AppFrameworkError> {
    let data: Value = client.get_json("/api/persona/list").await?;
    Ok(parse_persona_list(&data))
}

/// 上游 update 只认 persona_id / system_prompt / begin_dialogs，folder_id 会被忽略；
/// tools / skills 靠「键不在就不动」保留，所以这里绝不能补空数组。
pub async fn upsert_persona(
    client: &DashboardClient,
    persona: &AstrBotPersona,
    creating: bool,
) -> Result<(), AppFrameworkError> {
    let body = json!({
        "persona_id": persona.persona_id,
        "system_prompt": persona.system_prompt,
        "begin_dialogs": persona.begin_dialogs,
        "folder_id": if persona.folder_id.is_empty() { Value::Null } else { Value::String(persona.folder_id.clone()) },
    });
    if creating {
        client.post_ok("/api/persona/create", &body).await
    } else {
        client.post_ok("/api/persona/update", &body).await
    }
}

pub async fn delete_persona(client: &DashboardClient, persona_id: &str) -> Result<(), AppFrameworkError> {
    client
        .post_ok("/api/persona/delete", &json!({ "persona_id": persona_id }))
        .await
}

pub async fn list_kbs(client: &DashboardClient) -> Result<Vec<AstrBotKnowledgeBase>, AppFrameworkError> {
    let pages = fetch_all_pages(client, "/api/kb/list", 100, "items").await?;
    Ok(pages.iter().flat_map(parse_kb_list).collect())
}

pub async fn create_kb(client: &DashboardClient, req: &AstrBotKbCreate) -> Result<(), AppFrameworkError> {
    if req.embedding_provider_id.trim().is_empty() {
        return Err(AppFrameworkError::Validation(
            "创建知识库需要先有 embedding 提供商".into(),
        ));
    }
    client
        .post_ok(
            "/api/kb/create",
            &json!({
                "kb_name": req.kb_name,
                "description": req.description,
                "embedding_provider_id": req.embedding_provider_id,
            }),
        )
        .await
}

pub async fn delete_kb(client: &DashboardClient, kb_id: &str) -> Result<(), AppFrameworkError> {
    client.post_ok("/api/kb/delete", &json!({ "kb_id": kb_id })).await
}

pub async fn list_session_rules(
    client: &DashboardClient,
) -> Result<Vec<AstrBotSessionRule>, AppFrameworkError> {
    // 上游把 page_size 夹到 100，写大了没用
    let pages = fetch_all_pages(client, "/api/session/list-rule", 100, "rules").await?;
    Ok(pages.iter().flat_map(parse_session_rules).collect())
}

pub async fn update_session_rule(
    client: &DashboardClient,
    rule: &AstrBotSessionRule,
) -> Result<(), AppFrameworkError> {
    let value: Value = serde_json::from_str(&rule.rule_json)
        .unwrap_or_else(|_| Value::String(rule.rule_json.clone()));
    client
        .post_ok(
            "/api/session/update-rule",
            &json!({
                "umo": rule.umo,
                "rule_key": rule.rule_key,
                "rule_value": value,
            }),
        )
        .await
}

pub async fn delete_session_rule(
    client: &DashboardClient,
    umo: &str,
    rule_key: &str,
) -> Result<(), AppFrameworkError> {
    client
        .post_ok(
            "/api/session/delete-rule",
            &json!({ "umo": umo, "rule_key": rule_key }),
        )
        .await
}

pub async fn list_abconfs(client: &DashboardClient) -> Result<Vec<AstrBotAbconfInfo>, AppFrameworkError> {
    let data: Value = client.get_json("/api/config/abconfs").await?;
    Ok(parse_abconfs(&data))
}

pub async fn create_abconf(client: &DashboardClient, name: &str) -> Result<String, AppFrameworkError> {
    let data: Value = client
        .post_json("/api/config/abconf/new", &json!({ "name": name }))
        .await?;
    Ok(data
        .get("conf_id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string())
}

pub async fn delete_abconf(client: &DashboardClient, id: &str) -> Result<(), AppFrameworkError> {
    client
        .post_ok("/api/config/abconf/delete", &json!({ "id": id }))
        .await
}

pub async fn write_subagent(client: &DashboardClient, body: &Value) -> Result<(), AppFrameworkError> {
    client.post_ok("/api/subagent/config", body).await
}

pub async fn list_available_tools(client: &DashboardClient) -> Result<Vec<String>, AppFrameworkError> {
    let data: Value = client.get_json("/api/subagent/available-tools").await?;
    Ok(parse_tool_names(&data))
}

pub async fn list_source_models(
    client: &DashboardClient,
    source_id: &str,
) -> Result<Vec<String>, AppFrameworkError> {
    client.provider_source_models(source_id).await
}

fn parse_persona_list(data: &Value) -> Vec<AstrBotPersona> {
    let Some(arr) = data.as_array() else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|row| {
            let obj = row.as_object()?;
            Some(AstrBotPersona {
                persona_id: obj
                    .get("persona_id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                system_prompt: obj
                    .get("system_prompt")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                begin_dialogs: obj
                    .get("begin_dialogs")
                    .and_then(Value::as_array)
                    .map(|a| {
                        a.iter()
                            .filter_map(Value::as_str)
                            .map(ToOwned::to_owned)
                            .collect()
                    })
                    .unwrap_or_default(),
                folder_id: obj
                    .get("folder_id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            })
        })
        .filter(|p| !p.persona_id.is_empty())
        .collect()
}

fn parse_kb_list(data: &Value) -> Vec<AstrBotKnowledgeBase> {
    let Some(arr) = data.get("items").and_then(Value::as_array) else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|row| {
            let obj = row.as_object()?;
            Some(AstrBotKnowledgeBase {
                kb_id: first_str(obj, &["kb_id", "id"]).unwrap_or_default(),
                kb_name: first_str(obj, &["kb_name", "name"]).unwrap_or_default(),
                description: first_str(obj, &["description"]).unwrap_or_default(),
                embedding_provider_id: first_str(obj, &["embedding_provider_id"]).unwrap_or_default(),
            })
        })
        .filter(|k| !k.kb_id.is_empty() || !k.kb_name.is_empty())
        .collect()
}

fn parse_session_rules(data: &Value) -> Vec<AstrBotSessionRule> {
    let Some(arr) = data.get("rules").and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for row in arr {
        let Some(obj) = row.as_object() else {
            continue;
        };
        let umo = first_str(obj, &["umo", "UMO"]).unwrap_or_default();
        if umo.is_empty() {
            continue;
        }
        if let Some(rules) = obj.get("rules").and_then(Value::as_object) {
            for (key, val) in rules {
                out.push(AstrBotSessionRule {
                    umo: umo.clone(),
                    rule_key: key.clone(),
                    rule_json: val.to_string(),
                });
            }
        } else if let Some(key) = first_str(obj, &["rule_key", "key"]) {
            out.push(AstrBotSessionRule {
                umo,
                rule_key: key,
                rule_json: obj
                    .get("rule_value")
                    .or_else(|| obj.get("value"))
                    .map(ToString::to_string)
                    .unwrap_or_else(|| "{}".into()),
            });
        }
    }
    out
}

fn parse_abconfs(data: &Value) -> Vec<AstrBotAbconfInfo> {
    let Some(arr) = data.get("info_list").and_then(Value::as_array) else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|row| {
            let obj = row.as_object()?;
            Some(AstrBotAbconfInfo {
                id: first_str(obj, &["id"]).unwrap_or_default(),
                name: first_str(obj, &["name"]).unwrap_or_default(),
            })
        })
        .filter(|a| !a.id.is_empty())
        .collect()
}

/// 上游列表默认一页 10~20 条，不翻页就只能看到开头。
async fn fetch_all_pages(
    client: &DashboardClient,
    path: &str,
    page_size: usize,
    rows_key: &str,
) -> Result<Vec<Value>, AppFrameworkError> {
    const MAX_PAGES: usize = 50;
    let mut pages = Vec::new();
    let mut seen = 0usize;
    for page in 1..=MAX_PAGES {
        let data: Value = client
            .get_json(&format!("{path}?page={page}&page_size={page_size}"))
            .await?;
        let rows = data
            .get(rows_key)
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        let total = data.get("total").and_then(Value::as_u64).unwrap_or(0) as usize;
        pages.push(data);
        seen += rows;
        if rows == 0 || seen >= total {
            break;
        }
    }
    Ok(pages)
}

fn parse_tool_names(data: &Value) -> Vec<String> {
    let Some(arr) = data.as_array() else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|row| row.get("name").and_then(Value::as_str))
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn first_str(obj: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|k| obj.get(*k).and_then(Value::as_str))
        .map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn logged_in(server: &MockServer, instance_id: &str) -> DashboardClient {
        Mock::given(method("POST"))
            .and(path("/api/auth/login"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "status": "ok",
                "data": { "token": "jwt" }
            })))
            .mount(server)
            .await;
        let port: u16 = server.uri().rsplit(':').next().unwrap().parse().unwrap();
        super::super::dashboard_client::clear_token(instance_id);
        login_client(instance_id, port, "astrbot", "Abcdefg1")
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn abconfs_come_from_info_list() {
        let server = MockServer::start().await;
        let client = logged_in(&server, "rt-abconf").await;
        Mock::given(method("GET"))
            .and(path("/api/config/abconfs"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "status": "ok",
                "data": { "info_list": [
                    { "id": "default", "name": "default", "path": "cmd_config.json" },
                    { "id": "a1b2", "name": "测试组", "path": "cmd_config_a1b2.json" }
                ]}
            })))
            .mount(&server)
            .await;

        let list = list_abconfs(&client).await.unwrap();
        assert_eq!(
            list.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(),
            ["default", "a1b2"]
        );
        assert_eq!(list[1].name, "测试组");
    }

    #[tokio::test]
    async fn session_rules_walk_every_page_and_flatten_each_umo() {
        let server = MockServer::start().await;
        let client = logged_in(&server, "rt-rules").await;
        Mock::given(method("GET"))
            .and(path("/api/session/list-rule"))
            .and(query_param("page", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "status": "ok",
                "data": {
                    "total": 2,
                    "rules": [{
                        "umo": "aiocqhttp:GroupMessage:1",
                        "rules": { "kb_config": { "kb_ids": ["k1"] }, "session_service_config": { "llm_enabled": true } }
                    }]
                }
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/session/list-rule"))
            .and(query_param("page", "2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "status": "ok",
                "data": {
                    "total": 2,
                    "rules": [{
                        "umo": "aiocqhttp:FriendMessage:2",
                        "rules": { "kb_config": { "kb_ids": [] } }
                    }]
                }
            })))
            .mount(&server)
            .await;

        let rules = list_session_rules(&client).await.unwrap();
        assert_eq!(rules.len(), 3, "第二页没拉到，或者一个 UMO 的多条规则没展开");
        assert!(rules.iter().any(|r| r.umo == "aiocqhttp:FriendMessage:2"));
    }

    #[tokio::test]
    async fn kb_list_stops_after_the_last_page() {
        let server = MockServer::start().await;
        let client = logged_in(&server, "rt-kb").await;
        Mock::given(method("GET"))
            .and(path("/api/kb/list"))
            .and(query_param("page", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "status": "ok",
                "data": {
                    "total": 1,
                    "items": [{ "kb_id": "k1", "kb_name": "手册", "description": "", "embedding_provider_id": "e1" }]
                }
            })))
            .mount(&server)
            .await;

        let kbs = list_kbs(&client).await.unwrap();
        assert_eq!(kbs.len(), 1);
        assert_eq!(kbs[0].kb_name, "手册");
    }

    #[tokio::test]
    async fn probe_status_rejects_a_token_the_server_no_longer_accepts() {
        let server = MockServer::start().await;
        let port: u16 = server.uri().rsplit(':').next().unwrap().parse().unwrap();
        Mock::given(method("POST"))
            .and(path("/api/auth/login"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "status": "ok",
                "data": { "token": "jwt" }
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/config/abconfs"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;

        super::super::dashboard_client::clear_token("rt-gate");
        let status = probe_status("rt-gate", port, "astrbot", Some("Abcdefg1")).await;
        assert_eq!(status.gate, AstrBotDashboardGate::Auth);
        assert!(!status.authenticated);
    }
}
