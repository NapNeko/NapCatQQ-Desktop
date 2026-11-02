//! MaiBot 安装端口、WebUI 密钥、对接字段与备份清单。

mod adapter;

use ncd_domain::{AppConfigDocument, AppConfigFormat};
use toml_edit::{DocumentMut, Item};

pub use adapter::{
    MaiBotAdapterConfig, MaiBotChatFilter, MaiBotListMode, read_adapter_config, read_adapter_token,
    write_adapter_disabled, write_adapter_link,
};

use super::manifest::{ADAPTER_CONFIG, BOT_CONFIG, MAIBOT_DEFAULT_WEBUI_PORT, MODEL_CONFIG};

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
            hot_reload: true,
            reload_note: Some(
                "多数配置支持自动热重载；监听地址、端口及其他启动项需重启，请查看实例日志确认。"
                    .into(),
            ),
        },
        AppConfigDocument {
            id: DOC_MODEL_CONFIG.to_string(),
            label: "模型配置 model_config.toml".to_string(),
            rel_path: MODEL_CONFIG.to_string(),
            format: AppConfigFormat::Toml,
            hot_reload: true,
            reload_note: None,
        },
        AppConfigDocument {
            id: DOC_ADAPTER_CONFIG.to_string(),
            label: "NapCat 适配器 config.toml".to_string(),
            rel_path: ADAPTER_CONFIG.to_string(),
            format: AppConfigFormat::Toml,
            hot_reload: true,
            reload_note: Some(
                "适配器配置由插件运行时重载；连接设置的生效结果请查看实例日志。".into(),
            ),
        },
    ]
}

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

/// `(webui_port, legacy_ws_port)`；装包、对接这些不走类型化配置的地方用
pub fn read_bot_config_ports(text: Option<&str>) -> (u16, u16) {
    let root = parse_table(text);
    (
        port_of(
            sub(
                &root,
                if root.contains_key("http") {
                    "http"
                } else {
                    "webui"
                },
            ),
            "port",
            MAIBOT_DEFAULT_WEBUI_PORT,
        ),
        port_of(
            sub(&root, "maim_message"),
            "ws_server_port",
            DEFAULT_LEGACY_WS_PORT,
        ),
    )
}

pub fn read_webui_token(json: Option<&str>) -> String {
    json.and_then(|t| serde_json::from_str::<serde_json::Value>(t).ok())
        .and_then(|v| {
            v.get("access_token")
                .and_then(|t| t.as_str())
                .map(str::to_string)
        })
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
    let listen_section = if doc.contains_key("http") {
        "http"
    } else {
        "webui"
    };
    set_value(
        &mut doc,
        listen_section,
        "port",
        i64::from(webui_port).into(),
    );
    set_value(
        &mut doc,
        "maim_message",
        "ws_server_port",
        i64::from(legacy_ws_port).into(),
    );
    Ok(doc.to_string())
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
        assert!(
            after.contains("version = \"8.14.40\""),
            "已有版本号不能被桌面端改：{after}"
        );
        assert!(after.contains("nickname = \"麦麦\" # 名字"));
        assert!(after.contains("# WebUI 端口"));
        assert!(after.contains("port = 24001 # 默认"), "{after}");
        assert!(after.contains("host = [\"127.0.0.1\"]"));
        assert_eq!(read_bot_config_ports(Some(&after)), (24001, 24002));
    }
}
