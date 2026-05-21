use serde_json::{Map, Value};

use crate::errors::MigrationError;

pub const LEGACY_APP_COMPAT_VERSION: &str = "v2.0";
const LEGACY_CONFIG_VERSION: &str = "v1.7.28";

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
        .filter_map(|(key, value)| {
            value
                .as_object()
                .is_some_and(Map::is_empty)
                .then(|| key.clone())
        })
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
}
