//! 运行中才有的东西：状态、用量、聊过的会话、提供商探测、MCP 连接。全走麦麦 WebUI 的接口，不读它的库。
//!
//! 上游回的 JSON 字段多、跨版本会加字段，还会在字符串位置给 null。先用宽松的 `Upstream*`
//! 收（都是 Option，多给的键丢掉），再换成对外的强类型；对外的类型不带 null。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum MaiBotRuntimeGate {
    Ok,
    NotRunning,
    /// token 不对：`data/webui.json` 被改过
    Auth,
    /// 进程在跑但 WebUI 没应答：刚启动还没起来，或者 WebUI 被关了
    Unreachable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotRuntimeStatus {
    pub gate: MaiBotRuntimeGate,
    pub message: String,
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// 这次启动以来的秒数；从 WebUI 点重启后从 0 重新算
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uptime_secs: Option<f64>,
}

impl MaiBotRuntimeStatus {
    pub fn not_running() -> Self {
        Self::gate(MaiBotRuntimeGate::NotRunning, "启动麦麦后才能看运行状态")
    }

    pub fn gate(gate: MaiBotRuntimeGate, message: impl Into<String>) -> Self {
        Self {
            gate,
            message: message.into(),
            version: None,
            uptime_secs: None,
        }
    }
}

/// `GET /statistics/summary?hours=N` 里要用的几项
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotStatsSummary {
    pub total_messages: u32,
    pub total_replies: u32,
    pub total_requests: u32,
    #[ts(type = "number")]
    pub total_tokens: u64,
    /// 元，按模型页填的价格算；没填价格就是 0
    pub total_cost: f64,
    /// 秒
    pub avg_response_time: f64,
}

