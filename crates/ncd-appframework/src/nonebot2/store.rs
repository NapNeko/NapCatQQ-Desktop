//! NoneBot2 官方适配器 / 插件代管：改 `[tool.nonebot]`，装包走 `uv add/remove`。
//!
//! `nonebot.load_from_toml` 只加载插件；适配器必须由 `bot.py` 按 toml 动态 `register_adapter`。
//! 启停只改 toml 列表，包可留在 `.venv`。禁用条目记在 `[tool.ncd.nonebot]`，否则卸掉启用位后卡片会变成「未装」。

use std::time::Duration;

use ncd_domain::{AppConfigDocument, AppInstance, AppStoreResource};
use ncd_host::{Host, HostCommand, HostError, HostPath};
use ncd_traits::AppFrameworkError;
use serde::{Deserialize, Serialize};
use toml_edit::{Array, ArrayOfTables, DocumentMut, InlineTable, Item, Table, Value};

use super::component::NoneBot2Component;
use super::config::{DOC_ENV_PROD, nonebot2_config_documents};
use super::driver::{merge_driver, required_forward_mixins};
use super::manifest::{
    DRIVER_FASTAPI, DRIVER_HTTPX, DRIVER_WEBSOCKETS, ENV_DRIVER, NONEBOT2_BOT_PY,
    NONEBOT2_PYPROJECT, NONEBOT2_UV_LOCK, PYPI_ADAPTER_ONEBOT, PYPI_NONEBOT2_FORWARD,
};
use crate::adapter::PluginLogSink;
use crate::adapter::apply_with_backup_ex;
use crate::adopt::write_project_sidecar;
use crate::env_file::EnvFile;
use crate::store::{AppStoreFlavor, AppStoreInstalled, AppStoreMarketEntry};
use crate::uv_tooling::{read_uv_marker, resolve_uv};

pub const ONEBOT_V11_MODULE: &str = "nonebot.adapters.onebot.v11";
pub const NONEBOT_ADAPTERS_URL: &str = "https://registry.nonebot.dev/adapters.json";
pub const NONEBOT_PLUGINS_URL: &str = "https://registry.nonebot.dev/plugins.json";
const CMD_TIMEOUT: Duration = Duration::from_secs(20 * 60);
const PENDING_INSTALL: &str = ".ncd-nonebot-install.json";
const PENDING_INSTALL_TMP: &str = ".ncd-nonebot-install.tmp";
const PENDING_INSTALL_ERROR: &str = ".ncd-nonebot-install.error";

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct PendingInstall {
    resource: AppStoreResource,
    name: String,
    module_name: String,
    package: String,
}

impl PendingInstall {
    fn from_entry(entry: &AppStoreMarketEntry) -> Self {
        Self {
            resource: entry.resource,
            name: entry.name.clone(),
            module_name: entry.module_name.clone(),
            package: entry.package.clone(),
        }
    }
}

