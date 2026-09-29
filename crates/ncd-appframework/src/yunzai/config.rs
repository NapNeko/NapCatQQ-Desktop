//! 云崽类型化配置：`config/config/` 下六份（bot / other / group / server / redis / renderer）。
//!
//! 读：和上游 `getAllCfg` 一样，`default_config` 与 `config/config` 按顶层键浅合并；两份都没有的键
//! 用上游默认值兜底（导入的项目可能从没跑过，`config/config` 还不存在）。
//! 写：逐字段和盘上比，只把改过的键交给 yaml_patch，别的行（注释、没见过的键）不动。
//! `config/config` 里还没有的文件先抄 `default_config` 那份再改，注释跟着过来。

use ncd_domain::{AppConfigDocument, AppConfigFormat};
use ncd_host::{Host, HostPath};
use ncd_traits::AppFrameworkError;
use serde::{Deserialize, Serialize};
use serde_yaml::{Mapping, Value};
use ts_rs::TS;

use super::manifest::{
    YUNZAI_AUTH_HEADER, YUNZAI_CONFIG_DIR, YUNZAI_CONFIG_NAMES, YUNZAI_DEFAULT_CONFIG_DIR,
    YUNZAI_DEFAULT_PORT,
};
use crate::adapter::apply_with_backup_ex;
use crate::config_doc::{DocumentSnapshot, IssueSink, read_documents};
use crate::yaml_patch;

pub const YUNZAI_LOG_LEVELS: [&str; 8] = [
    "trace", "debug", "info", "warn", "fatal", "mark", "error", "off",
];
/// renderer.yaml 的 name：空 = 按模板里有没有脚本自动挑
pub const YUNZAI_RENDERERS: [&str; 3] = ["", "puppeteer", "shotium"];
/// 上游 stdin 适配器的主人占位，桌面端界面不显示、写回时原样留着
const STDIN_MASTER: &str = "stdin";
const STDIN_BOT_MASTER: &str = "stdin:stdin";

/// bot.yaml
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct YunzaiBotConfig {
    pub log_level: String,
    pub log_length: u32,
    pub log_object: bool,
    /// 单个插件加载超过多少秒算超时
    pub plugin_load_timeout: u32,
    /// 监听文件变化（改配置 / 单文件插件热加载都靠它）
    pub file_watch: bool,
    /// 每隔多少分钟自动 `#全部更新`；0 不自动更新
    pub update_time: u32,
    /// 每隔多少分钟自动重启；0 不自动重启
    pub restart_time: u32,
    pub update_cron: Vec<String>,
    pub restart_cron: Vec<String>,
    pub stop_cron: Vec<String>,
    pub start_cron: Vec<String>,
    pub cache_group_member: bool,
    /// 上线推送通知的冷却（分钟）
    pub online_msg_exp: u32,
    /// 发出去的文件链接保留多少分钟
    pub file_to_url_time: u32,
    /// `/` 开头的消息当作 `#`（上游键名 `/→#`）
    pub slash_to_hash: bool,
    pub chromium_path: String,
    pub puppeteer_ws: String,
    /// 截图超时（毫秒）；None 用渲染器默认
    #[ts(optional)]
    pub puppeteer_timeout: Option<u32>,
    pub proxy_address: String,
}

/// other.yaml
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct YunzaiOtherConfig {
    /// 1 自动同意加好友，0 不处理
    pub auto_friend: u8,
    /// 0 只同意主人拉的群，1 谁拉都同意
    pub auto_group: u8,
    /// 被拉进人数少于这个数的群自动退出；0 不退
    pub auto_quit: u32,
    /// 主人（所有 Bot 通用）；上游的 stdin 占位不在这里
    pub master_qq: Vec<String>,
    /// 只对某个 Bot 生效的主人，写法 `Bot号:主人号`
    pub master: Vec<String>,
    pub disable_private: bool,
    pub disable_msg: String,
    /// 私聊禁用时仍放行的关键字
    pub disable_adopt: Vec<String>,
    pub white_group: Vec<String>,
    pub white_user: Vec<String>,
    pub black_group: Vec<String>,
    pub black_user: Vec<String>,
}

/// group.yaml 的 `default`：所有群的默认
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct YunzaiGroupDefaults {
    /// 群里所有指令的冷却（毫秒）
    pub group_cd: u32,
    /// 同一个人的冷却（毫秒）
    pub single_cd: u32,
    /// 0 什么都回，1 只回 @ 和别名，2 非主人只回 @ 和别名
    pub only_reply_at: u8,
    pub bot_alias: Vec<String>,
    /// 添加消息（#添加）的权限：0 所有群员，1 群管理，2 主人
    pub add_limit: u8,
    pub add_private: u8,
    pub add_reply: u8,
    pub add_at: u8,
    /// 添加消息回复多少秒后撤回；0 不撤回
    pub add_recall: u32,
    /// 只启用这些功能（非空时其它功能都不响应）
    pub enable: Vec<String>,
    pub disable: Vec<String>,
}

