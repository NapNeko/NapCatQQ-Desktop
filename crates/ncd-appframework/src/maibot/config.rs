//! MaiBot 窄配置：Desktop 只管端口和聊天名单，模型 / 人格 / 插件交给 MaiBot 自己的 WebUI。
//!
//! - `config/bot_config.toml`：`[webui].port`（实例 `port` 跟着它）、`[maim_message].ws_server_port`
//! - `plugins/MaiBot-Napcat-Adapter/config.toml`：`[chat]` 名单；`[plugin]` / `[napcat_server]` 只由对接写
//! - `data/webui.json`：WebUI token，只读
//!
//! 改 TOML 走 toml_edit 就地改值，保留用户注释和桌面端不认识的键。

use ncd_domain::{AppConfigDocument, AppConfigFormat, AppConfigIssue};
use serde::{Deserialize, Serialize};
use toml_edit::{Array, DocumentMut, Item};
use ts_rs::TS;

use super::manifest::{
    ADAPTER_CONFIG, ADAPTER_CONFIG_VERSION, BOT_CONFIG, MAIBOT_DEFAULT_WEBUI_PORT, MODEL_CONFIG,
};

pub const DOC_BOT_CONFIG: &str = "bot_config";
pub const DOC_MODEL_CONFIG: &str = "model_config";
pub const DOC_ADAPTER_CONFIG: &str = "adapter_config";

/// 上游 `MaimMessageConfig.ws_server_port` 默认值
pub const DEFAULT_LEGACY_WS_PORT: u16 = 8000;
/// 适配器 `DEFAULT_NAPCAT_HOST` / `DEFAULT_NAPCAT_PORT`
const DEFAULT_NAPCAT_HOST: &str = "127.0.0.1";
const DEFAULT_NAPCAT_PORT: u16 = 3001;

pub fn maibot_config_documents() -> Vec<AppConfigDocument> {
    vec![
        AppConfigDocument {
            id: DOC_BOT_CONFIG.to_string(),
            label: "主配置 bot_config.toml".to_string(),
            rel_path: BOT_CONFIG.to_string(),
            format: AppConfigFormat::Toml,
            // 上游会热加载这份，但桌面端在这里只改端口，端口是启动时绑的
            hot_reload: false,
        },
        AppConfigDocument {
            id: DOC_MODEL_CONFIG.to_string(),
            label: "模型配置 model_config.toml".to_string(),
            rel_path: MODEL_CONFIG.to_string(),
            format: AppConfigFormat::Toml,
            hot_reload: true,
        },
        AppConfigDocument {
            id: DOC_ADAPTER_CONFIG.to_string(),
            label: "NapCat 适配器 config.toml".to_string(),
            rel_path: ADAPTER_CONFIG.to_string(),
            format: AppConfigFormat::Toml,
            hot_reload: true,
        },
    ]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum MaiBotListMode {
    /// 上游默认：只收名单里的；名单空着就一条都不收
    #[default]
    Whitelist,
    Blacklist,
}

impl MaiBotListMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Whitelist => "whitelist",
            Self::Blacklist => "blacklist",
        }
    }

    fn parse(raw: Option<&str>) -> Self {
        match raw.map(str::trim) {
            Some("blacklist") => Self::Blacklist,
            _ => Self::Whitelist,
        }
    }
}

/// 适配器 `[chat]`：哪些群聊 / 私聊的消息交给麦麦
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotChatFilter {
    pub enable_chat_list_filter: bool,
    pub group_list_type: MaiBotListMode,
    pub group_list: Vec<String>,
    pub private_list_type: MaiBotListMode,
    pub private_list: Vec<String>,
    pub ban_user_id: Vec<String>,
}

impl Default for MaiBotChatFilter {
    fn default() -> Self {
        Self {
            enable_chat_list_filter: true,
            group_list_type: MaiBotListMode::Whitelist,
            group_list: Vec::new(),
            private_list_type: MaiBotListMode::Whitelist,
            private_list: Vec::new(),
            ban_user_id: Vec::new(),
        }
    }
}

