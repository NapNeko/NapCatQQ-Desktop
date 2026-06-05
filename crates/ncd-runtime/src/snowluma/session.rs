//! SnowLuma 持久化会话 + 全局配置渲染。
//!
//! grep self-check whitelist: `serde_json::Value` 仅在 `build_webui_json_payload`
//! 输出 JSON 拼接处使用，本模块其它位置禁止出现 `serde_json::Value`。
//!
//! 落地能力：
//! - `SnowLumaSession` 强类型 serde struct（`createdAt` / `lastRenderedAt` 驼峰）。
//! - `load_or_create_session` / `update_last_rendered` 原子读写 `session.json`。
//! - `generate_strong_password` 强密码生成（含 4 类字符 + 打乱）。
//! - `build_webui_json_payload` scrypt(N=16384, r=8, p=1, dklen=64) + 16 字节 salt。
//! - `render_runtime_json` / `write_webui_json` 原子写盘到 `<runtime_root>/config/`。
//! - `render_daemon_globals` 协调 override > session 优先级 + 三文件渲染。

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rand::seq::SliceRandom;
use rand::thread_rng;
use serde::{Deserialize, Serialize};

use crate::snowluma::error::SnowLumaDaemonError;
use ncd_domain::SnowLumaAppConfig;

// ============================================================================
// 公共常量
// ============================================================================

/// 默认强密码长度（与 SnowLuma `webui/auth.ts` 默认 16 对齐）。
const DEFAULT_PASSWORD_LEN: usize = 16;
/// 强密码下限：≥ 10。
const MIN_PASSWORD_LEN: usize = 10;

const UPPERCASE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const LOWERCASE: &[u8] = b"abcdefghijklmnopqrstuvwxyz";
const DIGITS: &[u8] = b"0123456789";
/// 与 SnowLuma `webui/auth.ts:38-44` 对齐。
const SPECIALS: &[u8] = b"!@#$%^&*-_=+[]{};:,.<>/?";

// scrypt 参数与 legacy `snowluma_config_renderer.py` 完全对齐：
// log2(N) = 14 → N = 16384, r = 8, p = 1, dklen = 64, salt = 16 字节。
const SCRYPT_LOG_N: u8 = 14;
const SCRYPT_R: u32 = 8;
const SCRYPT_P: u32 = 1;
const SCRYPT_DKLEN: usize = 64;
const SCRYPT_SALT_BYTES: usize = 16;

// ============================================================================
// SnowLumaSession：`<data_root>/snowluma/session.json` 强类型 wrapper
// ============================================================================

/// SnowLuma 会话密码持久化。内部使用，永远不跨 Tauri 边界，因此故意不派生 ts-rs。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnowLumaSession {
    pub password: String,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "lastRenderedAt")]
    pub last_rendered_at: String,
}

/// 返回 `<snowluma_data_root>/app-config.json`。
pub fn app_config_path(snowluma_data_root: &Path) -> PathBuf {
    snowluma_data_root.join("app-config.json")
}

/// 读取 Desktop 写入的全局 SnowLuma WebUI 配置；缺失或解析失败时返回默认值。
pub fn load_snowluma_app_config(snowluma_data_root: &Path) -> SnowLumaAppConfig {
    let path = app_config_path(snowluma_data_root);
    if !path.exists() {
        return SnowLumaAppConfig::default();
    }
    let text = match fs::read_to_string(&path) {
        Ok(t) => t,
        Err(_) => return SnowLumaAppConfig::default(),
    };
    serde_json::from_str(&text).unwrap_or_default()
}

/// 返回 `<snowluma_data_root>/session.json`。
pub fn session_path(snowluma_data_root: &Path) -> PathBuf {
    snowluma_data_root.join("session.json")
}

