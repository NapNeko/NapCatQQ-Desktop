//! 中转代理客户端：HMAC-SHA256 签名 + 服务器时间漂移自愈。
//!
//! 迁移自 legacy Python `src/core/network/proxy_signer.py`。语义对齐：
//! - `sign_headers(path)` 生成 `{X-Timestamp, X-Signature, User-Agent}`
//! - 中转返回 403 时读响应头 `X-Server-Time` 校正本地时钟 offset，重试一次
//! - offset 持久化到 `<config_dir>/.proxy_clock_offset`（纯文本一行秒数）
//! - 单例（`OnceLock`），首次构造从磁盘加载 offset
//!
//! 构建期注入（见 `build.rs` + `proxy_constants`）：仓库 clone 拿不到真实
//! secret，`is_configured()` 返回 false，release 拉取直接走 GitHub 直连，
//! 不发无意义的中转请求。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use hmac::{Hmac, Mac};
use sha2::Sha256;

use crate::proxy_constants::{PROXY_BASE_URL, PROXY_SHARED_SECRET};

type HmacSha256 = Hmac<Sha256>;

const PLACEHOLDER_MARKER: &str = "PLACEHOLDER";
const OFFSET_FILENAME: &str = ".proxy_clock_offset";
/// 抖动小于 2 秒不写盘，避免频繁 IO（对齐 Python 实现）。
const OFFSET_PERSIST_THRESHOLD_SECS: i64 = 2;

/// 中转代理中某个仓库的别名（出现在 URL 路径 `/v1/release/{alias}`）。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReleaseAlias {
    Napcat,
    Snowluma,
    Ncd,
}

impl ReleaseAlias {
    pub fn as_str(self) -> &'static str {
        match self {
            ReleaseAlias::Napcat => "napcat",
            ReleaseAlias::Snowluma => "snowluma",
            ReleaseAlias::Ncd => "ncd",
        }
    }

    /// `/v1/release/{alias}` 形式，供签名。
    pub fn path(self) -> String {
        format!("/v1/release/{}", self.as_str())
    }
}

/// 中转代理是否已注入真实常量。
///
/// base url 为空或 secret 含 `PLACEHOLDER` 标记时视为未配置；release 拉取
/// 应直接走 GitHub 直连，不发无意义的中转请求。
pub fn is_proxy_configured() -> bool {
    !PROXY_BASE_URL.is_empty() && !PROXY_SHARED_SECRET.contains(PLACEHOLDER_MARKER)
}

/// 拼出某个仓库的中转 URL：`{base}/v1/release/{alias}`。
///
/// 未配置时返回 None。
pub fn proxy_release_url(alias: ReleaseAlias) -> Option<String> {
    if !is_proxy_configured() {
        return None;
    }
    let base = PROXY_BASE_URL.trim_end_matches('/');
    Some(format!("{base}/v1/release/{}", alias.as_str()))
}

/// 单例 ProxySigner 句柄。首次拿取时从磁盘加载 offset。
///
/// `config_dir` 用于定位 offset 持久化文件；传 None 表示不持久化（测试用）。
/// 多次调用会返回同一个内部实例（offset 内存态共享）。
pub fn proxy_signer(config_dir: Option<PathBuf>) -> &'static ProxySigner {
    static SIGNER: OnceLock<ProxySigner> = OnceLock::new();
    SIGNER.get_or_init(|| ProxySigner::new(config_dir))
}

/// 中转代理签名器。
///
/// 不直接持有 secret 字符串副本（`PROXY_SHARED_SECRET` 是编译期常量），
/// 只维护本地与服务器之间的时钟 offset（秒，可负）。
pub struct ProxySigner {
    config_dir: Option<PathBuf>,
    /// 内部可变性：offset 用 RwLock 单值保护即可。Mutex<i64> 简单够用。
    offset: std::sync::Mutex<i64>,
}

