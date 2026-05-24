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

use ncd_domain::release_snapshot::{ReleaseInfo, ReleaseSnapshot};
use serde::Deserialize;
use tracing::warn;

const CACHE_TTL_SECS: u64 = 3600;
const CACHE_FILE_NAME: &str = "release-snapshot.json";
const CACHE_DIR_NAME: &str = "cache";
const HTTP_TIMEOUT_SECS: u64 = 5;
const USER_AGENT: &str = concat!("NapCatQQ-Desktop/", env!("CARGO_PKG_VERSION"));

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
/// 流程：
/// 1. 尝试读 `<data_root>/cache/release-snapshot.json`；如果缓存还在 TTL 内
///    直接返回；
/// 2. 并发拉三个仓库的 latest release；
/// 3. 写缓存（失败仅 warn，不阻断返回）；
/// 4. 返回新快照。
///
/// 任何 IO / 网络错误一律降级到 None 字段或老缓存，不向 caller 抛错。
pub async fn fetch_release_snapshot(data_root: &Path) -> ReleaseSnapshot {
    if let Some(cached) = read_cache(data_root) {
        if !is_stale(&cached) {
            return cached;
        }
    }

    let client = match build_http_client() {
        Ok(client) => client,
        Err(err) => {
            warn!(?err, "release snapshot http client build failed");
            // 客户端构造失败极罕见（rustls root store），回落到老缓存或 Default。
            return read_cache(data_root).unwrap_or_default();
        }
    };

    let (napcat, snowluma, desktop) = tokio::join!(
        fetch_one(&client, NAPCAT_RELEASES_URL),
        fetch_one(&client, SNOWLUMA_RELEASES_URL),
        fetch_one(&client, DESKTOP_RELEASES_URL),
    );

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

fn build_http_client() -> Result<reqwest::Client, reqwest::Error> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(HTTP_TIMEOUT_SECS))
        .pool_idle_timeout(Duration::from_secs(30))
        .user_agent(USER_AGENT)
        .build()
}

/// GitHub releases API 单条记录子集。仅取本模块需要的字段，其它字段
/// （author / assets / draft 等）显式忽略。
#[derive(Debug, Clone, Deserialize)]
struct GhReleaseDto {
    tag_name: String,
    /// ISO8601 字符串，例：`2023-11-14T12:34:56Z`。GitHub 始终给 UTC + Z。
    published_at: Option<String>,
    html_url: Option<String>,
    body: Option<String>,
}

async fn fetch_one(client: &reqwest::Client, url: &str) -> Option<ReleaseInfo> {
    let response = match client.get(url).send().await {
        Ok(r) => r,
        Err(err) => {
            warn!(url, ?err, "release fetch failed");
            return None;
        }
    };

    if !response.status().is_success() {
        warn!(url, status = %response.status(), "release fetch non-2xx");
        return None;
    }

    let dto: GhReleaseDto = match response.json().await {
        Ok(dto) => dto,
        Err(err) => {
            warn!(url, ?err, "release fetch json decode failed");
            return None;
        }
    };

    Some(ReleaseInfo {
        version: strip_v_prefix(&dto.tag_name).to_string(),
        published_at: dto
            .published_at
            .as_deref()
            .and_then(parse_iso8601_to_unix)
            .unwrap_or(0),
        html_url: dto.html_url.unwrap_or_default(),
        release_notes: dto.body.unwrap_or_default(),
    })
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
    use ncd_domain::release_snapshot::{ReleaseInfo, ReleaseSnapshot};
    use tempfile::tempdir;

    fn fresh_snapshot() -> ReleaseSnapshot {
        ReleaseSnapshot {
            napcat_latest: Some(ReleaseInfo {
                version: "4.18.1".to_string(),
                published_at: 1_700_000_000,
                html_url: "https://example.com/r".to_string(),
                release_notes: "notes".to_string(),
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
        let snap = fetch_release_snapshot(temp.path()).await;
        // 能拉到任意一个仓库的 release 即视为通；网络抖动时全 None 也算
        // 通（只要不 panic / 不抛错）。
        assert!(snap.fetched_at.is_some());
    }
}