/// 读取或首启生成 session。
/// - 文件存在：反序列化返回。
/// - 文件不存在：`generate_strong_password(16)` + `now_iso8601()` 写入；返回 session。
/// 错误映射：IO → `Io(...)`，密码生成 / JSON → `Password(...)`。
pub fn load_or_create_session(
    snowluma_data_root: &Path,
) -> Result<SnowLumaSession, SnowLumaDaemonError> {
    let path = session_path(snowluma_data_root);
    if path.exists() {
        let text = fs::read_to_string(&path).map_err(|e| {
            SnowLumaDaemonError::Io(format!(
                "read session.json failed: {e} (path={})",
                path.display()
            ))
        })?;
        let session: SnowLumaSession = serde_json::from_str(&text).map_err(|e| {
            SnowLumaDaemonError::Password(format!(
                "deserialize session.json failed: {e} (path={})",
                path.display()
            ))
        })?;
        return Ok(session);
    }

    let password = generate_strong_password(DEFAULT_PASSWORD_LEN);
    let now = now_iso8601();
    let session = SnowLumaSession {
        password,
        created_at: now.clone(),
        last_rendered_at: now,
    };
    write_session(&path, &session)?;
    Ok(session)
}

/// 把 `last_rendered_at` 刷成当下时间并原子写回。
pub fn update_last_rendered(snowluma_data_root: &Path) -> Result<(), SnowLumaDaemonError> {
    let mut session = load_or_create_session(snowluma_data_root)?;
    session.last_rendered_at = now_iso8601();
    let path = session_path(snowluma_data_root);
    write_session(&path, &session)
}

fn write_session(path: &Path, session: &SnowLumaSession) -> Result<(), SnowLumaDaemonError> {
    let text = serde_json::to_string_pretty(session).map_err(|e| {
        SnowLumaDaemonError::Password(format!("serialize session.json failed: {e}"))
    })?;
    atomic_write(path, text.as_bytes())
}

// ============================================================================
// 强密码生成
// ============================================================================

/// 生成强随机密码。
/// 规则：
/// - 实际长度 = `len.max(10)`。
/// - 至少各含 1 个大写、小写、数字、特殊符号；其余位从四类合集随机抽取。
/// - 整体 `SliceRandom::shuffle` 打乱，避免 4 个固定首位字符泄漏类别。
/// - 不含空格（合集里没有 ` `）。
pub fn generate_strong_password(len: usize) -> String {
    let target_len = len.max(MIN_PASSWORD_LEN);
    let mut rng = thread_rng();

    let mut chars: Vec<u8> = Vec::with_capacity(target_len);
    chars.push(*UPPERCASE.choose(&mut rng).expect("uppercase non-empty"));
    chars.push(*LOWERCASE.choose(&mut rng).expect("lowercase non-empty"));
    chars.push(*DIGITS.choose(&mut rng).expect("digits non-empty"));
    chars.push(*SPECIALS.choose(&mut rng).expect("specials non-empty"));

    // 合集（顺序无关，shuffle 之后再写入）
    let mut pool: Vec<u8> =
        Vec::with_capacity(UPPERCASE.len() + LOWERCASE.len() + DIGITS.len() + SPECIALS.len());
    pool.extend_from_slice(UPPERCASE);
    pool.extend_from_slice(LOWERCASE);
    pool.extend_from_slice(DIGITS);
    pool.extend_from_slice(SPECIALS);

    let remaining = target_len.saturating_sub(chars.len());
    for _ in 0..remaining {
        chars.push(*pool.choose(&mut rng).expect("pool non-empty"));
    }

    chars.shuffle(&mut rng);

    // 字符全在 ASCII 字母 / 数字 / 已知特殊符号集合内，转换不会失败。
    String::from_utf8(chars).expect("password bytes are ascii")
}

// ============================================================================
// webui.json payload + scrypt
// ============================================================================

