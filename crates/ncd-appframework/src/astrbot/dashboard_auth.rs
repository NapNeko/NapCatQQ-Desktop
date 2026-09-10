//! AstrBot WebUI 账号落盘只有哈希；格式对齐上游 `auth_password.py`：
//! `pbkdf2_password = pbkdf2_sha256$600000$<salt hex>$<digest hex>`，`password` 仍写 md5 兼容旧核心。
//! 首启若两个哈希都空，AstrBot 会自己随机生成并只打到日志——所以桌面端要在首启前写好。

use rand::Rng;
use rand::seq::SliceRandom;
use serde_json::{Map, Value};
use sha2::Sha256;

use ncd_traits::AppFrameworkError;

pub const ASTRBOT_DEFAULT_USERNAME: &str = "astrbot";
const PBKDF2_ALGORITHM: &str = "pbkdf2_sha256";
const PBKDF2_ITERATIONS: u32 = 600_000;
const SALT_BYTES: usize = 16;
const PASSWORD_MIN_LEN: usize = 8;
const GENERATED_LEN: usize = 24;

const UPPER: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const LOWER: &[u8] = b"abcdefghijklmnopqrstuvwxyz";
const DIGIT: &[u8] = b"0123456789";

/// 上游 `generate_dashboard_password`：24 位字母数字，保证三类字符各至少一个。
pub fn generate_password() -> String {
    let mut rng = rand::thread_rng();
    let alphabet: Vec<u8> = [UPPER, LOWER, DIGIT].concat();
    let mut chars: Vec<u8> = vec![
        *UPPER.choose(&mut rng).expect("non-empty"),
        *LOWER.choose(&mut rng).expect("non-empty"),
        *DIGIT.choose(&mut rng).expect("non-empty"),
    ];
    chars.extend((0..GENERATED_LEN - 3).map(|_| *alphabet.choose(&mut rng).expect("non-empty")));
    chars.shuffle(&mut rng);
    String::from_utf8(chars).expect("ascii")
}

/// 上游 `validate_dashboard_password`；不满足时 AstrBot CLI / WebUI 都会拒绝。
pub fn validate_password(raw: &str) -> Result<(), String> {
    if raw.is_empty() {
        return Err("密码不能为空".into());
    }
    if raw.chars().count() < PASSWORD_MIN_LEN {
        return Err(format!("密码至少 {PASSWORD_MIN_LEN} 位"));
    }
    if !raw.chars().any(|c| c.is_ascii_uppercase()) {
        return Err("密码需含大写字母".into());
    }
    if !raw.chars().any(|c| c.is_ascii_lowercase()) {
        return Err("密码需含小写字母".into());
    }
    if !raw.chars().any(|c| c.is_ascii_digit()) {
        return Err("密码需含数字".into());
    }
    Ok(())
}

pub fn md5_hex(raw: &str) -> String {
    format!("{:x}", md5::compute(raw.as_bytes()))
}

fn pbkdf2_digest_hex(raw: &str, salt: &[u8], iterations: u32) -> String {
    let mut out = [0u8; 32];
    pbkdf2::pbkdf2_hmac::<Sha256>(raw.as_bytes(), salt, iterations, &mut out);
    hex(&out)
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}

pub fn hash_password(raw: &str) -> String {
    let mut salt = [0u8; SALT_BYTES];
    rand::thread_rng().fill(&mut salt);
    format!(
        "{PBKDF2_ALGORITHM}${PBKDF2_ITERATIONS}${}${}",
        hex(&salt),
        pbkdf2_digest_hex(raw, &salt, PBKDF2_ITERATIONS)
    )
}