/// 麦麦见过的一个聊天（群或私聊），给会话规则页选目标用
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotChatSession {
    pub session_id: String,
    pub display_name: String,
    /// group / private
    pub chat_type: String,
    /// 群号或对方账号，和配置里的 item_id 同一个东西
    pub target_id: String,
    pub platform: String,
    pub message_count: u32,
    /// 秒级时间戳
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_active_at: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotProviderModel {
    pub id: String,
    /// 服务商给的展示名，没有就和 id 一样
    pub name: String,
}

/// 测连接：先看地址通不通，给了 Key 再拿模型列表验 Key
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotProviderCheck {
    pub network_ok: bool,
    /// 缺省 = 没法判断（没填 Key，或服务商不支持列模型）
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key_valid: Option<bool>,
    #[ts(optional)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<f64>,
    /// 空串 = 没出错
    pub error: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotMcpServerStatus {
    pub name: String,
    pub transport: String,
    pub connected: bool,
    pub tool_count: u32,
    pub error: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotMcpStatus {
    /// MCP 还在初始化时是 false，这时 servers 是空的，不代表都断了
    pub initialized: bool,
    pub servers: Vec<MaiBotMcpServerStatus>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotMcpTool {
    pub name: String,
    pub title: String,
    pub description: String,
}

/// 用表单里的草稿连一次、列出工具就断开，不动麦麦正在用的连接
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotMcpTest {
    pub success: bool,
    pub error: String,
    pub tools: Vec<MaiBotMcpTool>,
}

fn text(v: Option<String>) -> String {
    v.unwrap_or_default()
}

/// `GET /system/status`
#[derive(Debug, Deserialize)]
pub(crate) struct UpstreamStatus {
    pub version: Option<String>,
    pub uptime: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct UpstreamSession {
    session_id: Option<String>,
    display_name: Option<String>,
    chat_type: Option<String>,
    target_id: Option<String>,
    platform: Option<String>,
    message_count: Option<u32>,
    last_active_at: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct UpstreamSessions {
    #[serde(default)]
    sessions: Vec<UpstreamSession>,
}

impl UpstreamSessions {
    pub fn into_sessions(self) -> Vec<MaiBotChatSession> {
        self.sessions
            .into_iter()
            .filter_map(|s| {
                let session_id = s.session_id.filter(|id| !id.is_empty())?;
                Some(MaiBotChatSession {
                    session_id,
                    display_name: text(s.display_name),
                    chat_type: s.chat_type.unwrap_or_else(|| "group".into()),
                    target_id: text(s.target_id),
                    platform: text(s.platform),
                    message_count: s.message_count.unwrap_or(0),
                    last_active_at: s.last_active_at,
                })
            })
            .collect()
    }
}

#[derive(Debug, Default, Deserialize)]
struct UpstreamModel {
    id: Option<String>,
    name: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct UpstreamModelList {
    #[serde(default)]
    models: Vec<UpstreamModel>,
}

impl UpstreamModelList {
    pub fn into_models(self) -> Vec<MaiBotProviderModel> {
        self.models
            .into_iter()
            .filter_map(|m| {
                let id = m.id.filter(|id| !id.is_empty())?;
                let name = m
                    .name
                    .filter(|n| !n.is_empty())
                    .unwrap_or_else(|| id.clone());
                Some(MaiBotProviderModel { id, name })
            })
            .collect()
    }
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct UpstreamProviderCheck {
    network_ok: Option<bool>,
    api_key_valid: Option<bool>,
    latency_ms: Option<f64>,
    error: Option<String>,
}

impl From<UpstreamProviderCheck> for MaiBotProviderCheck {
    fn from(c: UpstreamProviderCheck) -> Self {
        Self {
            network_ok: c.network_ok.unwrap_or(false),
            api_key_valid: c.api_key_valid,
            latency_ms: c.latency_ms,
            error: text(c.error),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct UpstreamMcpServer {
    name: Option<String>,
    transport: Option<String>,
    connected: Option<bool>,
    tool_count: Option<u32>,
    error: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct UpstreamMcpStatus {
    initialized: Option<bool>,
    #[serde(default)]
    servers: Vec<UpstreamMcpServer>,
}

impl From<UpstreamMcpStatus> for MaiBotMcpStatus {
    fn from(s: UpstreamMcpStatus) -> Self {
        Self {
            initialized: s.initialized.unwrap_or(false),
            servers: s
                .servers
                .into_iter()
                .map(|v| MaiBotMcpServerStatus {
                    name: text(v.name),
                    transport: text(v.transport),
                    connected: v.connected.unwrap_or(false),
                    tool_count: v.tool_count.unwrap_or(0),
                    error: text(v.error),
                })
                .collect(),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct UpstreamMcpTool {
    name: Option<String>,
    title: Option<String>,
    description: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct UpstreamMcpTest {
    success: Option<bool>,
    error: Option<String>,
    #[serde(default)]
    tools: Vec<UpstreamMcpTool>,
}

impl From<UpstreamMcpTest> for MaiBotMcpTest {
    fn from(t: UpstreamMcpTest) -> Self {
        Self {
            success: t.success.unwrap_or(false),
            error: text(t.error),
            tools: t
                .tools
                .into_iter()
                .map(|x| MaiBotMcpTool {
                    name: text(x.name),
                    title: text(x.title),
                    description: text(x.description),
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upstream_payloads_tolerate_nulls_and_extra_keys() {
        let s: UpstreamSessions = serde_json::from_str(
            r#"{"success":true,"total":2,"sessions":[
            {"id":3,"session_id":"abc","display_name":"测试群","chat_type":"group","target_id":"123",
             "platform":"qq","message_count":42,"group_name":null,"last_active_at":1790000000.5},
            {"id":4,"session_id":null,"display_name":null}]}"#,
        )
        .unwrap();
        let sessions = s.into_sessions();
        assert_eq!(sessions.len(), 1, "没有 session_id 的丢掉");
        assert_eq!(sessions[0].target_id, "123");
        assert_eq!(sessions[0].message_count, 42);

        let m: UpstreamModelList = serde_json::from_str(
            r#"{"success":true,"models":[{"id":"deepseek-chat","owned_by":null},{"id":""}],"count":2}"#,
        )
        .unwrap();
        assert_eq!(
            m.into_models(),
            vec![MaiBotProviderModel {
                id: "deepseek-chat".into(),
                name: "deepseek-chat".into()
            }]
        );

        let c: UpstreamProviderCheck = serde_json::from_str(
            r#"{"network_ok":true,"api_key_valid":null,"latency_ms":12.5,"error":null,"http_status":404}"#,
        )
        .unwrap();
        let c = MaiBotProviderCheck::from(c);
        assert!(c.network_ok && c.api_key_valid.is_none() && c.error.is_empty());

        let st: MaiBotStatsSummary = serde_json::from_str(
            r#"{"total_requests":5,"total_cost":0.12,"total_tokens":9000,"total_messages":30,"total_replies":7,
            "avg_response_time":2.5,"cache_hit_rate":null}"#,
        )
        .unwrap();
        assert_eq!(
            (st.total_messages, st.total_replies, st.total_tokens),
            (30, 7, 9000)
        );

        let mcp: UpstreamMcpStatus = serde_json::from_str(
            r#"{"initialized":true,"server_count":1,"servers":[{"name":"fs","transport":"stdio","connected":true,
            "protocol_version":"2025-06-18","tool_count":3,"error":""}]}"#,
        )
        .unwrap();
        let mcp = MaiBotMcpStatus::from(mcp);
        assert!(mcp.initialized && mcp.servers[0].connected && mcp.servers[0].tool_count == 3);
    }
}
