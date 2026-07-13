use serde_json::{Map, Value};

use ncd_domain::errors::MigrationError;
use ncd_domain::{AppSettings, OfflineEmailSettings, OfflineWebhookSettings};

pub const LEGACY_APP_COMPAT_VERSION: &str = "v2.0";
const LEGACY_CONFIG_VERSION: &str = "v1.7.28";
/// 新版设置页落盘文件名(与 tauri app_settings 命令一致)
pub const APP_SETTINGS_FILE: &str = "app-settings.json";

const LEGACY_BACKGROUND_KEYS: &[&str] = &[
    "BgHomePage",
    "BgHomePageOpacity",
    "BgHomePageLight",
    "BgHomePageDark",
    "BgAddPage",
    "BgAddPageOpacity",
    "BgAddPageLight",
    "BgAddPageDark",
    "BgListPage",
    "BgListPageOpacity",
    "BgListPageLight",
    "BgListPageDark",
    "BgUnitPage",
    "BgUnitPageOpacity",
    "BgUnitPageLight",
    "BgUnitPageDark",
    "BgSettingPage",
    "BgSettingPageOpacity",
    "BgSettingPageLight",
    "BgSettingPageDark",
];

const LEGACY_TITLE_TAB_BAR_KEYS: &[&str] = &[
    "TitleTabBar",
    "TitleTabBarIsMovable",
    "TitleTabBarIsScrollable",
    "TitleTabBarIsShadow",
    "TitleTabBarCloseButton",
    "TitleTabBarMinWidth",
    "TitleTabBarMaxWidth",
];

#[derive(Debug, Clone, PartialEq)]
pub struct AppConfigMigrationResult {
    pub payload: Value,
    pub source_version: String,
    pub target_version: String,
    pub rules_applied: Vec<String>,
}

/// 从旧版 QConfig `config.json` 抽出写入 `app-settings.json` 的字段
#[derive(Debug, Clone, PartialEq)]
pub struct AppSettingsSeedResult {
    pub settings: AppSettings,
    pub rules_applied: Vec<String>,
    pub took_event: bool,
    pub took_webhook: bool,
    pub took_email: bool,
    pub took_login_interval: bool,
}

impl AppSettingsSeedResult {
    pub fn has_any(&self) -> bool {
        self.took_event || self.took_webhook || self.took_email || self.took_login_interval
    }

    /// 把本 seed 里实际抽到的段合并进已有 AppSettings(不覆盖未出现的段)
    pub fn merge_into(self, mut base: AppSettings) -> AppSettings {
        if self.took_event {
            base.poller.offline_webhook_notice = self.settings.poller.offline_webhook_notice;
            base.poller.offline_email_notice = self.settings.poller.offline_email_notice;
        }
        if self.took_login_interval {
            base.poller.bot_login_check_interval_ms =
                self.settings.poller.bot_login_check_interval_ms;
        }
        if self.took_webhook {
            base.offline_webhook = self.settings.offline_webhook;
        }
        if self.took_email {
            base.offline_email = self.settings.offline_email;
        }
        base.offline_webhook.normalize();
        base
    }
}

pub fn migrate_app_config(payload: Value) -> AppConfigMigrationResult {
    let mut object = match payload {
        Value::Object(map) => map,
        _ => Map::new(),
    };
    let source_version = read_config_version(&object);
    let mut current_version = source_version.clone();
    let mut visited = Vec::new();
    let mut rules_applied = Vec::new();

    while current_version != LEGACY_APP_COMPAT_VERSION && !visited.contains(&current_version) {
        visited.push(current_version.clone());
        let (next, rules) = match current_version.as_str() {
            "v1.5.4" => ("v1.6.0", migrate_v154_to_v160(&mut object)),
            "v1.6.0" => ("v1.7.0", migrate_v160_to_v170(&mut object)),
            "v1.7.0" => ("v1.7.28", migrate_v170_to_v1728(&mut object)),
            "v1.7.28" => ("v2.0", migrate_v1728_to_v20(&mut object)),
            _ => break,
        };
        if rules.is_empty() {
            rules_applied.push(format!("{}->{}: no-op", current_version, next));
        } else {
            rules_applied.extend(
                rules
                    .into_iter()
                    .map(|rule| format!("{}->{}: {}", current_version, next, rule)),
            );
        }
        current_version = next.to_string();
    }

    rules_applied.extend(remove_transient_config_fields(&mut object));
    write_nested(
        &mut object,
        &["Info", "ConfigVersion"],
        Value::from(LEGACY_APP_COMPAT_VERSION),
        true,
    );

    AppConfigMigrationResult {
        payload: Value::Object(object),
        source_version,
        target_version: LEGACY_APP_COMPAT_VERSION.to_string(),
        rules_applied,
    }
}

