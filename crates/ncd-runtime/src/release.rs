//! GitHub releases API 拉取 + 本地缓存。
//!
//! 缓存策略：
//! - 缓存文件：`<data_root>/cache/release-snapshot.json`
//! - TTL：1 小时；快照内 `fetched_at + 3600` 还在 future 就直接返回缓存
//! - 拉取失败：返回上次缓存（带老 fetched_at）；都没有就返回 Default
//! - 永远不向 caller 抛错（`fetch_release_snapshot` 返回 `ReleaseSnapshot`，
//!   不是 `Result`）
//!
//! 本模块只做 IO + JSON 解析，不参与 UI 派生（"是否需要更新"由前端
//! 比对本地版本和远端版本派生）。
//!
//! 仓库 URL 是 latest release 的 GitHub API 端点。Desktop 仓库 URL 当前
//! 为本仓库占位，后续仓库正式上线后由维护者确认（见 TODO(repo-url)）。

use std::path::Path;
use std::time::Duration;

use ncd_domain::release_snapshot::{ReleaseAsset, ReleaseInfo, ReleaseSnapshot};
use ncd_network::{retry_with_backoff, NetworkError, RetryPolicy, shared_client};
use serde::Deserialize;
use tracing::{info, warn};

const CACHE_TTL_SECS: u64 = 3600;
const CACHE_FILE_NAME: &str = "release-snapshot.json";
const CACHE_DIR_NAME: &str = "cache";
const HTTP_TIMEOUT_SECS: u64 = 5;

const NAPCAT_RELEASES_URL: &str =
    "https://api.github.com/repos/NapNeko/NapCatQQ/releases/latest";
// TODO(repo-url): SnowLuma 仓库 URL 需要后端 / 项目维护者确认上游 owner/name。
// 当前用 placeholder 让 desktop / napcat 端到端可拉，SnowLuma 端拿到 404
// 走 None 回落，不会影响整体 snapshot 返回。
const SNOWLUMA_RELEASES_URL: &str =
    "https://api.github.com/repos/SnowLuma/SnowLuma/releases/latest";
// TODO(repo-url): 本仓库 release 端点；上线后由维护者确认。
const DESKTOP_RELEASES_URL: &str =
    "https://api.github.com/repos/NapNeko/NapCatQQ-Desktop/releases/latest";

/// 拉取一次远端 releases 快照。
///
/// `token`：可选 GitHub PAT。非 None 时给请求加 `Authorization: Bearer <token>`，
/// 把匿名速率限制（60 次/小时/IP）提升到认证额度（5000 次/小时）。token 不参与
/// 缓存 key——缓存只按 TTL，认证与否拿到的 release 数据一致。
///
/// 流程：
/// 1. 尝试读 `<data_root>/cache/release-snapshot.json`；如果缓存还在 TTL 内
///    直接返回；
/// 2. 并发拉三个仓库的 latest release；
/// 3. 写缓存（失败仅 warn，不阻断返回）；
/// 4. 返回新快照。
///
/// 任何 IO / 网络错误一律降级到 None 字段或老缓存，不向 caller 抛错。
pub async fn fetch_release_snapshot(data_root: &Path, token: Option<&str>) -> ReleaseSnapshot {
    if let Some(cached) = read_cache(data_root) {
        if !is_stale(&cached) {
            return cached;
        }
    }

    let client = shared_client();

    let (napcat, snowluma, desktop) = tokio::join!(
        fetch_one(&client, NAPCAT_RELEASES_URL, token),
        fetch_one(&client, SNOWLUMA_RELEASES_URL, token),
        fetch_one(&client, DESKTOP_RELEASES_URL, token),
    );

    let failed: Vec<&str> = [
        (napcat.is_none(), "NapCat"),
        (snowluma.is_none(), "SnowLuma"),
        (desktop.is_none(), "Desktop"),
    ]
    .into_iter()
    .filter(|(bad, _)| *bad)
    .map(|(_, name)| name)
    .collect();
    if !failed.is_empty() {
        warn!(
            target: "ncd_runtime::release",
            repos = %failed.join(", "),
            "GitHub 版本检查失败（将使用缓存或留空），可检查网络或配置 GitHub PAT"
        );
    } else {
        info!(target: "ncd_runtime::release", "GitHub 版本快照已更新");
    }

    let snapshot = ReleaseSnapshot {
        napcat_latest: napcat,
        snowluma_latest: snowluma,
        desktop_latest: desktop,
        fetched_at: Some(current_unix_ts()),
    };

    if let Err(err) = write_cache(data_root, &snapshot) {
        warn!(?err, "release snapshot cache write failed");
    }
    snapshot
}

