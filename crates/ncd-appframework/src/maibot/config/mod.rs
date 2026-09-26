//! MaiBot 的类型化配置：两份主配置全部字段 + NapCat 适配器的聊天名单 + WebUI token。
//!
//! - `config/bot_config.toml`、`config/model_config.toml`：全字段（结构体由上游类生成，见
//!   `schema`）。停止时 [`files`] 原位差量写，运行中由适配器走麦麦自己的 WebUI 接口写
//! - `plugins/MaiBot-Napcat-Adapter/config.toml`：`[chat]` 名单；`[plugin]` / `[napcat_server]` 只由对接写
//! - `data/webui.json`：WebUI token，只读
//!
//! 实例 `port` 跟着 `bot.webui.port`；两个口都只在这一处，不另存副本。

mod adapter;
pub mod files;
mod validate;

use ncd_domain::{AppConfigDocument, AppConfigFormat};
use serde::{Deserialize, Serialize};
use toml_edit::{DocumentMut, Item};
use ts_rs::TS;

pub use adapter::{
    MaiBotAdapterConfig, MaiBotChatFilter, MaiBotListMode, read_adapter_config, read_adapter_token,
    write_adapter_chat, write_adapter_disabled, write_adapter_link,
};
pub use validate::validate;

use super::manifest::{ADAPTER_CONFIG, BOT_CONFIG, MAIBOT_DEFAULT_WEBUI_PORT, MODEL_CONFIG};
use super::schema::{MaiBotBotConfigFile, MaiBotModelConfigFile};

pub const DOC_BOT_CONFIG: &str = "bot_config";
pub const DOC_MODEL_CONFIG: &str = "model_config";
pub const DOC_ADAPTER_CONFIG: &str = "adapter_config";

/// 上游 `MaimMessageConfig.ws_server_port` 默认值
pub const DEFAULT_LEGACY_WS_PORT: u16 = 8000;

