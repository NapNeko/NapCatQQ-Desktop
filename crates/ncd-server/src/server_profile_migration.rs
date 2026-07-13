use serde_json::{Map, Value};

use ncd_domain::errors::MigrationError;

use crate::server_manager::{AuthMethod, ServerProfile, ServerState};

pub const SERVER_PROFILE_COMPAT_VERSION: &str = "v2";
const LEGACY_SERVER_PROFILE_VERSION: &str = "v1";

#[derive(Debug, Clone, PartialEq)]
pub struct ServerProfileMigrationResult {
    pub payload: Value,
    pub source_version: String,
    pub target_version: String,
    pub rules_applied: Vec<String>,
    pub profiles: Vec<ServerProfile>,
}

pub fn migrate_server_profiles_payload(
    payload: Value,
) -> Result<ServerProfileMigrationResult, MigrationError> {
    let source_version = read_server_profile_version(&payload);
    let mut current_version = source_version.clone();
    let mut current_payload = payload;
    let mut rules_applied = Vec::new();

    while current_version != SERVER_PROFILE_COMPAT_VERSION {
        let (next, payload, rules) = match current_version.as_str() {
            "v1" => {
                let (payload, rules) = migrate_v1_to_v2(current_payload)?;
                ("v2", payload, rules)
            }
            other => {
                return Err(MigrationError::InvalidPayload(format!(
                    "unsupported servers.json schema version: {other}"
                )));
            }
        };
        rules_applied.extend(
            rules
                .into_iter()
                .map(|rule| format!("{}->{}: {}", current_version, next, rule)),
        );
        current_payload = payload;
        current_version = next.to_string();
    }

    result_from_current_payload(source_version, rules_applied, current_payload)
}

pub fn migrate_legacy_single_server_app_config(
    payload: &Value,
) -> Result<Option<ServerProfileMigrationResult>, MigrationError> {
    let Some(remote) = payload.get("Remote").and_then(Value::as_object) else {
        return Ok(None);
    };
    let Some(host) = string_field(remote, "Host") else {
        return Ok(None);
    };

    let profile = ServerProfile {
        id: short_uuid(),
        name: format!("已迁移服务器 ({host})"),
        host,
        port: u16_field(remote, "Port").unwrap_or(22),
        username: string_field(remote, "Username").unwrap_or_default(),
        auth_method: auth_method_from(remote, &["AuthMethod"], &["Password"]),
        private_key_path: string_field(remote, "PrivateKeyPath"),
        remember_credential: false,
        state: ServerState::Disconnected,
        health: None,
        webui_url: None,
    };

    let payload = serde_json::to_value(vec![profile.clone()])
        .map_err(|e| MigrationError::InvalidPayload(format!("serialize server profiles: {e}")))?;

    Ok(Some(ServerProfileMigrationResult {
        payload,
        source_version: "legacy-app-remote".to_string(),
        target_version: SERVER_PROFILE_COMPAT_VERSION.to_string(),
        rules_applied: vec!["Remote.* -> config/servers.json ServerProfile".to_string()],
        profiles: vec![profile],
    }))
}

fn result_from_current_payload(
    source_version: String,
    rules_applied: Vec<String>,
    current_payload: Value,
) -> Result<ServerProfileMigrationResult, MigrationError> {
    let profiles: Vec<ServerProfile> = serde_json::from_value(current_payload)
        .map_err(|e| MigrationError::InvalidPayload(format!("invalid server profiles: {e}")))?;
    let payload = serde_json::to_value(&profiles)
        .map_err(|e| MigrationError::InvalidPayload(format!("serialize server profiles: {e}")))?;

    Ok(ServerProfileMigrationResult {
        payload,
        source_version,
        target_version: SERVER_PROFILE_COMPAT_VERSION.to_string(),
        rules_applied,
        profiles,
    })
}