impl MaiBotChatFilter {
    /// 白名单开着、群聊私聊名单都空：适配器会把所有消息丢掉，麦麦一句都不回
    pub fn drops_everything(&self) -> bool {
        self.enable_chat_list_filter
            && self.group_list_type == MaiBotListMode::Whitelist
            && self.group_list.is_empty()
            && self.private_list_type == MaiBotListMode::Whitelist
            && self.private_list.is_empty()
    }
}

/// NapCat 适配器插件的现状；连接字段只读，由对接写
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotAdapterConfig {
    pub enabled: bool,
    pub napcat_host: String,
    #[ts(type = "number")]
    pub napcat_port: u16,
    pub has_token: bool,
    pub chat: MaiBotChatFilter,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotInstanceConfig {
    /// `[webui].port`；实例 `port` 跟着它
    #[ts(type = "number")]
    pub webui_port: u16,
    /// `[maim_message].ws_server_port`：旧版消息服务。适配器插件用不上，但上游总会监听，被占就起不来
    #[ts(type = "number")]
    pub legacy_ws_port: u16,
    /// `data/webui.json` 的 access_token；只读，改 token 去 WebUI
    pub webui_token: String,
    /// 适配器插件目录不在时为 None（装坏了或被删了）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub adapter: Option<MaiBotAdapterConfig>,
}

// ---- 读（缺文件 / 缺键一律按上游默认值）----

fn parse_table(text: Option<&str>) -> toml::Table {
    text.and_then(|t| toml::from_str::<toml::Table>(t).ok())
        .unwrap_or_default()
}

fn sub<'a>(table: &'a toml::Table, key: &str) -> Option<&'a toml::Table> {
    table.get(key).and_then(toml::Value::as_table)
}

fn port_of(table: Option<&toml::Table>, key: &str, default: u16) -> u16 {
    table
        .and_then(|t| t.get(key))
        .and_then(toml::Value::as_integer)
        .and_then(|p| u16::try_from(p).ok())
        .filter(|p| *p > 0)
        .unwrap_or(default)
}

/// `(webui_port, legacy_ws_port)`
pub fn read_bot_config_ports(text: Option<&str>) -> (u16, u16) {
    let root = parse_table(text);
    (
        port_of(sub(&root, "webui"), "port", MAIBOT_DEFAULT_WEBUI_PORT),
        port_of(sub(&root, "maim_message"), "ws_server_port", DEFAULT_LEGACY_WS_PORT),
    )
}

pub fn read_webui_token(json: Option<&str>) -> String {
    json.and_then(|t| serde_json::from_str::<serde_json::Value>(t).ok())
        .and_then(|v| v.get("access_token").and_then(|t| t.as_str()).map(str::to_string))
        .unwrap_or_default()
}

/// 名单项上游按字符串存，但手写成数字也认（`_normalize_string_list`）
fn string_list(table: Option<&toml::Table>, key: &str) -> Vec<String> {
    let Some(items) = table.and_then(|t| t.get(key)).and_then(toml::Value::as_array) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|v| match v {
            toml::Value::String(s) => Some(s.trim().to_string()),
            toml::Value::Integer(i) => Some(i.to_string()),
            _ => None,
        })
        .filter(|s| !s.is_empty())
        .collect()
}

fn str_of(t: Option<&toml::Table>, key: &str) -> Option<String> {
    t.and_then(|t| t.get(key))
        .and_then(toml::Value::as_str)
        .map(|s| s.trim().to_string())
}

fn bool_of(t: Option<&toml::Table>, key: &str, default: bool) -> bool {
    t.and_then(|t| t.get(key))
        .and_then(toml::Value::as_bool)
        .unwrap_or(default)
}