/// 从旧 Desktop QConfig 风格 config.json 抽取离线通知相关字段到 AppSettings
///
/// 映射:
/// - Event.BotOfflineWebHookNotice / BotOfflineEmailNotice -> poller 开关
/// - Performance.BotLoginCheckInterval -> poller 间隔
/// - WebHook.* -> offline_webhook(扁平字段,normalize 后可进 channels)
/// - Email.* -> offline_email
///
/// 不改动传入 payload;不碰 SL notifications.json。
pub fn app_settings_from_legacy_config(payload: &Value) -> AppSettingsSeedResult {
    let mut settings = AppSettings::default();
    let mut rules = Vec::new();
    let mut took_event = false;
    let mut took_webhook = false;
    let mut took_email = false;
    let mut took_login_interval = false;

    let Some(root) = payload.as_object() else {
        return AppSettingsSeedResult {
            settings,
            rules_applied: rules,
            took_event,
            took_webhook,
            took_email,
            took_login_interval,
        };
    };

    if let Some(event) = root.get("Event").and_then(Value::as_object) {
        if let Some(v) = event.get("BotOfflineWebHookNotice").and_then(value_as_bool) {
            settings.poller.offline_webhook_notice = v;
            took_event = true;
            rules.push("Event.BotOfflineWebHookNotice -> poller.offline_webhook_notice".into());
        }
        if let Some(v) = event.get("BotOfflineEmailNotice").and_then(value_as_bool) {
            settings.poller.offline_email_notice = v;
            took_event = true;
            rules.push("Event.BotOfflineEmailNotice -> poller.offline_email_notice".into());
        }
    }

    if let Some(perf) = root.get("Performance").and_then(Value::as_object) {
        if let Some(ms) = perf
            .get("BotLoginCheckInterval")
            .and_then(value_as_u64)
            .filter(|n| *n > 0)
        {
            settings.poller.bot_login_check_interval_ms = ms;
            took_login_interval = true;
            rules.push(
                "Performance.BotLoginCheckInterval -> poller.bot_login_check_interval_ms".into(),
            );
        }
    }

    if let Some(wh) = root.get("WebHook").and_then(Value::as_object) {
        let mut webhook = OfflineWebhookSettings::default();
        let mut any = false;
        if let Some(s) = wh.get("WebHookUrl").and_then(value_as_string) {
            webhook.url = s;
            any = true;
        }
        if let Some(s) = wh.get("WebHookSecret").and_then(value_as_string) {
            webhook.secret = s;
            any = true;
        }
        if let Some(s) = wh.get("WebHookJson").and_then(value_as_string) {
            if !s.trim().is_empty() {
                webhook.body_template = s;
            }
            any = true;
        }
        if let Some(s) = wh.get("WebHookMethod").and_then(value_as_string) {
            if !s.trim().is_empty() {
                webhook.method = s;
            }
            any = true;
        }
        if any {
            webhook.normalize();
            settings.offline_webhook = webhook;
            took_webhook = true;
            rules.push("WebHook.* -> offline_webhook".into());
        }
    }

    if let Some(em) = root.get("Email").and_then(Value::as_object) {
        let mut email = OfflineEmailSettings::default();
        let mut any = false;
        if let Some(s) = em.get("EmailSender").and_then(value_as_string) {
            email.sender = s;
            any = true;
        }
        if let Some(s) = em.get("EmailReceiver").and_then(value_as_string) {
            email.receiver = s;
            any = true;
        }
        if let Some(s) = em.get("EmailToken").and_then(value_as_string) {
            email.token = s;
            any = true;
        }
        if let Some(s) = em.get("EmailStmpServer").and_then(value_as_string) {
            email.smtp_server = s;
            any = true;
        }
        if let Some(port) = em.get("EmailStmpPort").and_then(value_as_u64) {
            if port > 0 && port <= u16::MAX as u64 {
                email.smtp_port = port as u16;
                any = true;
            }
        }
        if let Some(s) = em.get("EmailEncryption").and_then(value_as_string) {
            if !s.trim().is_empty() {
                email.encryption = s;
            }
            any = true;
        }
        if any {
            settings.offline_email = email;
            took_email = true;
            rules.push("Email.* -> offline_email".into());
        }
    }

    AppSettingsSeedResult {
        settings,
        rules_applied: rules,
        took_event,
        took_webhook,
        took_email,
        took_login_interval,
    }
}