/// nb-cli 同序：官网 → jsDelivr → 国内 jsDelivr → gh-proxy。
pub fn nonebot_registry_urls(file: &str) -> Vec<String> {
    let official = match file {
        "adapters.json" => NONEBOT_ADAPTERS_URL.to_string(),
        "plugins.json" => NONEBOT_PLUGINS_URL.to_string(),
        other => format!("https://registry.nonebot.dev/{other}"),
    };
    vec![
        official,
        format!("https://cdn.jsdelivr.net/gh/nonebot/registry@results/{file}"),
        format!("https://jsd.cdn.zzko.cn/gh/nonebot/registry@results/{file}"),
        format!(
            "https://gh-proxy.com/https://raw.githubusercontent.com/nonebot/registry/results/{file}"
        ),
    ]
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogItem {
    pub name: String,
    pub module_name: String,
    pub project_link: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AdapterTomlStyle {
    Inline,
    ArrayOfTables,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PluginTomlStyle {
    Array,
    Table,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoneBotCatalog {
    pub adapters: Vec<CatalogItem>,
    pub plugins: Vec<CatalogItem>,
    adapter_style: AdapterTomlStyle,
    plugin_style: PluginTomlStyle,
}

#[derive(Debug, Deserialize)]
struct RawAdapter {
    module_name: String,
    project_link: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    desc: String,
    #[serde(default)]
    author: String,
    #[serde(default)]
    homepage: String,
    #[serde(default)]
    tags: Vec<RawTag>,
    #[serde(default)]
    is_official: bool,
    #[serde(default)]
    version: String,
    #[serde(default)]
    time: String,
}

#[derive(Debug, Deserialize)]
struct RawPlugin {
    module_name: String,
    project_link: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    desc: String,
    #[serde(default)]
    author: String,
    #[serde(default)]
    homepage: String,
    #[serde(default)]
    tags: Vec<RawTag>,
    #[serde(default)]
    is_official: bool,
    #[serde(default)]
    version: String,
    #[serde(default)]
    time: String,
    #[serde(default)]
    valid: Option<bool>,
    #[serde(default)]
    supported_adapters: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawTag {
    Str(String),
    Obj {
        #[serde(default)]
        label: String,
    },
}

impl RawTag {
    fn label(&self) -> String {
        match self {
            Self::Str(s) => s.clone(),
            Self::Obj { label } => label.clone(),
        }
    }
}

pub fn parse_nonebot_adapters_json(
    text: &str,
) -> Result<Vec<AppStoreMarketEntry>, AppFrameworkError> {
    let rows: Vec<RawAdapter> = parse_json_array(text, "adapters")?;
    Ok(rows
        .into_iter()
        .filter(|r| !r.module_name.trim().is_empty() && !r.project_link.trim().is_empty())
        .map(|r| AppStoreMarketEntry {
            resource: AppStoreResource::Adapter,
            id: r.module_name.clone(),
            name: fallback_name(&r.name, &r.module_name),
            description: r.desc,
            version: r.version,
            author: r.author,
            homepage: r.homepage,
            time: r.time,
            package: r.project_link,
            module_name: r.module_name,
            flavor: AppStoreFlavor::Pypi,
            is_official: r.is_official,
            valid: true,
            tags: r
                .tags
                .into_iter()
                .map(|t| t.label())
                .filter(|s| !s.is_empty())
                .collect(),
            supported_adapters: Vec::new(),
            authors: Vec::new(),
            repos: Vec::new(),
            files: Vec::new(),
            allow_build: Vec::new(),
        })
        .collect())
}

pub fn parse_nonebot_plugins_json(
    text: &str,
) -> Result<Vec<AppStoreMarketEntry>, AppFrameworkError> {
    let rows: Vec<RawPlugin> = parse_json_array(text, "plugins")?;
    Ok(rows
        .into_iter()
        .filter(|r| r.valid.unwrap_or(true))
        .filter(|r| !r.module_name.trim().is_empty() && !r.project_link.trim().is_empty())
        .map(|r| AppStoreMarketEntry {
            resource: AppStoreResource::Plugin,
            id: r.module_name.clone(),
            name: fallback_name(&r.name, &r.module_name),
            description: r.desc,
            version: r.version,
            author: r.author,
            homepage: r.homepage,
            time: r.time,
            package: r.project_link,
            module_name: r.module_name,
            flavor: AppStoreFlavor::Pypi,
            is_official: r.is_official,
            valid: true,
            tags: r
                .tags
                .into_iter()
                .map(|t| t.label())
                .filter(|s| !s.is_empty())
                .collect(),
            supported_adapters: r.supported_adapters.unwrap_or_default(),
            authors: Vec::new(),
            repos: Vec::new(),
            files: Vec::new(),
            allow_build: Vec::new(),
        })
        .collect())
}

fn parse_json_array<T: for<'de> Deserialize<'de>>(
    text: &str,
    key: &str,
) -> Result<Vec<T>, AppFrameworkError> {
    let trimmed = text.trim_start();
    if trimmed.starts_with('[') {
        return serde_json::from_str(trimmed)
            .map_err(|e| AppFrameworkError::Validation(format!("解析官方目录失败: {e}")));
    }
    let v: serde_json::Value = serde_json::from_str(text)
        .map_err(|e| AppFrameworkError::Validation(format!("解析官方目录失败: {e}")))?;
    let arr = v
        .get(key)
        .cloned()
        .or_else(|| v.as_array().cloned().map(serde_json::Value::Array))
        .ok_or_else(|| AppFrameworkError::Validation("官方目录格式无法识别".into()))?;
    serde_json::from_value(arr)
        .map_err(|e| AppFrameworkError::Validation(format!("解析官方目录失败: {e}")))
}

fn fallback_name(name: &str, module: &str) -> String {
    let t = name.trim();
    if t.is_empty() {
        module.to_string()
    } else {
        t.to_string()
    }
}

pub fn parse_catalog(text: &str) -> Result<NoneBotCatalog, AppFrameworkError> {
    let doc = text
        .parse::<DocumentMut>()
        .map_err(|e| AppFrameworkError::Integration(format!("pyproject.toml 解析失败: {e}")))?;
    Ok(read_catalog(&doc))
}

pub fn apply_catalog(text: &str, catalog: &NoneBotCatalog) -> Result<String, AppFrameworkError> {
    let mut doc = text
        .parse::<DocumentMut>()
        .map_err(|e| AppFrameworkError::Integration(format!("pyproject.toml 解析失败: {e}")))?;
    write_catalog(&mut doc, catalog);
    Ok(doc.to_string())
}

fn detect_adapter_style(doc: &DocumentMut) -> AdapterTomlStyle {
    if tool_table(doc, &["nonebot"])
        .and_then(|t| t.get("adapters"))
        .and_then(|v| v.as_array_of_tables())
        .is_some()
    {
        AdapterTomlStyle::ArrayOfTables
    } else {
        AdapterTomlStyle::Inline
    }
}

fn detect_plugin_style(doc: &DocumentMut) -> PluginTomlStyle {
    if tool_table(doc, &["nonebot"])
        .and_then(|t| t.get("plugins"))
        .and_then(|v| v.as_table())
        .is_some()
    {
        PluginTomlStyle::Table
    } else {
        PluginTomlStyle::Array
    }
}

fn read_catalog(doc: &DocumentMut) -> NoneBotCatalog {
    // ncd 只记禁用项和 project_link；启用列表以 [tool.nonebot] 为准，避免下次写入洗掉用户/nb-cli 的改动
    let mut catalog = if let Some(ncd) = tool_table(doc, &["ncd", "nonebot"]) {
        NoneBotCatalog {
            adapters: parse_ncd_items(ncd, "adapters"),
            plugins: parse_ncd_items(ncd, "plugins"),
            adapter_style: detect_adapter_style(doc),
            plugin_style: detect_plugin_style(doc),
        }
    } else {
        NoneBotCatalog {
            adapters: Vec::new(),
            plugins: Vec::new(),
            adapter_style: detect_adapter_style(doc),
            plugin_style: detect_plugin_style(doc),
        }
    };
    overlay_enabled(&mut catalog.adapters, &parse_tool_adapters(doc));
    overlay_enabled(&mut catalog.plugins, &parse_tool_plugins(doc));
    catalog
}

fn overlay_enabled(catalog: &mut Vec<CatalogItem>, enabled: &[CatalogItem]) {
    let enabled_ids: std::collections::HashSet<&str> =
        enabled.iter().map(|i| i.module_name.as_str()).collect();
    for item in catalog.iter_mut() {
        item.enabled = enabled_ids.contains(item.module_name.as_str());
        if item.project_link.is_empty()
            && let Some(src) = enabled
                .iter()
                .find(|e| e.module_name == item.module_name && !e.project_link.is_empty())
        {
            item.project_link = src.project_link.clone();
        }
    }
    for src in enabled {
        if !catalog.iter().any(|c| c.module_name == src.module_name) {
            catalog.push(src.clone());
        }
    }
}

fn adapter_from_fields(module_name: &str, name: Option<&str>) -> Option<CatalogItem> {
    let module_name = module_name.trim();
    if module_name.is_empty() {
        return None;
    }
    Some(CatalogItem {
        project_link: guess_adapter_package(module_name),
        name: name
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(module_name)
            .to_string(),
        module_name: module_name.to_string(),
        enabled: true,
    })
}

fn parse_tool_adapters(doc: &DocumentMut) -> Vec<CatalogItem> {
    let Some(nb) = tool_table(doc, &["nonebot"]) else {
        return Vec::new();
    };
    if let Some(arr) = nb.get("adapters").and_then(|v| v.as_array()) {
        return arr
            .iter()
            .filter_map(|v| {
                let table = v.as_inline_table()?;
                adapter_from_fields(
                    table.get("module_name")?.as_str()?,
                    table.get("name").and_then(|x| x.as_str()),
                )
            })
            .collect();
    }
    if let Some(aot) = nb.get("adapters").and_then(|v| v.as_array_of_tables()) {
        return aot
            .iter()
            .filter_map(|table| {
                adapter_from_fields(
                    table.get("module_name")?.as_str()?,
                    table.get("name").and_then(|x| x.as_str()),
                )
            })
            .collect();
    }
    Vec::new()
}

fn parse_tool_plugins(doc: &DocumentMut) -> Vec<CatalogItem> {
    let Some(nb) = tool_table(doc, &["nonebot"]) else {
        return Vec::new();
    };
    let Some(item) = nb.get("plugins") else {
        return Vec::new();
    };
    if let Some(arr) = item.as_array() {
        return arr
            .iter()
            .filter_map(|v| plugin_from_module(v.as_str()?))
            .collect();
    }
    if let Some(table) = item.as_table() {
        let mut out: Vec<CatalogItem> = Vec::new();
        for (pkg, val) in table.iter() {
            let Some(arr) = val.as_array() else {
                continue;
            };
            for v in arr {
                if let Some(mut item) = v.as_str().and_then(plugin_from_module) {
                    if !pkg.starts_with('@') && item.project_link.is_empty() {
                        item.project_link = pkg.to_string();
                    }
                    if !out.iter().any(|c| c.module_name == item.module_name) {
                        out.push(item);
                    }
                }
            }
        }
        return out;
    }
    Vec::new()
}

fn plugin_from_module(module_name: &str) -> Option<CatalogItem> {
    let module_name = module_name.trim();
    if module_name.is_empty() {
        return None;
    }
    Some(CatalogItem {
        name: module_name.to_string(),
        project_link: String::new(),
        module_name: module_name.to_string(),
        enabled: true,
    })
}

fn parse_ncd_items(table: &Table, key: &str) -> Vec<CatalogItem> {
    let Some(arr) = table.get(key).and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|v| {
            let table = v.as_inline_table()?;
            let module_name = table.get("module_name")?.as_str()?.to_string();
            let name = table
                .get("name")
                .and_then(|x| x.as_str())
                .unwrap_or(&module_name)
                .to_string();
            Some(CatalogItem {
                project_link: table
                    .get("project_link")
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_string(),
                enabled: table
                    .get("enabled")
                    .and_then(|x| x.as_bool())
                    .unwrap_or(true),
                name,
                module_name,
            })
        })
        .collect()
}

fn write_catalog(doc: &mut DocumentMut, catalog: &NoneBotCatalog) {
    {
        let nonebot = ensure_table_path(doc, &["tool", "nonebot"]);
        match catalog.adapter_style {
            AdapterTomlStyle::Inline => {
                write_inline_or_replace(
                    nonebot,
                    "adapters",
                    enabled_adapter_array(&catalog.adapters),
                );
            }
            AdapterTomlStyle::ArrayOfTables => write_adapters_aot(nonebot, &catalog.adapters),
        }
        match catalog.plugin_style {
            PluginTomlStyle::Array => {
                write_inline_or_replace(nonebot, "plugins", enabled_plugin_array(&catalog.plugins));
            }
            PluginTomlStyle::Table => write_plugins_table(nonebot, &catalog.plugins),
        }
    }
    let ncd = ensure_table_path(doc, &["tool", "ncd", "nonebot"]);
    write_inline_or_replace(ncd, "adapters", catalog_array(&catalog.adapters));
    write_inline_or_replace(ncd, "plugins", catalog_array(&catalog.plugins));
}

fn enabled_adapter_array(items: &[CatalogItem]) -> Array {
    let mut arr = Array::new();
    for item in items.iter().filter(|i| i.enabled) {
        let mut table = InlineTable::new();
        table.insert("name", Value::from(item.name.as_str()));
        table.insert("module_name", Value::from(item.module_name.as_str()));
        arr.push(Value::InlineTable(table));
    }
    arr
}

fn enabled_plugin_array(items: &[CatalogItem]) -> Array {
    let mut arr = Array::new();
    for item in items.iter().filter(|i| i.enabled) {
        arr.push(item.module_name.as_str());
    }
    arr
}

fn catalog_array(items: &[CatalogItem]) -> Array {
    let mut arr = Array::new();
    for item in items {
        let mut table = InlineTable::new();
        table.insert("name", Value::from(item.name.as_str()));
        table.insert("module_name", Value::from(item.module_name.as_str()));
        table.insert("project_link", Value::from(item.project_link.as_str()));
        table.insert("enabled", Value::from(item.enabled));
        arr.push(Value::InlineTable(table));
    }
    arr
}

fn write_inline_or_replace(table: &mut Table, key: &str, arr: Array) {
    if let Some(existing) = table.get_mut(key).and_then(|item| item.as_array_mut()) {
        *existing = arr;
        return;
    }
    table.insert(key, Item::Value(Value::Array(arr)));
}

fn write_adapters_aot(nonebot: &mut Table, items: &[CatalogItem]) {
    let enabled: Vec<&CatalogItem> = items.iter().filter(|i| i.enabled).collect();
    if let Some(aot) = nonebot
        .get_mut("adapters")
        .and_then(Item::as_array_of_tables_mut)
    {
        let mut i = 0;
        while i < aot.len() {
            let module = aot
                .get(i)
                .and_then(|t| t.get("module_name"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if let Some(item) = enabled.iter().find(|e| e.module_name == module) {
                if let Some(t) = aot.get_mut(i)
                    && t.get("name").and_then(|v| v.as_str()) != Some(item.name.as_str())
                {
                    t.insert("name", Item::Value(Value::from(item.name.as_str())));
                }
                i += 1;
            } else {
                aot.remove(i);
            }
        }
        for item in enabled {
            let exists = aot.iter().any(|t| {
                t.get("module_name").and_then(|v| v.as_str()) == Some(item.module_name.as_str())
            });
            if exists {
                continue;
            }
            aot.push(adapter_table(item));
        }
        return;
    }
    let mut aot = ArrayOfTables::new();
    for item in enabled {
        aot.push(adapter_table(item));
    }
    nonebot.insert("adapters", Item::ArrayOfTables(aot));
}

fn adapter_table(item: &CatalogItem) -> Table {
    let mut t = Table::new();
    t.insert("name", Item::Value(Value::from(item.name.as_str())));
    t.insert(
        "module_name",
        Item::Value(Value::from(item.module_name.as_str())),
    );
    t
}

fn write_plugins_table(nonebot: &mut Table, items: &[CatalogItem]) {
    let enabled: Vec<&CatalogItem> = items.iter().filter(|i| i.enabled).collect();
    let enabled_mods: std::collections::HashSet<&str> =
        enabled.iter().map(|i| i.module_name.as_str()).collect();
    if nonebot.get("plugins").and_then(Item::as_table).is_none() {
        nonebot.insert("plugins", Item::Table(Table::new()));
    }
    let Some(plugins) = nonebot.get_mut("plugins").and_then(Item::as_table_mut) else {
        return;
    };

    let keys: Vec<String> = plugins.iter().map(|(k, _)| k.to_string()).collect();
    for key in keys {
        let empty = {
            let Some(arr) = plugins.get_mut(key.as_str()).and_then(Item::as_array_mut) else {
                continue;
            };
            let mut idx = 0;
            while idx < arr.len() {
                let keep = arr
                    .get(idx)
                    .and_then(|v| v.as_str())
                    .is_some_and(|s| enabled_mods.contains(s));
                if keep {
                    idx += 1;
                } else {
                    arr.remove(idx);
                }
            }
            arr.is_empty()
        };
        if empty {
            plugins.remove(key.as_str());
        }
    }

    for item in enabled {
        let present = plugins.iter().any(|(_, v)| {
            v.as_array().is_some_and(|a| {
                a.iter()
                    .any(|x| x.as_str() == Some(item.module_name.as_str()))
            })
        });
        if present {
            continue;
        }
        let key = if item.project_link.is_empty() {
            item.module_name.as_str()
        } else {
            item.project_link.as_str()
        };
        if let Some(arr) = plugins.get_mut(key).and_then(Item::as_array_mut) {
            arr.push(item.module_name.as_str());
        } else {
            let mut arr = Array::new();
            arr.push(item.module_name.as_str());
            plugins.insert(key, Item::Value(Value::Array(arr)));
        }
    }
}

fn tool_table<'a>(doc: &'a DocumentMut, rest: &[&str]) -> Option<&'a Table> {
    let mut cur = doc.get("tool")?.as_table()?;
    for key in rest {
        cur = cur.get(key)?.as_table()?;
    }
    Some(cur)
}

// 循环里每一层不是表就换成表，最后一层（空路径时是文档根）一定是表，expect 不会触发
#[allow(clippy::expect_used)]
fn ensure_table_path<'a>(doc: &'a mut DocumentMut, path: &[&str]) -> &'a mut Table {
    let mut item = doc.as_item_mut();
    for key in path {
        if item.get(key).map(|c| c.as_table().is_some()) != Some(true) {
            let mut t = Table::new();
            t.set_implicit(true);
            item[key] = Item::Table(t);
        }
        item = &mut item[key];
    }
    item.as_table_mut()
        .expect("ensure_table_path always creates a table")
}

pub fn guess_adapter_package(module_name: &str) -> String {
    if module_name.starts_with("nonebot.adapters.onebot.") {
        return PYPI_ADAPTER_ONEBOT.to_string();
    }
    if let Some(rest) = module_name.strip_prefix("nonebot.adapters.") {
        let first = rest.split('.').next().unwrap_or(rest);
        if !first.is_empty() {
            return format!("nonebot-adapter-{first}");
        }
    }
    String::new()
}

pub fn package_still_needed(catalog: &[CatalogItem], package: &str, except_module: &str) -> bool {
    let pkg = package.trim();
    if pkg.is_empty() {
        return false;
    }
    catalog.iter().any(|item| {
        item.module_name != except_module
            && !item.project_link.is_empty()
            && item.project_link == pkg
    })
}

pub fn refuse_linked_onebot_v11(
    instance: &AppInstance,
    module_name: &str,
) -> Result<(), AppFrameworkError> {
    if instance.link.is_some() && module_name == ONEBOT_V11_MODULE {
        return Err(AppFrameworkError::Validation(
            "已对接协议 Bot，不能禁用或卸载 OneBot V11".into(),
        ));
    }
    Ok(())
}

fn item_matches(item: &CatalogItem, id: &str) -> bool {
    item.module_name == id || item.name == id
}

fn resolve_catalog_item<'a>(
    list: &'a [CatalogItem],
    id: &str,
) -> Result<&'a CatalogItem, AppFrameworkError> {
    list.iter()
        .find(|i| item_matches(i, id))
        .ok_or_else(|| AppFrameworkError::Validation(format!("未安装该条目: {id}")))
}