pub fn read_adapter_config(text: Option<&str>) -> MaiBotAdapterConfig {
    let root = parse_table(text);
    let plugin = sub(&root, "plugin");
    let server = sub(&root, "napcat_server");
    let chat = sub(&root, "chat");
    MaiBotAdapterConfig {
        enabled: bool_of(plugin, "enabled", false),
        napcat_host: str_of(server, "host")
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| DEFAULT_NAPCAT_HOST.to_string()),
        napcat_port: port_of(server, "port", DEFAULT_NAPCAT_PORT),
        has_token: str_of(server, "token").is_some_and(|s| !s.is_empty()),
        chat: MaiBotChatFilter {
            enable_chat_list_filter: bool_of(chat, "enable_chat_list_filter", true),
            group_list_type: MaiBotListMode::parse(str_of(chat, "group_list_type").as_deref()),
            group_list: string_list(chat, "group_list"),
            private_list_type: MaiBotListMode::parse(str_of(chat, "private_list_type").as_deref()),
            private_list: string_list(chat, "private_list"),
            ban_user_id: string_list(chat, "ban_user_id"),
        },
    }
}

/// 适配器已配置的 NapCat token；空当没有
pub fn read_adapter_token(text: Option<&str>) -> Option<String> {
    let root = parse_table(text);
    str_of(sub(&root, "napcat_server"), "token").filter(|s| !s.is_empty())
}

// ---- 写（就地改值，保留注释、顺序和桌面端不认识的键）----

fn parse_doc(text: Option<&str>) -> Result<DocumentMut, String> {
    text.unwrap_or("")
        .parse::<DocumentMut>()
        .map_err(|e| format!("TOML 语法错误: {e}"))
}

/// 已有的键原地换值：键上方的注释挂在键的 decor 上，行尾注释挂在值上，`insert` 会连键一起换掉。
/// 没有的键才追加
fn set_value(doc: &mut DocumentMut, table: &str, key: &str, mut v: toml_edit::Value) {
    let item = doc
        .entry(table)
        .or_insert(Item::Table(toml_edit::Table::new()));
    if !item.is_table_like() {
        *item = Item::Table(toml_edit::Table::new());
    }
    let Some(t) = item.as_table_like_mut() else {
        return;
    };
    if let Some(old) = t.get_mut(key).and_then(Item::as_value_mut) {
        *v.decor_mut() = old.decor().clone();
        *old = v;
        return;
    }
    t.insert(key, Item::Value(v));
}

fn string_array(items: &[String]) -> toml_edit::Value {
    toml_edit::Value::Array(items.iter().map(|s| s.trim()).collect::<Array>())
}

/// 非空的插件配置缺 `plugin.config_version` 时上游插件运行时直接报错不加载，写之前先补上
fn ensure_adapter_version(doc: &mut DocumentMut) {
    let has = doc
        .get("plugin")
        .and_then(|p| p.get("config_version"))
        .and_then(Item::as_str)
        .is_some_and(|s| !s.trim().is_empty());
    if !has {
        set_value(doc, "plugin", "config_version", ADAPTER_CONFIG_VERSION.into());
    }
}

/// 首装种子也走这里：文件不存在时只写 `[inner].version` 和两个口，其余首启由上游按默认补齐
pub fn write_bot_config_ports(
    text: Option<&str>,
    config_version: &str,
    webui_port: u16,
    legacy_ws_port: u16,
) -> Result<String, String> {
    let mut doc = parse_doc(text)?;
    let has_version = doc
        .get("inner")
        .and_then(|i| i.get("version"))
        .and_then(Item::as_str)
        .is_some();
    if !has_version {
        set_value(&mut doc, "inner", "version", config_version.into());
    }
    set_value(&mut doc, "webui", "port", i64::from(webui_port).into());
    set_value(&mut doc, "maim_message", "ws_server_port", i64::from(legacy_ws_port).into());
    Ok(doc.to_string())
}