/// 构造 `webui.json` 的 5 字段 payload。
/// 本函数是模块内唯一允许引用 `serde_json::Value` 的位置（见文件头白名单）。
/// 错误：
/// - `password` 空字符串 → `Password("password 不能为空字符串")`。
/// - scrypt Params / scrypt 计算失败 → `Password("scrypt error: ...")`。
pub fn build_webui_json_payload(
    password: &str,
    must_change: bool,
) -> Result<serde_json::Map<String, serde_json::Value>, SnowLumaDaemonError> {
    if password.is_empty() {
        return Err(SnowLumaDaemonError::Password(
            "password 不能为空字符串".into(),
        ));
    }

    let salt: [u8; SCRYPT_SALT_BYTES] = rand::random();

    let params = scrypt::Params::new(SCRYPT_LOG_N, SCRYPT_R, SCRYPT_P, SCRYPT_DKLEN)
        .map_err(|e| SnowLumaDaemonError::Password(format!("scrypt error: {e}")))?;
    let mut hash = vec![0u8; SCRYPT_DKLEN];
    scrypt::scrypt(password.as_bytes(), &salt, &params, &mut hash[..])
        .map_err(|e| SnowLumaDaemonError::Password(format!("scrypt error: {e}")))?;

    let now = now_iso8601();
    let mut payload = serde_json::Map::with_capacity(5);
    payload.insert(
        "passwordHash".into(),
        serde_json::Value::String(hex::encode(&hash)),
    );
    payload.insert(
        "passwordSalt".into(),
        serde_json::Value::String(hex::encode(salt)),
    );
    payload.insert(
        "mustChangePassword".into(),
        serde_json::Value::Bool(must_change),
    );
    payload.insert("generatedAt".into(), serde_json::Value::String(now.clone()));
    payload.insert("updatedAt".into(), serde_json::Value::String(now));
    Ok(payload)
}

// ============================================================================
// runtime.json / webui.json 落盘
// ============================================================================

/// 写 `<runtime_root>/config/runtime.json`，内容仅 `{ "webuiPort": port }`。
pub fn render_runtime_json(runtime_root: &Path, port: u16) -> Result<(), SnowLumaDaemonError> {
    let path = runtime_root.join("config").join("runtime.json");
    let mut payload = serde_json::Map::with_capacity(1);
    payload.insert(
        "webuiPort".into(),
        serde_json::Value::Number(serde_json::Number::from(port)),
    );
    let text = serde_json::to_string_pretty(&serde_json::Value::Object(payload))
        .map_err(|e| SnowLumaDaemonError::Io(format!("serialize runtime.json failed: {e}")))?;
    atomic_write(&path, text.as_bytes())
}

/// 写 `<runtime_root>/config/webui.json`。
pub fn write_webui_json(
    runtime_root: &Path,
    payload: &serde_json::Map<String, serde_json::Value>,
) -> Result<(), SnowLumaDaemonError> {
    let path = runtime_root.join("config").join("webui.json");
    let text = serde_json::to_string_pretty(&serde_json::Value::Object(payload.clone()))
        .map_err(|e| SnowLumaDaemonError::Io(format!("serialize webui.json failed: {e}")))?;
    atomic_write(&path, text.as_bytes())
}

/// 协调 daemon 启动前的全局配置渲染。
/// 1. 解析有效密码：`override_pwd.trim()` 非空 → 用 override；否则
/// `load_or_create_session.password`。
/// 2. `render_runtime_json(port)`。
/// 3. `build_webui_json_payload(effective, must_change=false)` + `write_webui_json`。
/// 4. 仅当未使用 override 时调用 `update_last_rendered`（override 模式只是临时
/// 覆盖，不污染 session 的"上次渲染时间"语义）。
/// 返回本次启动生效的明文密码，由调用方喂给 `SnowLumaWebUiClient`。
pub fn render_daemon_globals(
    snowluma_data_root: &Path,
    runtime_root: &Path,
    override_pwd: Option<&str>,
    port: u16,
) -> Result<String, SnowLumaDaemonError> {
    let effective_pwd = match override_pwd.map(|s| s.trim()).filter(|s| !s.is_empty()) {
        Some(value) => value.to_string(),
        None => load_or_create_session(snowluma_data_root)?.password,
    };
    let used_override = override_pwd.map(|s| !s.trim().is_empty()).unwrap_or(false);

    render_runtime_json(runtime_root, port)?;
    let payload = build_webui_json_payload(&effective_pwd, false)?;
    write_webui_json(runtime_root, &payload)?;

    if !used_override {
        update_last_rendered(snowluma_data_root)?;
    }

    Ok(effective_pwd)
}