pub async fn list_installed(
    host: &dyn Host,
    instance: &AppInstance,
    resource: AppStoreResource,
) -> Result<Vec<AppStoreInstalled>, AppFrameworkError> {
    let root = HostPath::from_posix(&instance.install_dir);
    let text = read_text(host, &root.join(NONEBOT2_PYPROJECT))
        .await?
        .ok_or_else(|| {
            AppFrameworkError::Integration("NoneBot2 实例缺少 pyproject.toml，请先完成安装".into())
        })?;
    let catalog = parse_catalog(&text)?;
    let lock = read_text(host, &root.join("uv.lock")).await?;
    let items = match resource {
        AppStoreResource::Adapter => &catalog.adapters,
        AppStoreResource::Plugin => &catalog.plugins,
    };
    Ok(items
        .iter()
        .map(|item| AppStoreInstalled {
            id: item.module_name.clone(),
            name: item.name.clone(),
            resource,
            flavor: AppStoreFlavor::Pypi,
            version: lock.as_deref().and_then(|t| {
                if item.project_link.is_empty() {
                    None
                } else {
                    NoneBot2Component::lock_package_version(t, &item.project_link)
                }
            }),
            enabled: item.enabled,
            package: item.project_link.clone(),
            locked: false,
        })
        .collect())
}

pub async fn install_item(
    host: &dyn Host,
    instance: &AppInstance,
    entry: &AppStoreMarketEntry,
    log: Option<&PluginLogSink>,
) -> Result<(), AppFrameworkError> {
    reject_package(&entry.package)?;
    let pending = PendingInstall::from_entry(entry);
    if reconcile_pending_install(host, instance, log)
        .await?
        .as_ref()
        == Some(&pending)
    {
        return Ok(());
    }
    let root = HostPath::from_posix(&instance.install_dir);
    let bytes = serde_json::to_vec(&pending)
        .map_err(|e| AppFrameworkError::Integration(format!("序列化安装恢复记录失败: {e}")))?;
    // 先持久化意图再动依赖；临时文件写失败不会留下损坏的正式记录。
    let temporary = root.join(PENDING_INSTALL_TMP);
    host.write_file(&temporary, &bytes)
        .await
        .map_err(host_err)?;
    host.rename(&temporary, &root.join(PENDING_INSTALL))
        .await
        .map_err(host_err)?;
    reconcile_pending_install(host, instance, log).await?;
    Ok(())
}

// 调用方的商店变更入口持有 framework_config_gate；列表保持只读，不能从那里恢复。
async fn reconcile_pending_install(
    host: &dyn Host,
    instance: &AppInstance,
    log: Option<&PluginLogSink>,
) -> Result<Option<PendingInstall>, AppFrameworkError> {
    let root = HostPath::from_posix(&instance.install_dir);
    let Some(text) = read_text(host, &root.join(PENDING_INSTALL)).await? else {
        return Ok(None);
    };
    let pending: PendingInstall = serde_json::from_str(&text).map_err(|e| {
        AppFrameworkError::Integration(format!("安装恢复记录损坏，未覆盖原记录: {e}"))
    })?;
    reject_package(&pending.package)?;
    let error_path = root.join(PENDING_INSTALL_ERROR);
    let previous_error = read_text(host, &error_path).await?;
    emit_log(log, format!("恢复未完成的安装登记: {}", pending.name));
    let mut discarded_failed_add = false;
    let result = async {
        ensure_dynamic_bot_py(host, instance, log).await?;
        let project_path = root.join(NONEBOT2_PYPROJECT);
        let lock_path = root.join(NONEBOT2_UV_LOCK);
        let project_bytes = read_file_snapshot(host, &project_path)
            .await?
            .ok_or_else(|| {
                AppFrameworkError::Integration("NoneBot2 实例缺少 pyproject.toml".into())
            })?;
        let project = String::from_utf8_lossy(&project_bytes);
        let lock_bytes = read_file_snapshot(host, &lock_path).await?;
        let lock = lock_bytes.as_deref().map(String::from_utf8_lossy);
        if dependency_is_present(&project, &pending.package)?
            && lock.as_deref().is_some_and(|text| {
                let (name, _) = package_parts(&pending.package);
                NoneBot2Component::lock_package_version(text, &normalize_pypi_name(name)).is_some()
            })
        {
            // add 已成功或包原本就存在时，只按现有锁同步，不能重新 add 升级旧版本。
            run_uv(host, &root, &["sync", "--locked", "--inexact"], log)
                .await
                .map_err(uv_runtime_err)?;
        } else if let Err(error) =
            uv_add_with_cleanup(host, &root, &pending.package, false, false, log).await
        {
            let unchanged = async {
                Ok::<_, AppFrameworkError>(
                    read_file_snapshot(host, &project_path).await?.as_ref() == Some(&project_bytes)
                        && read_file_snapshot(host, &lock_path).await? == lock_bytes,
                )
            }
            .await
            .map_err(|snapshot_error| {
                AppFrameworkError::Runtime(format!(
                    "{error}；检查安装副作用失败: {snapshot_error}"
                ))
            })?;
            if unchanged {
                // 无项目/锁变更的失败没有登记可恢复，不能永远阻断其它商店操作。
                clear_pending_install(host, &root)
                    .await
                    .map_err(|cleanup_error| {
                        AppFrameworkError::Runtime(format!(
                            "{error}；清理未生效的安装恢复记录失败: {cleanup_error}"
                        ))
                    })?;
                discarded_failed_add = true;
            }
            return Err(error);
        }
        mutate_catalog(host, instance, false, |catalog| {
            upsert_item(
                catalog_list_mut(catalog, pending.resource),
                CatalogItem {
                    name: pending.name.clone(),
                    module_name: pending.module_name.clone(),
                    project_link: pending.package.clone(),
                    enabled: true,
                },
            );
        })
        .await?;
        if pending.resource == AppStoreResource::Adapter {
            ensure_forward_driver_with_cleanup(host, &root, false, log).await?;
        }
        // catalog 和 driver 都完成后才清除；清除失败仍可按现有锁重试。
        clear_pending_install(host, &root).await
    }
    .await;
    if let Err(error) = result {
        if discarded_failed_add {
            return Err(error);
        }
        let context = match previous_error {
            Some(previous) => format!("首次安装失败: {previous}；恢复失败: {error}"),
            None => error.to_string(),
        };
        let record_error = match host.exists(&error_path).await {
            Ok(true) => None,
            Ok(false) => host.write_file(&error_path, context.as_bytes()).await.err(),
            Err(error) => Some(error),
        };
        let record_context = record_error
            .map(|e| format!("；保存失败上下文也失败: {e}"))
            .unwrap_or_default();
        return Err(AppFrameworkError::Runtime(format!(
            "{context}{record_context}；安装恢复记录已保留，下次商店变更操作将重试"
        )));
    }
    Ok(Some(pending))
}

async fn read_file_snapshot(
    host: &dyn Host,
    path: &HostPath,
) -> Result<Option<Vec<u8>>, AppFrameworkError> {
    if !host.exists(path).await.map_err(host_err)? {
        return Ok(None);
    }
    Ok(Some(host.read_file(path).await.map_err(host_err)?.to_vec()))
}

async fn clear_pending_install(host: &dyn Host, root: &HostPath) -> Result<(), AppFrameworkError> {
    let error_path = root.join(PENDING_INSTALL_ERROR);
    if host.exists(&error_path).await.map_err(host_err)? {
        host.remove_file(&error_path).await.map_err(host_err)?;
    }
    host.remove_file(&root.join(PENDING_INSTALL))
        .await
        .map_err(host_err)
}