pub fn write_adapter_chat(text: Option<&str>, chat: &MaiBotChatFilter) -> Result<String, String> {
    let mut doc = parse_doc(text)?;
    ensure_adapter_version(&mut doc);
    set_value(&mut doc, "chat", "enable_chat_list_filter", chat.enable_chat_list_filter.into());
    set_value(&mut doc, "chat", "group_list_type", chat.group_list_type.as_str().into());
    set_value(&mut doc, "chat", "group_list", string_array(&chat.group_list));
    set_value(&mut doc, "chat", "private_list_type", chat.private_list_type.as_str().into());
    set_value(&mut doc, "chat", "private_list", string_array(&chat.private_list));
    set_value(&mut doc, "chat", "ban_user_id", string_array(&chat.ban_user_id));
    Ok(doc.to_string())
}

/// 对接：打开适配器并指向协议 Bot 的专属 WS 服务
pub fn write_adapter_link(
    text: Option<&str>,
    host: &str,
    port: u16,
    token: &str,
) -> Result<String, String> {
    let mut doc = parse_doc(text)?;
    ensure_adapter_version(&mut doc);
    set_value(&mut doc, "plugin", "enabled", true.into());
    set_value(&mut doc, "napcat_server", "host", host.into());
    set_value(&mut doc, "napcat_server", "port", i64::from(port).into());
    set_value(&mut doc, "napcat_server", "token", token.into());
    Ok(doc.to_string())
}

/// 解绑：关掉适配器，免得它对着删掉的服务端每 5 秒重连刷日志。连接字段留着，重新对接会覆盖
pub fn write_adapter_disabled(text: Option<&str>) -> Result<String, String> {
    let mut doc = parse_doc(text)?;
    ensure_adapter_version(&mut doc);
    set_value(&mut doc, "plugin", "enabled", false.into());
    Ok(doc.to_string())
}

pub fn validate(config: &MaiBotInstanceConfig) -> Vec<AppConfigIssue> {
    let mut issues = Vec::new();
    if config.webui_port == 0 {
        issues.push(AppConfigIssue::new("webui_port", "端口不能为 0"));
    }
    if config.legacy_ws_port == 0 {
        issues.push(AppConfigIssue::new("legacy_ws_port", "端口不能为 0"));
    }
    if config.webui_port != 0 && config.webui_port == config.legacy_ws_port {
        issues.push(AppConfigIssue::new("legacy_ws_port", "不能和 WebUI 用同一个端口"));
    }
    if let Some(adapter) = &config.adapter {
        let chat = &adapter.chat;
        for (path, list, what) in [
            ("adapter.chat.group_list", &chat.group_list, "群号"),
            ("adapter.chat.private_list", &chat.private_list, "QQ 号"),
            ("adapter.chat.ban_user_id", &chat.ban_user_id, "QQ 号"),
        ] {
            if let Some(bad) = list
                .iter()
                .map(|s| s.trim())
                .find(|s| s.is_empty() || !s.chars().all(|c| c.is_ascii_digit()))
            {
                issues.push(AppConfigIssue::new(path, format!("{what}只能是数字：{bad:?}")));
            }
        }
    }
    issues
}