impl ProxySigner {
    pub fn new(config_dir: Option<PathBuf>) -> Self {
        let mut signer = Self {
            config_dir: config_dir.clone(),
            offset: std::sync::Mutex::new(0),
        };
        signer.load_offset_from_disk();
        tracing::info!(
            target: "ncd_network::proxy",
            base_url = PROXY_BASE_URL,
            secret_len = PROXY_SHARED_SECRET.len(),
            placeholder = PROXY_SHARED_SECRET.contains(PLACEHOLDER_MARKER),
            "ProxySigner 初始化（offset 从磁盘加载）"
        );
        signer
    }

    /// 生成签名头。
    ///
    /// `path` 是 `/v1/release/{alias}` 形式。message = `{timestamp}.{path}`，
    /// HMAC-SHA256(secret, message) 得到 hex 签名。
    pub fn sign_headers(&self, path: &str) -> HashMap<&'static str, String> {
        let ts = {
            let offset = *self.offset.lock().expect("offset lock poisoned");
            now_unix_secs().saturating_add_signed(offset as i64).max(0)
        };
        let ts_str = ts.to_string();
        let message = format!("{ts_str}.{path}");
        let mut mac = HmacSha256::new_from_slice(PROXY_SHARED_SECRET.as_bytes())
            .expect("HMAC key length is valid for any size");
        mac.update(message.as_bytes());
        let sig = hex::encode(mac.finalize().into_bytes());

        let mut headers = HashMap::new();
        headers.insert("X-Timestamp", ts_str);
        headers.insert("X-Signature", sig);
        headers.insert(
            "User-Agent",
            concat!("NapCatQQ-Desktop/", env!("CARGO_PKG_VERSION")).to_string(),
        );
        headers
    }

    /// 读响应头里的 `X-Server-Time` 校正 offset，返回是否更新成功。
    ///
    /// - 缺失/解析失败 → false
    /// - 新 offset 与旧差值 < {@link OFFSET_PERSIST_THRESHOLD_SECS} → false（不写盘）
    /// - 否则更新内存 + 写盘，返回 true（调用方据此决定是否重试一次）
    pub fn update_offset_from_response(&self, headers: &reqwest::header::HeaderMap) -> bool {
        let Some(server_time) = header_server_time(headers) else {
            return false;
        };
        let local = now_unix_secs();
        let new_offset = server_time as i64 - local as i64;
        let updated = {
            let mut guard = self.offset.lock().expect("offset lock poisoned");
            let prev = *guard;
            if (new_offset - prev).abs() < OFFSET_PERSIST_THRESHOLD_SECS {
                return false;
            }
            *guard = new_offset;
            true
        };
        if updated {
            self.persist_offset(new_offset);
            tracing::info!(
                target: "ncd_network::proxy",
                offset_secs = new_offset,
                "代理时钟偏差已更新"
            );
        }
        updated
    }

    fn load_offset_from_disk(&mut self) {
        let Some(dir) = self.config_dir.as_ref() else {
            return;
        };
        let path = dir.join(OFFSET_FILENAME);
        let Ok(content) = std::fs::read_to_string(&path) else {
            return;
        };
        if let Ok(v) = content.trim().parse::<i64>() {
            *self.offset.get_mut().expect("offset lock poisoned") = v;
        }
    }

    fn persist_offset(&self, secs: i64) {
        let Some(dir) = self.config_dir.as_ref() else {
            return;
        };
        let path = dir.join(OFFSET_FILENAME);
        if let Err(err) = std::fs::create_dir_all(dir)
            .and_then(|()| std::fs::write(&path, secs.to_string()))
        {
            tracing::warn!(
                target: "ncd_network::proxy",
                ?err,
                path = %path.display(),
                "持久化代理时钟偏差失败"
            );
        }
    }
}

/// 把已生成的 ProxySigner 偏移文件删除（测试 / dev 用）。
pub fn _clear_offset_file(config_dir: &Path) {
    let _ = std::fs::remove_file(config_dir.join(OFFSET_FILENAME));
}

