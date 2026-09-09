//! Karin 实例配置的类型化视图：`.env` + `@karinjs/config/{config,adapter,groups,privates,render,redis}.json`。
//!
//! 事实来源 `.references/Karin/packages/core/src/{types/config/*.ts, utils/config/default.ts}` 与
//! WebUI `packages/web/src/components/config/system/env.tsx`（系统键清单）。
//!
//! - JSON 字段名与 Karin 文件一字不差（含 `isLocal` / `userCD` / `isSnapka` 这类驼峰），ts-rs 导出后前端直接对齐上游文档。
//! - 每个结构都带 `#[serde(flatten)] extra` 兜住未知字段：Karin 升级加键不丢；`#[serde(default)]` 兜住老版本缺键。
//! - `pm2.json` 不暴露：进程由桌面端托管。

use std::collections::{BTreeMap, BTreeSet};

use ncd_domain::{AppConfigDocument, AppConfigFormat, AppConfigIssue};
use ncd_host::{Host, HostPath};
use ncd_traits::AppFrameworkError;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use ts_rs::TS;

use crate::adapter::apply_with_backup_ex;
use crate::config_doc::{
    DocumentSnapshot, IssueSink, ensure_parent_dir, read_documents, render_json_pretty,
};
use crate::env_file::{EnvEntry, EnvFile};

use super::manifest::{ENV_HTTP_PORT, ENV_WS_SERVER_AUTH_KEY, KARIN_ENV_FILE};

pub const KARIN_CONFIG_DIR: &str = "@karinjs/config";

pub const DOC_ENV: &str = "env";
pub const DOC_CONFIG: &str = "config";
pub const DOC_ADAPTER: &str = "adapter";
pub const DOC_GROUPS: &str = "groups";
pub const DOC_PRIVATES: &str = "privates";
pub const DOC_RENDER: &str = "render";
pub const DOC_REDIS: &str = "redis";

/// WebUI 认定的系统键（顺序即展示顺序）；其余键归「自定义」
pub const KARIN_SYSTEM_ENV_KEYS: [&str; 18] = [
    "HTTP_ENABLE",
    "HTTP_PORT",
    "HTTP_HOST",
    "HTTP_AUTH_KEY",
    "WS_SERVER_AUTH_KEY",
    "REDIS_ENABLE",
    "PM2_RESTART",
    "TSX_WATCH",
    "LOG_LEVEL",
    "LOG_DAYS_TO_KEEP",
    "LOG_MAX_LOG_SIZE",
    "LOG_FNC_COLOR",
    "LOG_MAX_CONNECTIONS",
    "FFMPEG_PATH",
    "FFPROBE_PATH",
    "FFPLAY_PATH",
    "RUNTIME",
    "NODE_ENV",
];

pub const KARIN_LOG_LEVELS: [&str; 9] = [
    "all", "trace", "debug", "mark", "info", "warn", "error", "fatal", "off",
];
pub const KARIN_RUNTIMES: [&str; 3] = ["node", "pm2", "tsx"];
pub const KARIN_NODE_ENVS: [&str; 3] = ["development", "production", "test"];

type Extra = Map<String, Value>;

// ---------- .env ----------

/// `.env` 一条自定义键（非系统键）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct KarinEnvEntry {
    pub key: String,
    pub value: String,
    /// 键上一行的 `# 注释`
    #[serde(default)]
    pub comment: String,
}

/// `.env` 的类型化视图。系统键各自成字段；`comments` 只读回显上游写在键上一行的说明。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct KarinEnv {
    pub http_enable: bool,
    #[ts(type = "number")]
    pub http_port: u16,
    pub http_host: String,
    pub http_auth_key: String,
    pub ws_server_auth_key: String,
    pub redis_enable: bool,
    pub pm2_restart: bool,
    pub tsx_watch: bool,
    /// all / trace / debug / mark / info / warn / error / fatal / off
    pub log_level: String,
    pub log_days_to_keep: u32,
    /// 0 表示不分割
    pub log_max_log_size: u32,
    pub log_fnc_color: String,
    pub log_max_connections: u32,
    pub ffmpeg_path: String,
    pub ffprobe_path: String,
    pub ffplay_path: String,
    /// node / pm2 / tsx
    pub runtime: String,
    /// development / production / test
    pub node_env: String,
    /// 系统键 → 上游注释（只读，保存时不改）
    pub comments: BTreeMap<String, String>,
    pub custom: Vec<KarinEnvEntry>,
}

impl Default for KarinEnv {
    fn default() -> Self {
        Self {
            http_enable: true,
            http_port: 7777,
            http_host: "0.0.0.0".to_string(),
            http_auth_key: String::new(),
            ws_server_auth_key: String::new(),
            redis_enable: true,
            pm2_restart: true,
            tsx_watch: false,
            log_level: "info".to_string(),
            log_days_to_keep: 7,
            log_max_log_size: 0,
            log_fnc_color: "#E1D919".to_string(),
            log_max_connections: 5,
            ffmpeg_path: String::new(),
            ffprobe_path: String::new(),
            ffplay_path: String::new(),
            runtime: "node".to_string(),
            node_env: "production".to_string(),
            comments: BTreeMap::new(),
            custom: Vec::new(),
        }
    }
}

impl KarinEnv {
    pub fn is_system_key(key: &str) -> bool {
        KARIN_SYSTEM_ENV_KEYS.contains(&key)
    }