fn package_parts(package: &str) -> (&str, Vec<&str>) {
    let name_end = package
        .find(|c: char| !c.is_ascii_alphanumeric() && !matches!(c, '-' | '_' | '.'))
        .unwrap_or(package.len());
    let rest = &package[name_end..];
    let extras = rest
        .strip_prefix('[')
        .and_then(|text| text.split_once(']'))
        .map(|(extras, _)| extras.split(',').map(str::trim).collect())
        .unwrap_or_default();
    (&package[..name_end], extras)
}

fn dependency_is_present(project: &str, package: &str) -> Result<bool, AppFrameworkError> {
    let doc = project
        .parse::<DocumentMut>()
        .map_err(|e| AppFrameworkError::Integration(format!("pyproject.toml 解析失败: {e}")))?;
    let (name, extras) = package_parts(package);
    Ok(doc
        .get("project")
        .and_then(|project| project.get("dependencies"))
        .and_then(Item::as_array)
        .is_some_and(|dependencies| {
            dependencies.iter().filter_map(Value::as_str).any(|spec| {
                let (installed_name, installed_extras) = package_parts(spec.trim());
                !spec.contains(';')
                    && same_pypi_name(name, installed_name)
                    && extras.iter().all(|extra| {
                        installed_extras
                            .iter()
                            .any(|installed| same_pypi_name(extra, installed))
                    })
            })
        }))
}

pub async fn update_item(
    host: &dyn Host,
    instance: &AppInstance,
    entry: &AppStoreMarketEntry,
    log: Option<&PluginLogSink>,
) -> Result<(), AppFrameworkError> {
    reject_package(&entry.package)?;
    reconcile_pending_install(host, instance, log).await?;
    ensure_dynamic_bot_py(host, instance, log).await?;
    uv_add_at(
        host,
        &HostPath::from_posix(&instance.install_dir),
        &entry.package,
        true,
        log,
    )
    .await?;
    mutate_catalog(host, instance, false, |catalog| {
        let list = catalog_list_mut(catalog, entry.resource);
        upsert_item(
            list,
            CatalogItem {
                name: entry.name.clone(),
                module_name: entry.module_name.clone(),
                project_link: entry.package.clone(),
                enabled: true,
            },
        );
    })
    .await?;
    if entry.resource == AppStoreResource::Adapter {
        ensure_forward_driver(host, instance, log).await?;
    }
    Ok(())
}

pub async fn uninstall_item(
    host: &dyn Host,
    instance: &AppInstance,
    id: &str,
    resource: AppStoreResource,
    log: Option<&PluginLogSink>,
) -> Result<(), AppFrameworkError> {
    reconcile_pending_install(host, instance, log).await?;
    let catalog = load_catalog(host, instance).await?;
    let list = match resource {
        AppStoreResource::Adapter => &catalog.adapters,
        AppStoreResource::Plugin => &catalog.plugins,
    };
    let item = resolve_catalog_item(list, id)?;
    refuse_linked_onebot_v11(instance, &item.module_name)?;
    let module_name = item.module_name.clone();
    let mut remove_pkg: Option<String> = None;
    mutate_catalog(host, instance, false, |catalog| {
        let list = catalog_list_mut(catalog, resource);
        if let Some(idx) = list.iter().position(|i| i.module_name == module_name) {
            let item = list.remove(idx);
            let others = match resource {
                AppStoreResource::Adapter => catalog.adapters.as_slice(),
                AppStoreResource::Plugin => catalog.plugins.as_slice(),
            };
            if !item.project_link.is_empty()
                && !package_still_needed(others, &item.project_link, &item.module_name)
                && !package_still_needed(
                    match resource {
                        AppStoreResource::Adapter => &catalog.plugins,
                        AppStoreResource::Plugin => &catalog.adapters,
                    },
                    &item.project_link,
                    &item.module_name,
                )
            {
                remove_pkg = Some(item.project_link);
            }
        }
    })
    .await?;
    if let Some(pkg) = remove_pkg {
        uv_remove_at(
            host,
            &HostPath::from_posix(&instance.install_dir),
            &pkg,
            log,
        )
        .await?;
    }
    Ok(())
}

pub async fn set_enabled(
    host: &dyn Host,
    instance: &AppInstance,
    id: &str,
    resource: AppStoreResource,
    enabled: bool,
    overwrite: bool,
) -> Result<(), AppFrameworkError> {
    reconcile_pending_install(host, instance, None).await?;
    let catalog = load_catalog(host, instance).await?;
    let list = match resource {
        AppStoreResource::Adapter => &catalog.adapters,
        AppStoreResource::Plugin => &catalog.plugins,
    };
    let item = resolve_catalog_item(list, id)?;
    if !enabled {
        refuse_linked_onebot_v11(instance, &item.module_name)?;
    }
    let module_name = item.module_name.clone();
    if resource == AppStoreResource::Adapter {
        ensure_dynamic_bot_py(host, instance, None).await?;
    }
    mutate_catalog(host, instance, overwrite, |catalog| {
        let list = catalog_list_mut(catalog, resource);
        if let Some(item) = list.iter_mut().find(|i| i.module_name == module_name) {
            item.enabled = enabled;
        }
    })
    .await?;
    if resource == AppStoreResource::Adapter && enabled {
        ensure_forward_driver(host, instance, None).await?;
    }
    Ok(())
}

pub fn plugin_config_docs() -> Vec<AppConfigDocument> {
    nonebot2_config_documents()
        .into_iter()
        .filter(|d| d.id == DOC_ENV_PROD)
        .collect()
}

pub async fn ensure_dynamic_bot_py(
    host: &dyn Host,
    instance: &AppInstance,
    log: Option<&PluginLogSink>,
) -> Result<(), AppFrameworkError> {
    if instance.origin.is_imported() {
        return Ok(());
    }
    ensure_dynamic_bot_py_at(host, &HostPath::from_posix(&instance.install_dir), log).await
}

pub async fn ensure_dynamic_bot_py_at(
    host: &dyn Host,
    root: &HostPath,
    log: Option<&PluginLogSink>,
) -> Result<(), AppFrameworkError> {
    let path = root.join(NONEBOT2_BOT_PY);
    let existing = read_text(host, &path).await?;
    let rewrite = match existing.as_deref() {
        None => true,
        Some(text) => NoneBot2Component::bot_py_needs_rewrite(text),
    };
    if !rewrite {
        return Ok(());
    }
    emit_log(log, "入口改为按 toml 注册适配器(非 V11 失败可跳过)".into());
    let body = NoneBot2Component::render_bot_py();
    apply_with_backup_ex(host, std::slice::from_ref(&path), true, || async {
        host.write_file(&path, body.as_bytes())
            .await
            .map_err(|e| AppFrameworkError::Integration(e.to_string()))
    })
    .await
}

pub async fn ensure_forward_driver(
    host: &dyn Host,
    instance: &AppInstance,
    log: Option<&PluginLogSink>,
) -> Result<(), AppFrameworkError> {
    ensure_forward_driver_at(host, &HostPath::from_posix(&instance.install_dir), log).await
}

pub async fn ensure_forward_driver_at(
    host: &dyn Host,
    root: &HostPath,
    log: Option<&PluginLogSink>,
) -> Result<(), AppFrameworkError> {
    ensure_forward_driver_with_cleanup(host, root, true, log).await
}

async fn ensure_forward_driver_with_cleanup(
    host: &dyn Host,
    root: &HostPath,
    cleanup_on_failure: bool,
    log: Option<&PluginLogSink>,
) -> Result<(), AppFrameworkError> {
    let Some(toml) = read_text(host, &root.join(NONEBOT2_PYPROJECT)).await? else {
        return Ok(());
    };
    let catalog = parse_catalog(&toml)?;
    let mixins = required_forward_mixins(
        catalog
            .adapters
            .iter()
            .filter(|a| a.enabled)
            .map(|a| a.module_name.as_str()),
    );
    if mixins.is_empty() {
        return Ok(());
    }

    let layout = crate::nonebot2::config::load_env_layout(host, root).await?;
    let env_path = root.join(&layout.write_rel);
    let env_text = read_text(host, &env_path).await?.unwrap_or_default();
    let mut env = EnvFile::parse(&env_text);
    let current = env
        .get(ENV_DRIVER)
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| DRIVER_FASTAPI.to_string());
    let next = merge_driver(&current, &mixins);
    if next != current {
        emit_log(
            log,
            format!("DRIVER={next}（已启用适配器需要对应客户端 mixin）"),
        );
        env.set(ENV_DRIVER, &next);
        host.write_file(&env_path, env.render().as_bytes())
            .await
            .map_err(|e| AppFrameworkError::Integration(e.to_string()))?;
    }

    let lock = read_text(host, &root.join(NONEBOT2_UV_LOCK)).await?;
    let missing_httpx = mixins.contains(&DRIVER_HTTPX)
        && lock
            .as_deref()
            .and_then(|t| NoneBot2Component::lock_package_version(t, "httpx"))
            .is_none();
    let missing_ws = mixins.contains(&DRIVER_WEBSOCKETS)
        && lock
            .as_deref()
            .and_then(|t| NoneBot2Component::lock_package_version(t, "websockets"))
            .is_none();
    if missing_httpx || missing_ws {
        uv_add_with_cleanup(
            host,
            root,
            PYPI_NONEBOT2_FORWARD,
            false,
            cleanup_on_failure,
            log,
        )
        .await?;
    }
    Ok(())
}

async fn load_catalog(
    host: &dyn Host,
    instance: &AppInstance,
) -> Result<NoneBotCatalog, AppFrameworkError> {
    let path = HostPath::from_posix(&instance.install_dir).join(NONEBOT2_PYPROJECT);
    let text = read_text(host, &path).await?.ok_or_else(|| {
        AppFrameworkError::Integration("NoneBot2 实例缺少 pyproject.toml，请先完成安装".into())
    })?;
    parse_catalog(&text)
}

fn commit_catalog_text(
    base: &str,
    catalog: &NoneBotCatalog,
    latest: Option<&str>,
    overwrite: bool,
) -> Result<Option<String>, AppFrameworkError> {
    if !overwrite && latest.is_some_and(|t| t != base) {
        return Err(AppFrameworkError::ConfigConflict("pyproject".into()));
    }
    let source = if overwrite {
        latest.unwrap_or(base)
    } else {
        base
    };
    let out = if overwrite && source != base {
        let mut catalog = catalog.clone();
        let latest_cat = parse_catalog(source)?;
        catalog.adapter_style = latest_cat.adapter_style;
        catalog.plugin_style = latest_cat.plugin_style;
        apply_catalog(source, &catalog)?
    } else {
        apply_catalog(source, catalog)?
    };
    if out == source {
        return Ok(None);
    }
    Ok(Some(out))
}