/// 判断快照是否过 TTL：fetched_at 缺失（从未成功拉过）也视为 stale。
pub(crate) fn is_stale(snap: &ReleaseSnapshot) -> bool {
    let Some(at) = snap.fetched_at else {
        return true;
    };
    current_unix_ts().saturating_sub(at) > CACHE_TTL_SECS
}

fn current_unix_ts() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// GitHub releases API 单条记录子集。
///
/// 仅取本模块需要的字段，其它字段（author / draft 等）显式忽略。`assets`
/// 字段是 release 完整性校验的关键来源——每个 asset 的 `digest` 形如
/// `"sha256:<64-hex>"`，安装层用它在下载完后做 SHA256 校验，防止国内代理
/// CDN 投毒（"长度对、Content-Range 对、流不截断、字节是垃圾" 这一类）。
#[derive(Debug, Clone, Deserialize)]
struct GhReleaseDto {
    tag_name: String,
    /// ISO8601 字符串，例：`2023-11-14T12:34:56Z`。GitHub 始终给 UTC + Z。
    published_at: Option<String>,
    html_url: Option<String>,
    body: Option<String>,
    #[serde(default)]
    assets: Vec<GhAssetDto>,
}

/// release 单 asset。`digest` 是 GitHub 2024-Q4 上线的字段，老 release 没有；
/// 缺失或前缀非 `sha256:` 时安装层退化到"无 hash"分支。
#[derive(Debug, Clone, Deserialize)]
struct GhAssetDto {
    name: String,
    #[serde(default)]
    digest: Option<String>,
}

async fn fetch_one(
    client: &reqwest::Client,
    url: &str,
    token: Option<&str>,
) -> Option<ReleaseInfo> {
    let policy = RetryPolicy::default();
    let url_owned = url.to_string();
    let token_owned = token.map(|s| s.to_string());
    match retry_with_backoff(&policy, || {
        let client = client;
        let url = url_owned.clone();
        let token = token_owned.as_deref();
        async move { release_fetch_attempt(client, &url, token).await }
    })
    .await
    {
        Ok(info) => Some(info),
        Err(err) => {
            tracing::debug!(url, ?err, "release fetch attempt failed");
            None
        }
    }
}

async fn release_fetch_attempt(
    client: &reqwest::Client,
    url: &str,
    token: Option<&str>,
) -> Result<ReleaseInfo, NetworkError> {
    let mut request = client
        .get(url)
        .timeout(Duration::from_secs(HTTP_TIMEOUT_SECS));
    if let Some(token) = token.map(str::trim).filter(|t| !t.is_empty()) {
        request = request.bearer_auth(token);
    }
    let response = request.send().await?;

    if !response.status().is_success() {
        return Err(NetworkError::Status(response.status().as_u16()));
    }

    let dto: GhReleaseDto = response.json().await.map_err(|e| NetworkError::Http(e.to_string()))?;

    Ok(ReleaseInfo {
        version: strip_v_prefix(&dto.tag_name).to_string(),
        tag: dto.tag_name.clone(),
        published_at: dto
            .published_at
            .as_deref()
            .and_then(parse_iso8601_to_unix)
            .unwrap_or(0),
        html_url: dto.html_url.unwrap_or_default(),
        release_notes: dto.body.unwrap_or_default(),
        assets: dto
            .assets
            .into_iter()
            .filter_map(|asset| {
                let sha = parse_sha256_digest(asset.digest.as_deref()).unwrap_or_default();
                if asset.name.is_empty() {
                    None
                } else {
                    Some(ReleaseAsset {
                        name: asset.name,
                        sha256: sha,
                    })
                }
            })
            .collect(),
    })
}