    /// 从解析好的 `.env` 取值；缺键 / 解析失败回落默认值（与 Karin WebUI 同策略）
    pub fn from_env_file(env: &EnvFile) -> Self {
        let entries = env.entries();
        let d = Self::default();
        let text = |k: &str| entries.iter().find(|e| e.key == k).map(|e| e.value.clone());
        let str_or = |k: &str, fallback: &str| text(k).unwrap_or_else(|| fallback.to_string());
        let bool_or = |k: &str, fallback: bool| text(k).map_or(fallback, |v| v == "true");
        let u32_or = |k: &str, fallback: u32| {
            text(k)
                .and_then(|v| v.trim().parse::<u32>().ok())
                .unwrap_or(fallback)
        };
        let port = text(ENV_HTTP_PORT)
            .and_then(|v| v.trim().parse::<u16>().ok())
            .unwrap_or(d.http_port);

        let comments = entries
            .iter()
            .filter(|e| Self::is_system_key(&e.key) && !e.comment.is_empty())
            .map(|e| (e.key.clone(), e.comment.clone()))
            .collect();
        let custom = entries
            .iter()
            .filter(|e| !Self::is_system_key(&e.key))
            .map(|e| KarinEnvEntry {
                key: e.key.clone(),
                value: e.value.clone(),
                comment: e.comment.clone(),
            })
            .collect();

        Self {
            http_enable: bool_or("HTTP_ENABLE", d.http_enable),
            http_port: port,
            http_host: str_or("HTTP_HOST", &d.http_host),
            http_auth_key: str_or("HTTP_AUTH_KEY", &d.http_auth_key),
            ws_server_auth_key: str_or(ENV_WS_SERVER_AUTH_KEY, &d.ws_server_auth_key),
            redis_enable: bool_or("REDIS_ENABLE", d.redis_enable),
            pm2_restart: bool_or("PM2_RESTART", d.pm2_restart),
            tsx_watch: bool_or("TSX_WATCH", d.tsx_watch),
            log_level: str_or("LOG_LEVEL", &d.log_level),
            log_days_to_keep: u32_or("LOG_DAYS_TO_KEEP", d.log_days_to_keep),
            log_max_log_size: u32_or("LOG_MAX_LOG_SIZE", d.log_max_log_size),
            log_fnc_color: str_or("LOG_FNC_COLOR", &d.log_fnc_color),
            log_max_connections: u32_or("LOG_MAX_CONNECTIONS", d.log_max_connections),
            ffmpeg_path: str_or("FFMPEG_PATH", &d.ffmpeg_path),
            ffprobe_path: str_or("FFPROBE_PATH", &d.ffprobe_path),
            ffplay_path: str_or("FFPLAY_PATH", &d.ffplay_path),
            runtime: str_or("RUNTIME", &d.runtime),
            node_env: str_or("NODE_ENV", &d.node_env),
            comments,
            custom,
        }
    }

    /// 系统键按字段渲染成 `(KEY, value)`（顺序 = `KARIN_SYSTEM_ENV_KEYS`）
    pub fn system_pairs(&self) -> Vec<(&'static str, String)> {
        let b = |v: bool| if v { "true" } else { "false" }.to_string();
        vec![
            ("HTTP_ENABLE", b(self.http_enable)),
            (ENV_HTTP_PORT, self.http_port.to_string()),
            ("HTTP_HOST", self.http_host.clone()),
            ("HTTP_AUTH_KEY", self.http_auth_key.clone()),
            (ENV_WS_SERVER_AUTH_KEY, self.ws_server_auth_key.clone()),
            ("REDIS_ENABLE", b(self.redis_enable)),
            ("PM2_RESTART", b(self.pm2_restart)),
            ("TSX_WATCH", b(self.tsx_watch)),
            ("LOG_LEVEL", self.log_level.clone()),
            ("LOG_DAYS_TO_KEEP", self.log_days_to_keep.to_string()),
            ("LOG_MAX_LOG_SIZE", self.log_max_log_size.to_string()),
            ("LOG_FNC_COLOR", self.log_fnc_color.clone()),
            ("LOG_MAX_CONNECTIONS", self.log_max_connections.to_string()),
            ("FFMPEG_PATH", self.ffmpeg_path.clone()),
            ("FFPROBE_PATH", self.ffprobe_path.clone()),
            ("FFPLAY_PATH", self.ffplay_path.clone()),
            ("RUNTIME", self.runtime.clone()),
            ("NODE_ENV", self.node_env.clone()),
        ]
    }

    /// 写回：系统键原位改值；自定义键 set/remove（删掉文件里有而这里没有的非系统键）
    pub fn write_into(&self, env: &mut EnvFile) {
        let existing_custom: Vec<String> = env
            .entries()
            .into_iter()
            .filter(|e| !Self::is_system_key(&e.key))
            .map(|e| e.key)
            .collect();
        for (key, value) in self.system_pairs() {
            env.set(key, &value);
        }
        let keep: BTreeSet<&str> = self.custom.iter().map(|e| e.key.as_str()).collect();
        for key in existing_custom {
            if !keep.contains(key.as_str()) {
                env.remove(&key);
            }
        }
        for entry in &self.custom {
            env.set_with_comment(&entry.key, &entry.value, &entry.comment);
        }
    }

