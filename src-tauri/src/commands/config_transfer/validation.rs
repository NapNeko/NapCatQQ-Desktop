use std::collections::{BTreeMap, HashSet};
use std::sync::Mutex;

use ncd_domain::app_framework::{LOCAL_HOST_ID, REMOTE_HOST_ID_PREFIX};
use ncd_domain::chat_desktop::ChatAccountPreference;
use ncd_domain::errors::SecretError;
use ncd_domain::onebot_debug::{DebugCollections, DebugWorkspace};
use ncd_domain::{AppInstance, AppPlacement, SnowLumaAppConfig};
use ncd_runtime::app_framework::AdoptSnapshot;
use ncd_runtime::app_framework::FrameworkConfigBackup;
use ncd_traits::SecretStore;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::ConfigFrontendPreferences;
use super::registry::{TransferEntry, TransferKind, is_safe_instance_id, validate_relative_path};

const MAX_FRONTEND_BYTES: usize = 2 * 1024 * 1024;
const MAX_FRONTEND_KEYS: usize = 512;
const MAX_FRONTEND_VALUE_BYTES: usize = 512 * 1024;

/// Simulates legacy credential migration without touching the OS credential store or data root.
#[derive(Default)]
pub(super) struct MemorySecretStore(Mutex<BTreeMap<String, String>>);

impl SecretStore for MemorySecretStore {
    fn get(&self, key: &str) -> Result<Option<String>, SecretError> {
        Ok(self
            .0
            .lock()
            .map_err(|_| SecretError::Unavailable)?
            .get(key)
            .cloned())
    }

    fn put(&self, key: &str, value: &str) -> Result<(), SecretError> {
        self.0
            .lock()
            .map_err(|_| SecretError::Unavailable)?
            .insert(key.into(), value.into());
        Ok(())
    }