async fn mutate_catalog<F>(
    host: &dyn Host,
    instance: &AppInstance,
    overwrite: bool,
    f: F,
) -> Result<(), AppFrameworkError>
where
    F: FnOnce(&mut NoneBotCatalog),
{
    let path = HostPath::from_posix(&instance.install_dir).join(NONEBOT2_PYPROJECT);
    let text = read_text(host, &path).await?.ok_or_else(|| {
        AppFrameworkError::Integration("NoneBot2 实例缺少 pyproject.toml，请先完成安装".into())
    })?;
    let mut catalog = parse_catalog(&text)?;
    f(&mut catalog);
    let latest = read_text(host, &path).await?;
    let Some(out) = commit_catalog_text(&text, &catalog, latest.as_deref(), overwrite)? else {
        return Ok(());
    };
    apply_with_backup_ex(
        host,
        std::slice::from_ref(&path),
        write_project_sidecar(instance),
        || async {
            host.write_file(&path, out.as_bytes())
                .await
                .map_err(|e| AppFrameworkError::Integration(e.to_string()))
        },
    )
    .await
}

fn catalog_list_mut(
    catalog: &mut NoneBotCatalog,
    resource: AppStoreResource,
) -> &mut Vec<CatalogItem> {
    match resource {
        AppStoreResource::Adapter => &mut catalog.adapters,
        AppStoreResource::Plugin => &mut catalog.plugins,
    }
}

fn upsert_item(list: &mut Vec<CatalogItem>, item: CatalogItem) {
    if let Some(existing) = list.iter_mut().find(|i| i.module_name == item.module_name) {
        *existing = item;
        return;
    }
    list.push(item);
}

async fn uv_add_at(
    host: &dyn Host,
    root: &HostPath,
    package: &str,
    latest: bool,
    log: Option<&PluginLogSink>,
) -> Result<(), AppFrameworkError> {
    uv_add_with_cleanup(host, root, package, latest, true, log).await
}

async fn uv_add_with_cleanup(
    host: &dyn Host,
    root: &HostPath,
    package: &str,
    latest: bool,
    cleanup_on_failure: bool,
    log: Option<&PluginLogSink>,
) -> Result<(), AppFrameworkError> {
    let spec = if latest {
        format!("{package}@latest")
    } else {
        package.to_string()
    };
    emit_log(log, format!("uv add {spec}"));
    match run_uv(host, root, &["add", &spec], log).await {
        Ok(()) => Ok(()),
        Err(fail) => {
            if let Some(dep) = fail
                .build_package
                .as_deref()
                .filter(|dep| !same_pypi_name(dep, package) && reject_package(dep).is_ok())
            {
                emit_log(log, format!("依赖 {dep} 编译失败，改用预编译版本再试"));
                if run_uv(host, root, &["add", &spec, dep], log).await.is_ok() {
                    return Ok(());
                }
            }
            if cleanup_on_failure {
                let _ = run_uv(host, root, &["remove", package], None).await;
            }
            Err(uv_runtime_err(fail))
        }
    }
}

async fn uv_remove_at(
    host: &dyn Host,
    root: &HostPath,
    package: &str,
    log: Option<&PluginLogSink>,
) -> Result<(), AppFrameworkError> {
    reject_package(package)?;
    emit_log(log, format!("uv remove {package}"));
    run_uv(host, root, &["remove", package], log)
        .await
        .map_err(uv_runtime_err)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct UvFailure {
    message: String,
    build_package: Option<String>,
}

fn uv_runtime_err(fail: UvFailure) -> AppFrameworkError {
    AppFrameworkError::Runtime(fail.message)
}

fn same_pypi_name(left: &str, right: &str) -> bool {
    normalize_pypi_name(left) == normalize_pypi_name(right)
}

fn normalize_pypi_name(name: &str) -> String {
    name.trim()
        .to_ascii_lowercase()
        .split(['-', '_', '.'])
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn parse_failed_build(output: &str) -> Option<(String, String)> {
    for raw in output.lines() {
        let line = raw.trim().trim_start_matches(['×', 'x', 'X']).trim();
        let Some(rest) = line.strip_prefix("Failed to build `") else {
            continue;
        };
        let rest = rest.trim_end_matches('`').trim();
        let Some((name, version)) = rest.split_once("==") else {
            continue;
        };
        let name = name.trim();
        if !name.is_empty() {
            return Some((name.to_string(), version.trim().to_string()));
        }
    }
    None
}

fn network_uv_failure(output: &str) -> bool {
    let lower = output.to_ascii_lowercase();
    lower.contains("error sending request")
        || lower.contains("timed out")
        || lower.contains("connection refused")
        || lower.contains("failed to fetch")
        || lower.contains("error resolving")
        || lower.contains("dns")
}

fn first_useful_uv_line(output: &str) -> Option<&str> {
    output.lines().map(str::trim).find(|line| {
        !line.is_empty()
            && !line.starts_with("hint:")
            && !line.starts_with("File ")
            && !line.starts_with("Traceback")
            && (line.starts_with('×')
                || line.starts_with("error:")
                || line.contains("Failed to")
                || line.contains("No solution"))
    })
}

fn analyze_uv_output(output: &str) -> UvFailure {
    if let Some((package, version)) = parse_failed_build(output) {
        return UvFailure {
            message: format!("依赖 {package} {version} 无法编译，当前环境没有可用的预编译包"),
            build_package: Some(package),
        };
    }
    if network_uv_failure(output) {
        return UvFailure {
            message: "无法连接软件源，检查网络或代理后重试".into(),
            build_package: None,
        };
    }
    if output.contains("No solution found")
        || output.to_ascii_lowercase().contains("failed to resolve")
    {
        return UvFailure {
            message: first_useful_uv_line(output)
                .unwrap_or("依赖无法解析")
                .trim_start_matches('×')
                .trim()
                .to_string(),
            build_package: None,
        };
    }
    UvFailure {
        message: first_useful_uv_line(output)
            .unwrap_or("安装命令失败")
            .trim_start_matches('×')
            .trim()
            .to_string(),
        build_package: None,
    }
}

async fn run_uv(
    host: &dyn Host,
    root: &HostPath,
    args: &[&str],
    log: Option<&PluginLogSink>,
) -> Result<(), UvFailure> {
    let root = root.clone();
    let mut preferred = Vec::new();
    if let Some(marker) = read_uv_marker(host, &root).await {
        preferred.push(marker);
    }
    let uv = resolve_uv(host, &preferred).await.map_err(|e| UvFailure {
        message: e.to_string(),
        build_package: None,
    })?;
    let mut cmd = HostCommand::new(uv.uv_bin.as_posix())
        .args(args.iter().copied())
        .working_dir(root)
        .timeout(CMD_TIMEOUT)
        .env("UV_PROJECT_ENVIRONMENT", ".venv")
        .env("UV_NO_PROGRESS", "1");
    for (k, v) in [("PYTHONUTF8", "1"), ("PYTHONIOENCODING", "utf-8")] {
        cmd = cmd.env(k, v);
    }
    run_host_cmd(host, cmd, log).await
}

async fn run_host_cmd(
    host: &dyn Host,
    cmd: HostCommand,
    log: Option<&PluginLogSink>,
) -> Result<(), UvFailure> {
    let out = if let Some(sink) = log {
        let sink = sink.clone();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        let drain = tokio::spawn(async move {
            while let Some(line) = rx.recv().await {
                sink(line);
            }
        });
        let result = host
            .run_streaming(
                cmd,
                Box::new(move |_src, line| {
                    let trimmed = line.trim_end();
                    if !trimmed.is_empty() {
                        let _ = tx.send(trimmed.to_string());
                    }
                }),
            )
            .await;
        let _ = drain.await;
        result.map_err(|e| UvFailure {
            message: e.to_string(),
            build_package: None,
        })?
    } else {
        host.run_to_string(cmd).await.map_err(|e| UvFailure {
            message: e.to_string(),
            build_package: None,
        })?
    };
    if !out.success() {
        return Err(analyze_uv_output(&format!(
            "{}\n{}",
            out.stderr, out.stdout
        )));
    }
    Ok(())
}

async fn read_text(host: &dyn Host, path: &HostPath) -> Result<Option<String>, AppFrameworkError> {
    if !host.exists(path).await.map_err(host_err)? {
        return Ok(None);
    }
    let bytes = host.read_file(path).await.map_err(host_err)?;
    Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
}

fn reject_package(name: &str) -> Result<(), AppFrameworkError> {
    // 走 argv，但仍拒绝以 `-` 开头的名字，避免 `uv add --offline` 这类被当成 flag
    let name = name.trim();
    let (base, extra) = match name.find('[') {
        Some(i) if name.ends_with(']') && i > 0 => (&name[..i], Some(&name[i + 1..name.len() - 1])),
        None => (name, None),
        _ => {
            return Err(AppFrameworkError::Validation("包名非法".into()));
        }
    };
    let base_ok = !base.is_empty()
        && base
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric())
        && base
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    let extra_ok = extra.is_none_or(|e| {
        !e.is_empty()
            && e.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ','))
    });
    if !base_ok || !extra_ok {
        return Err(AppFrameworkError::Validation("包名非法".into()));
    }
    Ok(())
}

fn emit_log(log: Option<&PluginLogSink>, line: String) {
    if let Some(sink) = log {
        sink(line);
    }
}

