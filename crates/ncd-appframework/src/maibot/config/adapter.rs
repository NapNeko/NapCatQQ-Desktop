//! NapCat 适配器插件的 `config.toml`：`[chat]` 名单归类型化配置；`[plugin]` / `[napcat_server]`
//! 只由对接写（打开、指向协议 Bot 的专属 WS 服务、带上 token），解绑时关掉。

use serde::{Deserialize, Serialize};
use toml_edit::{Array, DocumentMut};
use ts_rs::TS;

use super::super::manifest::ADAPTER_CONFIG_VERSION;
use super::{parse_doc, parse_table, port_of, set_value, sub};

/// 适配器 `DEFAULT_NAPCAT_HOST` / `DEFAULT_NAPCAT_PORT`
const DEFAULT_NAPCAT_HOST: &str = "127.0.0.1";
const DEFAULT_NAPCAT_PORT: u16 = 3001;

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

fn string_array(items: &[String]) -> toml_edit::Value {
    toml_edit::Value::Array(items.iter().map(|s| s.trim()).collect::<Array>())
}

/// 非空的插件配置缺 `plugin.config_version` 时上游插件运行时直接报错不加载，写之前先补上
fn ensure_adapter_version(doc: &mut DocumentMut) {
    let has = doc
        .get("plugin")
        .and_then(|p| p.get("config_version"))
        .and_then(toml_edit::Item::as_str)
        .is_some_and(|s| !s.trim().is_empty());
    if !has {
        set_value(doc, "plugin", "config_version", ADAPTER_CONFIG_VERSION.into());
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_adapter_file_reads_as_upstream_defaults() {
        let a = read_adapter_config(None);
        assert!(!a.enabled, "上游默认不连");
        assert_eq!((a.napcat_host.as_str(), a.napcat_port), ("127.0.0.1", 3001));
        assert!(!a.has_token);
        assert!(a.chat.drops_everything(), "默认白名单为空，一条都不收");
        assert_eq!(read_adapter_token(None), None);
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
}
