//! SnowLuma 持久化会话 + 全局配置渲染
//!
//! grep self-check whitelist: serde_json::Value 仅在 build_webui_json_payload
//! 输出 JSON 拼接处使用,本模块其它位置禁止出现 serde_json::Value
//!
//! 落地能力:
//! - SnowLumaSession 强类型 serde struct(createdAt / lastRenderedAt 驼峰)
//! - load_or_create_session / update_last_rendered 原子读写 session.json
//! - generate_strong_password 强密码生成(含 4 类字符 + 打乱)
//! - build_webui_json_payload scrypt(N=16384, r=8, p=1, dklen=64) + 16 字节 salt
//! - render_runtime_json / write_webui_json 原子写盘到 <runtime_root>/config/
//! - render_daemon_globals 协调 override > session 优先级 + 三文件渲染

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rand::seq::SliceRandom;
use rand::thread_rng;
use serde::{Deserialize, Serialize};

use crate::snowluma::error::SnowLumaDaemonError;
use ncd_domain::SnowLumaAppConfig;

// 公共常量

/// 默认强密码长度(与 SnowLuma webui/auth.ts 默认 16 对齐)
const DEFAULT_PASSWORD_LEN: usize = 16;
/// 强密码下限:≥ 10
const MIN_PASSWORD_LEN: usize = 10;

const UPPERCASE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const LOWERCASE: &[u8] = b"abcdefghijklmnopqrstuvwxyz";
const DIGITS: &[u8] = b"0123456789";
/// 与 SnowLuma webui/auth.ts:38-44 对齐
const SPECIALS: &[u8] = b"!@#$%^&*-_=+[]{};:,.<>/?";

// scrypt 参数与 legacy snowluma_config_renderer.py 完全对齐:
// log2(N) = 14 → N = 16384, r = 8, p = 1, dklen = 64, salt = 16 字节
const SCRYPT_LOG_N: u8 = 14;
const SCRYPT_R: u32 = 8;
const SCRYPT_P: u32 = 1;
const SCRYPT_DKLEN: usize = 64;
const SCRYPT_SALT_BYTES: usize = 16;

// SnowLumaSession:session.json 强类型 wrapper

/// SnowLuma 会话密码持久化内部使用,永远不跨 Tauri 边界,因此故意不派生 ts-rs
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnowLumaSession {
    pub password: String,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "lastRenderedAt")]
    pub last_rendered_at: String,
}

/// 返回 <snowluma_data_root>/app-config.json
pub fn app_config_path(snowluma_data_root: &Path) -> PathBuf {
    snowluma_data_root.join("app-config.json")
}

/// 读取 Desktop 写入的全局 SnowLuma WebUI 配置;缺失或解析失败时返回默认值
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

/// 返回 <snowluma_data_root>/session.json
pub fn session_path(snowluma_data_root: &Path) -> PathBuf {
    snowluma_data_root.join("session.json")
}

/// 读取或首启生成 session
/// - 文件存在:反序列化返回
/// - 文件不存在:generate_strong_password(16) + now_iso8601() 写入;返回 session
///   错误映射:IO → Io(...),密码生成 / JSON → Password(...)
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

/// 把 last_rendered_at 刷成当下时间并原子写回
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

// 强密码生成

/// 生成强随机密码
/// 规则:
/// - 实际长度 = len.max(10)
/// - 至少各含 1 个大写,小写,数字,特殊符号;其余位从四类合集随机抽取
/// - 整体 SliceRandom::shuffle 打乱,避免 4 个固定首位字符泄漏类别
/// - 不含空格(合集里没有  )
#[allow(clippy::expect_used)]
pub fn generate_strong_password(len: usize) -> String {
    let target_len = len.max(MIN_PASSWORD_LEN);
    let mut rng = thread_rng();

    let mut chars: Vec<u8> = Vec::with_capacity(target_len);
    chars.push(*UPPERCASE.choose(&mut rng).expect("uppercase non-empty"));
    chars.push(*LOWERCASE.choose(&mut rng).expect("lowercase non-empty"));
    chars.push(*DIGITS.choose(&mut rng).expect("digits non-empty"));
    chars.push(*SPECIALS.choose(&mut rng).expect("specials non-empty"));

    // 合集(顺序无关,shuffle 之后再写入)
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

    // 字符全在 ASCII 字母 / 数字 / 已知特殊符号集合内,转换不会失败
    String::from_utf8(chars).expect("password bytes are ascii")
}

// webui.json payload + scrypt

/// 构造 webui.json 的 5 字段 payload
/// 本函数是模块内唯一允许引用 serde_json::Value 的位置(见文件头白名单)
/// 错误:
/// - password 空字符串 → Password("password 不能为空字符串")
/// - scrypt Params / scrypt 计算失败 → Password("scrypt error: ...")
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

// runtime.json / webui.json 落盘

/// 与上游 SnowLuma `findAvailablePort` 对齐：从 preferred 起最多试 50 个端口。
const WEBUI_PORT_MAX_TRIES: u16 = 50;