fn migrate_v1_to_v2(payload: Value) -> Result<(Value, Vec<String>), MigrationError> {
    let mut object = ensure_object(payload, "servers.json v1 root must be an object")?;
    let servers = object.remove("servers").ok_or_else(|| {
        MigrationError::InvalidPayload("servers.json v1 missing servers array".to_string())
    })?;
    let Value::Array(entries) = servers else {
        return Err(MigrationError::InvalidPayload(
            "servers.json v1 servers field must be an array".to_string(),
        ));
    };

    let mut migrated = Vec::new();
    for entry in entries {
        if let Some(profile) = migrate_v1_profile_entry(entry) {
            let value = serde_json::to_value(profile).map_err(|e| {
                MigrationError::InvalidPayload(format!("serialize migrated server profile: {e}"))
            })?;
            migrated.push(value);
        }
    }

    Ok((
        Value::Array(migrated),
        vec![
            "root.servers -> root array".to_string(),
            "credentials object -> flat ServerProfile fields".to_string(),
        ],
    ))
}

fn migrate_v1_profile_entry(value: Value) -> Option<ServerProfile> {
    let obj = value.as_object()?;
    let credentials = obj
        .get("credentials")
        .and_then(Value::as_object)
        .unwrap_or(obj);
    let host = string_field_any(credentials, &["host", "Host"])?;
    let username = string_field_any(credentials, &["username", "Username"]).unwrap_or_default();
    Some(ServerProfile {
        id: string_field(obj, "id").unwrap_or_else(short_uuid),
        name: string_field(obj, "name").unwrap_or_else(|| host.clone()),
        host,
        port: u16_field_any(credentials, &["port", "Port"]).unwrap_or(22),
        username,
        auth_method: auth_method_from(
            credentials,
            &["auth_method", "authMethod", "AuthMethod"],
            &["password", "Password"],
        ),
        private_key_path: string_field_any(
            credentials,
            &["private_key_path", "privateKeyPath", "PrivateKeyPath"],
        ),
        remember_credential: false,
        state: ServerState::Disconnected,
        health: None,
        webui_url: None,
    })
}

fn read_server_profile_version(payload: &Value) -> String {
    match payload {
        Value::Array(_) => SERVER_PROFILE_COMPAT_VERSION.to_string(),
        Value::Object(map) => map
            .get("schema_version")
            .and_then(Value::as_u64)
            .map(|version| format!("v{version}"))
            .unwrap_or_else(|| LEGACY_SERVER_PROFILE_VERSION.to_string()),
        _ => LEGACY_SERVER_PROFILE_VERSION.to_string(),
    }
}

fn ensure_object(value: Value, message: &str) -> Result<Map<String, Value>, MigrationError> {
    match value {
        Value::Object(map) => Ok(map),
        _ => Err(MigrationError::InvalidPayload(message.to_string())),
    }
}

fn string_field(map: &Map<String, Value>, key: &str) -> Option<String> {
    map.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn string_field_any(map: &Map<String, Value>, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| string_field(map, key))
}

fn u16_field(map: &Map<String, Value>, key: &str) -> Option<u16> {
    map.get(key)
        .and_then(Value::as_u64)
        .and_then(|value| u16::try_from(value).ok())
        .filter(|value| *value > 0)
}

fn u16_field_any(map: &Map<String, Value>, keys: &[&str]) -> Option<u16> {
    keys.iter().find_map(|key| u16_field(map, key))
}

fn auth_method_from(
    map: &Map<String, Value>,
    auth_keys: &[&str],
    password_keys: &[&str],
) -> AuthMethod {
    if let Some(auth_method) = string_field_any(map, auth_keys) {
        return match auth_method.to_ascii_lowercase().as_str() {
            "password" => AuthMethod::Password,
            _ => AuthMethod::Key,
        };
    }
    if string_field_any(map, password_keys).is_some() {
        AuthMethod::Password
    } else {
        AuthMethod::Key
    }
}