    fn validate(&self, sink: &mut IssueSink) {
        if self.http_port == 0 {
            sink.push("env/http_port", "端口需在 1–65535");
        }
        if !KARIN_LOG_LEVELS.contains(&self.log_level.as_str()) {
            sink.push(
                "env/log_level",
                format!("日志等级须是 {} 之一", KARIN_LOG_LEVELS.join(" / ")),
            );
        }
        if !KARIN_RUNTIMES.contains(&self.runtime.as_str()) {
            sink.push(
                "env/runtime",
                format!("运行器须是 {} 之一", KARIN_RUNTIMES.join(" / ")),
            );
        }
        if !KARIN_NODE_ENVS.contains(&self.node_env.as_str()) {
            sink.push(
                "env/node_env",
                format!("NODE_ENV 须是 {} 之一", KARIN_NODE_ENVS.join(" / ")),
            );
        }
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        for (i, entry) in self.custom.iter().enumerate() {
            let path = format!("env/custom/{i}/key");
            let key = entry.key.trim();
            if key.is_empty() {
                sink.push(path, "键不能为空");
                continue;
            }
            if !is_env_key(key) {
                sink.push(path, "键只能含字母、数字、下划线，且不能以数字开头");
                continue;
            }
            if Self::is_system_key(key) {
                sink.push(path, format!("{key} 是系统键，请在上方对应字段修改"));
                continue;
            }
            if !seen.insert(key) {
                sink.push(path, format!("键 {key} 重复"));
            }
        }
    }
}