// ============================================================================
// 内部工具
// ============================================================================

/// 原子写：写到 `<path>.tmp` 再 `rename`。父目录不存在自动创建。
fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), SnowLumaDaemonError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| {
            SnowLumaDaemonError::Io(format!(
                "create parent dirs failed: {e} (path={})",
                parent.display()
            ))
        })?;
    }
    let tmp = path.with_extension({
        let cur = path
            .extension()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        if cur.is_empty() {
            "tmp".to_string()
        } else {
            format!("{cur}.tmp")
        }
    });
    {
        let mut file = fs::File::create(&tmp).map_err(|e| {
            SnowLumaDaemonError::Io(format!(
                "create tmp file failed: {e} (path={})",
                tmp.display()
            ))
        })?;
        file.write_all(bytes).map_err(|e| {
            SnowLumaDaemonError::Io(format!(
                "write tmp file failed: {e} (path={})",
                tmp.display()
            ))
        })?;
        file.sync_all().map_err(|e| {
            SnowLumaDaemonError::Io(format!(
                "sync tmp file failed: {e} (path={})",
                tmp.display()
            ))
        })?;
    }
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        SnowLumaDaemonError::Io(format!(
            "rename tmp → target failed: {e} (path={})",
            path.display()
        ))
    })
}

/// 当前时间 ISO 8601 UTC 毫秒精度（`YYYY-MM-DDTHH:MM:SS.mmmZ`）。
/// 不依赖 `chrono` / `time`：从 `SystemTime` 拿 epoch millis，再用 Howard Hinnant
/// civil-from-days 算法算出年月日（参考 https://howardhinnant.github.io/date_algorithms.html）。
fn now_iso8601() -> String {
    let dur = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let total_millis = dur.as_millis() as i128;
    let total_secs = total_millis.div_euclid(1000) as i64;
    let millis = total_millis.rem_euclid(1000) as u32;

    let days = total_secs.div_euclid(86_400);
    let secs_of_day = total_secs.rem_euclid(86_400);
    let hour = (secs_of_day / 3600) as u32;
    let minute = ((secs_of_day / 60) % 60) as u32;
    let second = (secs_of_day % 60) as u32;

    let (year, month, day) = civil_from_days(days);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        year, month, day, hour, minute, second, millis
    )
}

/// Howard Hinnant civil-from-days（输入：自 1970-01-01 起天数；输出：(year, month, day)）。
/// 与 chrono 的 `NaiveDateTime::from_timestamp` 路径在 1900..=9999 范围内逐字节一致。
fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe.wrapping_sub(doe / 1460) + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m as u32, d as u32)
}