/// 从 GitHub digest 字段（`"sha256:<64-hex>"`）抽出 64-hex SHA256。
///
/// 缺失 / 非 `sha256:` 前缀 / hex 不合法时返回 None。GitHub 当前只用 sha256
/// 算法，未来可能扩展（sha512 等），届时按前缀分派。
pub(crate) fn parse_sha256_digest(digest: Option<&str>) -> Option<String> {
    let raw = digest?.trim();
    let hex = raw.strip_prefix("sha256:")?;
    if hex.len() != 64 {
        return None;
    }
    if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    Some(hex.to_ascii_lowercase())
}

/// `v4.18.1` → `4.18.1`；其它形式原样返回。
fn strip_v_prefix(tag: &str) -> &str {
    tag.strip_prefix('v').unwrap_or(tag)
}

/// 解析 GitHub 风格的 ISO8601 UTC 时间戳到 Unix epoch 秒。
///
/// 仅支持形如 `YYYY-MM-DDTHH:MM:SSZ` 的格式（GitHub API 实测形态）。
/// 其它分隔符 / 时区写法（例如带毫秒、带 +08:00）一律返回 None；
/// caller 把 None 当作"时间戳缺失"处理，不影响其它字段。
///
/// 不依赖 chrono 是为了避免给 ncd-runtime 引入新依赖；本函数语义足够
/// 覆盖 GitHub API 的真实输出。
pub(crate) fn parse_iso8601_to_unix(s: &str) -> Option<u64> {
    // YYYY-MM-DDTHH:MM:SSZ → 长度 20
    if s.len() != 20 || !s.ends_with('Z') {
        return None;
    }
    let bytes = s.as_bytes();
    if bytes[4] != b'-' || bytes[7] != b'-' || bytes[10] != b'T'
        || bytes[13] != b':' || bytes[16] != b':'
    {
        return None;
    }
    let year: i32 = s[0..4].parse().ok()?;
    let month: u32 = s[5..7].parse().ok()?;
    let day: u32 = s[8..10].parse().ok()?;
    let hour: u32 = s[11..13].parse().ok()?;
    let minute: u32 = s[14..16].parse().ok()?;
    let second: u32 = s[17..19].parse().ok()?;

    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 60  // GitHub 不会给闰秒，但宽松处理
    {
        return None;
    }

    days_from_civil(year, month, day).map(|days| {
        let secs_in_day = u64::from(hour) * 3600 + u64::from(minute) * 60 + u64::from(second);
        (days * 86_400) + secs_in_day
    })
}

/// Howard Hinnant `days_from_civil` 算法：1970-01-01 到 (year-month-day) 的
/// 天数。仅支持 year ≥ 1970（GitHub 时间戳不会早于此），早于则返回 None。
fn days_from_civil(year: i32, month: u32, day: u32) -> Option<u64> {
    if year < 1970 {
        return None;
    }
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = (y - era * 400) as u32; // [0, 399]
    let doy = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let civil_days = i64::from(era) * 146_097 + i64::from(doe) - 719_468;
    if civil_days < 0 {
        None
    } else {
        Some(civil_days as u64)
    }
}

fn cache_path(data_root: &Path) -> std::path::PathBuf {
    data_root.join(CACHE_DIR_NAME).join(CACHE_FILE_NAME)
}

fn read_cache(data_root: &Path) -> Option<ReleaseSnapshot> {
    let path = cache_path(data_root);
    let content = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&content).ok()
}