/// group.yaml 里的单独设置：`群号`、`Bot号:default`、`Bot号:群号`。没写的字段继承默认
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct YunzaiGroupOverride {
    pub key: String,
    #[ts(optional)]
    pub group_cd: Option<u32>,
    #[ts(optional)]
    pub single_cd: Option<u32>,
    #[ts(optional)]
    pub only_reply_at: Option<u8>,
    #[ts(optional)]
    pub bot_alias: Option<Vec<String>>,
    #[ts(optional)]
    pub add_limit: Option<u8>,
    #[ts(optional)]
    pub add_private: Option<u8>,
    #[ts(optional)]
    pub add_reply: Option<u8>,
    #[ts(optional)]
    pub add_at: Option<u8>,
    #[ts(optional)]
    pub add_recall: Option<u32>,
    #[ts(optional)]
    pub enable: Option<Vec<String>>,
    #[ts(optional)]
    pub disable: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct YunzaiGroupConfig {
    pub default: YunzaiGroupDefaults,
    pub overrides: Vec<YunzaiGroupOverride>,
}

/// server.yaml
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct YunzaiServerConfig {
    pub port: u16,
    /// 发给协议端的文件链接用这个地址拼
    pub url: String,
    /// 访问不存在的路径时跳去哪
    pub redirect: String,
    /// `auth.Authorization` 去掉 `Bearer ` 的部分；对接时 NapCat 带的就是它。空 = 不鉴权
    pub access_token: String,
    /// auth 里除 Authorization 之外的头（只读展示：它们也要全对上，NapCat 带不了）
    pub extra_auth_headers: Vec<String>,
}

/// redis.yaml
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct YunzaiRedisConfig {
    /// 连不上时云崽自己拉起的 redis-server
    pub path: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub db: u32,
}

/// renderer.yaml
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct YunzaiRendererConfig {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct YunzaiInstanceConfig {
    pub bot: YunzaiBotConfig,
    pub other: YunzaiOtherConfig,
    pub group: YunzaiGroupConfig,
    pub server: YunzaiServerConfig,
    pub redis: YunzaiRedisConfig,
    pub renderer: YunzaiRendererConfig,
}

impl YunzaiInstanceConfig {
    /// 上游 default_config 的值（两份文件都读不到时兜底）
    pub fn upstream_default() -> Self {
        Self {
            bot: YunzaiBotConfig {
                log_level: "info".into(),
                log_length: 10000,
                log_object: true,
                plugin_load_timeout: 60,
                file_watch: true,
                update_time: 1440,
                restart_time: 0,
                update_cron: Vec::new(),
                restart_cron: Vec::new(),
                stop_cron: Vec::new(),
                start_cron: Vec::new(),
                cache_group_member: true,
                online_msg_exp: 1440,
                file_to_url_time: 1,
                slash_to_hash: true,
                chromium_path: String::new(),
                puppeteer_ws: String::new(),
                puppeteer_timeout: None,
                proxy_address: String::new(),
            },
            other: YunzaiOtherConfig {
                auto_friend: 1,
                auto_group: 0,
                auto_quit: 50,
                master_qq: Vec::new(),
                master: Vec::new(),
                disable_private: false,
                disable_msg: "私聊功能已禁用，仅支持发送cookie，抽卡记录链接，记录日志文件".into(),
                disable_adopt: vec!["stoken".into()],
                white_group: Vec::new(),
                white_user: Vec::new(),
                black_group: Vec::new(),
                black_user: Vec::new(),
            },
            group: YunzaiGroupConfig {
                default: YunzaiGroupDefaults {
                    group_cd: 500,
                    single_cd: 2000,
                    only_reply_at: 0,
                    bot_alias: vec!["云崽".into(), "云宝".into()],
                    add_limit: 0,
                    add_private: 1,
                    add_reply: 1,
                    add_at: 0,
                    add_recall: 60,
                    enable: Vec::new(),
                    disable: Vec::new(),
                },
                overrides: Vec::new(),
            },
            server: YunzaiServerConfig {
                port: YUNZAI_DEFAULT_PORT,
                url: format!("http://localhost:{YUNZAI_DEFAULT_PORT}"),
                redirect: "https://git.trss.me/Yunzai".into(),
                access_token: String::new(),
                extra_auth_headers: Vec::new(),
            },
            redis: YunzaiRedisConfig {
                path: "redis-server".into(),
                host: "127.0.0.1".into(),
                port: 6379,
                username: String::new(),
                password: String::new(),
                db: 0,
            },
            renderer: YunzaiRendererConfig {
                name: String::new(),
            },
        }
    }

    pub fn listen_port(&self) -> u16 {
        self.server.port
    }
}

pub fn config_rel(name: &str) -> String {
    format!("{YUNZAI_CONFIG_DIR}/{name}.yaml")
}

pub fn default_config_rel(name: &str) -> String {
    format!("{YUNZAI_DEFAULT_CONFIG_DIR}/{name}.yaml")
}

/// 「原始文件」Tab 列的九份。server / redis / db 只在启动时读，改了要重启
pub fn yunzai_config_documents() -> Vec<AppConfigDocument> {
    YUNZAI_CONFIG_NAMES
        .iter()
        .map(|name| AppConfigDocument {
            id: (*name).to_string(),
            label: format!("{name}.yaml"),
            rel_path: config_rel(name),
            format: AppConfigFormat::Yaml,
            hot_reload: !matches!(*name, "server" | "redis" | "db"),
        })
        .collect()
}

// ---- 读 ----

fn key_str(k: &Value) -> Option<String> {
    match k {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn get<'a>(map: &'a Mapping, key: &str) -> Option<&'a Value> {
    map.iter()
        .find(|(k, _)| key_str(k).as_deref() == Some(key))
        .map(|(_, v)| v)
}

fn scalar_string(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn as_bool(v: Option<&Value>) -> Option<bool> {
    match v? {
        Value::Bool(b) => Some(*b),
        Value::Number(n) => n.as_i64().map(|i| i != 0),
        _ => None,
    }
}

fn as_u64(v: Option<&Value>) -> Option<u64> {
    match v? {
        Value::Number(n) => n.as_u64().or_else(|| n.as_f64().map(|f| f.max(0.0) as u64)),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

fn as_u32(v: Option<&Value>) -> Option<u32> {
    as_u64(v).map(|n| n.min(u32::MAX as u64) as u32)
}

fn as_u8(v: Option<&Value>) -> Option<u8> {
    as_u64(v).map(|n| n.min(u8::MAX as u64) as u8)
}

fn as_u16(v: Option<&Value>) -> Option<u16> {
    as_u64(v).filter(|n| *n <= u16::MAX as u64).map(|n| n as u16)
}

fn as_string(v: Option<&Value>) -> Option<String> {
    match v? {
        Value::Null => Some(String::new()),
        other => scalar_string(other),
    }
}

/// 列表字段：上游好几处单个值和数组都认（`update_cron` 可以写一个字符串），null 是空
fn as_list(v: Option<&Value>) -> Option<Vec<String>> {
    match v? {
        Value::Null => Some(Vec::new()),
        Value::Sequence(items) => Some(items.iter().filter_map(scalar_string).collect()),
        other => scalar_string(other).map(|s| vec![s]),
    }
}

fn parse_mapping(text: Option<&str>, label: &str) -> Result<Mapping, AppFrameworkError> {
    let Some(text) = text else {
        return Ok(Mapping::new());
    };
    if text.trim().is_empty() {
        return Ok(Mapping::new());
    }
    match serde_yaml::from_str::<Value>(text) {
        Ok(Value::Mapping(m)) => Ok(m),
        Ok(Value::Null) => Ok(Mapping::new()),
        Ok(_) => Err(AppFrameworkError::Validation(format!(
            "{label} 顶层不是键值表"
        ))),
        Err(e) => Err(AppFrameworkError::Validation(format!(
            "{label} 不是合法的 YAML：{e}"
        ))),
    }
}

/// 上游 `getAllCfg`：默认那份打底，用户那份的顶层键整条盖上去
fn merged(default: &Mapping, user: &Mapping) -> Mapping {
    let mut out = default.clone();
    for (k, v) in user {
        let key = key_str(k);
        // 默认那份写成数字、用户那份写成字符串的同名键也算同一个
        let existing = out.keys().find(|dk| key_str(dk) == key).cloned();
        out.insert(existing.unwrap_or_else(|| k.clone()), v.clone());
    }
    out
}

/// 一份配置的两个来源：用户那份原文（写回的底子）和合并后的有效值
pub struct ConfigSource {
    pub name: &'static str,
    pub user_text: Option<String>,
    pub default_text: Option<String>,
    pub effective: Mapping,
}

impl ConfigSource {
    fn parse(
        name: &'static str,
        user_text: Option<String>,
        default_text: Option<String>,
    ) -> Result<Self, AppFrameworkError> {
        let user = parse_mapping(user_text.as_deref(), &config_rel(name))?;
        // 默认那份坏了不挡读取：上游自己读坏文件也只是当它不存在
        let default = parse_mapping(default_text.as_deref(), &default_config_rel(name))
            .unwrap_or_default();
        Ok(Self {
            name,
            effective: merged(&default, &user),
            user_text,
            default_text,
        })
    }

    /// 写回的底子：用户那份在就用它，不在就抄默认那份（注释跟着过来）
    pub fn base_text(&self) -> String {
        self.user_text
            .clone()
            .or_else(|| self.default_text.clone())
            .unwrap_or_default()
    }
}

fn bot_from(m: &Mapping, d: &YunzaiBotConfig) -> YunzaiBotConfig {
    YunzaiBotConfig {
        log_level: as_string(get(m, "log_level"))
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| d.log_level.clone()),
        log_length: as_u32(get(m, "log_length")).unwrap_or(d.log_length),
        log_object: as_bool(get(m, "log_object")).unwrap_or(d.log_object),
        plugin_load_timeout: as_u32(get(m, "plugin_load_timeout")).unwrap_or(d.plugin_load_timeout),
        file_watch: as_bool(get(m, "file_watch")).unwrap_or(d.file_watch),
        // 上游 `if (cfg.bot.update_time)`：null 和 0 一样是关
        update_time: as_u32(get(m, "update_time")).unwrap_or(0),
        restart_time: as_u32(get(m, "restart_time")).unwrap_or(0),
        update_cron: as_list(get(m, "update_cron")).unwrap_or_default(),
        restart_cron: as_list(get(m, "restart_cron")).unwrap_or_default(),
        stop_cron: as_list(get(m, "stop_cron")).unwrap_or_default(),
        start_cron: as_list(get(m, "start_cron")).unwrap_or_default(),
        cache_group_member: as_bool(get(m, "cache_group_member")).unwrap_or(d.cache_group_member),
        online_msg_exp: as_u32(get(m, "online_msg_exp")).unwrap_or(d.online_msg_exp),
        file_to_url_time: as_u32(get(m, "file_to_url_time")).unwrap_or(d.file_to_url_time),
        slash_to_hash: as_bool(get(m, "/→#")).unwrap_or(d.slash_to_hash),
        chromium_path: as_string(get(m, "chromium_path")).unwrap_or_default(),
        puppeteer_ws: as_string(get(m, "puppeteer_ws")).unwrap_or_default(),
        puppeteer_timeout: as_u32(get(m, "puppeteer_timeout")),
        proxy_address: as_string(get(m, "proxyAddress")).unwrap_or_default(),
    }
}

fn other_from(m: &Mapping, d: &YunzaiOtherConfig) -> YunzaiOtherConfig {
    let list = |key: &str| as_list(get(m, key)).unwrap_or_default();
    YunzaiOtherConfig {
        auto_friend: as_u8(get(m, "autoFriend")).unwrap_or(d.auto_friend),
        auto_group: as_u8(get(m, "autoGroup")).unwrap_or(d.auto_group),
        auto_quit: as_u32(get(m, "autoQuit")).unwrap_or(0),
        master_qq: list("masterQQ")
            .into_iter()
            .filter(|s| s != STDIN_MASTER)
            .collect(),
        master: list("master")
            .into_iter()
            .filter(|s| s != STDIN_BOT_MASTER)
            .collect(),
        disable_private: as_bool(get(m, "disablePrivate")).unwrap_or(false),
        disable_msg: as_string(get(m, "disableMsg")).unwrap_or_default(),
        disable_adopt: list("disableAdopt"),
        white_group: list("whiteGroup"),
        white_user: list("whiteUser"),
        black_group: list("blackGroup"),
        black_user: list("blackUser"),
    }
}

fn group_defaults_from(v: Option<&Value>, d: &YunzaiGroupDefaults) -> YunzaiGroupDefaults {
    let empty = Mapping::new();
    let m = match v {
        Some(Value::Mapping(m)) => m,
        _ => &empty,
    };
    YunzaiGroupDefaults {
        group_cd: as_u32(get(m, "groupCD")).unwrap_or(d.group_cd),
        single_cd: as_u32(get(m, "singleCD")).unwrap_or(d.single_cd),
        only_reply_at: as_u8(get(m, "onlyReplyAt")).unwrap_or(d.only_reply_at),
        bot_alias: as_list(get(m, "botAlias")).unwrap_or_else(|| d.bot_alias.clone()),
        add_limit: as_u8(get(m, "addLimit")).unwrap_or(d.add_limit),
        add_private: as_u8(get(m, "addPrivate")).unwrap_or(d.add_private),
        add_reply: as_u8(get(m, "addReply")).unwrap_or(d.add_reply),
        add_at: as_u8(get(m, "addAt")).unwrap_or(d.add_at),
        add_recall: as_u32(get(m, "addRecall")).unwrap_or(d.add_recall),
        enable: as_list(get(m, "enable")).unwrap_or_default(),
        disable: as_list(get(m, "disable")).unwrap_or_default(),
    }
}

fn group_override_from(key: String, m: &Mapping) -> YunzaiGroupOverride {
    YunzaiGroupOverride {
        key,
        group_cd: as_u32(get(m, "groupCD")),
        single_cd: as_u32(get(m, "singleCD")),
        only_reply_at: as_u8(get(m, "onlyReplyAt")),
        bot_alias: as_list(get(m, "botAlias")),
        add_limit: as_u8(get(m, "addLimit")),
        add_private: as_u8(get(m, "addPrivate")),
        add_reply: as_u8(get(m, "addReply")),
        add_at: as_u8(get(m, "addAt")),
        add_recall: as_u32(get(m, "addRecall")),
        enable: as_list(get(m, "enable")),
        disable: as_list(get(m, "disable")),
    }
}

fn group_from(m: &Mapping, d: &YunzaiGroupConfig) -> YunzaiGroupConfig {
    let overrides = m
        .iter()
        .filter_map(|(k, v)| {
            let key = key_str(k)?;
            if key == "default" {
                return None;
            }
            match v {
                Value::Mapping(inner) => Some(group_override_from(key, inner)),
                _ => None,
            }
        })
        .collect();
    YunzaiGroupConfig {
        default: group_defaults_from(get(m, "default"), &d.default),
        overrides,
    }
}

fn server_from(m: &Mapping, d: &YunzaiServerConfig) -> YunzaiServerConfig {
    let (mut token, mut extra) = (String::new(), Vec::new());
    if let Some(Value::Mapping(auth)) = get(m, "auth") {
        for (k, v) in auth {
            let Some(name) = key_str(k) else { continue };
            if name.eq_ignore_ascii_case(YUNZAI_AUTH_HEADER) {
                let raw = scalar_string(v).unwrap_or_default();
                token = raw
                    .strip_prefix("Bearer ")
                    .unwrap_or(raw.as_str())
                    .trim()
                    .to_string();
            } else {
                extra.push(name);
            }
        }
    }
    YunzaiServerConfig {
        port: as_u16(get(m, "port")).filter(|p| *p > 0).unwrap_or(d.port),
        url: as_string(get(m, "url")).unwrap_or_default(),
        redirect: as_string(get(m, "redirect")).unwrap_or_default(),
        access_token: token,
        extra_auth_headers: extra,
    }
}

fn redis_from(m: &Mapping, d: &YunzaiRedisConfig) -> YunzaiRedisConfig {
    YunzaiRedisConfig {
        path: as_string(get(m, "path"))
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| d.path.clone()),
        host: as_string(get(m, "host"))
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| d.host.clone()),
        port: as_u16(get(m, "port")).filter(|p| *p > 0).unwrap_or(d.port),
        username: as_string(get(m, "username")).unwrap_or_default(),
        password: as_string(get(m, "password")).unwrap_or_default(),
        db: as_u32(get(m, "db")).unwrap_or(0),
    }
}

fn renderer_from(m: &Mapping) -> YunzaiRendererConfig {
    YunzaiRendererConfig {
        name: as_string(get(m, "name")).unwrap_or_default(),
    }
}

/// 六份来源拼成类型化配置
pub fn config_from_sources(sources: &[ConfigSource]) -> YunzaiInstanceConfig {
    let d = YunzaiInstanceConfig::upstream_default();
    let empty = Mapping::new();
    let map = |name: &str| {
        sources
            .iter()
            .find(|s| s.name == name)
            .map(|s| &s.effective)
            .unwrap_or(&empty)
    };
    YunzaiInstanceConfig {
        bot: bot_from(map("bot"), &d.bot),
        other: other_from(map("other"), &d.other),
        group: group_from(map("group"), &d.group),
        server: server_from(map("server"), &d.server),
        redis: redis_from(map("redis"), &d.redis),
        renderer: renderer_from(map("renderer")),
    }
}

/// 类型化配置用到的六份（不含 db / milky / satori）
pub const TYPED_NAMES: [&str; 6] = ["bot", "other", "group", "server", "redis", "renderer"];

async fn read_optional(host: &dyn Host, path: &HostPath) -> Result<Option<String>, AppFrameworkError> {
    if !host
        .exists(path)
        .await
        .map_err(|e| AppFrameworkError::Host(e.to_string()))?
    {
        return Ok(None);
    }
    let bytes = host
        .read_file(path)
        .await
        .map_err(|e| AppFrameworkError::Host(e.to_string()))?;
    Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
}

pub async fn read_sources(
    host: &dyn Host,
    install_dir: &HostPath,
    snaps: &[DocumentSnapshot],
) -> Result<Vec<ConfigSource>, AppFrameworkError> {
    let mut out = Vec::with_capacity(TYPED_NAMES.len());
    for name in TYPED_NAMES {
        let user = snaps
            .iter()
            .find(|s| s.doc.id == name)
            .and_then(|s| s.text.clone());
        let default = read_optional(host, &install_dir.join(default_config_rel(name))).await?;
        out.push(ConfigSource::parse(name, user, default)?);
    }
    Ok(out)
}

/// 读：九份文档的快照（版本号用）+ 类型化配置
pub async fn read_yunzai_config(
    host: &dyn Host,
    install_dir: &HostPath,
) -> Result<(YunzaiInstanceConfig, Vec<DocumentSnapshot>), AppFrameworkError> {
    let snaps = read_documents(host, install_dir, &yunzai_config_documents()).await?;
    let sources = read_sources(host, install_dir, &snaps).await?;
    Ok((config_from_sources(&sources), snaps))
}

// ---- 校验 ----

fn check_id_list(sink: &mut IssueSink, path: &str, items: &[String]) {
    for (i, item) in items.iter().enumerate() {
        let t = item.trim();
        if t.is_empty() {
            sink.push(format!("{path}/{i}"), "不能留空");
        } else if t.chars().any(char::is_whitespace) {
            sink.push(format!("{path}/{i}"), "不能带空格");
        }
    }
}

pub fn validate(cfg: &YunzaiInstanceConfig) -> Vec<ncd_domain::AppConfigIssue> {
    let mut sink = IssueSink::default();
    sink.one_of("bot/log_level", &cfg.bot.log_level, &YUNZAI_LOG_LEVELS);
    sink.one_of("renderer/name", &cfg.renderer.name, &YUNZAI_RENDERERS);
    if cfg.server.port == 0 {
        sink.push("server/port", "端口不能为 0");
    }
    if cfg.redis.port == 0 {
        sink.push("redis/port", "端口不能为 0");
    } else if cfg.redis.port == cfg.server.port && is_loopback(&cfg.redis.host) {
        sink.push("redis/port", "不能和云崽自己的端口相同");
    }
    if cfg.redis.host.trim().is_empty() {
        sink.push("redis/host", "不能留空");
    }
    if cfg.redis.path.trim().is_empty() {
        sink.push("redis/path", "不能留空");
    }
    if cfg.server.access_token.chars().any(char::is_whitespace) {
        sink.push("server/access_token", "不能带空格");
    }
    for (path, v) in [("other/auto_friend", cfg.other.auto_friend), ("other/auto_group", cfg.other.auto_group)] {
        if v > 1 {
            sink.push(path, "只能是 0 或 1");
        }
    }
    check_id_list(&mut sink, "other/master_qq", &cfg.other.master_qq);
    for (i, pair) in cfg.other.master.iter().enumerate() {
        let ok = pair
            .split_once(':')
            .is_some_and(|(b, u)| !b.trim().is_empty() && !u.trim().is_empty());
        if !ok {
            sink.push(format!("other/master/{i}"), "写成 Bot号:主人号");
        }
    }
    for (path, list) in [
        ("other/white_group", &cfg.other.white_group),
        ("other/white_user", &cfg.other.white_user),
        ("other/black_group", &cfg.other.black_group),
        ("other/black_user", &cfg.other.black_user),
    ] {
        check_id_list(&mut sink, path, list);
    }
    let d = &cfg.group.default;
    if d.only_reply_at > 2 {
        sink.push("group/default/only_reply_at", "只能是 0、1、2");
    }
    if d.add_limit > 2 {
        sink.push("group/default/add_limit", "只能是 0、1、2");
    }
    let mut seen = std::collections::HashSet::new();
    for (i, o) in cfg.group.overrides.iter().enumerate() {
        let key = o.key.trim();
        let path = format!("group/overrides/{i}/key");
        if key.is_empty() {
            sink.push(path, "填群号、Bot号:default 或 Bot号:群号");
        } else if key == "default" {
            sink.push(path, "default 是所有群的默认，不用再单独加");
        } else if key.chars().any(|c| c.is_whitespace() || c == '#') {
            sink.push(path, "不能带空格或 #");
        } else if !seen.insert(key.to_string()) {
            sink.push(path, "重复了");
        }
        if o.only_reply_at.is_some_and(|v| v > 2) {
            sink.push(format!("group/overrides/{i}/only_reply_at"), "只能是 0、1、2");
        }
        if o.add_limit.is_some_and(|v| v > 2) {
            sink.push(format!("group/overrides/{i}/add_limit"), "只能是 0、1、2");
        }
    }
    sink.into_vec()
}

fn is_loopback(host: &str) -> bool {
    matches!(host.trim(), "127.0.0.1" | "localhost" | "::1")
}

// ---- 写 ----

fn num(n: u64) -> Value {
    Value::Number(serde_yaml::Number::from(n))
}

fn text_or_null(s: &str) -> Value {
    if s.is_empty() {
        Value::Null
    } else {
        Value::String(s.to_string())
    }
}

fn list_value(items: &[String]) -> Value {
    if items.is_empty() {
        return Value::Null;
    }
    Value::Sequence(items.iter().map(|s| Value::String(s.clone())).collect())
}

/// 号码列表：纯数字写成数字，和上游默认配置、手写习惯一致；上游读的时候 `Number(i) || i` 两种都认
fn id_list_value(items: &[String]) -> Value {
    if items.is_empty() {
        return Value::Null;
    }
    Value::Sequence(items.iter().map(|s| id_value(s.trim())).collect())
}

fn id_value(s: &str) -> Value {
    match s.parse::<u64>() {
        Ok(n) if !s.starts_with('0') || s == "0" => num(n),
        _ => Value::String(s.to_string()),
    }
}

/// 一处改动：`value` 为 None 是删键
struct Patch {
    doc: &'static str,
    path: Vec<String>,
    value: Option<Value>,
}

fn set(doc: &'static str, path: &[&str], value: Value) -> Patch {
    Patch {
        doc,
        path: path.iter().map(|s| s.to_string()).collect(),
        value: Some(value),
    }
}

fn remove(doc: &'static str, path: &[&str]) -> Patch {
    Patch {
        doc,
        path: path.iter().map(|s| s.to_string()).collect(),
        value: None,
    }
}

macro_rules! diff_field {
    ($out:ident, $doc:expr, $key:expr, $old:expr, $new:expr, $conv:expr) => {
        if $old != $new {
            $out.push(set($doc, &[$key], $conv(&$new)));
        }
    };
}

fn bot_patches(old: &YunzaiBotConfig, new: &YunzaiBotConfig, out: &mut Vec<Patch>) {
    let s = |v: &String| text_or_null(v);
    let b = |v: &bool| Value::Bool(*v);
    let n = |v: &u32| num(u64::from(*v));
    let l = |v: &Vec<String>| list_value(v);
    diff_field!(out, "bot", "log_level", old.log_level, new.log_level, s);
    diff_field!(out, "bot", "log_length", old.log_length, new.log_length, n);
    diff_field!(out, "bot", "log_object", old.log_object, new.log_object, b);
    diff_field!(out, "bot", "plugin_load_timeout", old.plugin_load_timeout, new.plugin_load_timeout, n);
    diff_field!(out, "bot", "file_watch", old.file_watch, new.file_watch, b);
    diff_field!(out, "bot", "update_time", old.update_time, new.update_time, n);
    diff_field!(out, "bot", "restart_time", old.restart_time, new.restart_time, n);
    diff_field!(out, "bot", "update_cron", old.update_cron, new.update_cron, l);
    diff_field!(out, "bot", "restart_cron", old.restart_cron, new.restart_cron, l);
    diff_field!(out, "bot", "stop_cron", old.stop_cron, new.stop_cron, l);
    diff_field!(out, "bot", "start_cron", old.start_cron, new.start_cron, l);
    diff_field!(out, "bot", "cache_group_member", old.cache_group_member, new.cache_group_member, b);
    diff_field!(out, "bot", "online_msg_exp", old.online_msg_exp, new.online_msg_exp, n);
    diff_field!(out, "bot", "file_to_url_time", old.file_to_url_time, new.file_to_url_time, n);
    diff_field!(out, "bot", "/→#", old.slash_to_hash, new.slash_to_hash, b);
    diff_field!(out, "bot", "chromium_path", old.chromium_path, new.chromium_path, s);
    diff_field!(out, "bot", "puppeteer_ws", old.puppeteer_ws, new.puppeteer_ws, s);
    diff_field!(out, "bot", "puppeteer_timeout", old.puppeteer_timeout, new.puppeteer_timeout,
        |v: &Option<u32>| v.map(|x| num(u64::from(x))).unwrap_or(Value::Null));
    diff_field!(out, "bot", "proxyAddress", old.proxy_address, new.proxy_address, s);
}

fn other_patches(
    old: &YunzaiOtherConfig,
    new: &YunzaiOtherConfig,
    source: Option<&ConfigSource>,
    out: &mut Vec<Patch>,
) {
    let n8 = |v: &u8| num(u64::from(*v));
    let n = |v: &u32| num(u64::from(*v));
    let s = |v: &String| text_or_null(v);
    let ids = |v: &Vec<String>| id_list_value(v);
    diff_field!(out, "other", "autoFriend", old.auto_friend, new.auto_friend, n8);
    diff_field!(out, "other", "autoGroup", old.auto_group, new.auto_group, n8);
    diff_field!(out, "other", "autoQuit", old.auto_quit, new.auto_quit, n);
    // stdin 占位界面上看不到，写回时照原样留在最前面
    let kept = |key: &str, marker: &str| -> Vec<String> {
        source
            .and_then(|s| as_list(get(&s.effective, key)))
            .unwrap_or_default()
            .into_iter()
            .filter(|v| v == marker)
            .collect()
    };
    if old.master_qq != new.master_qq {
        let mut all = kept("masterQQ", STDIN_MASTER);
        all.extend(new.master_qq.iter().cloned());
        out.push(set("other", &["masterQQ"], id_list_value(&all)));
    }
    if old.master != new.master {
        let mut all = kept("master", STDIN_BOT_MASTER);
        all.extend(new.master.iter().map(|s| s.trim().to_string()));
        out.push(set("other", &["master"], list_value(&all)));
    }
    diff_field!(out, "other", "disablePrivate", old.disable_private, new.disable_private, |v: &bool| Value::Bool(*v));
    diff_field!(out, "other", "disableMsg", old.disable_msg, new.disable_msg, s);
    diff_field!(out, "other", "disableAdopt", old.disable_adopt, new.disable_adopt, |v: &Vec<String>| list_value(v));
    diff_field!(out, "other", "whiteGroup", old.white_group, new.white_group, ids);
    diff_field!(out, "other", "whiteUser", old.white_user, new.white_user, ids);
    diff_field!(out, "other", "blackGroup", old.black_group, new.black_group, ids);
    diff_field!(out, "other", "blackUser", old.black_user, new.black_user, ids);
}

fn group_patches(old: &YunzaiGroupConfig, new: &YunzaiGroupConfig, out: &mut Vec<Patch>) {
    let (od, nd) = (&old.default, &new.default);
    let n = |v: &u32| num(u64::from(*v));
    let n8 = |v: &u8| num(u64::from(*v));
    let l = |v: &Vec<String>| list_value(v);
    macro_rules! default_field {
        ($key:expr, $field:ident, $conv:expr) => {
            if od.$field != nd.$field {
                out.push(set("group", &["default", $key], $conv(&nd.$field)));
            }
        };
    }
    default_field!("groupCD", group_cd, n);
    default_field!("singleCD", single_cd, n);
    default_field!("onlyReplyAt", only_reply_at, n8);
    default_field!("botAlias", bot_alias, l);
    default_field!("addLimit", add_limit, n8);
    default_field!("addPrivate", add_private, n8);
    default_field!("addReply", add_reply, n8);
    default_field!("addAt", add_at, n8);
    default_field!("addRecall", add_recall, n);
    default_field!("enable", enable, l);
    default_field!("disable", disable, l);

    for gone in old.overrides.iter().filter(|o| !new.overrides.iter().any(|n| n.key == o.key)) {
        out.push(remove("group", &[gone.key.as_str()]));
    }
    for rule in &new.overrides {
        let key = rule.key.trim();
        let before = old.overrides.iter().find(|o| o.key == rule.key);
        let mut touched = false;
        macro_rules! override_field {
            ($ykey:expr, $field:ident, $conv:expr) => {
                let prev = before.and_then(|b| b.$field.clone());
                if prev != rule.$field {
                    touched = true;
                    match &rule.$field {
                        Some(v) => out.push(set("group", &[key, $ykey], $conv(v))),
                        None => out.push(remove("group", &[key, $ykey])),
                    }
                }
            };
        }
        override_field!("groupCD", group_cd, n);
        override_field!("singleCD", single_cd, n);
        override_field!("onlyReplyAt", only_reply_at, n8);
        override_field!("botAlias", bot_alias, l);
        override_field!("addLimit", add_limit, n8);
        override_field!("addPrivate", add_private, n8);
        override_field!("addReply", add_reply, n8);
        override_field!("addAt", add_at, n8);
        override_field!("addRecall", add_recall, n);
        override_field!("enable", enable, l);
        override_field!("disable", disable, l);
        if before.is_none() && !touched {
            // 新加了一条但什么都没填：先占个空表，免得用户以为没存上
            out.push(set("group", &[key], Value::Mapping(Mapping::new())));
        }
    }
}

/// auth 里 Authorization 那一条换成新 token（空就删掉）；`keep_other` 为假时别的头一并去掉
pub fn auth_value(current: Option<&Value>, token: &str, keep_other: bool) -> Value {
    let mut auth = match current {
        Some(Value::Mapping(m)) if keep_other => m.clone(),
        _ => Mapping::new(),
    };
    let existing: Vec<Value> = auth
        .keys()
        .filter(|k| key_str(k).is_some_and(|s| s.eq_ignore_ascii_case(YUNZAI_AUTH_HEADER)))
        .cloned()
        .collect();
    for k in existing {
        auth.remove(&k);
    }
    if !token.is_empty() {
        auth.insert(
            Value::String(YUNZAI_AUTH_HEADER.into()),
            Value::String(format!("Bearer {token}")),
        );
    }
    if auth.is_empty() {
        Value::Null
    } else {
        Value::Mapping(auth)
    }
}

/// `http://localhost:2536` 里的端口跟着换；用户改成别的样子（没写端口 / 反代地址）就不碰
pub fn url_with_port(url: &str, old_port: u16, new_port: u16) -> Option<String> {
    let marker = format!(":{old_port}");
    let (scheme_end, rest) = url.split_once("://")?;
    let host_part = rest.split('/').next().unwrap_or(rest);
    if !host_part.ends_with(&marker) {
        return None;
    }
    let new_host = format!("{}:{new_port}", &host_part[..host_part.len() - marker.len()]);
    Some(format!("{scheme_end}://{new_host}{}", &rest[host_part.len()..]))
}

fn server_patches(
    old: &YunzaiServerConfig,
    new: &YunzaiServerConfig,
    source: Option<&ConfigSource>,
    out: &mut Vec<Patch>,
) {
    if old.port != new.port {
        out.push(set("server", &["port"], num(u64::from(new.port))));
    }
    if old.url != new.url {
        out.push(set("server", &["url"], text_or_null(&new.url)));
    } else if old.port != new.port {
        if let Some(url) = url_with_port(&new.url, old.port, new.port) {
            out.push(set("server", &["url"], Value::String(url)));
        }
    }
    if old.redirect != new.redirect {
        out.push(set("server", &["redirect"], text_or_null(&new.redirect)));
    }
    if old.access_token != new.access_token {
        let current = source.and_then(|s| get(&s.effective, "auth"));
        out.push(set("server", &["auth"], auth_value(current, &new.access_token, true)));
    }
}

fn redis_patches(old: &YunzaiRedisConfig, new: &YunzaiRedisConfig, out: &mut Vec<Patch>) {
    let s = |v: &String| text_or_null(v);
    diff_field!(out, "redis", "path", old.path, new.path, s);
    diff_field!(out, "redis", "host", old.host, new.host, s);
    diff_field!(out, "redis", "port", old.port, new.port, |v: &u16| num(u64::from(*v)));
    diff_field!(out, "redis", "username", old.username, new.username, s);
    diff_field!(out, "redis", "password", old.password, new.password, s);
    diff_field!(out, "redis", "db", old.db, new.db, |v: &u32| num(u64::from(*v)));
}

/// 盘上现状 → 目标配置，要重写的文件及其新内容（没变的文件不在里面）
pub fn render_changes(
    sources: &[ConfigSource],
    current: &YunzaiInstanceConfig,
    next: &YunzaiInstanceConfig,
) -> Result<Vec<(&'static str, String)>, AppFrameworkError> {
    let src = |name: &str| sources.iter().find(|s| s.name == name);
    let mut patches = Vec::new();
    bot_patches(&current.bot, &next.bot, &mut patches);
    other_patches(&current.other, &next.other, src("other"), &mut patches);
    group_patches(&current.group, &next.group, &mut patches);
    server_patches(&current.server, &next.server, src("server"), &mut patches);
    redis_patches(&current.redis, &next.redis, &mut patches);
    if current.renderer.name != next.renderer.name {
        patches.push(set("renderer", &["name"], text_or_null(&next.renderer.name)));
    }

    let mut out = Vec::new();
    for name in TYPED_NAMES {
        let mine: Vec<&Patch> = patches.iter().filter(|p| p.doc == name).collect();
        if mine.is_empty() {
            continue;
        }
        let source = src(name);
        let mut text = source.map(ConfigSource::base_text).unwrap_or_default();
        for p in mine {
            let path: Vec<&str> = p.path.iter().map(String::as_str).collect();
            text = match &p.value {
                Some(v) => yaml_patch::set_value(&text, &path, v),
                None => yaml_patch::remove_key(&text, &path),
            }
            .map_err(|e| AppFrameworkError::Integration(format!("{}：{e}", config_rel(name))))?;
        }
        if source.and_then(|s| s.user_text.as_deref()) != Some(text.as_str()) {
            out.push((name, text));
        }
    }
    Ok(out)
}

/// 写：校验 → 按字段打补丁 → 备份写 → 回读
pub async fn write_yunzai_config(
    host: &dyn Host,
    install_dir: &HostPath,
    next: &YunzaiInstanceConfig,
    current_snaps: &[DocumentSnapshot],
    write_sidecar: bool,
) -> Result<(YunzaiInstanceConfig, Vec<DocumentSnapshot>), AppFrameworkError> {
    let issues = validate(next);
    if !issues.is_empty() {
        return Err(AppFrameworkError::ConfigInvalid(issues));
    }
    let sources = read_sources(host, install_dir, current_snaps).await?;
    let current = config_from_sources(&sources);
    let changes = render_changes(&sources, &current, next)?;
    if !changes.is_empty() {
        host.create_dir_all(&install_dir.join(YUNZAI_CONFIG_DIR))
            .await
            .map_err(|e| AppFrameworkError::Host(e.to_string()))?;
        let paths: Vec<HostPath> = changes
            .iter()
            .map(|(name, _)| install_dir.join(config_rel(name)))
            .collect();
        apply_with_backup_ex(host, &paths, write_sidecar, || async {
            for (path, (_, text)) in paths.iter().zip(&changes) {
                host.write_file(path, text.as_bytes())
                    .await
                    .map_err(|e| AppFrameworkError::Integration(e.to_string()))?;
            }
            Ok(())
        })
        .await?;
    }
    read_yunzai_config(host, install_dir).await
}

/// 对接依赖的两样：云崽监听口和反向 WS 的 token
pub fn link_inputs_changed(before: &YunzaiInstanceConfig, after: &YunzaiInstanceConfig) -> bool {
    before.server.port != after.server.port || before.server.access_token != after.server.access_token
}

/// 上游只在启动时读的：监听口、Redis、文件监听开关、定时任务（启动时排好的）、插件加载超时
pub fn restart_inputs_changed(before: &YunzaiInstanceConfig, after: &YunzaiInstanceConfig) -> bool {
    let (b, a) = (&before.bot, &after.bot);
    before.server.port != after.server.port
        || before.redis != after.redis
        || b.file_watch != a.file_watch
        || b.update_time != a.update_time
        || b.restart_time != a.restart_time
        || b.update_cron != a.update_cron
        || b.restart_cron != a.restart_cron
        || b.stop_cron != a.stop_cron
        || b.start_cron != a.start_cron
        || b.plugin_load_timeout != a.plugin_load_timeout
}

#[cfg(test)]
mod tests {
    use super::*;

    // .references/TRSS-Yunzai/config/default_config 原文（other / server / group 三份）
    const OTHER: &str = "# 是否自动同意加好友 1-同意 0-不处理\nautoFriend: 1\n# 是否自动同意群邀请 0-仅主人 1-所有人\nautoGroup: 0\n# 是否自动退群人数，当被好友拉进群时，群人数小于配置值自动退出， 默认50，0则不处理\nautoQuit: 50\n# 主人帐号\nmasterQQ:\n  - \"stdin\"\n# Bot账号:主人帐号\nmaster:\n  - \"stdin:stdin\"\n\n# 禁用私聊功能 true：私聊只接受ck以及抽卡链接（Bot主人不受限制），false：私聊可以触发全部指令，默认false\ndisablePrivate: false\n# 禁用私聊Bot提示内容\ndisableMsg: \"私聊功能已禁用，仅支持发送cookie，抽卡记录链接，记录日志文件\"\n# 私聊通行字符串\ndisableAdopt:\n  - stoken\n\n#白名单群\nwhiteGroup:\n#白名单用户\nwhiteUser:\n#黑名单群\nblackGroup:\n  - 213938015\n#黑名单用户\nblackUser:\n  - 528952540";
    const SERVER: &str = "# 服务器地址\nurl: http://localhost:2536\n# 服务器端口\nport: 2536\n# 服务器缺省跳转地址\nredirect: https://git.trss.me/Yunzai\n# 服务器鉴权\nauth:\n# Authorization: Bearer <access_token>\nhttps:\n# url: https://localhost:2537\n# port: 2537\n# key: config/localhost.key\n# cert: config/localhost.crt";
    const GROUP: &str = "# 默认设置\ndefault:\n  groupCD: 500   # 群聊中所有指令操作冷却时间，单位毫秒,0则无限制\n  singleCD: 2000 # 群聊中个人操作冷却时间，单位毫秒\n\n  onlyReplyAt: 0 # 是否只仅关注主动提及Bot的消息 0-否 1-是 2-非主人\n  botAlias:      # 开启后则只回复提及Bot的消息及特定前缀的消息\n    - 云崽\n    - 云宝\n\n  addLimit: 0    # 是否限制添加消息 0-所有群员 1-群管理员 2-主人\n  addPrivate: 1  # 是否允许私聊添加\n  addReply: 1    # 是否回复触发消息\n  addAt: 0       # 是否提及触发用户\n  addRecall: 60  # 是否撤回回复消息\n\n  enable:        # 只启用功能，配置后只有该功能才响应\n  disable:       # 禁用功能，功能名称,例如：十连、角色查询、体力查询、用户绑定、抽卡记录、添加表情、欢迎新人、退群通知\n    - 禁用示例\n    - 支持多个\n\n# Bot单独设置\n114514:default:\n  onlyReplyAt: 1\n  botAlias:\n    - 臭崽\n    - 臭宝\n\n# 群单独设置\n123456:\n  groupCD: 500\n  singleCD: 2000\n\n# [Bot:群]单独设置\n114514:123456:\n  enable:\n  disable:";

    /// 刚装好还没跑过：config/config 里只有桌面端种过的那几份，其余读默认
    fn fresh_sources() -> Vec<ConfigSource> {
        vec![
            ConfigSource::parse("other", None, Some(OTHER.into())).unwrap(),
            ConfigSource::parse("server", Some(SERVER.into()), Some(SERVER.into())).unwrap(),
            ConfigSource::parse("group", Some(GROUP.into()), Some(GROUP.into())).unwrap(),
        ]
    }

    #[test]
    fn upstream_defaults_read_back_as_upstream_default() {
        let cfg = config_from_sources(&fresh_sources());
        let d = YunzaiInstanceConfig::upstream_default();
        assert_eq!(cfg.other.master_qq, Vec::<String>::new(), "stdin 占位不显示");
        assert_eq!(cfg.other.black_group, vec!["213938015".to_string()]);
        assert_eq!(cfg.other.disable_msg, d.other.disable_msg);
        assert_eq!(cfg.server.port, 2536);
        assert_eq!(cfg.server.access_token, "");
        let mut want = d.group.default.clone();
        want.disable = vec!["禁用示例".into(), "支持多个".into()];
        assert_eq!(cfg.group.default, want);
        let keys: Vec<&str> = cfg.group.overrides.iter().map(|o| o.key.as_str()).collect();
        assert_eq!(keys, vec!["114514:default", "123456", "114514:123456"]);
        assert_eq!(cfg.group.overrides[0].only_reply_at, Some(1));
        assert_eq!(cfg.group.overrides[2].enable, Some(Vec::new()));
        assert_eq!(cfg.group.overrides[2].group_cd, None);
    }

    #[test]
    fn master_edit_copies_default_file_keeps_stdin_and_comments() {
        let sources = fresh_sources();
        let current = config_from_sources(&sources);
        let mut next = current.clone();
        next.other.master_qq = vec!["10001".into(), "wx_abc".into()];
        let changes = render_changes(&sources, &current, &next).unwrap();
        assert_eq!(changes.len(), 1);
        let (name, text) = &changes[0];
        assert_eq!(*name, "other");
        assert!(text.contains("# 主人帐号\nmasterQQ:\n  - stdin\n  - 10001\n  - wx_abc\n# Bot账号:主人帐号"));
        assert!(text.contains("# 禁用私聊Bot提示内容"), "抄默认那份时注释都在");
        let parsed = ConfigSource::parse("other", Some(text.clone()), None).unwrap();
        assert_eq!(config_from_sources(&[parsed]).other.master_qq, next.other.master_qq);
    }

    #[test]
    fn port_change_moves_url_and_token_fills_auth() {
        let sources = fresh_sources();
        let current = config_from_sources(&sources);
        let mut next = current.clone();
        next.server.port = 24100;
        next.server.access_token = "tok123".into();
        let changes = render_changes(&sources, &current, &next).unwrap();
        let text = &changes.iter().find(|(n, _)| *n == "server").unwrap().1;
        assert!(text.contains("url: http://localhost:24100\n"));
        assert!(text.contains("port: 24100\n"));
        assert!(text.contains("auth:\n  Authorization: Bearer tok123\n# Authorization: Bearer <access_token>"));
        let parsed = ConfigSource::parse("server", Some(text.clone()), None).unwrap();
        let back = config_from_sources(&[parsed]);
        assert_eq!(back.server.access_token, "tok123");
        assert_eq!(back.server.port, 24100);
        assert!(link_inputs_changed(&current, &back));
        assert!(restart_inputs_changed(&current, &back));
    }

    #[test]
    fn override_add_edit_remove_touch_only_their_lines() {
        let sources = fresh_sources();
        let current = config_from_sources(&sources);
        let mut next = current.clone();
        next.group.overrides.retain(|o| o.key != "123456");
        next.group.overrides[0].bot_alias = Some(vec!["小云".into()]);
        next.group.overrides.push(YunzaiGroupOverride {
            key: "777888".into(),
            only_reply_at: Some(1),
            ..group_override_from("x".into(), &Mapping::new())
        });
        next.group.default.group_cd = 0;
        let changes = render_changes(&sources, &current, &next).unwrap();
        let text = &changes[0].1;
        assert!(text.contains("  groupCD: 0   # 群聊中所有指令操作冷却时间"));
        assert!(!text.contains("\n123456:\n"));
        assert!(text.contains("114514:default:\n  onlyReplyAt: 1\n  botAlias:\n    - 小云\n"));
        assert!(text.ends_with("777888:\n  onlyReplyAt: 1"), "{text}");
        let parsed = ConfigSource::parse("group", Some(text.clone()), None).unwrap();
        let back = config_from_sources(&[parsed]).group;
        assert_eq!(back.default.group_cd, 0);
        assert_eq!(
            back.overrides.iter().map(|o| o.key.as_str()).collect::<Vec<_>>(),
            vec!["114514:default", "114514:123456", "777888"]
        );
    }

    #[test]
    fn unchanged_config_writes_nothing() {
        let sources = fresh_sources();
        let current = config_from_sources(&sources);
        assert!(render_changes(&sources, &current, &current).unwrap().is_empty());
    }

    #[test]
    fn validate_points_at_fields() {
        let mut cfg = YunzaiInstanceConfig::upstream_default();
        cfg.bot.log_level = "loud".into();
        cfg.redis.port = cfg.server.port;
        cfg.other.master = vec!["123".into()];
        cfg.group.overrides = vec![
            group_override_from("default".into(), &Mapping::new()),
            group_override_from("1 2".into(), &Mapping::new()),
        ];
        let paths: Vec<String> = validate(&cfg).into_iter().map(|i| i.path).collect();
        for want in [
            "bot/log_level",
            "redis/port",
            "other/master/0",
            "group/overrides/0/key",
            "group/overrides/1/key",
        ] {
            assert!(paths.iter().any(|p| p == want), "缺 {want}: {paths:?}");
        }
        assert!(validate(&YunzaiInstanceConfig::upstream_default()).is_empty());
    }

    #[test]
    fn url_port_follows_only_when_it_matches() {
        assert_eq!(
            url_with_port("http://localhost:2536", 2536, 3000).as_deref(),
            Some("http://localhost:3000")
        );
        assert_eq!(
            url_with_port("http://1.2.3.4:2536/x", 2536, 3000).as_deref(),
            Some("http://1.2.3.4:3000/x")
        );
        assert_eq!(url_with_port("https://bot.example.com", 2536, 3000), None);
    }

    #[test]
    fn link_auth_drops_other_headers_but_typed_write_keeps_them() {
        let current: Value = serde_yaml::from_str("X-Key: a\nauthorization: Bearer old").unwrap();
        let kept = auth_value(Some(&current), "new", true);
        assert_eq!(kept["X-Key"], Value::String("a".into()));
        assert_eq!(kept["Authorization"], Value::String("Bearer new".into()));
        assert!(kept.get("authorization").is_none());
        let only = auth_value(Some(&current), "new", false);
        assert!(only.get("X-Key").is_none());
        assert_eq!(auth_value(None, "", true), Value::Null);
    }

    #[test]
    fn documents_mark_startup_only_files() {
        let docs = yunzai_config_documents();
        assert_eq!(docs.len(), 9);
        let server = docs.iter().find(|d| d.id == "server").unwrap();
        assert_eq!(server.rel_path, "config/config/server.yaml");
        assert!(!server.hot_reload);
        assert!(docs.iter().find(|d| d.id == "group").unwrap().hot_reload);
        assert!(docs.iter().all(|d| d.format == AppConfigFormat::Yaml));
    }
}
