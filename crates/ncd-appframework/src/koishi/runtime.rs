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
}
