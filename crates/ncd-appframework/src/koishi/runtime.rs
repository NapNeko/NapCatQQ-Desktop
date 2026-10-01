//! Koishi 专有的运行期能力：Bot 在线 / 性能（控制台 `status` 推送）、插件 schema 与已装列表（实例目录里现取）、
//! 重启 worker。类型是 Koishi 专有的，不进通用 trait 的方法签名，编排层经 `koishi_runtime()` 拿。

use std::time::Duration;

use async_trait::async_trait;
use ncd_domain::AppInstance;
use ncd_host::Host;
use ncd_traits::AppFrameworkError;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use super::probe::{KoishiPackageInfo, KoishiPluginSchema};

/// 概览卡片能不能画：没在跑、连不上控制台、控制台开了登录
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/koishi/")]
pub enum KoishiRuntimeGate {
    Ok,
    NotRunning,
    Unreachable,
    Auth,
}

/// 上游 `Universal.Status`（0 离线 / 1 在线 / 2 连接中 / 3 断开中 / 4 重连中）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/koishi/")]
pub enum KoishiBotState {
    Offline,
    Online,
    Connect,
    Disconnect,
    Reconnect,
}

impl KoishiBotState {
    pub fn from_code(code: u64) -> Self {
        match code {
            1 => Self::Online,
            2 => Self::Connect,
            3 => Self::Disconnect,
            4 => Self::Reconnect,
            _ => Self::Offline,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/koishi/")]
pub struct KoishiBotStatus {
    /// 上游的 sid（`平台:账号`）
    pub sid: String,
    pub platform: String,
    pub self_id: String,
    pub name: String,
    pub avatar: String,
    pub state: KoishiBotState,
    pub error: Option<String>,
    #[ts(type = "number")]
    pub message_sent: u64,
    #[ts(type = "number")]
    pub message_received: u64,
    /// 这个 Bot 挂在哪个插件条目下（插件标识）
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/koishi/")]
pub struct KoishiRuntimeStatus {
    pub gate: KoishiRuntimeGate,
    pub message: Option<String>,
    pub bots: Vec<KoishiBotStatus>,
    /// 内存占用：[本进程占总内存, 系统已用]，0~1
    pub memory: Option<Vec<f64>>,
    /// CPU：[本进程, 系统]，0~1
    pub cpu: Option<Vec<f64>>,
}

impl KoishiRuntimeStatus {
    pub fn gate(gate: KoishiRuntimeGate, message: impl Into<String>) -> Self {
        Self {
            gate,
            message: Some(message.into()),
            bots: Vec::new(),
            memory: None,
            cpu: None,
        }
    }

    pub fn not_running() -> Self {
        Self {
            gate: KoishiRuntimeGate::NotRunning,
            message: None,
            bots: Vec::new(),
            memory: None,
            cpu: None,
        }
    }
}

fn str_of(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_default()
}

fn pair(v: Option<&Value>) -> Option<Vec<f64>> {
    let arr = v?.as_array()?;
    Some(arr.iter().filter_map(Value::as_f64).collect())
}

/// 控制台 `status` 推送 → 概览要的样子。旧版 Login 没有 `user`，名字 / 头像在顶层
pub fn parse_status(value: &Value) -> KoishiRuntimeStatus {
    let mut bots: Vec<KoishiBotStatus> = value
        .get("bots")
        .and_then(Value::as_object)
        .map(|m| {
            m.iter()
                .map(|(sid, b)| {
                    let user = b.get("user").cloned().unwrap_or(Value::Null);
                    let self_id = [str_of(b, "selfId"), str_of(&user, "id")]
                        .into_iter()
                        .find(|s| !s.is_empty())
                        .unwrap_or_default();
                    let name = [str_of(&user, "name"), str_of(b, "name")]
                        .into_iter()
                        .find(|s| !s.is_empty())
                        .unwrap_or_default();
                    let avatar = [str_of(&user, "avatar"), str_of(b, "avatar")]
                        .into_iter()
                        .find(|s| !s.is_empty())
                        .unwrap_or_default();
                    KoishiBotStatus {
                        sid: sid.clone(),
                        platform: str_of(b, "platform"),
                        self_id,
                        name,
                        avatar,
                        state: KoishiBotState::from_code(
                            b.get("status").and_then(Value::as_u64).unwrap_or(0),
                        ),
                        error: b
                            .get("error")
                            .and_then(Value::as_str)
                            .filter(|s| !s.is_empty())
                            .map(str::to_string),
                        message_sent: b.get("messageSent").and_then(Value::as_u64).unwrap_or(0),
                        message_received: b
                            .get("messageReceived")
                            .and_then(Value::as_u64)
                            .unwrap_or(0),
                        paths: b
                            .get("paths")
                            .and_then(Value::as_array)
                            .map(|a| {
                                a.iter()
                                    .filter_map(Value::as_str)
                                    .map(str::to_string)
                                    .collect()
                            })
                            .unwrap_or_default(),
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    bots.sort_by(|a, b| a.sid.cmp(&b.sid));
    KoishiRuntimeStatus {
        gate: KoishiRuntimeGate::Ok,
        message: None,
        bots,
        memory: pair(value.get("memory")),
        cpu: pair(value.get("cpu")),
    }
}

// ---------------------------------------------------------------------------
// 控制台功能的运行期类型（试聊沙盒 / 文件 / 数据库 / 指令）
// ---------------------------------------------------------------------------

/// 上游 sandbox 插件的 Message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/koishi/")]
pub struct KoishiSandboxMessage {
    pub id: String,
    pub user: String,
    pub channel: String,
    pub content: String,
    pub platform: String,
    #[serde(default)]
    #[ts(type = "unknown | null")]
    pub quote: Option<serde_json::Value>,
}

impl KoishiSandboxMessage {
    pub fn from_value(v: &Value) -> Option<Self> {
        let str_of = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("");
        let id = str_of("id");
        if id.is_empty() {
            return None;
        }
        Some(Self {
            id: id.to_string(),
            user: str_of("user").to_string(),
            channel: str_of("channel").to_string(),
            content: str_of("content").to_string(),
            platform: str_of("platform").to_string(),
            quote: v.get("quote").cloned().filter(|q| !q.is_null()),
        })
    }
}

/// explorer 插件的文件树节点
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/koishi/")]
pub struct KoishiFileEntry {
    /// file / directory / symlink
    #[serde(rename = "type")]
    pub entry_type: String,
    pub name: String,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub children: Option<Vec<KoishiFileEntry>>,
}

/// explorer/read 的结果：原始字节在 base64 里，文本编码是上游 chardet 猜的
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/koishi/")]
pub struct KoishiFileContent {
    pub base64: String,
    pub mime: Option<String>,
    pub encoding: Option<String>,
}

/// dataview 的 database 推送里一张表的概况
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/koishi/")]
pub struct KoishiDatabaseTable {
    pub name: String,
    /// 主键列（可能多列）
    pub primary: Vec<String>,
    /// 列定义（上游 model 的 fields，原样透传给前端画图头）
    #[ts(type = "Record<string, unknown>")]
    pub fields: Value,
    /// 行数（stats 没数就是 None）
    #[ts(type = "number | null")]
    pub count: Option<u64>,
}

/// 控制台 entry 推送里一条指令的状态（initial / override 已在后端合成生效值）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/koishi/")]
pub struct KoishiCommandRow {
    pub name: String,
    pub children: Vec<String>,
    /// 是用户在控制台里新建的（区别于插件注册的）
    pub created: bool,
    /// 挂在哪个插件条目下（插件标识）
    pub paths: Vec<String>,
    /// 当前别名（override.aliases 的键）
    pub aliases: Vec<String>,
    /// 生效中的 Command.Config（authority / maxUsage / minInterval / showTip…）
    #[ts(type = "Record<string, unknown>")]
    pub config: Value,
}

/// dataview 的序列化：JSON 里字符串前缀 's'、日期前缀 'd'。入参出参都走它
pub fn dataview_serialize(value: &Value) -> String {
    match value {
        Value::String(s) => serde_json::to_string(&format!("s{s}")).unwrap_or_default(),
        Value::Array(arr) => {
            let inner: Vec<String> = arr.iter().map(dataview_serialize).collect();
            format!("[{}]", inner.join(","))
        }
        Value::Object(map) => {
            let inner: Vec<String> = map
                .iter()
                .map(|(k, v)| format!("{}:{}", serde_json::to_string(k).unwrap_or_default(), dataview_serialize(v)))
                .collect();
            format!("{{{}}}", inner.join(","))
        }
        other => other.to_string(),
    }
}

/// 反过来：dataview 返回的串解析成普通 JSON（日期前缀 'd' 的留下 ISO 文本）
pub fn dataview_deserialize(text: &str) -> Result<Value, AppFrameworkError> {
    let v: Value = serde_json::from_str(text)
        .map_err(|e| AppFrameworkError::Runtime(format!("dataview 返回解不开：{e}")))?;
    Ok(dataview_unwrap(v))
}

fn dataview_unwrap(v: Value) -> Value {
    match v {
        Value::String(s) => {
            if let Some(rest) = s.strip_prefix('s').or_else(|| s.strip_prefix('d')) {
                Value::String(rest.to_string())
            } else {
                Value::String(s)
            }
        }
        Value::Array(arr) => Value::Array(arr.into_iter().map(dataview_unwrap).collect()),
        Value::Object(map) => Value::Object(map.into_iter().map(|(k, v)| (k, dataview_unwrap(v))).collect()),
        other => other,
    }
}

/// 状态推送等多久：连上后它会马上推一次，之后每 5 秒推一次
pub const STATUS_WAIT: Duration = Duration::from_secs(6);

#[async_trait]
pub trait KoishiRuntimeApi: Send + Sync {
    /// 不报错：连不上 / 开了登录都折成 gate
    async fn status(&self, instance: &AppInstance, port: u16) -> KoishiRuntimeStatus;

    /// 插件表单；`names` 里的空串 = 全局设置
    async fn plugin_schemas(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        names: &[String],
    ) -> Result<Vec<KoishiPluginSchema>, AppFrameworkError>;

    /// package.json 里的插件依赖（加插件时从这里挑）
    async fn installed_packages(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
    ) -> Result<Vec<KoishiPackageInfo>, AppFrameworkError>;

    /// 让 worker 按盘上的配置重拉（上游 `manager/app-reload`，daemon 不换，桌面端记的进程不变）
    async fn restart(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        port: u16,
    ) -> Result<(), AppFrameworkError>;

    /// 沙盒发一条消息；`channel` 私聊 `@用户名`、群聊 `#`
    async fn sandbox_send(
        &self,
        instance: &AppInstance,
        port: u16,
        platform: &str,
        user: &str,
        channel: &str,
        content: &str,
    ) -> Result<(), AppFrameworkError>;

    /// 沙盒消息缓冲（这条控制台连接收到的全部），前端轮询
    async fn sandbox_messages(
        &self,
        instance: &AppInstance,
        port: u16,
    ) -> Result<Vec<KoishiSandboxMessage>, AppFrameworkError>;

    /// explorer 插件的文件树（推送里取）
    async fn explorer_tree(
        &self,
        instance: &AppInstance,
        port: u16,
    ) -> Result<Vec<KoishiFileEntry>, AppFrameworkError>;

    async fn explorer_read(
        &self,
        instance: &AppInstance,
        port: u16,
        path: &str,
    ) -> Result<KoishiFileContent, AppFrameworkError>;

    /// 文本直接写，二进制给 base64
    async fn explorer_write(
        &self,
        instance: &AppInstance,
        port: u16,
        path: &str,
        content: &str,
        binary: bool,
    ) -> Result<(), AppFrameworkError>;

    async fn explorer_mkdir(
        &self,
        instance: &AppInstance,
        port: u16,
        path: &str,
    ) -> Result<(), AppFrameworkError>;

    async fn explorer_remove(
        &self,
        instance: &AppInstance,
        port: u16,
        path: &str,
    ) -> Result<(), AppFrameworkError>;

    async fn explorer_rename(
        &self,
        instance: &AppInstance,
        port: u16,
        from: &str,
        to: &str,
    ) -> Result<(), AppFrameworkError>;

    /// 数据库表目录（dataview 的 `database` 推送：字段定义 + 行数）
    async fn database_tables(
        &self,
        instance: &AppInstance,
        port: u16,
    ) -> Result<Vec<KoishiDatabaseTable>, AppFrameworkError>;

    /// 表里的行（dataview 的 `database/get` 监听器，分页）
    async fn database_rows(
        &self,
        instance: &AppInstance,
        port: u16,
        table: &str,
        offset: u64,
        limit: u64,
    ) -> Result<Vec<Value>, AppFrameworkError>;

    /// 指令列表（控制台 `entry` 推送里 data 带 `paths` / `initial` 的那份）
    async fn commands(
        &self,
        instance: &AppInstance,
        port: u16,
    ) -> Result<Vec<KoishiCommandRow>, AppFrameworkError>;

    /// 改指令配置（authority / maxUsage / minInterval…），上游 `command/update`
    async fn command_update(
        &self,
        instance: &AppInstance,
        port: u16,
        name: &str,
        config: Value,
    ) -> Result<(), AppFrameworkError>;

    /// 整表替换别名，上游 `command/aliases`
    async fn command_aliases(
        &self,
        instance: &AppInstance,
        port: u16,
        name: &str,
        aliases: Vec<String>,
    ) -> Result<(), AppFrameworkError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn status_push_maps_bots_and_load() {
        let v = json!({
            "memory": [0.01, 0.47],
            "cpu": [0.0, 0.12],
            "bots": {
                "onebot:10001": {
                    "platform": "onebot", "selfId": "10001", "status": 1,
                    "user": {"id": "10001", "name": "小k", "avatar": "http://a"},
                    "messageSent": 3, "messageReceived": 9, "paths": ["ncd-link"]
                },
                "sandbox:x": {"platform": "sandbox", "status": 0, "name": "old", "error": ""}
            }
        });
        let s = parse_status(&v);
        assert_eq!(s.gate, KoishiRuntimeGate::Ok);
        assert_eq!(s.bots.len(), 2);
        let ob = &s.bots[0];
        assert_eq!(ob.sid, "onebot:10001");
        assert_eq!(ob.state, KoishiBotState::Online);
        assert_eq!((ob.name.as_str(), ob.self_id.as_str()), ("小k", "10001"));
        assert_eq!(ob.message_received, 9);
        assert_eq!(ob.paths, vec!["ncd-link".to_string()]);
        assert_eq!(s.bots[1].name, "old");
        assert_eq!(s.bots[1].error, None);
        assert_eq!(s.memory, Some(vec![0.01, 0.47]));
        assert_eq!(parse_status(&json!({})).bots.len(), 0);
    }

    #[test]
    fn dataview_codec_roundtrip() {
        // 字符串前缀 s、日期前缀 d（上游 serialize 的格式）
        let v = json!({ "name": "user", "n": 3, "tags": ["a", "sB"], "at": null });
        let s = dataview_serialize(&v);
        // 键不动，值里的字符串带 s 前缀；本来就 s 开头的叠成 ss
        assert!(s.contains(r#""name":"suser""#), "{s}");
        assert!(s.contains(r#""ssB""#), "{s}");
        let back = dataview_deserialize(&s).unwrap();
        assert_eq!(back, v);
        // 日期串解成 ISO 文本（不带 d 前缀）
        let rows = dataview_deserialize(r#"[{"createdAt":"d2026-10-01T08:10:40.031Z"}]"#).unwrap();
        assert_eq!(rows[0]["createdAt"], json!("2026-10-01T08:10:40.031Z"));
        assert!(dataview_deserialize("not json").is_err());
    }

    #[test]
    fn sandbox_message_from_value() {
        let m = KoishiSandboxMessage::from_value(&json!({
            "id": "m1", "user": "koishi", "channel": "@Alice", "content": "hi", "platform": "sandbox:x",
        }))
        .unwrap();
        assert_eq!(m.user, "koishi");
        assert_eq!(m.quote, None);
        assert!(KoishiSandboxMessage::from_value(&json!({"user": "x"})).is_none());
    }
}