/// 对接输入是适配器的连接字段，类型化配置不让改，所以永远算没变
pub fn link_inputs_changed(_before: &MaiBotInstanceConfig, _after: &MaiBotInstanceConfig) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_files_read_as_upstream_defaults() {
        assert_eq!(read_bot_config_ports(None), (8001, 8000));
        let a = read_adapter_config(None);
        assert!(!a.enabled, "上游默认不连");
        assert_eq!((a.napcat_host.as_str(), a.napcat_port), ("127.0.0.1", 3001));
        assert!(!a.has_token);
        assert!(a.chat.drops_everything(), "默认白名单为空，一条都不收");
        assert_eq!(read_webui_token(None), "");
        assert_eq!(read_adapter_token(None), None);
    }

    #[test]
    fn seed_bot_config_is_minimal_and_reads_back() {
        let text = write_bot_config_ports(None, "8.14.40", 23001, 23002).unwrap();
        let root: toml::Table = toml::from_str(&text).unwrap();
        assert_eq!(root["inner"]["version"].as_str(), Some("8.14.40"));
        assert_eq!(read_bot_config_ports(Some(&text)), (23001, 23002));
        assert_eq!(root.len(), 3, "只写 inner / webui / maim_message：{text}");
    }

    #[test]
    fn port_edit_keeps_comments_version_and_other_keys() {
        let before = "[inner]\nversion = \"8.14.40\"\n\n[bot]\nnickname = \"麦麦\" # 名字\n\n[webui]\n# WebUI 端口\nport = 8001 # 默认\nhost = [\"127.0.0.1\"]\n";
        let after = write_bot_config_ports(Some(before), "9.9.9", 24001, 24002).unwrap();
        assert!(after.contains("version = \"8.14.40\""), "已有版本号不能被桌面端改：{after}");
        assert!(after.contains("nickname = \"麦麦\" # 名字"));
        assert!(after.contains("# WebUI 端口"));
        assert!(after.contains("port = 24001 # 默认"), "{after}");
        assert!(after.contains("host = [\"127.0.0.1\"]"));
        assert_eq!(read_bot_config_ports(Some(&after)), (24001, 24002));
    }

    #[test]
    fn link_write_enables_adapter_and_adds_config_version() {
        let text = write_adapter_link(None, "127.0.0.1", 23456, "tok").unwrap();
        let root: toml::Table = toml::from_str(&text).unwrap();
        assert_eq!(root["plugin"]["config_version"].as_str(), Some("0.1.0"));
        let a = read_adapter_config(Some(&text));
        assert!(a.enabled);
        assert_eq!(a.napcat_port, 23456);
        assert!(a.has_token);
        assert_eq!(read_adapter_token(Some(&text)).as_deref(), Some("tok"));

        let off = write_adapter_disabled(Some(&text)).unwrap();
        let a = read_adapter_config(Some(&off));
        assert!(!a.enabled);
        assert_eq!(a.napcat_port, 23456, "解绑不抹连接字段");
    }

    #[test]
    fn chat_write_keeps_unknown_sections_and_existing_version() {
        let before = "[plugin]\nenabled = true\nconfig_version = \"0.2.0\"\n\n[notice]\npoke = false\n\n[chat]\ngroup_list = [123456, \"7890\"]\n";
        let a = read_adapter_config(Some(before));
        assert_eq!(a.chat.group_list, vec!["123456", "7890"], "手写数字也认");
        let mut chat = a.chat.clone();
        chat.group_list.push("42".into());
        chat.group_list_type = MaiBotListMode::Blacklist;
        let after = write_adapter_chat(Some(before), &chat).unwrap();
        assert!(after.contains("config_version = \"0.2.0\""), "{after}");
        assert!(after.contains("[notice]\npoke = false"));
        let back = read_adapter_config(Some(&after));
        assert_eq!(back.chat.group_list, vec!["123456", "7890", "42"]);
        assert_eq!(back.chat.group_list_type, MaiBotListMode::Blacklist);
        assert!(back.enabled);
    }

    fn cfg(webui: u16, legacy: u16, groups: &[&str]) -> MaiBotInstanceConfig {
        MaiBotInstanceConfig {
            webui_port: webui,
            legacy_ws_port: legacy,
            webui_token: String::new(),
            adapter: Some(MaiBotAdapterConfig {
                enabled: false,
                napcat_host: "127.0.0.1".into(),
                napcat_port: 3001,
                has_token: false,
                chat: MaiBotChatFilter {
                    group_list: groups.iter().map(|s| s.to_string()).collect(),
                    ..MaiBotChatFilter::default()
                },
            }),
        }
    }

    #[test]
    fn validate_ports_and_list_entries() {
        assert!(validate(&cfg(8001, 8000, &["123"])).is_empty());
        let paths: Vec<String> = validate(&cfg(8001, 8001, &["12a"]))
            .into_iter()
            .map(|i| i.path)
            .collect();
        assert_eq!(paths, vec!["legacy_ws_port", "adapter.chat.group_list"]);
        assert_eq!(validate(&cfg(0, 8000, &[]))[0].path, "webui_port");
    }
}