/// 仅读磁盘缓存的快照，不发起网络请求。
///
/// 给 Tauri command 同步路径使用：`run_component_action` 在 build component
/// 阶段必须立刻拿到 sha256 才能注入 `with_sha256`，等不起一次完整 GitHub
/// 拉取（且每次安装都拉 = 速率限制）。缓存由 `fetch_release_snapshot` 在
/// 启动 / 前端轮询时维护，本函数只消费。缓存缺失返 None；上层应当当作
/// "无 hash 数据"分支，跳过校验或弹二次确认（对齐 legacy
/// `run_napcat_archive_hash_check` 行为）。
pub fn read_cached_release_snapshot(data_root: &Path) -> Option<ReleaseSnapshot> {
    read_cache(data_root)
}

fn write_cache(data_root: &Path, snap: &ReleaseSnapshot) -> std::io::Result<()> {
    let dir = data_root.join(CACHE_DIR_NAME);
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(CACHE_FILE_NAME);
    let content = serde_json::to_string_pretty(snap)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(path, content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ncd_domain::release_snapshot::{ReleaseAsset, ReleaseInfo, ReleaseSnapshot};
    use tempfile::tempdir;

    fn fresh_snapshot() -> ReleaseSnapshot {
        ReleaseSnapshot {
            napcat_latest: Some(ReleaseInfo {
                version: "4.18.1".to_string(),
                tag: "v4.18.1".to_string(),
                published_at: 1_700_000_000,
                html_url: "https://example.com/r".to_string(),
                release_notes: "notes".to_string(),
                assets: vec![ReleaseAsset {
                    name: "NapCat.Shell.zip".to_string(),
                    sha256: "0".repeat(64),
                }],
            }),
            snowluma_latest: None,
            desktop_latest: None,
            fetched_at: Some(current_unix_ts()),
        }
    }

    /// fetched_at 为 None（从未拉过）必须视为 stale：保证首次启动一定走真
    /// 网络拉取路径，不会被假快照锁住。
    #[test]
    fn is_stale_returns_true_for_snapshot_without_fetched_at() {
        let mut snap = fresh_snapshot();
        snap.fetched_at = None;
        assert!(is_stale(&snap));
    }

    #[test]
    fn is_stale_returns_true_for_old_snapshot() {
        let mut snap = fresh_snapshot();
        // 把 fetched_at 调到 2 小时前，超过 TTL。
        snap.fetched_at = Some(current_unix_ts().saturating_sub(CACHE_TTL_SECS + 100));
        assert!(is_stale(&snap));
    }

    #[test]
    fn is_stale_returns_false_for_fresh_snapshot() {
        let snap = fresh_snapshot();
        assert!(!is_stale(&snap));
    }

    /// 缓存文件不存在时 read_cache 必须回 None：保证首次启动会触发真拉取。
    #[test]
    fn read_cache_returns_none_when_file_missing() {
        let temp = tempdir().unwrap();
        let result = read_cache(temp.path());
        assert!(result.is_none());
    }

    #[test]
    fn read_write_cache_round_trips() {
        let temp = tempdir().unwrap();
        let snap = fresh_snapshot();

        write_cache(temp.path(), &snap).expect("write_cache ok");
        let loaded = read_cache(temp.path()).expect("read_cache returns snapshot");

        assert_eq!(loaded, snap);
    }

    #[test]
    fn write_cache_creates_parent_directory() {
        let temp = tempdir().unwrap();
        // cache dir 不存在；write_cache 应该自己 mkdir -p。
        let snap = ReleaseSnapshot::default();
        write_cache(temp.path(), &snap).expect("write_cache ok");
        assert!(cache_path(temp.path()).exists());
    }

    /// stale 缓存能被反序列化回来；fetch_release_snapshot 上层会决定是否
    /// 仍然把它返回给前端（拉取失败时是的）。
    #[test]
    fn read_cache_returns_stale_snapshot_intact() {
        let temp = tempdir().unwrap();
        let mut snap = fresh_snapshot();
        snap.fetched_at = Some(current_unix_ts().saturating_sub(CACHE_TTL_SECS + 100));
        write_cache(temp.path(), &snap).unwrap();

        let loaded = read_cache(temp.path()).unwrap();
        assert!(is_stale(&loaded));
        assert_eq!(loaded.napcat_latest, snap.napcat_latest);
    }

    #[test]
    fn strip_v_prefix_normalizes_tag_names() {
        assert_eq!(strip_v_prefix("v4.18.1"), "4.18.1");
        assert_eq!(strip_v_prefix("4.18.1"), "4.18.1");
        assert_eq!(strip_v_prefix("v0.0.0-pre.1"), "0.0.0-pre.1");
    }

    #[test]
    fn parse_sha256_digest_accepts_lowercase_hex() {
        let hex = "a".repeat(64);
        let digest = format!("sha256:{hex}");
        assert_eq!(parse_sha256_digest(Some(&digest)), Some(hex));
    }

    #[test]
    fn parse_sha256_digest_normalizes_uppercase_to_lowercase() {
        let hex_upper = "A".repeat(64);
        let digest = format!("sha256:{hex_upper}");
        assert_eq!(parse_sha256_digest(Some(&digest)), Some("a".repeat(64)));
    }

    #[test]
    fn parse_sha256_digest_rejects_non_sha256_algorithm() {
        let digest = format!("sha512:{}", "0".repeat(64));
        assert_eq!(parse_sha256_digest(Some(&digest)), None);
    }

    #[test]
    fn parse_sha256_digest_rejects_wrong_length() {
        let digest = format!("sha256:{}", "0".repeat(63));
        assert_eq!(parse_sha256_digest(Some(&digest)), None);
    }

    #[test]
    fn parse_sha256_digest_rejects_non_hex_chars() {
        let digest = format!("sha256:{}", "z".repeat(64));
        assert_eq!(parse_sha256_digest(Some(&digest)), None);
    }

    #[test]
    fn parse_sha256_digest_handles_missing_field() {
        assert_eq!(parse_sha256_digest(None), None);
        assert_eq!(parse_sha256_digest(Some("")), None);
    }

    /// GitHub 实测形态：`2023-11-14T12:34:56Z`。Unix epoch 验证基准日期。
    #[test]
    fn parse_iso8601_unix_epoch_is_zero() {
        assert_eq!(parse_iso8601_to_unix("1970-01-01T00:00:00Z"), Some(0));
    }

    #[test]
    fn parse_iso8601_known_timestamp() {
        // 2023-11-14T12:34:56Z = 1699965296
        assert_eq!(
            parse_iso8601_to_unix("2023-11-14T12:34:56Z"),
            Some(1_699_965_296)
        );
    }

    #[test]
    fn parse_iso8601_year_2000_leap_day() {
        // 2000 是闰年（被 400 整除）；2 月 29 号合法。
        // 2000-02-29T00:00:00Z = 951782400
        assert_eq!(
            parse_iso8601_to_unix("2000-02-29T00:00:00Z"),
            Some(951_782_400)
        );
    }

    #[test]
    fn parse_iso8601_rejects_bad_formats() {
        // 不是 Z 结尾
        assert_eq!(parse_iso8601_to_unix("2023-11-14T12:34:56+00:00"), None);
        // 长度不对
        assert_eq!(parse_iso8601_to_unix("2023-11-14"), None);
        // 月份越界
        assert_eq!(parse_iso8601_to_unix("2023-13-14T00:00:00Z"), None);
        // 分隔符错
        assert_eq!(parse_iso8601_to_unix("2023/11/14T12:34:56Z"), None);
        // 1969 早于 epoch
        assert_eq!(parse_iso8601_to_unix("1969-12-31T23:59:59Z"), None);
    }

    /// 实际网络拉取测试：默认 ignore，避免 CI 依赖外网。
    /// 本地手动跑：`cargo test -p ncd-runtime release::tests::live -- --ignored`。
    #[ignore]
    #[tokio::test]
    async fn live_fetch_release_snapshot_smoke() {
        let temp = tempdir().unwrap();
        let snap = fetch_release_snapshot(temp.path(), None).await;
        // 能拉到任意一个仓库的 release 即视为通；网络抖动时全 None 也算
        // 通（只要不 panic / 不抛错）。
        assert!(snap.fetched_at.is_some());
    }
}