// ============================================================================
// Tests（smoke level；完整覆盖由 接手）
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_path_appends_session_json() {
        let root = PathBuf::from("/tmp/snowluma");
        let path = session_path(&root);
        assert_eq!(path, PathBuf::from("/tmp/snowluma").join("session.json"));
    }

    #[test]
    fn generate_strong_password_min_length() {
        // len 小于 10 时被钳到 10。
        let pwd = generate_strong_password(4);
        assert!(pwd.len() >= MIN_PASSWORD_LEN);

        // 大于下限时按指定长度返回。
        let pwd = generate_strong_password(DEFAULT_PASSWORD_LEN);
        assert_eq!(pwd.len(), DEFAULT_PASSWORD_LEN);

        // 4 类字符各至少 1 个；不含空格。
        assert!(pwd.bytes().any(|b| UPPERCASE.contains(&b)));
        assert!(pwd.bytes().any(|b| LOWERCASE.contains(&b)));
        assert!(pwd.bytes().any(|b| DIGITS.contains(&b)));
        assert!(pwd.bytes().any(|b| SPECIALS.contains(&b)));
        assert!(!pwd.contains(' '));
    }

    #[test]
    fn build_webui_json_payload_has_5_fields() {
        let payload = build_webui_json_payload("hello-world-1!", false).expect("build payload");
        assert_eq!(payload.len(), 5);
        for key in [
            "passwordHash",
            "passwordSalt",
            "mustChangePassword",
            "generatedAt",
            "updatedAt",
        ] {
            assert!(payload.contains_key(key), "missing key: {key}");
        }

        // hash / salt 必须是 hex（对应字节数 = 64 / 16 → hex 长度 = 128 / 32）。
        let hash = payload
            .get("passwordHash")
            .and_then(|v| v.as_str())
            .expect("hash str");
        assert_eq!(hash.len(), SCRYPT_DKLEN * 2);
        assert!(hash.bytes().all(|b| b.is_ascii_hexdigit()));

        let salt = payload
            .get("passwordSalt")
            .and_then(|v| v.as_str())
            .expect("salt str");
        assert_eq!(salt.len(), SCRYPT_SALT_BYTES * 2);
        assert!(salt.bytes().all(|b| b.is_ascii_hexdigit()));

        assert_eq!(
            payload.get("mustChangePassword"),
            Some(&serde_json::Value::Bool(false))
        );
    }

    // ------------------------------------------------------------------------
    // 追加测试：强密码 100 次抽样 / 字节级 round-trip / scrypt 行为 /
    // ISO 8601 格式锁 / 文件级幂等 + 时间戳推进 /
    // render_daemon_globals 优先级
    // ------------------------------------------------------------------------

    /// 100 次随机生成，每次都满足"长度 ≥ 10、含 4 类字符、不含空格"。
    #[test]
    fn generate_strong_password_includes_all_classes_100x() {
        for i in 0..100 {
            let pwd = generate_strong_password(MIN_PASSWORD_LEN);
            assert!(
                pwd.len() >= MIN_PASSWORD_LEN,
                "iter {i}: length {} < min {}",
                pwd.len(),
                MIN_PASSWORD_LEN
            );
            assert!(
                pwd.bytes().any(|b| UPPERCASE.contains(&b)),
                "iter {i}: missing uppercase in {pwd:?}"
            );
            assert!(
                pwd.bytes().any(|b| LOWERCASE.contains(&b)),
                "iter {i}: missing lowercase in {pwd:?}"
            );
            assert!(
                pwd.bytes().any(|b| DIGITS.contains(&b)),
                "iter {i}: missing digit in {pwd:?}"
            );
            assert!(
                pwd.bytes().any(|b| SPECIALS.contains(&b)),
                "iter {i}: missing special in {pwd:?}"
            );
            assert!(!pwd.contains(' '), "iter {i}: contains space in {pwd:?}");
        }
    }

    /// `len < MIN_PASSWORD_LEN` → 钳到 10；`len >= MIN_PASSWORD_LEN` → 原样返回。
    #[test]
    fn generate_strong_password_clamps_to_min_length() {
        let short = generate_strong_password(4);
        assert_eq!(short.len(), MIN_PASSWORD_LEN);

        let exact = generate_strong_password(20);
        assert_eq!(exact.len(), 20);
    }

    /// `generate_strong_password` 输出必须只落在四类字符合集里（含特殊符号集合精确锁定）。
    #[test]
    fn generate_strong_password_emits_only_whitelisted_chars() {
        let pwd = generate_strong_password(64);
        for (idx, b) in pwd.bytes().enumerate() {
            let allowed = UPPERCASE.contains(&b)
                || LOWERCASE.contains(&b)
                || DIGITS.contains(&b)
                || SPECIALS.contains(&b);
            assert!(
                allowed,
                "byte {idx} = 0x{b:02x} ({:?}) not in whitelist (pwd={pwd:?})",
                b as char
            );
        }
    }

    /// `SnowLumaSession` 字节级 round-trip：camelCase 字段 + 二次序列化字节相等。
    #[test]
    fn snowluma_session_round_trips_camel_case_fields() {
        let session = SnowLumaSession {
            password: "P@ssw0rd!".to_string(),
            created_at: "2024-01-01T00:00:00.000Z".to_string(),
            last_rendered_at: "2024-12-31T23:59:59.999Z".to_string(),
        };

        // 第一次序列化必须含 camelCase 字段名 + 明文 password。
        let json1 = serde_json::to_string(&session).expect("serialize");
        assert!(
            json1.contains("\"createdAt\""),
            "missing createdAt in {json1}"
        );
        assert!(
            json1.contains("\"lastRenderedAt\""),
            "missing lastRenderedAt in {json1}"
        );
        assert!(
            json1.contains("\"password\""),
            "missing password in {json1}"
        );
        // 反向锁：snake_case 字段名不得出现。
        assert!(
            !json1.contains("\"created_at\""),
            "snake_case leaked in {json1}"
        );
        assert!(
            !json1.contains("\"last_rendered_at\""),
            "snake_case leaked in {json1}"
        );

        // 反序列化 → 等价。
        let parsed: SnowLumaSession = serde_json::from_str(&json1).expect("deserialize");
        assert_eq!(parsed, session);

        // 再序列化字节相等。
        let json2 = serde_json::to_string(&parsed).expect("re-serialize");
        assert_eq!(json1, json2, "round-trip not byte-equal");
    }

    /// 首启写入 `session.json`；再次调用直接读，密码与 createdAt 全部稳定。
    #[test]
    fn load_or_create_session_is_idempotent_after_first_call() {
        let temp = ncd_test_support::TempWorkspace::new().expect("tempdir");
        let root = temp.path();

        let first = load_or_create_session(root).expect("first");
        let second = load_or_create_session(root).expect("second");

        assert_eq!(first.password, second.password);
        assert_eq!(first.created_at, second.created_at);
        assert_eq!(first.last_rendered_at, second.last_rendered_at);

        // 文件确实落在 `<root>/session.json`。
        assert!(session_path(root).exists());
    }

    /// 同一明文密码连续两次构造 payload：salt 与 hash 必须双双不同。
    #[test]
    fn build_webui_json_payload_uses_different_salts() {
        let pwd = "p@SsW0rd-1234";
        let a = build_webui_json_payload(pwd, false).expect("payload a");
        let b = build_webui_json_payload(pwd, false).expect("payload b");

        let salt_a = a.get("passwordSalt").and_then(|v| v.as_str()).unwrap();
        let salt_b = b.get("passwordSalt").and_then(|v| v.as_str()).unwrap();
        assert_ne!(salt_a, salt_b, "salts collided: {salt_a} == {salt_b}");

        let hash_a = a.get("passwordHash").and_then(|v| v.as_str()).unwrap();
        let hash_b = b.get("passwordHash").and_then(|v| v.as_str()).unwrap();
        assert_ne!(hash_a, hash_b, "hashes collided: {hash_a} == {hash_b}");
    }

    /// 验证 `now_iso8601()` 输出（透过 `load_or_create_session.created_at`）严格匹配
    /// `^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$`。无 `regex` crate，手写 24 字符校验。
    #[test]
    fn now_iso8601_matches_iso_8601_format() {
        let temp = ncd_test_support::TempWorkspace::new().expect("tempdir");
        let session = load_or_create_session(temp.path()).expect("session");
        assert_iso8601_millis(&session.created_at);
        assert_iso8601_millis(&session.last_rendered_at);
    }

    /// 手写 ISO 8601 毫秒精度 UTC 校验器：
    /// 0..=3 位年、`-`、2 位月、`-`、2 位日、`T`、2 位时、`:`、2 位分、`:`、2 位秒、`.`、3 位毫秒、`Z`。
    fn assert_iso8601_millis(s: &str) {
        assert_eq!(s.len(), 24, "expected 24 chars, got {}: {s:?}", s.len());
        let bytes = s.as_bytes();
        // 数字/分隔符位置查表：
        // 0 1 2 3 - 5 6 - 8 9 T 11 12 : 14 15 : 17 18 . 20 21 22 Z
        let digit_positions = [0, 1, 2, 3, 5, 6, 8, 9, 11, 12, 14, 15, 17, 18, 20, 21, 22];
        for &i in &digit_positions {
            let b = bytes[i];
            assert!(
                b.is_ascii_digit(),
                "byte {i} = 0x{b:02x} not digit (s={s:?})"
            );
        }
        assert_eq!(bytes[4], b'-', "expected '-' at 4 (s={s:?})");
        assert_eq!(bytes[7], b'-', "expected '-' at 7 (s={s:?})");
        assert_eq!(bytes[10], b'T', "expected 'T' at 10 (s={s:?})");
        assert_eq!(bytes[13], b':', "expected ':' at 13 (s={s:?})");
        assert_eq!(bytes[16], b':', "expected ':' at 16 (s={s:?})");
        assert_eq!(bytes[19], b'.', "expected '.' at 19 (s={s:?})");
        assert_eq!(bytes[23], b'Z', "expected 'Z' at 23 (s={s:?})");
    }

    /// `update_last_rendered` 必须把时间戳推进到 ≥ 原值，并且大概率严格 >（毫秒精度时钟）。
    #[test]
    fn update_last_rendered_advances_timestamp() {
        let temp = ncd_test_support::TempWorkspace::new().expect("tempdir");
        let root = temp.path();

        let original = load_or_create_session(root).expect("create session");
        // 至少跨越 1ms tick，留余量给 Windows 时钟分辨率（典型 ~15ms）。
        std::thread::sleep(std::time::Duration::from_millis(50));

        update_last_rendered(root).expect("update last rendered");
        let reloaded = load_or_create_session(root).expect("reload");

        // ISO 8601 毫秒精度 UTC 字符串等宽，ascii 字典序 == 时间序。
        assert!(
            reloaded.last_rendered_at.as_str() >= original.last_rendered_at.as_str(),
            "expected {:?} >= {:?}",
            reloaded.last_rendered_at,
            original.last_rendered_at
        );
        assert_ne!(
            reloaded.last_rendered_at, original.last_rendered_at,
            "timestamp did not advance after 50ms sleep"
        );
        // 密码 / createdAt 不应被覆盖。
        assert_eq!(reloaded.password, original.password);
        assert_eq!(reloaded.created_at, original.created_at);
    }

    /// `render_daemon_globals` §3.5 优先级：override 非空白 → 用 override；否则 fallback 到
    /// session 密码。无论走哪条分支，runtime.json / webui.json 都应原子落盘。
    #[test]
    fn render_daemon_globals_uses_override_when_present() {
        let snow_dir = ncd_test_support::TempWorkspace::new().expect("snow tempdir");
        let runtime_dir = ncd_test_support::TempWorkspace::new().expect("runtime tempdir");
        let snow = snow_dir.path();
        let runtime = runtime_dir.path();

        // 1) override = Some("OVERRIDE!@123") → 返回 override 原值。
        let override_pwd = "OVERRIDE!@123";
        let returned = render_daemon_globals(snow, runtime, Some(override_pwd), 5099)
            .expect("render override");
        assert_eq!(returned, override_pwd);

        // runtime.json 内容锁：仅 `webuiPort` 一个字段且为 5099。
        let runtime_json = std::fs::read_to_string(runtime.join("config").join("runtime.json"))
            .expect("read runtime.json");
        let parsed: serde_json::Value =
            serde_json::from_str(&runtime_json).expect("parse runtime.json");
        assert_eq!(parsed["webuiPort"], serde_json::json!(5099));

        // webui.json 必须存在。
        let webui_path = runtime.join("config").join("webui.json");
        assert!(webui_path.exists(), "webui.json not written");

        // 2) override = None → 走 session 路径，返回 session 密码。
        let session = load_or_create_session(snow).expect("read session for assertion");
        let returned_none = render_daemon_globals(snow, runtime, None, 5099).expect("render none");
        assert_eq!(returned_none, session.password);

        // 3) override = Some("") / Some(" ") → trim 后空，等同 fallback 到 session。
        for empty in ["", " ", "\t\n"] {
            let r = render_daemon_globals(snow, runtime, Some(empty), 5099)
                .unwrap_or_else(|e| panic!("render empty {empty:?}: {e:?}"));
            assert_eq!(
                r, session.password,
                "empty/whitespace override should fallback to session (input={empty:?})"
            );
        }
    }
}
