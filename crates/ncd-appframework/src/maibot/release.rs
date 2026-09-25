//! 装哪一版：适配器最新 Release 的 `_manifest.json` 声明了兼容的 MaiBot 版本范围，
//! 在 MaiBot 的 Release 里挑落在范围内的最新一版。GitHub API 不通就装内置组合。
//!
//! 先定适配器再挑 MaiBot：适配器的正式 Release 跟着 MaiBot 正式版走（v1.4.0 对 1.2.x；
//! v1.5.0 只打了 tag，给还没发布的 1.3.x），反过来按 MaiBot 最新版找适配器，上游刚发版时会扑空。

use std::time::Duration;

use serde::Deserialize;

const API_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveExt {
    /// 本机：进程内解压，不依赖外部工具
    Zip,
    /// 远端：tar 几乎都自带，unzip 很多最小化镜像没有
    TarGz,
}

impl ArchiveExt {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Zip => "zip",
            Self::TarGz => "tar.gz",
        }
    }
}

/// GitHub 源码包；解出来有一层 `<repo>-<tag 去掉开头 v>/` 顶级目录
pub fn archive_url(repo: &str, tag: &str, ext: ArchiveExt) -> String {
    format!(
        "https://github.com/{repo}/archive/refs/tags/{tag}.{}",
        ext.as_str()
    )
}

#[derive(Deserialize)]
struct ReleaseRow {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
}

/// GitHub releases 列表去掉草稿和预发布，保持返回顺序（新的在前）
pub fn stable_release_tags(json: &str) -> Result<Vec<String>, String> {
    let rows: Vec<ReleaseRow> =
        serde_json::from_str(json).map_err(|e| format!("解析 Release 列表失败: {e}"))?;
    Ok(rows
        .into_iter()
        .filter(|r| !r.draft && !r.prerelease && !r.tag_name.trim().is_empty())
        .map(|r| r.tag_name)
        .collect())
}

#[derive(Deserialize)]
struct AdapterManifest {
    host_application: Option<HostRange>,
}

#[derive(Deserialize)]
struct HostRange {
    min_version: String,
    max_version: String,
}

/// 适配器 `_manifest.json` 的 `host_application` 范围
pub fn adapter_host_range(json: &str) -> Option<(String, String)> {
    let m: AdapterManifest = serde_json::from_str(json).ok()?;
    let r = m.host_application?;
    Some((r.min_version.trim().to_string(), r.max_version.trim().to_string()))
}

/// 三段式版本，容忍 tag 前缀 `v`
pub fn parse_semver(s: &str) -> Option<(u64, u64, u64)> {
    let s = s.trim();
    let s = s.strip_prefix('v').unwrap_or(s);
    let mut it = s.split('.');
    let major = it.next()?.parse().ok()?;
    let minor = it.next()?.parse().ok()?;
    let patch = it.next()?.parse().ok()?;
    if it.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

/// 上游插件运行时的判定（`update_compatibility_notice._is_host_compatible`）：
/// 落在 [min, max]，或与 max 同 major.minor、补丁号更高
pub fn host_compatible(version: &str, min: &str, max: &str) -> bool {
    let (Some(v), Some(lo), Some(hi)) = (parse_semver(version), parse_semver(min), parse_semver(max))
    else {
        return false;
    };
    (lo <= v && v <= hi) || (v.0 == hi.0 && v.1 == hi.1 && v > hi)
}

/// 范围内版本号最高的 MaiBot tag
pub fn pick_maibot_tag(tags: &[String], min: &str, max: &str) -> Option<String> {
    tags.iter()
        .filter(|t| host_compatible(t, min, max))
        .filter_map(|t| parse_semver(t).map(|v| (v, t)))
        .max_by_key(|(v, _)| *v)
        .map(|(_, t)| t.clone())
}

/// 拉一个仓库的正式 Release tag（新的在前）。直连 api.github.com：国内中转只认它登记过的仓库
pub async fn fetch_stable_release_tags(repo: &str) -> Result<Vec<String>, String> {
    let url = format!("https://api.github.com/repos/{repo}/releases?per_page=30");
    let resp = ncd_network::shared_client()
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .timeout(API_TIMEOUT)
        .send()
        .await
        .map_err(|e| format!("请求 {repo} 的 Release 失败: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("请求 {repo} 的 Release 失败: HTTP {}", resp.status()));
    }
    let text = resp
        .text()
        .await
        .map_err(|e| format!("读取 {repo} 的 Release 失败: {e}"))?;
    stable_release_tags(&text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archive_urls_by_host_kind() {
        assert_eq!(
            archive_url("Mai-with-u/MaiBot", "1.2.5", ArchiveExt::Zip),
            "https://github.com/Mai-with-u/MaiBot/archive/refs/tags/1.2.5.zip"
        );
        assert_eq!(
            archive_url("Mai-with-u/MaiBot-Napcat-Adapter", "v1.4.0", ArchiveExt::TarGz),
            "https://github.com/Mai-with-u/MaiBot-Napcat-Adapter/archive/refs/tags/v1.4.0.tar.gz"
        );
    }

    #[test]
    fn stable_tags_skip_drafts_and_prereleases() {
        let json = r#"[
            {"tag_name":"1.3.0-beta","prerelease":true},
            {"tag_name":"1.2.5","prerelease":false,"draft":false},
            {"tag_name":"wip","draft":true},
            {"tag_name":"1.2.4"}
        ]"#;
        assert_eq!(stable_release_tags(json).unwrap(), vec!["1.2.5", "1.2.4"]);
        assert!(stable_release_tags("{").is_err());
    }

    #[test]
    fn adapter_range_from_real_manifest_shape() {
        let json = r#"{"manifest_version":2,"version":"1.4.0",
            "host_application":{"min_version":"1.2.0","max_version":"1.2.99"},
            "id":"maibot-team.napcat-adapter","plugin_type":"adapter"}"#;
        assert_eq!(
            adapter_host_range(json),
            Some(("1.2.0".to_string(), "1.2.99".to_string()))
        );
        assert_eq!(adapter_host_range(r#"{"version":"1"}"#), None);
    }

    #[test]
    fn compatibility_follows_upstream_rule() {
        assert!(host_compatible("1.2.5", "1.2.0", "1.2.99"));
        assert!(!host_compatible("1.3.0", "1.2.0", "1.2.99"));
        assert!(!host_compatible("1.1.4", "1.2.0", "1.2.99"));
        assert!(host_compatible("1.2.7", "1.2.0", "1.2.5"), "同 major.minor 的更高补丁放行");
        assert!(!host_compatible("garbage", "1.2.0", "1.2.99"));
        assert_eq!(parse_semver("v1.4.0"), Some((1, 4, 0)));
        assert_eq!(parse_semver("1.2"), None);
        assert_eq!(parse_semver("1.2.3.4"), None);
    }

    #[test]
    fn picks_newest_maibot_inside_adapter_range() {
        let tags: Vec<String> = ["1.3.0", "1.2.5", "1.2.10", "1.1.4"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(pick_maibot_tag(&tags, "1.2.0", "1.2.99").as_deref(), Some("1.2.10"));
        assert_eq!(pick_maibot_tag(&tags, "2.0.0", "2.0.99"), None);
    }
}