fn short_uuid() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 8];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_v1_wrapper_to_current_array_payload() {
        let result = migrate_server_profiles_payload(serde_json::json!({
            "schema_version": 1,
            "servers": [{
                "id": "legacy-s1",
                "name": "Legacy Server",
                "credentials": {
                    "host": "10.0.0.8",
                    "port": 22022,
                    "username": "ubuntu",
                    "auth_method": "password",
                    "private_key_path": "C:/Users/me/.ssh/id_ed25519"
                },
                "backend_flavor": "snowluma"
            }]
        }))
        .unwrap();

        assert_eq!(result.source_version, "v1");
        assert_eq!(result.target_version, "v2");
        assert_eq!(result.profiles.len(), 1);
        let profile = &result.profiles[0];
        assert_eq!(profile.id, "legacy-s1");
        assert_eq!(profile.name, "Legacy Server");
        assert_eq!(profile.host, "10.0.0.8");
        assert_eq!(profile.port, 22022);
        assert_eq!(profile.username, "ubuntu");
        assert_eq!(profile.auth_method, AuthMethod::Password);
        assert_eq!(profile.state, ServerState::Disconnected);
        assert!(result.payload.is_array());
    }

    #[test]
    fn current_array_round_trips_as_v2() {
        let result = migrate_server_profiles_payload(serde_json::json!([{
            "id": "s1",
            "name": "Server",
            "host": "10.0.0.9",
            "port": 22,
            "username": "root",
            "authMethod": "key",
            "privateKeyPath": "C:/Users/me/.ssh/id_ed25519",
            "rememberCredential": false,
            "state": "disconnected"
        }]))
        .unwrap();

        assert_eq!(result.source_version, "v2");
        assert_eq!(result.profiles.len(), 1);
        assert_eq!(result.profiles[0].auth_method, AuthMethod::Key);
    }

    #[test]
    fn migrates_flat_v1_wrapper_to_current_array_payload() {
        let result = migrate_server_profiles_payload(serde_json::json!({
            "servers": [{
                "id": "legacy-flat",
                "host": "127.0.0.1",
                "port": 22,
                "username": "napcat",
                "password": "placeholder"
            }]
        }))
        .unwrap();

        assert_eq!(result.source_version, "v1");
        assert_eq!(result.profiles.len(), 1);
        let profile = &result.profiles[0];
        assert_eq!(profile.id, "legacy-flat");
        assert_eq!(profile.host, "127.0.0.1");
        assert_eq!(profile.username, "napcat");
        assert_eq!(profile.auth_method, AuthMethod::Password);
        assert!(result.payload[0].get("password").is_none());
    }

    #[test]
    fn migrates_legacy_remote_section_to_server_profile() {
        let result = migrate_legacy_single_server_app_config(&serde_json::json!({
            "Remote": {
                "Host": "10.0.0.8",
                "Port": 22022,
                "Username": "ubuntu",
                "AuthMethod": "key",
                "PrivateKeyPath": "C:/Users/me/.ssh/id_ed25519",
                "Password": "do-not-persist"
            }
        }))
        .unwrap()
        .unwrap();

        assert_eq!(result.source_version, "legacy-app-remote");
        assert_eq!(result.profiles.len(), 1);
        let profile = &result.profiles[0];
        assert_eq!(profile.host, "10.0.0.8");
        assert_eq!(profile.port, 22022);
        assert_eq!(profile.username, "ubuntu");
        assert_eq!(profile.auth_method, AuthMethod::Key);
        assert_eq!(
            profile.private_key_path.as_deref(),
            Some("C:/Users/me/.ssh/id_ed25519")
        );
    }

    #[test]
    fn invalid_v1_payload_returns_error() {
        let err =
            migrate_server_profiles_payload(serde_json::json!({"not": "servers"})).unwrap_err();

        assert!(err.to_string().contains("servers"));
    }
}