fn host_err(e: HostError) -> AppFrameworkError {
    AppFrameworkError::Host(e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config_backup::tests::TestHost;
    use ncd_domain::{
        AppFrameworkId, AppInstanceId, AppInstanceState, AppLinkRecord, AppPlacement,
    };
    use ncd_host::CommandOutput;
    use std::sync::Arc;

    const SCAFFOLD: &str = r#"[project]
name = "nonebot2-n1"
dependencies = [
    "nonebot2[fastapi]",
    "nonebot-adapter-onebot",
]

[tool.nonebot]
adapters = [
    { name = "OneBot V11", module_name = "nonebot.adapters.onebot.v11" },
]
plugins = []
"#;

    fn inst(linked: bool) -> AppInstance {
        AppInstance {
            id: AppInstanceId::new("n1"),
            framework_id: AppFrameworkId::new("nonebot2"),
            display_name: "NB".into(),
            placement: AppPlacement::LocalNative,
            host_id: "local".into(),
            install_dir: "/x/n1".into(),
            port: 8080,
            state: AppInstanceState::Installed,
            link: linked.then_some(AppLinkRecord {
                bot_id: ncd_domain::BotId::new("10001"),
                mode: ncd_domain::OneBotLinkMode::ReverseWs,
                connection_name: "ncd-app:n1".into(),
                linked_at_ms: 1,
                resident_forward_port: None,
            }),
            installed_version: None,
            last_error: None,
            created_at_ms: 1,
            install_renderer: false,
            origin: ncd_domain::AppInstanceOrigin::Created,
            auto_start: true,
        }
    }

    fn pending_host() -> Arc<TestHost> {
        let host = TestHost::new("nonebot-store");
        host.put("/x/n1/pyproject.toml", SCAFFOLD.as_bytes());
        host.put(
            "/x/n1/bot.py",
            NoneBot2Component::render_bot_py().as_bytes(),
        );
        host.set_command_handler(fake_uv);
        host
    }

    fn entry(package: &str, module: &str, resource: AppStoreResource) -> AppStoreMarketEntry {
        serde_json::from_value(serde_json::json!({
            "resource": resource, "id": module, "name": module, "description": "",
            "package": package, "module_name": module, "flavor": "pypi"
        }))
        .unwrap()
    }

    fn command_output(success: bool, stdout: &str, stderr: &str) -> CommandOutput {
        CommandOutput {
            exit_code: Some(if success { 0 } else { 1 }),
            stdout: stdout.into(),
            stderr: stderr.into(),
        }
    }

    fn fake_uv(host: &TestHost, cmd: &HostCommand) -> Result<CommandOutput, HostError> {
        if cmd.args == ["--version"] {
            return Ok(command_output(true, "uv 0.8.0", ""));
        }
        let root = cmd.working_dir.as_ref().unwrap();
        let project_path = root.join(NONEBOT2_PYPROJECT);
        let lock_path = root.join(NONEBOT2_UV_LOCK);
        let mut lock = String::from_utf8(host.content(lock_path.as_posix()).unwrap_or_default())
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
        match cmd.args.first().map(String::as_str) {
            Some("add") => {
                let package = cmd.args[1].trim_end_matches("@latest");
                let project = String::from_utf8(host.content(project_path.as_posix()).unwrap())
                    .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
                let mut doc = project
                    .parse::<DocumentMut>()
                    .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
                let dependencies = doc["project"]["dependencies"].as_array_mut().unwrap();
                if !dependencies
                    .iter()
                    .any(|value| value.as_str() == Some(package))
                {
                    dependencies.push(package);
                }
                host.put(project_path.as_posix(), doc.to_string().as_bytes());
                let (name, extras) = package_parts(package);
                for dependency in std::iter::once(normalize_pypi_name(name)).chain(
                    extras
                        .into_iter()
                        .filter(|extra| matches!(*extra, "httpx" | "websockets"))
                        .map(str::to_string),
                ) {
                    if NoneBot2Component::lock_package_version(&lock, &dependency).is_none() {
                        lock.push_str(&format!(
                            "\n[[package]]\nname = {dependency:?}\nversion = \"1.2.3\"\n"
                        ));
                    }
                }
                host.put(lock_path.as_posix(), lock.as_bytes());
            }
            Some("sync") => {
                assert_eq!(cmd.args, ["sync", "--locked", "--inexact"]);
            }
            _ => panic!("unexpected uv command: {:?}", cmd.args),
        }
        host.put(
            root.join(".venv/fake-installed").as_posix(),
            lock.as_bytes(),
        );
        Ok(command_output(true, "", ""))
    }

    fn package_commands(host: &TestHost, action: &str) -> Vec<Vec<String>> {
        host.commands()
            .into_iter()
            .filter(|cmd| cmd.args.first().map(String::as_str) == Some(action))
            .map(|cmd| cmd.args)
            .collect()
    }

    #[tokio::test]
    async fn pending_install_recovers_catalog_without_readding_or_changing_lock() {
        let host = pending_host();
        let plugin = entry(
            "nonebot-plugin-foo",
            "nonebot_plugin_foo",
            AppStoreResource::Plugin,
        );
        host.fail_next_write("/x/n1/pyproject.toml");
        let error = install_item(host.as_ref(), &inst(false), &plugin, None)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("permission denied"), "{error}");
        assert!(host.content("/x/n1/.venv/fake-installed").is_some());
        assert!(host.content("/x/n1/.ncd-nonebot-install.json").is_some());
        let lock = host.content("/x/n1/uv.lock").unwrap();
        let calls = host.commands().len();
        assert!(
            list_installed(host.as_ref(), &inst(false), AppStoreResource::Plugin)
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            host.commands().len(),
            calls,
            "listing must remain read-only"
        );

        install_item(host.as_ref(), &inst(false), &plugin, None)
            .await
            .unwrap();
        let installed = list_installed(host.as_ref(), &inst(false), AppStoreResource::Plugin)
            .await
            .unwrap();
        assert_eq!(installed.len(), 1);
        assert_eq!(installed[0].version.as_deref(), Some("1.2.3"));
        assert!(installed[0].enabled);
        assert_eq!(host.content("/x/n1/uv.lock").unwrap(), lock);
        assert!(host.content("/x/n1/.ncd-nonebot-install.json").is_none());
        assert!(host.content("/x/n1/.ncd-nonebot-install.error").is_none());
        assert_eq!(package_commands(host.as_ref(), "add").len(), 1);
        assert_eq!(package_commands(host.as_ref(), "sync").len(), 1);
        assert!(package_commands(host.as_ref(), "remove").is_empty());
    }

    #[tokio::test]
    async fn pending_install_recovery_failure_keeps_context_and_blocks_next_intent() {
        let host = pending_host();
        let foo = entry(
            "nonebot-plugin-foo",
            "nonebot_plugin_foo",
            AppStoreResource::Plugin,
        );
        let bar = entry(
            "nonebot-plugin-bar",
            "nonebot_plugin_bar",
            AppStoreResource::Plugin,
        );
        host.fail_next_write("/x/n1/pyproject.toml");
        install_item(host.as_ref(), &inst(false), &foo, None)
            .await
            .unwrap_err();
        let intent = host.content("/x/n1/.ncd-nonebot-install.json").unwrap();
        host.set_command_handler(|host, cmd| {
            if cmd.args.first().map(String::as_str) == Some("sync") {
                return Ok(command_output(false, "", "error: sync rejected"));
            }
            fake_uv(host, cmd)
        });
        let error = install_item(host.as_ref(), &inst(false), &bar, None)
            .await
            .unwrap_err();
        let message = error.to_string();
        assert!(message.contains("permission denied"), "{message}");
        assert!(message.contains("sync rejected"), "{message}");
        assert_eq!(
            host.content("/x/n1/.ncd-nonebot-install.json").unwrap(),
            intent
        );
        assert_eq!(
            package_commands(host.as_ref(), "add"),
            [vec!["add".to_string(), foo.package.clone()]]
        );

        host.set_command_handler(fake_uv);
        install_item(host.as_ref(), &inst(false), &bar, None)
            .await
            .unwrap();
        let installed = list_installed(host.as_ref(), &inst(false), AppStoreResource::Plugin)
            .await
            .unwrap();
        assert_eq!(installed.len(), 2);
        assert!(host.content("/x/n1/.ncd-nonebot-install.json").is_none());
    }

    #[tokio::test]
    async fn pending_install_intent_write_failure_is_retryable_before_uv() {
        let host = pending_host();
        let plugin = entry(
            "nonebot-plugin-foo",
            "nonebot_plugin_foo",
            AppStoreResource::Plugin,
        );
        host.fail_next_write("/x/n1/.ncd-nonebot-install.tmp");
        install_item(host.as_ref(), &inst(false), &plugin, None)
            .await
            .unwrap_err();
        assert!(host.commands().is_empty());
        assert!(host.content("/x/n1/.ncd-nonebot-install.json").is_none());
        install_item(host.as_ref(), &inst(false), &plugin, None)
            .await
            .unwrap();
        assert_eq!(package_commands(host.as_ref(), "add").len(), 1);
        assert!(host.content("/x/n1/.ncd-nonebot-install.tmp").is_none());
    }

    #[tokio::test]
    async fn pending_install_clear_failure_keeps_registered_item_retryable() {
        let host = pending_host();
        let plugin = entry(
            "nonebot-plugin-foo",
            "nonebot_plugin_foo",
            AppStoreResource::Plugin,
        );
        host.fail_next_remove("/x/n1/.ncd-nonebot-install.json");
        let error = install_item(host.as_ref(), &inst(false), &plugin, None)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("remove"), "{error}");
        assert_eq!(
            list_installed(host.as_ref(), &inst(false), AppStoreResource::Plugin)
                .await
                .unwrap()
                .len(),
            1
        );
        assert!(host.content("/x/n1/.ncd-nonebot-install.json").is_some());
        install_item(host.as_ref(), &inst(false), &plugin, None)
            .await
            .unwrap();
        assert!(host.content("/x/n1/.ncd-nonebot-install.json").is_none());
        assert_eq!(package_commands(host.as_ref(), "add").len(), 1);
    }

    #[tokio::test]
    async fn pending_install_preserves_existing_package_version_on_registration_failure() {
        let host = pending_host();
        let project = SCAFFOLD.replace(
            "\"nonebot2[fastapi]\",",
            "\"nonebot2[fastapi]\",\n    \"nonebot-plugin-foo==0.9.0\",",
        );
        host.put("/x/n1/pyproject.toml", project.as_bytes());
        let lock = b"[[package]]\nname = \"nonebot-plugin-foo\"\nversion = \"0.9.0\"\n";
        host.put("/x/n1/uv.lock", lock);
        host.put("/x/n1/.venv/unmanaged-package", b"keep");
        let plugin = entry(
            "nonebot_plugin.foo",
            "nonebot_plugin_foo",
            AppStoreResource::Plugin,
        );
        host.fail_next_write("/x/n1/pyproject.toml");
        install_item(host.as_ref(), &inst(false), &plugin, None)
            .await
            .unwrap_err();
        install_item(host.as_ref(), &inst(false), &plugin, None)
            .await
            .unwrap();
        assert_eq!(host.content("/x/n1/uv.lock").unwrap(), lock);
        assert_eq!(
            host.content("/x/n1/.venv/unmanaged-package").unwrap(),
            b"keep"
        );
        assert!(package_commands(host.as_ref(), "add").is_empty());
        assert!(package_commands(host.as_ref(), "remove").is_empty());
        assert_eq!(package_commands(host.as_ref(), "sync").len(), 2);
    }

    #[tokio::test]
    async fn pending_install_add_failure_never_removes_existing_package() {
        let host = pending_host();
        let project = SCAFFOLD.replace(
            "\"nonebot2[fastapi]\",",
            "\"nonebot2[fastapi]\",\n    \"nonebot-plugin-foo[old]==0.9.0\",",
        );
        host.put("/x/n1/pyproject.toml", project.as_bytes());
        let lock = b"[[package]]\nname = \"nonebot-plugin-foo\"\nversion = \"0.9.0\"\n";
        host.put("/x/n1/uv.lock", lock);
        host.set_command_handler(|host, cmd| {
            if cmd.args.first().map(String::as_str) == Some("add") {
                return Ok(command_output(false, "", "error: add rejected"));
            }
            fake_uv(host, cmd)
        });
        let plugin = entry(
            "nonebot-plugin-foo[new]",
            "nonebot_plugin_foo",
            AppStoreResource::Plugin,
        );
        let error = install_item(host.as_ref(), &inst(false), &plugin, None)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("add rejected"), "{error}");
        assert_eq!(host.content("/x/n1/uv.lock").unwrap(), lock);
        assert_eq!(
            host.content("/x/n1/pyproject.toml").unwrap(),
            project.as_bytes()
        );
        assert!(package_commands(host.as_ref(), "remove").is_empty());
        assert!(host.content("/x/n1/.ncd-nonebot-install.json").is_none());

        host.set_command_handler(fake_uv);
        let other = entry(
            "nonebot-plugin-bar",
            "nonebot_plugin_bar",
            AppStoreResource::Plugin,
        );
        install_item(host.as_ref(), &inst(false), &other, None)
            .await
            .unwrap();
        let lock = String::from_utf8(host.content("/x/n1/uv.lock").unwrap()).unwrap();
        assert_eq!(
            NoneBot2Component::lock_package_version(&lock, "nonebot-plugin-foo").as_deref(),
            Some("0.9.0")
        );
        assert!(package_commands(host.as_ref(), "remove").is_empty());
    }

    #[tokio::test]
    async fn pending_install_partial_add_and_error_record_failure_remain_retryable() {
        let host = pending_host();
        let plugin = entry(
            "nonebot-plugin-foo",
            "nonebot_plugin_foo",
            AppStoreResource::Plugin,
        );
        host.set_command_handler(|host, cmd| {
            let output = fake_uv(host, cmd)?;
            if cmd.args.first().map(String::as_str) == Some("add") {
                host.fail_next_write("/x/n1/.ncd-nonebot-install.error");
                return Ok(command_output(
                    false,
                    "",
                    "error: failed after project change",
                ));
            }
            Ok(output)
        });
        let error = install_item(host.as_ref(), &inst(false), &plugin, None)
            .await
            .unwrap_err();
        let message = error.to_string();
        assert!(message.contains("failed after project change"), "{message}");
        assert!(message.contains("保存失败上下文也失败"), "{message}");
        assert!(host.content("/x/n1/.ncd-nonebot-install.json").is_some());
        host.set_command_handler(fake_uv);
        install_item(host.as_ref(), &inst(false), &plugin, None)
            .await
            .unwrap();
        assert_eq!(package_commands(host.as_ref(), "add").len(), 1);
        assert_eq!(package_commands(host.as_ref(), "sync").len(), 1);
        assert!(host.content("/x/n1/.ncd-nonebot-install.json").is_none());
        assert!(package_commands(host.as_ref(), "remove").is_empty());
    }

    #[tokio::test]
    async fn pending_install_clear_failure_preserves_original_error_for_later_recovery_failure() {
        let host = pending_host();
        let plugin = entry(
            "nonebot-plugin-foo",
            "nonebot_plugin_foo",
            AppStoreResource::Plugin,
        );
        host.fail_next_write("/x/n1/pyproject.toml");
        install_item(host.as_ref(), &inst(false), &plugin, None)
            .await
            .unwrap_err();
        host.fail_next_remove("/x/n1/.ncd-nonebot-install.json");
        install_item(host.as_ref(), &inst(false), &plugin, None)
            .await
            .unwrap_err();
        let context =
            String::from_utf8(host.content("/x/n1/.ncd-nonebot-install.error").unwrap()).unwrap();
        assert!(context.contains("pyproject.toml (write)"), "{context}");
        assert!(
            context.contains(".ncd-nonebot-install.json (remove)"),
            "{context}"
        );
        host.set_command_handler(|host, cmd| {
            if cmd.args.first().map(String::as_str) == Some("sync") {
                return Ok(command_output(false, "", "error: later sync failed"));
            }
            fake_uv(host, cmd)
        });
        let error = install_item(host.as_ref(), &inst(false), &plugin, None)
            .await
            .unwrap_err();
        let message = error.to_string();
        assert!(message.contains("pyproject.toml (write)"), "{message}");
        assert!(message.contains("later sync failed"), "{message}");
        assert!(host.content("/x/n1/.ncd-nonebot-install.json").is_some());
        host.set_command_handler(fake_uv);
        install_item(host.as_ref(), &inst(false), &plugin, None)
            .await
            .unwrap();
        assert!(host.content("/x/n1/.ncd-nonebot-install.error").is_none());
        assert!(host.content("/x/n1/.ncd-nonebot-install.json").is_none());
    }

    #[tokio::test]
    async fn pending_install_driver_failure_keeps_intent_until_driver_recovers() {
        let host = pending_host();
        host.put(
            "/x/n1/uv.lock",
            b"[[package]]\nname = \"nonebot2\"\nversion = \"2.4.0\"\n",
        );
        host.set_command_handler(|host, cmd| {
            if cmd.args.get(1).map(String::as_str) == Some(PYPI_NONEBOT2_FORWARD) {
                return Ok(command_output(false, "", "error: driver rejected"));
            }
            fake_uv(host, cmd)
        });
        let adapter = entry(
            "nonebot-adapter-qq",
            "nonebot.adapters.qq",
            AppStoreResource::Adapter,
        );
        let error = install_item(host.as_ref(), &inst(false), &adapter, None)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("driver rejected"), "{error}");
        assert!(host.content("/x/n1/.ncd-nonebot-install.json").is_some());
        assert!(package_commands(host.as_ref(), "remove").is_empty());
        host.set_command_handler(fake_uv);
        install_item(host.as_ref(), &inst(false), &adapter, None)
            .await
            .unwrap();
        let lock = String::from_utf8(host.content("/x/n1/uv.lock").unwrap()).unwrap();
        assert_eq!(
            NoneBot2Component::lock_package_version(&lock, "nonebot2").as_deref(),
            Some("2.4.0")
        );
        assert!(NoneBot2Component::lock_package_version(&lock, "httpx").is_some());
        assert!(NoneBot2Component::lock_package_version(&lock, "websockets").is_some());
        assert!(host.content("/x/n1/.ncd-nonebot-install.json").is_none());
    }

    #[test]
    fn pending_install_dependency_check_requires_requested_extras_and_unconditional_name() {
        let project = "[project]\ndependencies = ['Foo_Bar[old,new]>=1', 'conditional; python_version < \"3.10\"']\n";
        assert!(dependency_is_present(project, "foo.bar[new]").unwrap());
        assert!(!dependency_is_present(project, "foo-bar[missing]").unwrap());
        assert!(!dependency_is_present(project, "foo-barr").unwrap());
        assert!(!dependency_is_present(project, "conditional").unwrap());
    }

    #[test]
    fn catalog_round_trip_keeps_v11_and_can_add_v12() {
        let catalog = parse_catalog(SCAFFOLD).unwrap();
        assert_eq!(catalog.adapters.len(), 1);
        assert_eq!(catalog.adapters[0].module_name, ONEBOT_V11_MODULE);
        assert_eq!(catalog.adapters[0].project_link, PYPI_ADAPTER_ONEBOT);

        let mut next = catalog.clone();
        next.adapters.push(CatalogItem {
            name: "OneBot V12".into(),
            module_name: "nonebot.adapters.onebot.v12".into(),
            project_link: PYPI_ADAPTER_ONEBOT.into(),
            enabled: true,
        });
        let text = apply_catalog(SCAFFOLD, &next).unwrap();
        assert!(text.contains("nonebot.adapters.onebot.v11"));
        assert!(text.contains("nonebot.adapters.onebot.v12"));
        assert!(package_still_needed(
            &next.adapters,
            PYPI_ADAPTER_ONEBOT,
            "nonebot.adapters.onebot.v12"
        ));
    }

    #[test]
    fn disable_keeps_catalog_but_drops_tool_nonebot() {
        let mut catalog = parse_catalog(SCAFFOLD).unwrap();
        catalog.adapters[0].enabled = false;
        catalog.plugins.push(CatalogItem {
            name: "nonebot_plugin_foo".into(),
            module_name: "nonebot_plugin_foo".into(),
            project_link: "nonebot-plugin-foo".into(),
            enabled: false,
        });
        let text = apply_catalog(SCAFFOLD, &catalog).unwrap();
        let again = parse_catalog(&text).unwrap();
        assert!(!again.adapters[0].enabled);
        assert!(!again.plugins[0].enabled);
        let doc: toml::Value = toml::from_str(&text).unwrap();
        let adapters = doc["tool"]["nonebot"]["adapters"].as_array().unwrap();
        assert!(adapters.is_empty());
        let plugins = doc["tool"]["nonebot"]["plugins"].as_array().unwrap();
        assert!(plugins.is_empty());
    }

    #[test]
    fn linked_instance_cannot_drop_v11() {
        assert!(refuse_linked_onebot_v11(&inst(true), ONEBOT_V11_MODULE).is_err());
        assert!(refuse_linked_onebot_v11(&inst(false), ONEBOT_V11_MODULE).is_ok());
        assert!(refuse_linked_onebot_v11(&inst(true), "nonebot.adapters.console").is_ok());
    }

    #[test]
    fn parse_official_adapters_and_plugins() {
        let adapters = parse_nonebot_adapters_json(
            r#"[{"module_name":"nonebot.adapters.onebot.v11","project_link":"nonebot-adapter-onebot","name":"OneBot V11","desc":"d","author":"a","homepage":"h","tags":[{"label":"official"}],"is_official":true,"version":"2.4.6"}]"#,
        )
        .unwrap();
        assert_eq!(adapters[0].id, ONEBOT_V11_MODULE);
        assert_eq!(adapters[0].resource, AppStoreResource::Adapter);
        assert_eq!(adapters[0].tags, vec!["official".to_string()]);

        let plugins = parse_nonebot_plugins_json(
            r#"[{"module_name":"nonebot_plugin_foo","project_link":"nonebot-plugin-foo","name":"Foo","desc":"d","valid":true,"supported_adapters":["nonebot.adapters.onebot.v11"]},{"module_name":"bad","project_link":"bad","valid":false}]"#,
        )
        .unwrap();
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].supported_adapters[0], ONEBOT_V11_MODULE);
    }

    #[test]
    fn managed_bot_py_rewrites_stale_dynamic_template() {
        let current = NoneBot2Component::render_bot_py();
        assert!(!NoneBot2Component::bot_py_needs_rewrite(current));
        assert!(!NoneBot2Component::bot_py_needs_rewrite(
            "from nonebot.adapters.onebot.v11 import Adapter as OneBotV11Adapter\n"
        ));
        let v1 = "def _register_adapters(driver):\n    module = importlib.import_module(name)\n    driver.register_adapter(module.Adapter)\nnonebot.load_from_toml(\"pyproject.toml\")\n";
        assert!(NoneBot2Component::bot_py_needs_rewrite(v1));
        assert!(NoneBot2Component::bot_py_needs_rewrite(
            "# ncd-managed-bot-py:1\nimport importlib\n"
        ));
        assert!(!NoneBot2Component::bot_py_needs_rewrite(
            "# ncd-managed-bot-py:2\nimport importlib\n"
        ));
        assert!(!NoneBot2Component::bot_py_needs_rewrite(
            "print('my custom bot')\n"
        ));
    }

    #[test]
    fn enabled_telegram_only_needs_httpx_not_websockets() {
        let catalog = parse_catalog(
            r#"
[tool.nonebot]
adapters = [
  { name = "OneBot V11", module_name = "nonebot.adapters.onebot.v11" },
  { name = "Telegram", module_name = "nonebot.adapters.telegram" },
]
"#,
        )
        .unwrap();
        let mixins = required_forward_mixins(
            catalog
                .adapters
                .iter()
                .filter(|a| a.enabled)
                .map(|a| a.module_name.as_str()),
        );
        assert_eq!(mixins, vec!["~httpx"]);
        assert_eq!(merge_driver("~fastapi", &mixins), "~fastapi+~httpx");
    }

    #[test]
    fn ncd_does_not_shadow_tool_nonebot_additions() {
        let text = r#"
[tool.nonebot]
adapters = [{ name = "OneBot V11", module_name = "nonebot.adapters.onebot.v11" }]
plugins = ["nonebot_plugin_foo"]

[tool.ncd.nonebot]
adapters = [
    { name = "OneBot V11", module_name = "nonebot.adapters.onebot.v11", project_link = "nonebot-adapter-onebot", enabled = true },
]
plugins = []
"#;
        let catalog = parse_catalog(text).unwrap();
        assert_eq!(catalog.plugins.len(), 1);
        assert_eq!(catalog.plugins[0].module_name, "nonebot_plugin_foo");
        assert!(catalog.plugins[0].enabled);
    }

    #[test]
    fn reads_array_of_tables_adapters_and_plugin_table() {
        let text = r#"
[tool.nonebot]
plugin_dirs = ["plugins"]

[[tool.nonebot.adapters]]
name = "Console"
module_name = "nonebot.adapters.console"

[tool.nonebot.plugins]
nonebot-plugin-foo = ["nonebot_plugin_foo"]
"#;
        let catalog = parse_catalog(text).unwrap();
        assert_eq!(catalog.adapters[0].module_name, "nonebot.adapters.console");
        assert_eq!(catalog.plugins[0].module_name, "nonebot_plugin_foo");
    }

    #[test]
    fn reject_package_blocks_flags() {
        assert!(reject_package("--offline").is_err());
        assert!(reject_package("nonebot-adapter-onebot").is_ok());
        assert!(reject_package("nonebot-plugin-foo[extra]").is_ok());
        assert!(reject_package("foo/bar").is_err());
    }

    #[test]
    fn linked_gate_resolves_display_name() {
        let catalog = parse_catalog(SCAFFOLD).unwrap();
        let item = resolve_catalog_item(&catalog.adapters, "OneBot V11").unwrap();
        assert_eq!(item.module_name, ONEBOT_V11_MODULE);
        assert!(refuse_linked_onebot_v11(&inst(true), &item.module_name).is_err());
    }

    #[test]
    fn apply_catalog_keeps_comment_and_plugin_table() {
        let text = r#"
[tool.nonebot]
# adapters stay
adapters = [
    { name = "OneBot V11", module_name = "nonebot.adapters.onebot.v11" },
]
plugin_dirs = ["plugins"]

[tool.nonebot.plugins]
# local
"@local" = ["echo_ext"]
"#;
        let mut catalog = parse_catalog(text).unwrap();
        catalog.plugins.push(CatalogItem {
            name: "foo".into(),
            module_name: "nonebot_plugin_foo".into(),
            project_link: "nonebot-plugin-foo".into(),
            enabled: true,
        });
        let out = apply_catalog(text, &catalog).unwrap();
        assert!(out.contains("# adapters stay"), "{out}");
        assert!(out.contains("plugin_dirs"), "{out}");
        assert!(out.contains("@local"), "{out}");
        assert!(out.contains("echo_ext"), "{out}");
        assert!(out.contains("nonebot-plugin-foo"), "{out}");
        assert!(out.contains("[tool.nonebot.plugins]"), "{out}");
        let again = parse_catalog(&out).unwrap();
        assert_eq!(again.plugin_style, PluginTomlStyle::Table);
        assert!(again.plugins.iter().any(|p| p.module_name == "echo_ext"));
        assert!(
            again
                .plugins
                .iter()
                .any(|p| p.module_name == "nonebot_plugin_foo")
        );
    }

    #[test]
    fn enabled_qq_adapter_needs_fastapi_httpx_websockets() {
        let catalog = parse_catalog(
            r#"
[tool.nonebot]
adapters = [
  { name = "OneBot V11", module_name = "nonebot.adapters.onebot.v11" },
  { name = "QQ", module_name = "nonebot.adapters.qq" },
]
"#,
        )
        .unwrap();
        let mixins = required_forward_mixins(
            catalog
                .adapters
                .iter()
                .filter(|a| a.enabled)
                .map(|a| a.module_name.as_str()),
        );
        assert_eq!(mixins, vec!["~httpx", "~websockets"]);
        assert_eq!(
            merge_driver("~fastapi", &mixins),
            "~fastapi+~httpx+~websockets"
        );
    }

    #[test]
    fn apply_catalog_keeps_array_of_tables_adapters() {
        let text = r#"
[tool.nonebot]
plugin_dirs = ["plugins"]

[[tool.nonebot.adapters]]
name = "OneBot V11"
module_name = "nonebot.adapters.onebot.v11"
"#;
        let mut catalog = parse_catalog(text).unwrap();
        catalog.adapters.push(CatalogItem {
            name: "Console".into(),
            module_name: "nonebot.adapters.console".into(),
            project_link: "nonebot-adapter-console".into(),
            enabled: true,
        });
        let out = apply_catalog(text, &catalog).unwrap();
        assert!(out.contains("[[tool.nonebot.adapters]]"), "{out}");
        assert!(out.contains("nonebot.adapters.console"), "{out}");
        assert!(out.contains("plugin_dirs"), "{out}");
    }

    #[test]
    fn catalog_write_conflicts_when_base_changed() {
        let catalog = parse_catalog(SCAFFOLD).unwrap();
        let err = commit_catalog_text(SCAFFOLD, &catalog, Some("changed"), false).unwrap_err();
        assert!(matches!(err, AppFrameworkError::ConfigConflict(_)));

        let latest = r#"
[project]
name = "from-raw-tab"

[tool.nonebot]
plugin_dirs = ["plugins", "extra"]
adapters = [
    { name = "OneBot V11", module_name = "nonebot.adapters.onebot.v11" },
]
plugins = []
"#;
        let out = commit_catalog_text(SCAFFOLD, &catalog, Some(latest), true)
            .unwrap()
            .expect("overwrite should still write");
        assert!(out.contains("from-raw-tab"), "{out}");
        assert!(out.contains("extra"), "{out}");

        let latest_aot = r#"
[tool.nonebot]
plugin_dirs = ["plugins"]

[[tool.nonebot.adapters]]
name = "OneBot V11"
module_name = "nonebot.adapters.onebot.v11"
"#;
        let aot = commit_catalog_text(SCAFFOLD, &catalog, Some(latest_aot), true)
            .unwrap()
            .expect("overwrite onto AoT");
        assert!(aot.contains("[[tool.nonebot.adapters]]"), "{aot}");
    }

    #[test]
    fn uv_output_picks_build_failure_not_frozen_hint() {
        let output = r#"
× Failed to build `pillow==9.5.0`
├─▶ The build backend returned an error
╰─▶ Call to `setuptools.build_meta:__legacy__.build_wheel` failed (exit
      code: 1)
hint: `pillow` (v9.5.0) was included because `nonebot2-6d4853b2` (v0.1.0) depends on `nonebot-plugin-txt2img`
hint: If you want to add the package regardless of the failed resolution, provide the `--frozen` flag to skip locking and syncing
"#;
        let fail = analyze_uv_output(output);
        assert_eq!(fail.build_package.as_deref(), Some("pillow"));
        assert!(fail.message.contains("pillow"));
        assert!(fail.message.contains("无法编译"));
        assert!(!fail.message.contains("--frozen"));
    }

    #[test]
    fn uv_output_humanizes_network_and_skips_hint() {
        let fail = analyze_uv_output("error sending request\nhint: try again");
        assert_eq!(fail.message, "无法连接软件源，检查网络或代理后重试");
        assert_eq!(fail.build_package, None);
        assert!(same_pypi_name(
            "nonebot_plugin_txt2img",
            "nonebot-plugin-txt2img"
        ));
    }
}