fn value_as_bool(value: &Value) -> Option<bool> {
    match value {
        Value::Bool(b) => Some(*b),
        Value::String(s) => match s.trim().to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" => Some(true),
            "false" | "0" | "no" => Some(false),
            _ => None,
        },
        Value::Number(n) => n.as_u64().map(|v| v != 0),
        _ => None,
    }
}

fn value_as_u64(value: &Value) -> Option<u64> {
    match value {
        Value::Number(n) => n.as_u64().or_else(|| n.as_f64().map(|f| f.round() as u64)),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

fn value_as_string(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn migrate_v154_to_v160(payload: &mut Map<String, Value>) -> Vec<String> {
    let mut rules = Vec::new();
    if move_nested(
        payload,
        &["Personalized", "CloseBtnAction"],
        &["General", "CloseBtnAction"],
    ) {
        rules.push("Personalized.CloseBtnAction -> General.CloseBtnAction".to_string());
    }
    for key in LEGACY_BACKGROUND_KEYS {
        if remove_nested(payload, &["Personalize", key]) {
            rules.push(format!("Personalize.{} removed", key));
        }
    }
    if remove_nested(payload, &["HideTips", "HideUsingGoBtnTips"]) {
        rules.push("HideTips.HideUsingGoBtnTips removed".to_string());
    }
    cleanup_empty_sections(payload);
    rules
}

fn migrate_v160_to_v170(payload: &mut Map<String, Value>) -> Vec<String> {
    let mut rules = Vec::new();
    if move_nested(payload, &["Info", "main_window"], &["Info", "MainWindow"]) {
        rules.push("Info.main_window -> Info.MainWindow".to_string());
    }
    rules
}

fn migrate_v170_to_v1728(payload: &mut Map<String, Value>) -> Vec<String> {
    let mut rules = Vec::new();
    if write_nested(
        payload,
        &["Info", "EulaAccepted"],
        Value::Bool(false),
        false,
    ) {
        rules.push("Info.EulaAccepted default".to_string());
    }
    for key in LEGACY_TITLE_TAB_BAR_KEYS {
        if remove_nested(payload, &["Personalize", key]) {
            rules.push(format!("Personalize.{} removed", key));
        }
    }
    cleanup_empty_sections(payload);
    rules
}

fn migrate_v1728_to_v20(payload: &mut Map<String, Value>) -> Vec<String> {
    let mut rules = Vec::new();
    let theme_mode = nested(payload, &["Personalize", "ThemeMode"]).cloned();
    if let Some(value) = theme_mode {
        if write_nested(payload, &["QFluentWidgets", "ThemeMode"], value, false) {
            rules.push("Personalize.ThemeMode -> QFluentWidgets.ThemeMode".to_string());
        }
    }
    let theme_color = nested(payload, &["Personalize", "ThemeColor"]).cloned();
    if let Some(value) = theme_color {
        if write_nested(payload, &["QFluentWidgets", "ThemeColor"], value, false) {
            rules.push("Personalize.ThemeColor -> QFluentWidgets.ThemeColor".to_string());
        }
    }
    if write_nested(
        payload,
        &["Home", "IgnoredNoticeKeys"],
        Value::from("[]"),
        false,
    ) {
        rules.push("Home.IgnoredNoticeKeys default".to_string());
    }
    if write_nested(
        payload,
        &["Home", "SnoozedNoticeItems"],
        Value::from("{}"),
        false,
    ) {
        rules.push("Home.SnoozedNoticeItems default".to_string());
    }
    rules
}

fn remove_transient_config_fields(payload: &mut Map<String, Value>) -> Vec<String> {
    let mut rules = Vec::new();
    if remove_nested(payload, &["Remote", "Password"]) {
        rules.push("transient: Remote.Password removed".to_string());
    }
    cleanup_empty_sections(payload);
    rules
}

fn read_config_version(payload: &Map<String, Value>) -> String {
    if let Some(version) = nested(payload, &["Info", "ConfigVersion"]).and_then(Value::as_str) {
        if !version.trim().is_empty() {
            return version.trim().to_string();
        }
    }
    if nested(payload, &["Info", "ConfigSchemaVersion"]).is_some() {
        return LEGACY_APP_COMPAT_VERSION.to_string();
    }
    if LEGACY_BACKGROUND_KEYS
        .iter()
        .any(|key| nested(payload, &["Personalize", key]).is_some())
        || nested(payload, &["HideTips", "HideUsingGoBtnTips"]).is_some()
    {
        return "v1.5.4".to_string();
    }
    if nested(payload, &["Info", "main_window"]).is_some() {
        return "v1.6.0".to_string();
    }
    if LEGACY_TITLE_TAB_BAR_KEYS
        .iter()
        .any(|key| nested(payload, &["Personalize", key]).is_some())
        || nested(payload, &["Info", "EulaAccepted"]).is_none()
    {
        return "v1.7.0".to_string();
    }
    LEGACY_CONFIG_VERSION.to_string()
}

fn nested<'a>(payload: &'a Map<String, Value>, path: &[&str]) -> Option<&'a Value> {
    let mut current = payload;
    for segment in &path[..path.len().saturating_sub(1)] {
        current = current.get(*segment)?.as_object()?;
    }
    current.get(path[path.len() - 1])
}

#[allow(clippy::expect_used)]
fn write_nested(
    payload: &mut Map<String, Value>,
    path: &[&str],
    value: Value,
    overwrite: bool,
) -> bool {
    let mut current = payload;
    for segment in &path[..path.len() - 1] {
        let entry = current
            .entry((*segment).to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        if !entry.is_object() {
            *entry = Value::Object(Map::new());
        }
        current = entry.as_object_mut().expect("object inserted above");
    }
    let leaf = path[path.len() - 1];
    if !overwrite && current.contains_key(leaf) {
        return false;
    }
    current.insert(leaf.to_string(), value);
    true
}

fn remove_nested(payload: &mut Map<String, Value>, path: &[&str]) -> bool {
    let mut current = payload;
    for segment in &path[..path.len() - 1] {
        let Some(next) = current.get_mut(*segment).and_then(Value::as_object_mut) else {
            return false;
        };
        current = next;
    }
    current.remove(path[path.len() - 1]).is_some()
}

fn move_nested(payload: &mut Map<String, Value>, source: &[&str], target: &[&str]) -> bool {
    let Some(value) = take_nested(payload, source) else {
        return false;
    };
    if write_nested(payload, target, value.clone(), false) {
        true
    } else {
        write_nested(payload, source, value, false);
        false
    }
}

fn take_nested(payload: &mut Map<String, Value>, path: &[&str]) -> Option<Value> {
    let mut current = payload;
    for segment in &path[..path.len() - 1] {
        current = current.get_mut(*segment)?.as_object_mut()?;
    }
    current.remove(path[path.len() - 1])
}

fn cleanup_empty_sections(payload: &mut Map<String, Value>) {
    let empty: Vec<String> = payload
        .iter()
        .filter(|(_, value)| value.as_object().is_some_and(Map::is_empty))
        .map(|(key, _)| key.clone())
        .collect();
    for key in empty {
        payload.remove(&key);
    }
}

pub fn ensure_object_payload(payload: Value) -> Result<Map<String, Value>, MigrationError> {
    match payload {
        Value::Object(map) => Ok(map),
        _ => Err(MigrationError::InvalidPayload(
            "JSON payload root must be an object".to_string(),
        )),
    }
}

/// 旧版应用配置 config.json 的已知顶层段判断"像不像应用配置"用,避免把误选的
/// 无关 JSON(数组 / 字符串 / 别的程序的 config.json)当应用配置迁移
const APP_CONFIG_KNOWN_SECTIONS: &[&str] = &[
    "Info",
    "General",
    "Personalize",
    "Personalized",
    "QFluentWidgets",
    "Home",
    "HideTips",
    "Remote",
    "Event",
    "WebHook",
    "Email",
    "Performance",
];

/// 判断一段 JSON 是否"像"旧版应用配置 config.json
///
/// 要求是非空 JSON 对象,且至少带一个已知配置段(Info / Personalize / ...)否则
/// 视作误选的无关文件,迁移层会跳过它而不是强转成空对象写 ConfigVersion 当成功——
/// 后者会把垃圾 / 数组 / 字符串 config.json 静默"迁移成功"bot 配置另走强类型路径,
/// 不受此影响
pub fn looks_like_app_config(value: &Value) -> bool {
    value.as_object().is_some_and(|obj| {
        APP_CONFIG_KNOWN_SECTIONS
            .iter()
            .any(|k| obj.contains_key(*k))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_legacy_app_keys() {
        let result = migrate_app_config(serde_json::json!({
            "Info": {"main_window": true},
            "Personalized": {"CloseBtnAction": "close"},
            "Personalize": {"BgHomePage": "x", "ThemeMode": "Dark"},
            "Remote": {"Password": "secret"}
        }));

        assert_eq!(result.payload["Info"]["ConfigVersion"], "v2.0");
        assert_eq!(result.payload["Info"]["MainWindow"], true);
        assert!(result.payload["Personalize"].get("BgHomePage").is_none());
        assert!(result.payload["Remote"].get("Password").is_none());
    }

    #[test]
    fn looks_like_app_config_accepts_known_sections_rejects_garbage() {
        assert!(looks_like_app_config(
            &serde_json::json!({"Info": {"ConfigVersion": "v2.0"}})
        ));
        assert!(looks_like_app_config(
            &serde_json::json!({"Personalize": {"ThemeMode": "Dark"}})
        ));
        assert!(looks_like_app_config(
            &serde_json::json!({"WebHook": {"WebHookUrl": "https://x"}})
        ));
        assert!(!looks_like_app_config(&serde_json::json!([1, 2, 3])));
        assert!(!looks_like_app_config(&serde_json::json!("just a string")));
        assert!(!looks_like_app_config(&serde_json::json!(null)));
        assert!(!looks_like_app_config(&serde_json::json!({})));
        assert!(!looks_like_app_config(
            &serde_json::json!({"unrelated": true})
        ));
    }

    #[test]
    fn seeds_app_settings_from_legacy_webhook_and_email() {
        let legacy = serde_json::json!({
            "Info": {"ConfigVersion": "v2.0"},
            "Event": {
                "BotOfflineWebHookNotice": true,
                "BotOfflineEmailNotice": true
            },
            "Performance": {
                "BotLoginCheckInterval": 2500
            },
            "WebHook": {
                "WebHookUrl": "https://sct.example/send",
                "WebHookSecret": "tok",
                "WebHookJson": "{\"title\":\"{bot_name}\"}",
                "WebHookMethod": "post"
            },
            "Email": {
                "EmailSender": "a@b.com",
                "EmailReceiver": "c@d.com",
                "EmailToken": "auth",
                "EmailStmpServer": "smtp.example.com",
                "EmailStmpPort": 465,
                "EmailEncryption": "SSL"
            }
        });

        let seed = app_settings_from_legacy_config(&legacy);
        assert!(seed.has_any());
        assert!(
            seed.took_event && seed.took_webhook && seed.took_email && seed.took_login_interval
        );
        assert!(seed.settings.poller.offline_webhook_notice);
        assert!(seed.settings.poller.offline_email_notice);
        assert_eq!(seed.settings.poller.bot_login_check_interval_ms, 2500);
        assert_eq!(
            seed.settings.offline_webhook.url,
            "https://sct.example/send"
        );
        assert_eq!(seed.settings.offline_webhook.secret, "tok");
        assert_eq!(seed.settings.offline_webhook.method, "POST");
        assert!(!seed.settings.offline_webhook.channels.is_empty());
        assert_eq!(seed.settings.offline_email.sender, "a@b.com");
        assert_eq!(seed.settings.offline_email.receiver, "c@d.com");
        assert_eq!(seed.settings.offline_email.token, "auth");
        assert_eq!(seed.settings.offline_email.smtp_server, "smtp.example.com");
        assert_eq!(seed.settings.offline_email.smtp_port, 465);
        assert_eq!(seed.settings.offline_email.encryption, "SSL");
    }

    #[test]
    fn seed_merge_preserves_untouched_sections() {
        let mut base = AppSettings::default();
        base.poller.bot_login_check_interval_ms = 9000;
        base.offline_email.sender = "keep@me".into();

        let seed = app_settings_from_legacy_config(&serde_json::json!({
            "WebHook": {
                "WebHookUrl": "https://only-webhook"
            }
        }));
        assert!(seed.took_webhook);
        assert!(!seed.took_email);
        let merged = seed.merge_into(base);
        assert_eq!(merged.offline_webhook.url, "https://only-webhook");
        assert_eq!(merged.offline_email.sender, "keep@me");
        assert_eq!(merged.poller.bot_login_check_interval_ms, 9000);
    }

    #[test]
    fn empty_legacy_yields_no_seed_fields() {
        let seed = app_settings_from_legacy_config(&serde_json::json!({
            "Info": {"ConfigVersion": "v2.0"}
        }));
        assert!(!seed.has_any());
        assert!(seed.rules_applied.is_empty());
    }
}