fn is_env_key(key: &str) -> bool {
    let mut chars = key.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

// ---------- config.json ----------

/// friend / group / directs / guilds / channels 五类事件开关与黑白名单
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct KarinEventScope {
    pub enable: bool,
    pub enable_list: Vec<String>,
    pub disable_list: Vec<String>,
    pub log_enable_list: Vec<String>,
    pub log_disable_list: Vec<String>,
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: Extra,
}

impl Default for KarinEventScope {
    fn default() -> Self {
        Self {
            enable: true,
            enable_list: Vec::new(),
            disable_list: Vec::new(),
            log_enable_list: Vec::new(),
            log_disable_list: Vec::new(),
            extra: Extra::new(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct KarinUserScope {
    pub enable_list: Vec<String>,
    pub disable_list: Vec<String>,
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: Extra,
}

/// `config.json`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct KarinCoreConfig {
    pub master: Vec<String>,
    pub admin: Vec<String>,
    pub user: KarinUserScope,
    pub friend: KarinEventScope,
    pub group: KarinEventScope,
    pub directs: KarinEventScope,
    pub guilds: KarinEventScope,
    pub channels: KarinEventScope,
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: Extra,
}

impl Default for KarinCoreConfig {
    fn default() -> Self {
        Self {
            master: vec!["console".to_string()],
            admin: Vec::new(),
            user: KarinUserScope::default(),
            friend: KarinEventScope::default(),
            group: KarinEventScope::default(),
            directs: KarinEventScope::default(),
            guilds: KarinEventScope::default(),
            channels: KarinEventScope::default(),
            extra: Extra::new(),
        }
    }
}

// ---------- adapter.json ----------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct KarinConsoleAdapter {
    #[serde(rename = "isLocal")]
    pub is_local: bool,
    pub token: String,
    pub host: String,
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: Extra,
}

impl Default for KarinConsoleAdapter {
    fn default() -> Self {
        Self {
            is_local: true,
            token: String::new(),
            host: String::new(),
            extra: Extra::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct KarinOneBotWsServer {
    pub enable: bool,
    /// 秒
    pub timeout: u32,
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: Extra,
}

impl Default for KarinOneBotWsServer {
    fn default() -> Self {
        Self {
            enable: true,
            timeout: 120,
            extra: Extra::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct KarinOneBotWsClient {
    pub enable: bool,
    pub url: String,
    pub token: String,
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: Extra,
}

impl Default for KarinOneBotWsClient {
    fn default() -> Self {
        Self {
            enable: false,
            url: "ws://127.0.0.1:7778".to_string(),
            token: String::new(),
            extra: Extra::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct KarinOneBotHttpServer {
    pub enable: bool,
    pub self_id: String,
    pub url: String,
    /// 旧版单一鉴权键（上游已弃用，保留以免丢）
    pub token: String,
    pub api_token: String,
    pub post_token: String,
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: Extra,
}

impl Default for KarinOneBotHttpServer {
    fn default() -> Self {
        Self {
            enable: false,
            self_id: "default".to_string(),
            url: "http://127.0.0.1:6099".to_string(),
            token: String::new(),
            api_token: String::new(),
            post_token: String::new(),
            extra: Extra::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct KarinOneBotAdapter {
    pub ws_server: KarinOneBotWsServer,
    pub ws_client: Vec<KarinOneBotWsClient>,
    pub http_server: Vec<KarinOneBotHttpServer>,
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: Extra,
}

impl Default for KarinOneBotAdapter {
    fn default() -> Self {
        Self {
            ws_server: KarinOneBotWsServer::default(),
            ws_client: vec![KarinOneBotWsClient::default()],
            http_server: vec![KarinOneBotHttpServer::default()],
            extra: Extra::new(),
        }
    }
}

/// `adapter.json`
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct KarinAdapterConfig {
    pub console: KarinConsoleAdapter,
    pub onebot: KarinOneBotAdapter,
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: Extra,
}

/// 只收 enable 的正向客户端,默认占位 `ws://127.0.0.1:7778` 是关着的
pub fn outbound_onebot_ws_urls(text: &str) -> Vec<String> {
    let Ok(cfg) = serde_json::from_str::<KarinAdapterConfig>(text) else {
        return Vec::new();
    };
    cfg.onebot
        .ws_client
        .into_iter()
        .filter(|c| c.enable)
        .map(|c| c.url.trim().to_string())
        .filter(|u| !u.is_empty())
        .collect()
}

// ---------- groups.json / privates.json ----------

/// 群 / 频道 / 私聊响应规则（groups.json 与 privates.json 共用；私聊没有 userCD / member_*）。
/// `mode`: 0 所有 / 1 仅@ / 2 仅管理员 / 3 仅别名 / 4 别名或@ / 5 管理员无限制 / 6 仅主人
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct KarinScopeRule {
    /// `default` / `global` / `Bot:selfId` / `Bot:selfId:groupId` / `Bot:selfId:guildId` / `Bot:selfId:guildId:channelId` / `Bot:selfId:userId`
    pub key: String,
    pub inherit: bool,
    /// 群整体冷却（秒）
    pub cd: u32,
    /// 群内单人冷却（秒）；私聊规则无此项
    #[serde(rename = "userCD", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub user_cd: Option<u32>,
    pub mode: u8,
    pub alias: Vec<String>,
    pub enable: Vec<String>,
    pub disable: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub member_enable: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub member_disable: Option<Vec<String>>,
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: Extra,
}

impl Default for KarinScopeRule {
    fn default() -> Self {
        Self {
            key: "default".to_string(),
            inherit: true,
            cd: 0,
            user_cd: None,
            mode: 0,
            alias: Vec::new(),
            enable: Vec::new(),
            disable: Vec::new(),
            member_enable: None,
            member_disable: None,
            extra: Extra::new(),
        }
    }
}

impl KarinScopeRule {
    pub fn group(key: &str) -> Self {
        Self {
            key: key.to_string(),
            user_cd: Some(0),
            member_enable: Some(Vec::new()),
            member_disable: Some(Vec::new()),
            ..Self::default()
        }
    }

    pub fn private(key: &str) -> Self {
        Self {
            key: key.to_string(),
            ..Self::default()
        }
    }
}

pub const KARIN_GROUP_RULE_KEYS: [&str; 6] = [
    "default",
    "global",
    "Bot:selfId",
    "Bot:selfId:groupId",
    "Bot:selfId:guildId",
    "Bot:selfId:guildId:channelId",
];
pub const KARIN_PRIVATE_RULE_KEYS: [&str; 4] = ["default", "global", "Bot:selfId", "Bot:selfId:userId"];

pub fn default_groups() -> Vec<KarinScopeRule> {
    KARIN_GROUP_RULE_KEYS
        .iter()
        .map(|k| KarinScopeRule::group(k))
        .collect()
}

pub fn default_privates() -> Vec<KarinScopeRule> {
    KARIN_PRIVATE_RULE_KEYS
        .iter()
        .map(|k| KarinScopeRule::private(k))
        .collect()
}

fn validate_rules(rules: &[KarinScopeRule], root: &str, sink: &mut IssueSink) {
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for (i, rule) in rules.iter().enumerate() {
        let key = rule.key.trim();
        if key.is_empty() {
            sink.push(format!("{root}/{i}/key"), "规则键不能为空");
        } else if !seen.insert(key) {
            sink.push(format!("{root}/{i}/key"), format!("规则键 {key} 重复"));
        }
        if rule.mode > 6 {
            sink.push(format!("{root}/{i}/mode"), "mode 须在 0–6");
        }
    }
}

// ---------- render.json ----------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct KarinRenderWsServer {
    pub enable: bool,
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: Extra,
}

impl Default for KarinRenderWsServer {
    fn default() -> Self {
        Self {
            enable: true,
            extra: Extra::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct KarinRenderWsClient {
    pub enable: bool,
    pub url: String,
    pub token: String,
    #[serde(rename = "isSnapka")]
    pub is_snapka: bool,
    /// 毫秒
    #[serde(rename = "reconnectTime", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub reconnect_time: Option<u32>,
    /// 毫秒
    #[serde(rename = "heartbeatTime", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub heartbeat_time: Option<u32>,
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: Extra,
}

impl Default for KarinRenderWsClient {
    fn default() -> Self {
        Self {
            enable: false,
            url: "ws://127.0.0.1:7005".to_string(),
            token: "123456".to_string(),
            is_snapka: false,
            reconnect_time: Some(5000),
            heartbeat_time: Some(30000),
            extra: Extra::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct KarinRenderHttpServer {
    pub enable: bool,
    pub url: String,
    pub token: String,
    #[serde(rename = "isSnapka")]
    pub is_snapka: bool,
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: Extra,
}

impl Default for KarinRenderHttpServer {
    fn default() -> Self {
        Self {
            enable: false,
            url: "http://127.0.0.1:7005".to_string(),
            token: "123456".to_string(),
            is_snapka: false,
            extra: Extra::new(),
        }
    }
}

/// `render.json`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct KarinRenderConfig {
    pub ws_server: KarinRenderWsServer,
    pub ws_client: Vec<KarinRenderWsClient>,
    pub http_server: Vec<KarinRenderHttpServer>,
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: Extra,
}

impl Default for KarinRenderConfig {
    fn default() -> Self {
        Self {
            ws_server: KarinRenderWsServer::default(),
            ws_client: vec![KarinRenderWsClient::default()],
            http_server: vec![KarinRenderHttpServer::default()],
            extra: Extra::new(),
        }
    }
}

// ---------- redis.json ----------

/// `redis.json`（Karin 不监听此文件，改完要重启实例）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct KarinRedisConfig {
    pub url: String,
    pub username: String,
    pub password: String,
    pub database: u32,
    #[serde(flatten)]
    #[ts(skip)]
    pub extra: Extra,
}

impl Default for KarinRedisConfig {
    fn default() -> Self {
        Self {
            url: "redis://127.0.0.1:6379".to_string(),
            username: String::new(),
            password: String::new(),
            database: 0,
            extra: Extra::new(),
        }
    }
}

// ---------- 聚合 ----------

/// 一个 Karin 实例的全部可编辑配置
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct KarinInstanceConfig {
    pub env: KarinEnv,
    pub config: KarinCoreConfig,
    pub adapter: KarinAdapterConfig,
    pub groups: Vec<KarinScopeRule>,
    pub privates: Vec<KarinScopeRule>,
    pub render: KarinRenderConfig,
    pub redis: KarinRedisConfig,
}

impl KarinInstanceConfig {
    /// 与 Karin `default.ts` 一致的默认配置（缺文件时的兜底，也是 mock 的种子）
    pub fn upstream_default() -> Self {
        Self {
            env: KarinEnv::default(),
            config: KarinCoreConfig::default(),
            adapter: KarinAdapterConfig::default(),
            groups: default_groups(),
            privates: default_privates(),
            render: KarinRenderConfig::default(),
            redis: KarinRedisConfig::default(),
        }
    }

    pub fn validate(&self) -> Vec<AppConfigIssue> {
        let mut sink = IssueSink::default();
        self.env.validate(&mut sink);

        if self.adapter.onebot.ws_server.timeout == 0 {
            sink.push("adapter/onebot/ws_server/timeout", "超时须大于 0 秒");
        }
        for (i, c) in self.adapter.onebot.ws_client.iter().enumerate() {
            if !is_ws_url(&c.url) {
                sink.push(
                    format!("adapter/onebot/ws_client/{i}/url"),
                    "须以 ws:// 或 wss:// 开头",
                );
            }
        }
        for (i, s) in self.adapter.onebot.http_server.iter().enumerate() {
            if !is_http_url(&s.url) {
                sink.push(
                    format!("adapter/onebot/http_server/{i}/url"),
                    "须以 http:// 或 https:// 开头",
                );
            }
            if s.self_id.trim().is_empty() {
                sink.push(
                    format!("adapter/onebot/http_server/{i}/self_id"),
                    "self_id 不能为空",
                );
            }
        }

        validate_rules(&self.groups, "groups", &mut sink);
        validate_rules(&self.privates, "privates", &mut sink);

        for (i, c) in self.render.ws_client.iter().enumerate() {
            if !is_ws_url(&c.url) {
                sink.push(
                    format!("render/ws_client/{i}/url"),
                    "须以 ws:// 或 wss:// 开头",
                );
            }
        }
        for (i, s) in self.render.http_server.iter().enumerate() {
            if !is_http_url(&s.url) {
                sink.push(
                    format!("render/http_server/{i}/url"),
                    "须以 http:// 或 https:// 开头",
                );
            }
        }

        let redis = self.redis.url.trim();
        if !(redis.starts_with("redis://") || redis.starts_with("rediss://")) {
            sink.push("redis/url", "须以 redis:// 或 rediss:// 开头");
        }
        sink.into_vec()
    }
}

fn is_ws_url(url: &str) -> bool {
    let u = url.trim();
    u.starts_with("ws://") || u.starts_with("wss://")
}

fn is_http_url(url: &str) -> bool {
    let u = url.trim();
    u.starts_with("http://") || u.starts_with("https://")
}

// ---------- 文档清单与读写 ----------

fn doc(id: &str, label: &str, rel_path: String, format: AppConfigFormat, hot_reload: bool) -> AppConfigDocument {
    AppConfigDocument {
        id: id.to_string(),
        label: label.to_string(),
        rel_path,
        format,
        hot_reload,
    }
}

/// Karin 的 7 份可编辑文档（顺序固定，合并版本号依赖此顺序）
pub fn karin_config_documents() -> Vec<AppConfigDocument> {
    let json = |name: &str| format!("{KARIN_CONFIG_DIR}/{name}.json");
    vec![
        doc(DOC_ENV, ".env", KARIN_ENV_FILE.to_string(), AppConfigFormat::DotEnv, true),
        doc(DOC_CONFIG, "config.json", json("config"), AppConfigFormat::Json, true),
        doc(DOC_ADAPTER, "adapter.json", json("adapter"), AppConfigFormat::Json, true),
        doc(DOC_GROUPS, "groups.json", json("groups"), AppConfigFormat::Json, true),
        doc(DOC_PRIVATES, "privates.json", json("privates"), AppConfigFormat::Json, true),
        doc(DOC_RENDER, "render.json", json("render"), AppConfigFormat::Json, true),
        doc(DOC_REDIS, "redis.json", json("redis"), AppConfigFormat::Json, false),
    ]
}

fn snapshot<'a>(snaps: &'a [DocumentSnapshot], id: &str) -> Option<&'a DocumentSnapshot> {
    snaps.iter().find(|s| s.doc.id == id)
}

fn parse_json_doc<T: for<'de> Deserialize<'de> + Default>(
    snaps: &[DocumentSnapshot],
    id: &str,
    fallback: impl FnOnce() -> T,
) -> Result<T, AppFrameworkError> {
    match snapshot(snaps, id).and_then(|s| s.text.as_deref()) {
        Some(text) if !text.trim().is_empty() => serde_json::from_str::<T>(text)
            .map_err(|e| AppFrameworkError::Integration(format!("{id}.json 解析失败: {e}"))),
        _ => Ok(fallback()),
    }
}

/// 从已读快照拼出类型化配置（`.env` 必须存在；JSON 缺文件按上游默认补）
pub fn parse_karin_config(snaps: &[DocumentSnapshot]) -> Result<KarinInstanceConfig, AppFrameworkError> {
    let env_text = snapshot(snaps, DOC_ENV)
        .and_then(|s| s.text.as_deref())
        .ok_or_else(|| {
            AppFrameworkError::Integration("Karin 实例缺少 .env，请先完成安装".to_string())
        })?;
    let env = KarinEnv::from_env_file(&EnvFile::parse(env_text));
    Ok(KarinInstanceConfig {
        env,
        config: parse_json_doc(snaps, DOC_CONFIG, KarinCoreConfig::default)?,
        adapter: parse_json_doc(snaps, DOC_ADAPTER, KarinAdapterConfig::default)?,
        groups: parse_json_doc(snaps, DOC_GROUPS, default_groups)?,
        privates: parse_json_doc(snaps, DOC_PRIVATES, default_privates)?,
        render: parse_json_doc(snaps, DOC_RENDER, KarinRenderConfig::default)?,
        redis: parse_json_doc(snaps, DOC_REDIS, KarinRedisConfig::default)?,
    })
}

pub async fn read_karin_config(
    host: &dyn Host,
    install_dir: &HostPath,
) -> Result<(KarinInstanceConfig, Vec<DocumentSnapshot>), AppFrameworkError> {
    let snaps = read_documents(host, install_dir, &karin_config_documents()).await?;
    let config = parse_karin_config(&snaps)?;
    Ok((config, snaps))
}

/// 一份待写文件（只列真正变了的）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingWrite {
    pub doc_id: String,
    pub path: HostPath,
    pub text: String,
}

/// 与当前快照比对，算出需要落盘的文件：`.env` 文本级比对，JSON 语义级比对（避免只因键序 / 缩进不同就重写）
pub fn plan_karin_writes(
    install_dir: &HostPath,
    config: &KarinInstanceConfig,
    current: &[DocumentSnapshot],
) -> Result<Vec<PendingWrite>, AppFrameworkError> {
    let issues = config.validate();
    if !issues.is_empty() {
        return Err(AppFrameworkError::ConfigInvalid(issues));
    }
    let docs = karin_config_documents();
    let mut out = Vec::new();

    for d in &docs {
        let snap = snapshot(current, &d.id);
        let text = if d.id == DOC_ENV {
            let mut env = EnvFile::parse(snap.and_then(|s| s.text.as_deref()).unwrap_or_default());
            config.env.write_into(&mut env);
            env.render()
        } else {
            let value = match d.id.as_str() {
                DOC_CONFIG => serde_json::to_value(&config.config),
                DOC_ADAPTER => serde_json::to_value(&config.adapter),
                DOC_GROUPS => serde_json::to_value(&config.groups),
                DOC_PRIVATES => serde_json::to_value(&config.privates),
                DOC_RENDER => serde_json::to_value(&config.render),
                DOC_REDIS => serde_json::to_value(&config.redis),
                other => {
                    return Err(AppFrameworkError::Integration(format!(
                        "未知的 Karin 配置文档: {other}"
                    )));
                }
            }
            .map_err(|e| AppFrameworkError::Integration(format!("序列化 {} 失败: {e}", d.id)))?;
            let unchanged = snap
                .and_then(|s| s.text.as_deref())
                .and_then(|t| serde_json::from_str::<Value>(t).ok())
                .is_some_and(|cur| cur == value);
            if unchanged {
                continue;
            }
            render_json_pretty(&value)?
        };
        if snap.and_then(|s| s.text.as_deref()) == Some(text.as_str()) {
            continue;
        }
        out.push(PendingWrite {
            doc_id: d.id.clone(),
            path: install_dir.join(&d.rel_path),
            text,
        });
    }
    Ok(out)
}

/// 校验 → 计算差异 → 备份写入（任一失败全部还原）→ 重新读取返回新快照
pub async fn write_karin_config(
    host: &dyn Host,
    install_dir: &HostPath,
    config: &KarinInstanceConfig,
    current: &[DocumentSnapshot],
    write_sidecar: bool,
) -> Result<(KarinInstanceConfig, Vec<DocumentSnapshot>), AppFrameworkError> {
    let writes = plan_karin_writes(install_dir, config, current)?;
    if !writes.is_empty() {
        for w in &writes {
            ensure_parent_dir(host, &w.path).await?;
        }
        let paths: Vec<HostPath> = writes.iter().map(|w| w.path.clone()).collect();
        apply_with_backup_ex(host, &paths, write_sidecar, || async {
            for w in &writes {
                host.write_file(&w.path, w.text.as_bytes())
                    .await
                    .map_err(|e| AppFrameworkError::Integration(e.to_string()))?;
            }
            Ok(())
        })
        .await?;
    }
    read_karin_config(host, install_dir).await
}

/// 对接依赖的两把钥匙是否变了（编排层据此决定要不要重新 upsert 协议 Bot 侧连接）
pub fn link_inputs_changed(before: &KarinEnv, after: &KarinEnv) -> bool {
    before.http_port != after.http_port || before.ws_server_auth_key != after.ws_server_auth_key
}

/// 把 `.env` 条目列表转成 `KarinEnvEntry`（测试 / mock 用）
pub fn env_entries_to_custom(entries: &[EnvEntry]) -> Vec<KarinEnvEntry> {
    entries
        .iter()
        .filter(|e| !KarinEnv::is_system_key(&e.key))
        .map(|e| KarinEnvEntry {
            key: e.key.clone(),
            value: e.value.clone(),
            comment: e.comment.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const ENV_TEXT: &str = "# 是否启用HTTP\nHTTP_ENABLE=true\n# HTTP监听端口\nHTTP_PORT=7801\n# HTTP鉴权秘钥 仅用于karin自身Api\nHTTP_AUTH_KEY=abc\n# ws_server鉴权秘钥\nWS_SERVER_AUTH_KEY=tok\n# 日志等级\nLOG_LEVEL=debug\nLOG_FNC_COLOR=\"#E1D919\"\n# 日志实时Api最多支持同时连接数\nLOG_API_MAX_CONNECTIONS=5\nMY_FLAG=1\nRUNTIME=node\n";

    fn snaps_from(env: Option<&str>, jsons: &[(&str, &str)]) -> Vec<DocumentSnapshot> {
        karin_config_documents()
            .into_iter()
            .map(|d| {
                let text = if d.id == DOC_ENV {
                    env.map(str::to_string)
                } else {
                    jsons
                        .iter()
                        .find(|(id, _)| *id == d.id)
                        .map(|(_, t)| (*t).to_string())
                };
                let revision = text
                    .as_deref()
                    .map(|t| crate::config_doc::revision_of(t.as_bytes()))
                    .unwrap_or_else(|| crate::config_doc::MISSING_REVISION.to_string());
                DocumentSnapshot {
                    doc: d,
                    text,
                    revision,
                }
            })
            .collect()
    }

    #[test]
    fn env_view_splits_system_and_custom_keys_with_comments() {
        let env = KarinEnv::from_env_file(&EnvFile::parse(ENV_TEXT));
        assert_eq!(env.http_port, 7801);
        assert_eq!(env.log_level, "debug");
        assert_eq!(env.ws_server_auth_key, "tok");
        // 缺键回落默认
        assert_eq!(env.http_host, "0.0.0.0");
        assert_eq!(env.log_max_connections, 5);
        assert_eq!(env.comments.get("HTTP_PORT").map(String::as_str), Some("HTTP监听端口"));
        let custom: Vec<(&str, &str, &str)> = env
            .custom
            .iter()
            .map(|e| (e.key.as_str(), e.value.as_str(), e.comment.as_str()))
            .collect();
        assert_eq!(
            custom,
            [
                ("LOG_API_MAX_CONNECTIONS", "5", "日志实时Api最多支持同时连接数"),
                ("MY_FLAG", "1", "")
            ]
        );
    }

    #[test]
    fn env_write_back_preserves_comments_and_manages_custom_keys() {
        let mut env = KarinEnv::from_env_file(&EnvFile::parse(ENV_TEXT));
        env.http_port = 7802;
        env.custom.retain(|e| e.key != "MY_FLAG");
        env.custom.push(KarinEnvEntry {
            key: "NEW_KEY".into(),
            value: "hello world".into(),
            comment: "新键".into(),
        });
        let mut file = EnvFile::parse(ENV_TEXT);
        env.write_into(&mut file);
        let out = file.render();
        assert!(out.contains("# HTTP监听端口\nHTTP_PORT=7802\n"));
        assert!(!out.contains("MY_FLAG"));
        assert!(out.contains("# 新键\nNEW_KEY=\"hello world\"\n"));
        assert!(out.contains("LOG_FNC_COLOR=\"#E1D919\""));
        // 缺失的系统键被补上
        assert!(out.contains("HTTP_HOST=0.0.0.0"));
        // 再读一遍一致
        let again = KarinEnv::from_env_file(&EnvFile::parse(&out));
        assert_eq!(again.http_port, 7802);
        assert_eq!(again.custom.len(), 2);
    }

    #[test]
    fn upstream_default_round_trips_through_json() {
        let cfg = KarinInstanceConfig::upstream_default();
        let text = serde_json::to_string(&cfg).unwrap();
        let back: KarinInstanceConfig = serde_json::from_str(&text).unwrap();
        assert_eq!(cfg, back);
        assert!(cfg.validate().is_empty());
        // 与 default.ts 关键值一致
        assert_eq!(cfg.config.master, vec!["console"]);
        assert_eq!(cfg.adapter.onebot.ws_server.timeout, 120);
        assert_eq!(cfg.groups.len(), 6);
        assert_eq!(cfg.groups[0].user_cd, Some(0));
        assert_eq!(cfg.privates.len(), 4);
        assert_eq!(cfg.privates[0].user_cd, None);
        assert_eq!(cfg.render.ws_client[0].reconnect_time, Some(5000));
        assert_eq!(cfg.redis.url, "redis://127.0.0.1:6379");
    }

    #[test]
    fn unknown_fields_survive_round_trip_and_camel_case_names_are_kept() {
        let text = r#"{"console":{"isLocal":false,"token":"t","host":"h","future":1},"onebot":{"ws_server":{"enable":true,"timeout":60},"ws_client":[],"http_server":[],"grpc":{"x":1}},"plugins":["a"]}"#;
        let cfg: KarinAdapterConfig = serde_json::from_str(text).unwrap();
        assert!(!cfg.console.is_local);
        assert_eq!(cfg.console.extra.get("future"), Some(&json!(1)));
        assert_eq!(cfg.onebot.extra.get("grpc"), Some(&json!({"x":1})));
        let v = serde_json::to_value(&cfg).unwrap();
        assert_eq!(v["console"]["isLocal"], json!(false));
        assert_eq!(v["console"]["future"], json!(1));
        assert_eq!(v["plugins"], json!(["a"]));

        let rule: KarinScopeRule = serde_json::from_str(
            r#"{"key":"Bot:1:2","userCD":3,"mode":4,"memberEnable":["legacy"]}"#,
        )
        .unwrap();
        assert_eq!(rule.user_cd, Some(3));
        assert!(rule.inherit);
        let rv = serde_json::to_value(&rule).unwrap();
        assert_eq!(rv["userCD"], json!(3));
        assert_eq!(rv["memberEnable"], json!(["legacy"]));
        assert!(rv.get("member_enable").is_none());
    }

    #[test]
    fn validate_reports_field_paths() {
        let mut cfg = KarinInstanceConfig::upstream_default();
        cfg.env.http_port = 0;
        cfg.env.log_level = "loud".into();
        cfg.env.custom = vec![
            KarinEnvEntry { key: "HTTP_PORT".into(), value: "1".into(), comment: String::new() },
            KarinEnvEntry { key: "A".into(), value: "1".into(), comment: String::new() },
            KarinEnvEntry { key: "A".into(), value: "2".into(), comment: String::new() },
            KarinEnvEntry { key: "9x".into(), value: "2".into(), comment: String::new() },
        ];
        cfg.adapter.onebot.ws_client[0].url = "http://nope".into();
        cfg.adapter.onebot.http_server[0].url = "ws://nope".into();
        cfg.groups[1].mode = 7;
        cfg.groups[2].key = "default".into();
        cfg.render.ws_client[0].url = "".into();
        cfg.redis.url = "127.0.0.1".into();
        let paths: Vec<String> = cfg.validate().into_iter().map(|i| i.path).collect();
        assert_eq!(
            paths,
            [
                "env/http_port",
                "env/log_level",
                "env/custom/0/key",
                "env/custom/2/key",
                "env/custom/3/key",
                "adapter/onebot/ws_client/0/url",
                "adapter/onebot/http_server/0/url",
                "groups/1/mode",
                "groups/2/key",
                "render/ws_client/0/url",
                "redis/url",
            ]
        );
    }

    #[test]
    fn parse_fills_missing_json_with_defaults_and_requires_env() {
        assert!(matches!(
            parse_karin_config(&snaps_from(None, &[])),
            Err(AppFrameworkError::Integration(_))
        ));
        let cfg = parse_karin_config(&snaps_from(Some(ENV_TEXT), &[])).unwrap();
        assert_eq!(cfg.groups.len(), 6);
        assert_eq!(cfg.config.master, vec!["console"]);
        let bad = snaps_from(Some(ENV_TEXT), &[(DOC_REDIS, "{not json")]);
        assert!(matches!(parse_karin_config(&bad), Err(AppFrameworkError::Integration(_))));
    }

    #[test]
    fn plan_writes_only_changed_documents() {
        let install = HostPath::from_posix("/srv/karin");
        let current = snaps_from(
            Some(ENV_TEXT),
            &[(
                DOC_REDIS,
                // 键序与缩进都和我们输出不同，但语义相同 → 不应重写
                "{\"database\":0,\"password\":\"\",\"url\":\"redis://127.0.0.1:6379\",\"username\":\"\"}",
            )],
        );
        let mut cfg = parse_karin_config(&current).unwrap();
        // 未改动：只有缺失的 JSON 文件会被创建
        let writes = plan_karin_writes(&install, &cfg, &current).unwrap();
        let ids: Vec<&str> = writes.iter().map(|w| w.doc_id.as_str()).collect();
        assert_eq!(ids, ["env", "config", "adapter", "groups", "privates", "render"]);
        // env 变了是因为补齐了缺失系统键；确认 redis 未在列表
        assert!(writes[0].text.contains("HTTP_HOST=0.0.0.0"));

        cfg.redis.database = 3;
        cfg.config.master.push("123".into());
        let writes = plan_karin_writes(&install, &cfg, &current).unwrap();
        let redis = writes.iter().find(|w| w.doc_id == DOC_REDIS).unwrap();
        assert_eq!(redis.path.as_posix(), "/srv/karin/@karinjs/config/redis.json");
        assert!(redis.text.ends_with("}\n"));
        let v: Value = serde_json::from_str(&redis.text).unwrap();
        assert_eq!(v["database"], json!(3));
    }

    #[test]
    fn plan_rejects_invalid_config() {
        let install = HostPath::from_posix("/srv/karin");
        let current = snaps_from(Some(ENV_TEXT), &[]);
        let mut cfg = parse_karin_config(&current).unwrap();
        cfg.redis.url = "nope".into();
        assert!(matches!(
            plan_karin_writes(&install, &cfg, &current),
            Err(AppFrameworkError::ConfigInvalid(issues)) if issues.len() == 1
        ));
    }

    #[test]
    fn link_inputs_changed_tracks_port_and_ws_key() {
        let a = KarinEnv::default();
        let mut b = a.clone();
        assert!(!link_inputs_changed(&a, &b));
        b.http_port = 8000;
        assert!(link_inputs_changed(&a, &b));
        b = a.clone();
        b.ws_server_auth_key = "x".into();
        assert!(link_inputs_changed(&a, &b));
        b = a.clone();
        b.log_level = "debug".into();
        assert!(!link_inputs_changed(&a, &b));
    }

    #[test]
    fn outbound_ws_urls_skip_disabled_placeholder() {
        let disabled = r#"{"onebot":{"ws_client":[{"enable":false,"url":"ws://127.0.0.1:7778"}]}}"#;
        assert!(outbound_onebot_ws_urls(disabled).is_empty());
        let on = r#"{"onebot":{"ws_client":[{"enable":true,"url":"ws://127.0.0.1:3001"}]}}"#;
        assert_eq!(
            outbound_onebot_ws_urls(on),
            vec!["ws://127.0.0.1:3001".to_string()]
        );
    }
}