/// 从 reqwest HeaderMap 抽 `X-Server-Time`（大小写不敏感）。
fn header_server_time(headers: &reqwest::header::HeaderMap) -> Option<u64> {
    headers
        .get("X-Server-Time")
        .or_else(|| headers.get("x-server-time"))?
        .to_str()
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()
}

fn now_unix_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::HeaderMap;

    #[test]
    fn alias_paths_are_stable() {
        assert_eq!(ReleaseAlias::Napcat.path(), "/v1/release/napcat");
        assert_eq!(ReleaseAlias::Snowluma.path(), "/v1/release/snowluma");
        assert_eq!(ReleaseAlias::Ncd.path(), "/v1/release/ncd");
    }

    #[test]
    fn proxy_url_contains_base_and_alias() {
        // 仅在已注入时拼 URL；本仓库默认占位 → None。
        if !is_proxy_configured() {
            assert_eq!(proxy_release_url(ReleaseAlias::Napcat), None);
            return;
        }
        let url = proxy_release_url(ReleaseAlias::Napcat).unwrap();
        assert!(url.contains("/v1/release/napcat"), "url = {url}");
    }

    #[test]
    fn sign_headers_produces_timestamp_and_signature() {
        let tmp = tempfile::tempdir().unwrap();
        let signer = ProxySigner::new(Some(tmp.path().to_path_buf()));
        let headers = signer.sign_headers("/v1/release/napcat");
        assert!(headers.contains_key("X-Timestamp"));
        assert!(headers.contains_key("X-Signature"));
        let sig = headers.get("X-Signature").unwrap();
        // HMAC-SHA256 → 64 hex
        assert_eq!(sig.len(), 64);
        assert!(sig.bytes().all(|b| b.is_ascii_hexdigit()));
    }

    #[test]
    fn update_offset_reads_server_time_header() {
        let tmp = tempfile::tempdir().unwrap();
        let signer = ProxySigner::new(Some(tmp.path().to_path_buf()));

        let local = now_unix_secs();
        let server_time = local + 120; // +2 分钟漂移
        let mut headers = HeaderMap::new();
        headers.insert("X-Server-Time", server_time.to_string().parse().unwrap());

        assert!(signer.update_offset_from_response(&headers));
        // 抖动 <2s 不更新
        assert!(!signer.update_offset_from_response(&headers));

        // 持久化能读回
        let signer2 = ProxySigner::new(Some(tmp.path().to_path_buf()));
        let guard = signer2.offset.lock().unwrap();
        assert!((*guard - 120).abs() <= 1, "offset = {}", *guard);
    }

    #[test]
    fn update_offset_ignores_missing_or_bad_header() {
        let tmp = tempfile::tempdir().unwrap();
        let signer = ProxySigner::new(Some(tmp.path().to_path_buf()));
        assert!(!signer.update_offset_from_response(&HeaderMap::new()));

        let mut headers = HeaderMap::new();
        headers.insert("X-Server-Time", "not-a-number".parse().unwrap());
        assert!(!signer.update_offset_from_response(&headers));
    }

    /// 签名确定性：同一 secret + 同一 timestamp + 同一 path → 同一签名。
    #[test]
    fn signature_is_deterministic_for_same_inputs() {
        let tmp = tempfile::tempdir().unwrap();
        let signer = ProxySigner::new(Some(tmp.path().to_path_buf()));

        // 固定 offset=0，确保 timestamp 来自系统时钟但同一瞬间内一致。
        let h1 = signer.sign_headers("/v1/release/napcat");
        let h2 = signer.sign_headers("/v1/release/napcat");
        // 同一秒内（测试运行很快）：timestamp 应相等，签名也相等。
        if h1["X-Timestamp"] == h2["X-Timestamp"] {
            assert_eq!(h1["X-Signature"], h2["X-Signature"]);
        }
        // 不同 path 一定不同签名（只要 timestamp 相同）。
        let h3 = signer.sign_headers("/v1/release/ncd");
        if h1["X-Timestamp"] == h3["X-Timestamp"] {
            assert_ne!(h1["X-Signature"], h3["X-Signature"]);
        }
    }
}