/// 上游 `verify_dashboard_password`：md5（32 位 hex）或 pbkdf2 两种存法都认。
pub fn verify_password(stored: &str, candidate: &str) -> bool {
    let stored = stored.trim();
    if stored.len() == 32 && stored.chars().all(|c| c.is_ascii_hexdigit()) {
        return stored.eq_ignore_ascii_case(&md5_hex(candidate));
    }
    let mut parts = stored.split('$');
    if parts.next() != Some(PBKDF2_ALGORITHM) {
        return false;
    }
    let (Some(iter), Some(salt), Some(digest), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    let Ok(iterations) = iter.parse::<u32>() else {
        return false;
    };
    let Some(salt) = unhex(salt) else {
        return false;
    };
    pbkdf2_digest_hex(candidate, &salt, iterations).eq_ignore_ascii_case(digest)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DashboardAccount {
    pub username: String,
    /// `password`（md5）优先，没有再看 `pbkdf2_password`；两者都空 = 首启才生成
    pub stored_hash: Option<String>,
}

fn dashboard(root: &Value) -> Option<&Map<String, Value>> {
    root.get("dashboard").and_then(Value::as_object)
}

fn non_empty_str(v: Option<&Value>) -> Option<String> {
    v.and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
}

pub fn read_dashboard_account(root: &Value) -> DashboardAccount {
    let dash = dashboard(root);
    let username = dash
        .and_then(|d| non_empty_str(d.get("username")))
        .unwrap_or_else(|| ASTRBOT_DEFAULT_USERNAME.to_string());
    let stored_hash = dash.and_then(|d| {
        non_empty_str(d.get("password")).or_else(|| non_empty_str(d.get("pbkdf2_password")))
    });
    DashboardAccount {
        username,
        stored_hash,
    }
}

/// 记住的密码是否仍是落盘那一个；落盘没哈希返回 None。
pub fn password_matches(root: &Value, remembered: &str) -> Option<bool> {
    read_dashboard_account(root)
        .stored_hash
        .map(|h| verify_password(&h, remembered))
}

/// 对齐上游 CLI `conf set dashboard.password`：写两种哈希并清掉「首登强制改密」标记，
/// 否则 WebUI 会在第一次登录时逼用户改掉桌面端刚设的密码。
pub fn set_dashboard_account(
    root: &mut Value,
    username: Option<&str>,
    raw_password: &str,
) -> Result<(), AppFrameworkError> {
    validate_password(raw_password).map_err(AppFrameworkError::Validation)?;
    let obj = root
        .as_object_mut()
        .ok_or_else(|| AppFrameworkError::Integration("cmd_config.json 根必须是对象".into()))?;
    let dash = obj
        .entry("dashboard".to_string())
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or_else(|| AppFrameworkError::Integration("dashboard 必须是对象".into()))?;
    if let Some(name) = username.map(str::trim).filter(|s| !s.is_empty()) {
        dash.insert("username".into(), Value::String(name.to_string()));
    } else if !dash.contains_key("username") {
        dash.insert(
            "username".into(),
            Value::String(ASTRBOT_DEFAULT_USERNAME.to_string()),
        );
    }
    dash.insert(
        "pbkdf2_password".into(),
        Value::String(hash_password(raw_password)),
    );
    dash.insert("password".into(), Value::String(md5_hex(raw_password)));
    dash.insert("password_storage_upgraded".into(), Value::Bool(true));
    dash.insert("password_change_required".into(), Value::Bool(false));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_password_satisfies_upstream_policy() {
        for _ in 0..16 {
            let p = generate_password();
            assert_eq!(p.len(), GENERATED_LEN);
            validate_password(&p).unwrap();
        }
    }

    #[test]
    fn validate_mirrors_upstream_rules() {
        assert!(validate_password("").is_err());
        assert!(validate_password("Ab1").is_err());
        assert!(validate_password("abcdefgh1").is_err());
        assert!(validate_password("ABCDEFGH1").is_err());
        assert!(validate_password("Abcdefghi").is_err());
        assert!(validate_password("Abcdefg1").is_ok());
    }

    #[test]
    fn md5_matches_legacy_default() {
        // 老版本 cmd_config.json 里 dashboard.password 的默认值就是 md5("astrbot")
        assert_eq!(md5_hex("astrbot"), "77b90590a8945a7d36c963981a307dc9");
    }

    #[test]
    fn pbkdf2_primitive_matches_known_vector() {
        // RFC 7914 附带的 PBKDF2-HMAC-SHA256 向量（password/salt, 1 轮, 32 字节）
        assert_eq!(
            pbkdf2_digest_hex("password", b"salt", 1),
            "120fb6cffcf8b32c43e7225256c4f837a86548c92ccc35480805987cb70be17b"
        );
    }

    #[test]
    fn hash_round_trips_and_has_upstream_shape() {
        let h = hash_password("Abcdefg1");
        let parts: Vec<&str> = h.split('$').collect();
        assert_eq!(parts.len(), 4);
        assert_eq!(parts[0], "pbkdf2_sha256");
        assert_eq!(parts[1], "600000");
        assert_eq!(parts[2].len(), SALT_BYTES * 2);
        assert_eq!(parts[3].len(), 64);
        assert!(verify_password(&h, "Abcdefg1"));
        assert!(!verify_password(&h, "Abcdefg2"));
        assert!(verify_password("77b90590a8945a7d36c963981a307dc9", "astrbot"));
        assert!(!verify_password("garbage", "astrbot"));
    }

    #[test]
    fn set_account_writes_both_hashes_and_clears_change_flag() {
        let mut root: Value = serde_json::from_str(
            r#"{"dashboard":{"enable":true,"username":"astrbot","password":"","pbkdf2_password":"","password_change_required":true,"port":6185},"platform":[]}"#,
        )
        .unwrap();
        set_dashboard_account(&mut root, Some("admin"), "Abcdefg1").unwrap();
        let d = root["dashboard"].as_object().unwrap();
        assert_eq!(d["username"], "admin");
        assert_eq!(d["password"], md5_hex("Abcdefg1"));
        assert!(d["pbkdf2_password"].as_str().unwrap().starts_with("pbkdf2_sha256$"));
        assert_eq!(d["password_storage_upgraded"], true);
        assert_eq!(d["password_change_required"], false);
        assert_eq!(d["port"], 6185);
        assert_eq!(password_matches(&root, "Abcdefg1"), Some(true));
        assert_eq!(password_matches(&root, "nope"), Some(false));
    }

    #[test]
    fn set_account_keeps_username_when_none() {
        let mut root: Value =
            serde_json::from_str(r#"{"dashboard":{"username":"ops"}}"#).unwrap();
        set_dashboard_account(&mut root, None, "Abcdefg1").unwrap();
        assert_eq!(root["dashboard"]["username"], "ops");
        let mut empty = Value::Object(Map::new());
        set_dashboard_account(&mut empty, None, "Abcdefg1").unwrap();
        assert_eq!(empty["dashboard"]["username"], ASTRBOT_DEFAULT_USERNAME);
    }

    #[test]
    fn set_account_rejects_weak_password() {
        let mut root = Value::Object(Map::new());
        assert!(set_dashboard_account(&mut root, None, "short").is_err());
    }

    #[test]
    fn read_account_defaults_and_unset_hash() {
        let root: Value = serde_json::from_str(r#"{"dashboard":{"password":"","pbkdf2_password":""}}"#).unwrap();
        let acc = read_dashboard_account(&root);
        assert_eq!(acc.username, "astrbot");
        assert_eq!(acc.stored_hash, None);
        assert_eq!(password_matches(&root, "x"), None);
    }
}