/// 探测 TCP 端口是否可被 `0.0.0.0` 绑定（与 SL WebUI 默认 host 一致）。
fn is_tcp_port_available(port: u16) -> bool {
    std::net::TcpListener::bind(("0.0.0.0", port)).is_ok()
}

/// 从 preferred 起找一个本机可绑定的 WebUI 端口。
///
/// 背景：上游 SL 在 `desiredPort` 被占时会 `findAvailablePort` 自动 +1，
/// 但不会回写 Desktop 侧 client 使用的端口；若 Desktop 仍连 preferred，
/// 就会出现 wait_ready 30s 全失败。启动前先选空闲口并写入 runtime.json，
/// 让 node 与 Desktop 使用同一端口。
pub fn find_available_webui_port(preferred: u16) -> Result<u16, SnowLumaDaemonError> {
    let start = preferred.max(1);
    for offset in 0..WEBUI_PORT_MAX_TRIES {
        let Some(port) = start.checked_add(offset) else {
            break;
        };
        if port == 0 {
            continue;
        }
        if is_tcp_port_available(port) {
            return Ok(port);
        }
    }
    Err(SnowLumaDaemonError::Io(format!(
        "no available TCP port found near {start} (tried {WEBUI_PORT_MAX_TRIES})"
    )))
}

/// 从 daemon stdout/stderr 解析 SL 实际绑定的 WebUI 端口。
///
/// 上游日志形态：
/// - `port 5099 is in use, using 5100 instead`
/// - `listening http://0.0.0.0:5100` / `listening https://127.0.0.1:5100`
///
/// 优先取 "using N instead"（明确换口），否则取最近一条 listening 行的端口。
pub fn parse_bound_webui_port_from_logs<I, S>(lines: I) -> Option<u16>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut listening_port: Option<u16> = None;
    for line in lines {
        let line = line.as_ref();
        if let Some(port) = parse_port_in_use_using_instead(line) {
            return Some(port);
        }
        if let Some(port) = parse_listening_url_port(line) {
            listening_port = Some(port);
        }
    }
    listening_port
}

fn parse_port_in_use_using_instead(line: &str) -> Option<u16> {
    // "... port 5099 is in use, using 5100 instead"
    const MARKER: &str = " is in use, using ";
    let idx = line.find(MARKER)?;
    let after = &line[idx + MARKER.len()..];
    let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    let port: u16 = digits.parse().ok()?;
    (port > 0).then_some(port)
}

fn parse_listening_url_port(line: &str) -> Option<u16> {
    // "... listening http://0.0.0.0:5100" / "listening https://[::]:5100"
    let lower = line.to_ascii_lowercase();
    let listen_idx = lower.find("listening ")?;
    let rest = &line[listen_idx + "listening ".len()..];
    // 取最后一个 ':' 后的端口数字（IPv6 URL 也是 scheme://host:port）
    let colon = rest.rfind(':')?;
    let tail = &rest[colon + 1..];
    let digits: String = tail.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    let port: u16 = digits.parse().ok()?;
    (port > 0).then_some(port)
}

/// 写 <runtime_root>/config/runtime.json,内容仅 { "webuiPort": port }
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

/// 写 <runtime_root>/config/webui.json
pub fn write_webui_json(
    runtime_root: &Path,
    payload: &serde_json::Map<String, serde_json::Value>,
) -> Result<(), SnowLumaDaemonError> {
    let path = runtime_root.join("config").join("webui.json");
    let text = serde_json::to_string_pretty(&serde_json::Value::Object(payload.clone()))
        .map_err(|e| SnowLumaDaemonError::Io(format!("serialize webui.json failed: {e}")))?;
    atomic_write(&path, text.as_bytes())
}

/// 协调 daemon 启动前的全局配置渲染
/// 1. 解析有效密码:override_pwd.trim() 非空 → 用 override;否则
///    load_or_create_session.password
/// 2. render_runtime_json(port)
/// 3. build_webui_json_payload(effective, must_change=false) + write_webui_json
/// 4. 仅当未使用 override 时调用 update_last_rendered(override 模式只是临时
///    覆盖,不污染 session 的"上次渲染时间"语义)
///    返回本次启动生效的明文密码,由调用方喂给 SnowLumaWebUiClient
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

// 内部工具

/// 原子写:写到 <path>.tmp 再 rename父目录不存在自动创建
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

/// 当前时间 ISO 8601 UTC 毫秒精度(YYYY-MM-DDTHH:MM:SS.mmmZ)
/// 不依赖 chrono / time:从 SystemTime 拿 epoch millis,再用 Howard Hinnant
/// civil-from-days 算法算出年月日(参考 https://howardhinnant.github.io/date_algorithms.html)
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

/// Howard Hinnant civil-from-days(输入:自 1970-01-01 起天数;输出:(year, month, day))
/// 与 chrono 的 NaiveDateTime::from_timestamp 路径在 1900..=9999 范围内逐字节一致
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

// Tests(smoke level;完整覆盖由 接手)

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