    fn delete(&self, key: &str) -> Result<(), SecretError> {
        self.0
            .lock()
            .map_err(|_| SecretError::Unavailable)?
            .remove(key);
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppInstancesFile {
    version: u32,
    instances: Vec<AppInstance>,
}

#[derive(Serialize, Deserialize)]
struct ChatPreferencesFile {
    accounts: Vec<ChatAccountPreference>,
}

fn decode<T: serde::de::DeserializeOwned>(value: Value, name: &str) -> Result<T, String> {
    serde_json::from_value(value)
        .map_err(|error| format!("{name} 不是合法配置，已中止导入: {error}"))
}

fn encode(value: &impl Serialize, name: &str) -> Result<Value, String> {
    serde_json::to_value(value).map_err(|error| format!("序列化 {name} 失败: {error}"))
}

fn require_version(value: &Value, name: &str, versions: &[u64]) -> Result<(), String> {
    if !value
        .get("version")
        .and_then(Value::as_u64)
        .is_some_and(|version| versions.contains(&version))
    {
        return Err(format!("{name} 的配置版本不受支持或缺失，已中止导入"));
    }
    Ok(())
}

fn require_array(value: &Value, key: &str, name: &str) -> Result<(), String> {
    if !value.get(key).is_some_and(Value::is_array) {
        return Err(format!("{name} 的 {key} 字段必须是数组，已中止导入"));
    }
    Ok(())
}

fn valid_host(host: &str) -> bool {
    host == LOCAL_HOST_ID
        || host
            .strip_prefix(REMOTE_HOST_ID_PREFIX)
            .is_some_and(is_safe_instance_id)
}

fn validate_instances(value: Value, name: &str) -> Result<Value, String> {
    require_version(&value, name, &[1])?;
    let file: AppInstancesFile = decode(value, name)?;
    let mut ids = HashSet::new();
    let mut homes = HashSet::new();
    for instance in &file.instances {
        let install_path = instance.install_dir.trim_end_matches('/');
        let valid_placement = match instance.placement {
            AppPlacement::LocalNative => instance.host_id == LOCAL_HOST_ID,
            AppPlacement::RemoteNative => instance.host_id.starts_with(REMOTE_HOST_ID_PREFIX),
            AppPlacement::RemoteDocker => false,
        };
        if !is_safe_instance_id(instance.id.as_str())
            || !is_safe_instance_id(instance.framework_id.as_str())
            || !ids.insert(instance.id.as_str())
            // 同主机同目录只能有一个实例，否则起停、恢复、删除会互相打到对方的文件
            || !homes.insert((instance.host_id.as_str(), install_path))
            || !valid_host(&instance.host_id)
            || !valid_placement
            || instance.display_name.trim().is_empty()
            || instance.install_dir.trim().is_empty()
            || instance.install_dir.contains('\0')
            || !instance.install_dir.starts_with('/')
            || instance.install_dir.contains(['\\', ':'])
            || install_path.is_empty()
            || install_path
                .strip_prefix('/')
                .unwrap_or(install_path)
                .split('/')
                .any(|part| part.is_empty() || matches!(part, "." | ".."))
            || instance.port == 0
        {
            return Err(format!(
                "{name} 含重复或非法实例、主机与部署位置不匹配，已中止导入"
            ));
        }
        if instance
            .link
            .as_ref()
            .is_some_and(|link| link.bot_id.as_str().is_empty() || link.connection_name.is_empty())
        {
            return Err(format!("{name} 含非法实例关联，已中止导入"));
        }
    }
    encode(&file, name)
}

fn validate_chat(value: Value, name: &str) -> Result<Value, String> {
    let file: ChatPreferencesFile = decode(value, name)?;
    if file.accounts.len() > 64
        || file
            .accounts
            .iter()
            .filter(|account| account.enabled && account.background)
            .count()
            > 8
    {
        return Err(format!(
            "{name} 聊天账号数量超出限制（最多 64 个账号、8 个后台账号）"
        ));
    }
    let mut identities = HashSet::new();
    for account in &file.accounts {
        account
            .validate()
            .map_err(|error| format!("{name}: {error}"))?;
        if !identities.insert((&account.bot_id, &account.self_id)) {
            return Err(format!("{name} 含重复聊天账号，已中止导入"));
        }
    }
    let payload = encode(&file, name)?;
    // The runtime loader enforces the same disk budget.
    if serde_json::to_vec_pretty(&payload)
        .map_err(|error| error.to_string())?
        .len()
        > 128 * 1024
    {
        return Err(format!("{name} 聊天偏好文件过大"));
    }
    Ok(payload)
}

fn validate_workspace(value: Value, name: &str) -> Result<Value, String> {
    // Frontend v2 uses width 0 as an explicit layout sentinel; retain it unchanged.
    require_version(&value, name, &[1, 2])?;
    require_array(&value, "tabs", name)?;
    let workspace: DebugWorkspace = decode(value, name)?;
    if workspace.tabs.len() > 50
        || workspace.closed_tabs.len() > 10
        || workspace.recent_actions.len() > 20
    {
        return Err(format!("{name} 工作区标签或最近动作数量超出限制"));
    }
    let mut ids = HashSet::new();
    for tab in workspace.tabs.iter().chain(&workspace.closed_tabs) {
        if tab.id.trim().is_empty() || !ids.insert(&tab.id) {
            return Err(format!("{name} 含重复或空标签 ID，已中止导入"));
        }
    }
    if workspace
        .active_tab
        .as_ref()
        .is_some_and(|active| !workspace.tabs.iter().any(|tab| &tab.id == active))
    {
        return Err(format!("{name} 激活标签不存在，已中止导入"));
    }
    encode(&workspace, name)
}

fn validate_collections(value: Value, name: &str) -> Result<Value, String> {
    require_version(&value, name, &[1])?;
    require_array(&value, "folders", name)?;
    require_array(&value, "requests", name)?;
    let collections: DebugCollections = decode(value, name)?;
    let mut ids = HashSet::new();
    for id in collections
        .folders
        .iter()
        .map(|folder| &folder.id)
        .chain(collections.requests.iter().map(|request| &request.id))
    {
        if id.trim().is_empty() || !ids.insert(id) {
            return Err(format!("{name} 含重复或空收藏 ID，已中止导入"));
        }
    }
    let folders: HashSet<_> = collections
        .folders
        .iter()
        .map(|folder| &folder.id)
        .collect();
    if collections.requests.iter().any(|request| {
        request
            .folder_id
            .as_ref()
            .is_some_and(|id| !folders.contains(id))
    }) {
        return Err(format!("{name} 收藏引用了不存在的文件夹，已中止导入"));
    }
    encode(&collections, name)
}

fn validate_snowluma(value: Value, name: &str) -> Result<Value, String> {
    if !value.is_object()
        || !["snowlumaWebuiPort", "snowlumaWebuiPasswordOverride"]
            .iter()
            .any(|key| value.get(key).is_some())
    {
        return Err(format!("{name} 缺少 SnowLuma 全局设置字段，已中止导入"));
    }
    let config: SnowLumaAppConfig = decode(value, name)?;
    if config.webui_port == 0 {
        return Err(format!("{name} WebUI 端口不能是 0"));
    }
    encode(&config, name)
}

fn validate_adopt(value: Value, entry: &TransferEntry) -> Result<Value, String> {
    let name = &entry.archive_name;
    require_version(&value, name, &[1])?;
    let snapshot: AdoptSnapshot = decode(value, name)?;
    let file_id = name
        .strip_prefix("config/app-adopts/")
        .and_then(|leaf| leaf.strip_suffix(".json"));
    if file_id != Some(snapshot.instance_id.as_str())
        || !is_safe_instance_id(&snapshot.instance_id)
        || !valid_host(&snapshot.host_id)
        || snapshot.install_dir.trim().is_empty()
        || snapshot.install_dir.contains('\0')
    {
        return Err(format!("{name} 领养快照实例 ID 与文件名不符或主机路径无效"));
    }
    let mut paths = HashSet::new();
    for file in &snapshot.files {
        validate_relative_path(&file.rel_path).map_err(|error| format!("{name}: {error}"))?;
        if !paths.insert(&file.rel_path) {
            return Err(format!("{name} 含重复的领养文件路径"));
        }
    }
    encode(&snapshot, name)
}

fn validate_framework(value: Value, entry: &TransferEntry) -> Result<Value, String> {
    let backup: FrameworkConfigBackup = decode(value, &entry.archive_name)?;
    backup
        .validate()
        .map_err(|e| format!("{}: {e}", entry.archive_name))?;
    if entry.data_relative != format!("config/framework-configs/{}.json", backup.instance_id) {
        return Err(format!(
            "{} 框架配置实例 ID 与文件名不符",
            entry.archive_name
        ));
    }
    encode(&backup, &entry.archive_name)
}

/// 框架备份必须绑定包中同一实例，不能用配置包指定任意主机路径写文件。
pub(super) fn validate_framework_links(entries: &[super::source::RawEntry]) -> Result<(), String> {
    let backups: Vec<_> = entries
        .iter()
        .filter(|e| e.descriptor.kind == TransferKind::Framework)
        .collect();
    if backups.is_empty() {
        return Ok(());
    }
    let instances = entries
        .iter()
        .find(|e| e.descriptor.kind == TransferKind::Instances)
        .ok_or("框架配置备份缺少 app-instances.json，无法确定目标实例")?;
    let file: AppInstancesFile = decode(instances.value.clone(), "app-instances.json")?;
    for entry in backups {
        let backup: FrameworkConfigBackup =
            decode(entry.value.clone(), &entry.descriptor.archive_name)?;
        let instance = file
            .instances
            .iter()
            .find(|i| {
                i.id.as_str() == backup.instance_id
                    && i.framework_id.as_str() == backup.framework_id
            })
            .ok_or_else(|| format!("{} 没有对应的同框架实例档案", entry.descriptor.archive_name))?;
        if let Some(server_id) = instance.host_id.strip_prefix(REMOTE_HOST_ID_PREFIX) {
            let servers = entries
                .iter()
                .find(|e| e.descriptor.kind == TransferKind::Servers)
                .ok_or("远端框架配置备份缺少 servers.json，无法核对主机档案")?;
            let profiles: Vec<ncd_runtime::ServerProfile> = decode(
                super::normalize_servers_import(servers.value.clone())?,
                "servers.json",
            )?;
            if !profiles.iter().any(|profile| profile.id == server_id) {
                return Err(format!(
                    "{} 缺少对应的服务器档案 {server_id}",
                    entry.descriptor.archive_name
                ));
            }
        }
    }
    Ok(())
}

pub(super) fn validate_frontend(preferences: &ConfigFrontendPreferences) -> Result<(), String> {
    if preferences.version != 1 || preferences.storage.len() > MAX_FRONTEND_KEYS {
        return Err("frontend-preferences.json 版本不受支持或偏好键过多".into());
    }
    for (key, raw) in &preferences.storage {
        if raw.len() > MAX_FRONTEND_VALUE_BYTES {
            return Err(format!("frontend-preferences.json 偏好 {key} 超出大小限制"));
        }
        match key.as_str() {
            "ncd.terminal.prefs.v1" | "ncd.terminal.layout.v1" | "ncd.chat.ui.v1" => {
                let value: Value = serde_json::from_str(raw)
                    .map_err(|error| format!("浏览器偏好 {key} 不是合法 JSON: {error}"))?;
                validate_frontend_object(key, &value)?;
            }
            "ncd:bot_custom_order:v1" => {
                let value: Value = serde_json::from_str(raw)
                    .map_err(|error| format!("浏览器偏好 {key} 不是合法 JSON: {error}"))?;
                if !valid_string_list(&value, 1024, false) {
                    return Err(format!("浏览器偏好 {key} 必须是字符串数组"));
                }
            }
            _ if key
                .strip_prefix("ncd.maibot.chat.name.")
                .is_some_and(is_safe_instance_id) =>
            {
                if raw.chars().count() > 128 || raw.contains('\0') {
                    return Err(format!("浏览器偏好 {key} 昵称格式无效"));
                }
            }
            _ => {
                return Err(format!(
                    "frontend-preferences.json 含未允许的浏览器偏好键: {key}"
                ));
            }
        }
    }
    if serde_json::to_vec(preferences)
        .map_err(|error| error.to_string())?
        .len()
        > MAX_FRONTEND_BYTES
    {
        return Err("frontend-preferences.json 超出 2 MiB 总大小限制".into());
    }
    Ok(())
}

fn valid_string_list(value: &Value, limit: usize, allow_empty: bool) -> bool {
    value.as_array().is_some_and(|items| {
        items.len() <= limit
            && items.iter().all(|item| {
                item.as_str().is_some_and(|text| {
                    (allow_empty || !text.is_empty()) && text.chars().count() <= 256
                })
            })
    })
}

fn validate_frontend_object(key: &str, value: &Value) -> Result<(), String> {
    let invalid = || format!("浏览器偏好 {key} 格式无效");
    let object = value.as_object().ok_or_else(invalid)?;
    let numbers: &[&str] = match key {
        "ncd.terminal.prefs.v1" => &["fontSize", "lineHeight", "scrollback"],
        "ncd.terminal.layout.v1" => &["height", "filesWidth"],
        _ => &["listWidth", "composerHeight"],
    };
    for field in numbers {
        if let Some(value) = object.get(*field) {
            if *field == "composerHeight" && value.is_null() {
                continue;
            }
            if !value.as_f64().is_some_and(f64::is_finite) {
                return Err(invalid());
            }
        }
    }
    let booleans: &[&str] = match key {
        "ncd.terminal.prefs.v1" => &[
            "cursorBlink",
            "copyOnSelect",
            "highlight",
            "confirmMultilinePaste",
            "gpu",
        ],
        "ncd.terminal.layout.v1" => &["filesOpen"],
        _ => &[],
    };
    for field in booleans {
        if object.get(*field).is_some_and(|value| !value.is_boolean()) {
            return Err(invalid());
        }
    }
    if key == "ncd.terminal.prefs.v1" {
        for (field, choices) in [
            ("cursorStyle", &["block", "bar", "underline"][..]),
            ("rightClick", &["menu", "paste"][..]),
            ("colorScheme", &["auto", "dark"][..]),
            (
                "defaultShell",
                &["pwsh", "windows_powershell", "cmd", "git_bash", "wsl"][..],
            ),
        ] {
            if let Some(value) = object.get(field) {
                if field == "defaultShell" && value.is_null() {
                    continue;
                }
                if !value.as_str().is_some_and(|text| choices.contains(&text)) {
                    return Err(invalid());
                }
            }
        }
        if let Some(value) = object.get("snippets") {
            if !value.as_array().is_some_and(|snippets| {
                snippets.len() <= 1000
                    && snippets.iter().all(|item| {
                        item.get("label")
                            .and_then(Value::as_str)
                            .is_some_and(|text| text.chars().count() <= 256)
                            && item
                                .get("command")
                                .and_then(Value::as_str)
                                .is_some_and(|text| text.chars().count() <= 32768)
                    })
            }) {
                return Err(invalid());
            }
        }
    }
    if key == "ncd.chat.ui.v1" {
        if let Some(value) = object.get("hiddenConversations") {
            if !value.as_object().is_some_and(|accounts| {
                accounts.len() <= 128
                    && accounts
                        .values()
                        .all(|keys| valid_string_list(keys, 1024, true))
            }) {
                return Err(invalid());
            }
        }
    }
    Ok(())
}

pub(super) fn normalize_entry(
    entry: &TransferEntry,
    value: Value,
    secrets: &dyn SecretStore,
) -> Result<(Value, Option<Value>), String> {
    let name = &entry.archive_name;
    let normalized = match entry.kind {
        TransferKind::AppConfig => return super::normalize_app_config_import(value),
        TransferKind::BotConfig => super::normalize_bot_config_import(value, secrets)?,
        TransferKind::AppSettings => super::normalize_app_settings_import(value)?,
        TransferKind::Servers => super::normalize_servers_import(value)?,
        TransferKind::Instances => validate_instances(value, name)?,
        TransferKind::Chat => validate_chat(value, name)?,
        TransferKind::DebugWorkspace => validate_workspace(value, name)?,
        TransferKind::DebugCollections => validate_collections(value, name)?,
        TransferKind::SnowLuma => validate_snowluma(value, name)?,
        TransferKind::Adopt => validate_adopt(value, entry)?,
        TransferKind::Framework => validate_framework(value, entry)?,
        TransferKind::Frontend => {
            let preferences: ConfigFrontendPreferences = decode(value, name)?;
            validate_frontend(&preferences)?;
            encode(&preferences, name)?
        }
    };
    Ok((normalized, None))
}