pub fn maibot_config_documents() -> Vec<AppConfigDocument> {
    vec![
        AppConfigDocument {
            id: DOC_BOT_CONFIG.to_string(),
            label: "主配置 bot_config.toml".to_string(),
            rel_path: BOT_CONFIG.to_string(),
            format: AppConfigFormat::Toml,
            // 上游盯着这份热加载；端口、日志这类启动时才读的字段由 restart_inputs_changed 单独判
            hot_reload: true,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct MaiBotInstanceConfig {
    /// `config/bot_config.toml` 全部字段；`webui.port` 就是实例端口
    pub bot: Box<MaiBotBotConfigFile>,
    /// `config/model_config.toml` 全部字段
    pub models: Box<MaiBotModelConfigFile>,
    /// `data/webui.json` 的 access_token；只读
    pub webui_token: String,
    /// 适配器插件目录不在时为 None（装坏了或被删了）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub adapter: Option<MaiBotAdapterConfig>,
}

impl MaiBotInstanceConfig {
    pub fn webui_port(&self) -> u16 {
        u16::try_from(self.bot.webui.port).unwrap_or(0)
    }

    pub fn legacy_ws_port(&self) -> u16 {
        u16::try_from(self.bot.maim_message.ws_server_port).unwrap_or(0)
    }
}

// ---- 共用的 TOML 小工具（适配器那份也用）----

pub(super) fn parse_table(text: Option<&str>) -> toml::Table {
    text.and_then(|t| toml::from_str::<toml::Table>(t).ok())
        .unwrap_or_default()
}

pub(super) fn sub<'a>(table: &'a toml::Table, key: &str) -> Option<&'a toml::Table> {
    table.get(key).and_then(toml::Value::as_table)
}

pub(super) fn port_of(table: Option<&toml::Table>, key: &str, default: u16) -> u16 {
    table
        .and_then(|t| t.get(key))
        .and_then(toml::Value::as_integer)
        .and_then(|p| u16::try_from(p).ok())
        .filter(|p| *p > 0)
        .unwrap_or(default)
}

pub(super) fn parse_doc(text: Option<&str>) -> Result<DocumentMut, String> {
    text.unwrap_or("")
        .parse::<DocumentMut>()
        .map_err(|e| format!("TOML 语法错误: {e}"))
}

/// 已有的键原地换值：键上方的注释挂在键的 decor 上，行尾注释挂在值上，`insert` 会连键一起换掉。
/// 没有的键才追加
pub(super) fn set_value(doc: &mut DocumentMut, table: &str, key: &str, mut v: toml_edit::Value) {
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

// ---- 首装种子 / 对接要的几项 ----

/// `(webui_port, legacy_ws_port)`；装包、对接这些不走类型化配置的地方用
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

/// 首装种子：文件不存在时只写 `[inner].version` 和两个口，其余首启由上游按默认补齐
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

/// 对接输入是适配器的连接字段，类型化配置不让改，所以永远算没变
pub fn link_inputs_changed(_before: &MaiBotInstanceConfig, _after: &MaiBotInstanceConfig) -> bool {
    false
}

/// 上游热加载管不到的字段：启动时绑的口和 host、只在启动时读一次的日志 / 插件运行时 / 控制台开关。
/// 走 WebUI 接口写也一样要重启，所以 WebUI 口也算在内（不只靠编排层的端口变化判断）
pub fn restart_inputs_changed(before: &MaiBotInstanceConfig, after: &MaiBotInstanceConfig) -> bool {
    let (b, a) = (&before.bot, &after.bot);
    b.webui.port != a.webui.port
        || b.webui.enabled != a.webui.enabled
        || b.webui.host != a.webui.host
        || b.webui.allowed_ips != a.webui.allowed_ips
        || b.webui.trusted_proxies != a.webui.trusted_proxies
        || b.webui.trust_xff != a.webui.trust_xff
        || b.maim_message != a.maim_message
        || b.log != a.log
        || b.plugin_runtime.enabled != a.plugin_runtime.enabled
        || b.debug.enable_console_input != a.debug.enable_console_input
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_files_read_as_upstream_defaults() {
        assert_eq!(read_bot_config_ports(None), (8001, 8000));
        assert_eq!(read_webui_token(None), "");
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

    fn cfg() -> MaiBotInstanceConfig {
        MaiBotInstanceConfig {
            bot: Box::default(),
            // 类的默认值里提供商和模型都是空的；首启写出来的那份（DeepSeek）才是用户看到的默认
            models: Box::new(super::super::schema::read_model_config_file(None).unwrap()),
            webui_token: String::new(),
            adapter: Some(MaiBotAdapterConfig {
                enabled: false,
                napcat_host: "127.0.0.1".into(),
                napcat_port: 3001,
                has_token: false,
                chat: MaiBotChatFilter::default(),
            }),
        }
    }

    #[test]
    fn upstream_defaults_pass_validation() {
        assert_eq!(validate(&cfg()), vec![], "上游默认配置本身要能过校验");
    }

    #[test]
    fn validate_ports_lists_and_cross_field_rules() {
        let mut c = cfg();
        c.bot.maim_message.ws_server_port = c.bot.webui.port;
        if let Some(a) = c.adapter.as_mut() {
            a.chat.group_list = vec!["12a".into()];
        }
        c.bot.message_receive.ban_msgs_regex = vec!["(?<=a)b".into(), "(".into()];
        c.models.models[0].api_provider = "不存在".into();
        c.models.model_task_config.utils.model_list = vec!["没有这个".into()];
        let paths: Vec<String> = validate(&c).into_iter().map(|i| i.path).collect();
        assert!(paths.contains(&"bot/maim_message/ws_server_port".to_string()), "{paths:?}");
        assert!(paths.contains(&"adapter/chat/group_list".to_string()));
        assert!(paths.contains(&"bot/message_receive/ban_msgs_regex/1".to_string()));
        assert!(!paths.contains(&"bot/message_receive/ban_msgs_regex/0".to_string()), "环视是合法的 Python 正则");
        assert!(paths.contains(&"models/models/0/api_provider".to_string()));
        assert!(paths.contains(&"models/model_task_config/utils/model_list/0".to_string()));
    }

    #[test]
    fn restart_needed_only_for_startup_bound_fields() {
        let before = cfg();
        let mut after = before.clone();
        after.bot.personality.personality = "换个人格".into();
        after.models.api_providers[0].api_key = "sk".into();
        assert!(!restart_inputs_changed(&before, &after), "人格、模型热生效");
        after.bot.maim_message.ws_server_port += 1;
        assert!(restart_inputs_changed(&before, &after));
    }
}
