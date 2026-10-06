//! 查 PyPI 上某个发行包有哪些版本（供「装任意版本」的版本选择器用）。
//!
//! 只解析 `releases` 的键与 `info.version`，不下载任何文件。
//!
//! **排序用 PEP 440，不是 SemVer**：PyPI 上的版本是 PEP 440 拼写
//! （`1.2.1a1` / `1.0.0rc1` / `1.0.0.post1`），而 Rust 生态的 `semver` crate
//! 是严格 SemVer 2.0——`a1` / `rc1` 它一个都不认，拿它排序会让预发布全部
//! 退化成「不可比较」而排错位置。所以这里自己解析 PEP 440 的关键部分。
//!
//! **剔除 yanked**：纯 yanked 或没有文件的 release 装不上，列出来只会让用户白试。

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// 某个发行包在 PyPI 上的版本清单
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct PackageVersions {
    /// PyPI 的规范发行名（如 `neobot-app`）
    pub name: String,
    /// 可安装版本，按 PEP 440 **降序**（新的在前）
    pub versions: Vec<String>,
    /// 最新**正式版**；只有预发布时为 None（界面据此提示「默认装不到预发布」）
    pub latest: Option<String>,
    /// 是否存在预发布版本
    pub has_prerelease: bool,
}

/// PEP 440 的关键部分：`[N!]N(.N)*[{a|b|rc}N][.postN][.devN]`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pep440Version {
    /// epoch（`1!` 前缀）；没有是 0
    pub epoch: u64,
    /// release 段，如 `1.2.1` -> `[1, 2, 1]`
    pub release: Vec<u64>,
    /// 预发布：a / b / rc + 序号
    pub pre: Option<(PreKind, u64)>,
    /// `.postN`
    pub post: Option<u64>,
    /// `.devN`
    pub dev: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PreKind {
    A,
    B,
    Rc,
}

impl Pep440Version {
    /// 是否是预发布（有 pre 或 dev）。PEP 440：两者都算预发布
    pub fn is_prerelease(&self) -> bool {
        self.pre.is_some() || self.dev.is_some()
    }

    /// PEP 440 排序键。
    ///
    /// 粗略但正确的规则（本项目只用来排序 + 判预发布，不做完整 PEP 440 全集）：
    /// 1. epoch 大的在前
    /// 2. release 逐段比，短的补 0（`1.2` == `1.2.0`）
    /// 3. 有预发布的**低于**同 release 的正式版；两个预发布之间 a < b < rc，再比序号
    /// 4. `.devN` 低于同 release 的任何其它形态
    /// 5. `.postN` 高于同 release 的正式版
    fn sort_key(&self) -> (u64, Vec<u64>, u8, u64, i64, i64) {
        // pre_rank：dev 最低(0)，其次预发布(1 + kind)，正式(4)，post 最高(5)
        let pre_rank = if self.dev.is_some() && self.pre.is_none() && self.post.is_none() {
            0u8
        } else if let Some((kind, _)) = self.pre {
            1 + kind as u8
        } else if self.post.is_some() {
            5
        } else {
            4
        };
        let pre_num = self.pre.map(|(_, n)| n).unwrap_or(0);
        let post = self.post.map(|n| n as i64).unwrap_or(-1);
        // dev 直接比：没有 dev 记 MAX，于是「有 dev」低于同形态的「无 dev」，
        // dev 序号本身也自然升序；取负会把两个方向都排反
        let dev = self.dev.map(|n| n as i64).unwrap_or(i64::MAX);
        (
            self.epoch,
            self.release.clone(),
            pre_rank,
            pre_num,
            post,
            dev,
        )
    }
}

impl PartialOrd for Pep440Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Pep440Version {
    fn cmp(&self, other: &Self) -> Ordering {
        let a = self.sort_key();
        let b = other.sort_key();
        // release 短的补 0 再比，否则 `[1,2]` 与 `[1,2,0]` 比错
        let n = a.1.len().max(b.1.len());
        let ra = |v: &Vec<u64>, i: usize| v.get(i).copied().unwrap_or(0);
        let rel = (0..n)
            .map(|i| ra(&a.1, i).cmp(&ra(&b.1, i)))
            .find(|o| *o != Ordering::Equal)
            .unwrap_or(Ordering::Equal);
        a.0.cmp(&b.0)
            .then(rel)
            .then(a.2.cmp(&b.2))
            .then(a.3.cmp(&b.3))
            .then(a.4.cmp(&b.4))
            .then(a.5.cmp(&b.5))
    }
}

/// 解析 PEP 440。认不出的拼写返回 None（上游历史上有过畸形版本号）。
pub fn parse_pep440(raw: &str) -> Option<Pep440Version> {
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }
    let lower = s.to_ascii_lowercase();

    // epoch
    let (epoch, rest) = match lower.split_once('!') {
        Some((e, r)) => (e.parse::<u64>().ok()?, r),
        None => (0, lower.as_str()),
    };

    // release：开头连续的数字段
    let mut release = Vec::new();
    let mut idx = 0;
    let bytes = rest.as_bytes();
    loop {
        let start = idx;
        while idx < bytes.len() && bytes[idx].is_ascii_digit() {
            idx += 1;
        }
        if start == idx {
            break;
        }
        release.push(rest[start..idx].parse::<u64>().ok()?);
        if idx < bytes.len() && bytes[idx] == b'.' {
            // 后面还得是数字才是 release 的下一段
            if idx + 1 < bytes.len() && bytes[idx + 1].is_ascii_digit() {
                idx += 1;
                continue;
            }
        }
        break;
    }
    if release.is_empty() {
        return None;
    }

    let tail = &rest[idx..];
    let mut pre = None;
    let mut post = None;
    let mut dev = None;

    // 归一化分隔符：`-` / `_` 当 `.`；`1.0.0-alpha` -> `1.0.0.alpha`
    let mut tail = tail.replace(['-', '_'], ".");

    // .devN
    if let Some(pos) = tail.find("dev") {
        let after = &tail[pos + 3..];
        let digits: String = after.chars().take_while(char::is_ascii_digit).collect();
        dev = Some(digits.parse::<u64>().ok().unwrap_or(0));
        tail = tail[..pos].to_string();
    }
    // .postN
    if let Some(pos) = tail.find("post") {
        let after = &tail[pos + 4..];
        let digits: String = after.chars().take_while(char::is_ascii_digit).collect();
        post = Some(digits.parse::<u64>().ok().unwrap_or(0));
        tail = tail[..pos].to_string();
    }
    // pre：a / b / rc / alpha / beta / c / pre / preview
    for (needle, kind) in [
        ("alpha", PreKind::A),
        ("beta", PreKind::B),
        ("preview", PreKind::Rc),
        ("pre", PreKind::Rc),
        ("rc", PreKind::Rc),
        ("a", PreKind::A),
        ("b", PreKind::B),
        ("c", PreKind::Rc),
    ] {
        if let Some(pos) = tail.find(needle) {
            let after = &tail[pos + needle.len()..];
            // 标签与序号之间允许一个分隔点（`1.0.0-alpha.23` 归一化后是 `.alpha.23`），
            // 不剥掉的话序号取到空串，会被当成 0
            let after = after.strip_prefix('.').unwrap_or(after);
            let digits: String = after.chars().take_while(char::is_ascii_digit).collect();
            pre = Some((kind, digits.parse::<u64>().ok().unwrap_or(0)));
            break;
        }
    }

    Some(Pep440Version {
        epoch,
        release,
        pre,
        post,
        dev,
    })
}

/// PyPI JSON API 里我们关心的部分
#[derive(Debug, Deserialize)]
struct PypiPayload {
    info: Option<PypiInfo>,
    releases: Option<std::collections::BTreeMap<String, Vec<PypiFile>>>,
}

#[derive(Debug, Deserialize)]
struct PypiInfo {
    version: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PypiFile {
    #[serde(default)]
    yanked: bool,
}

/// 从 PyPI JSON 里挑出可安装的版本。纯函数：不联网，便于用固定报文钉住行为。
pub fn parse_versions(name: &str, body: &str) -> Result<PackageVersions, String> {
    let payload: PypiPayload =
        serde_json::from_str(body).map_err(|e| format!("PyPI 回包解析失败：{e}"))?;
    let releases = payload.releases.unwrap_or_default();

    let mut parsed: Vec<(Pep440Version, String)> = Vec::new();
    let mut unparseable: Vec<String> = Vec::new();
    let mut has_prerelease = false;
    for (raw, files) in &releases {
        // 每个文件都 yanked、或压根没有文件：都装不上
        if files.is_empty() || files.iter().all(|f| f.yanked) {
            continue;
        }
        match parse_pep440(raw) {
            Some(v) => {
                if v.is_prerelease() {
                    has_prerelease = true;
                }
                parsed.push((v, raw.clone()));
            }
            // 畸形拼写：仍然列出来让人能选，只是排最后
            None => unparseable.push(raw.clone()),
        }
    }
    parsed.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));

    let mut listed: Vec<String> = parsed.iter().map(|(_, raw)| raw.clone()).collect();
    unparseable.sort();
    unparseable.reverse();
    listed.extend(unparseable);

    // latest = 「按默认策略会装到哪个」= 最新**正式版**。
    //
    // PyPI 的 info.version 不能直接用：它是「最新版本」，会指向预发布
    // （例如只有 2.0.0a1 时它就给 2.0.0a1），而 uv 默认策略下装的是稳定版。
    // 所以只在 info.version 确实是正式版、且真在可装列表里时才采信；
    // 否则回落到最高的正式版；一个正式版都没有就是 None。
    let is_stable = |v: &str| parse_pep440(v).map(|p| !p.is_prerelease()).unwrap_or(false);
    let latest = payload
        .info
        .and_then(|i| i.version)
        .filter(|v| is_stable(v) && listed.iter().any(|l| l == v))
        .or_else(|| {
            parsed
                .iter()
                .find(|(v, _)| !v.is_prerelease())
                .map(|(_, raw)| raw.clone())
        });

    Ok(PackageVersions {
        name: name.to_string(),
        versions: listed,
        latest,
        has_prerelease,
    })
}

/// 拉某发行包的版本清单。`base` 便于测试注入（默认 pypi.org）。
pub async fn fetch_versions(name: &str, base: Option<&str>) -> Result<PackageVersions, String> {
    let base = base
        .unwrap_or("https://pypi.org/pypi")
        .trim_end_matches('/');
    let url = format!("{base}/{name}/json");
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("HTTP 客户端创建失败：{e}"))?;
    let resp = client
        .get(&url)
        .header("User-Agent", "NapCatQQ-Desktop")
        .send()
        .await
        .map_err(|e| format!("请求 PyPI 失败：{e}"))?;
    let status = resp.status();
    if !status.is_success() {
        return Err(format!("PyPI 返回 {status}"));
    }
    let body = resp
        .text()
        .await
        .map_err(|e| format!("读 PyPI 回包失败：{e}"))?;
    parse_versions(name, &body)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
      "info": { "version": "1.2.0" },
      "releases": {
        "1.0.0": [{ "yanked": false }],
        "1.0.0a7": [{ "yanked": false }],
        "1.2.0": [{ "yanked": false }],
        "1.2.1a1": [{ "yanked": false }],
        "1.1.0": [{ "yanked": false }],
        "1.0.1": [],
        "0.9.0": [{ "yanked": true }]
      }
    }"#;

    #[test]
    fn lists_versions_newest_first_and_skips_yanked() {
        let v = parse_versions("neobot-app", SAMPLE).unwrap();
        assert_eq!(v.name, "neobot-app");
        assert!(
            !v.versions.contains(&"0.9.0".to_string()),
            "整条 yank 的不能列"
        );
        assert!(
            !v.versions.contains(&"1.0.1".to_string()),
            "没有文件的残留不能列"
        );
        assert_eq!(
            v.versions,
            vec!["1.2.1a1", "1.2.0", "1.1.0", "1.0.0", "1.0.0a7"],
            "PEP 440 降序。注意两条方向相反、容易写反的规则：\
             1.2.1a1 > 1.2.0（预发布的 release 段更高，所以排在前面），\
             但 1.0.0a7 < 1.0.0（同一 release 段内预发布低于正式版，所以排在后面）"
        );
    }

    #[test]
    fn latest_is_the_stable_one_and_prerelease_is_flagged() {
        let v = parse_versions("neobot-app", SAMPLE).unwrap();
        assert_eq!(v.latest.as_deref(), Some("1.2.0"));
        assert!(v.has_prerelease);
    }

    #[test]
    fn latest_falls_back_when_info_version_is_not_installable() {
        let body = r#"{
          "info": { "version": "9.9.9" },
          "releases": { "1.0.0": [{ "yanked": false }], "2.0.0a1": [{ "yanked": false }] }
        }"#;
        let v = parse_versions("x", body).unwrap();
        assert_eq!(v.latest.as_deref(), Some("1.0.0"));
        assert!(v.has_prerelease);
    }

    /// PyPI 的 `info.version` 是「最新版本」，会指向预发布；而界面要的是
    /// 「默认会装到哪个」——按 uv 的 if-necessary 策略那是最新**正式版**，
    /// 只有预发布时必须为空，否则版本选择器会撒谎。
    #[test]
    fn only_prereleases_means_no_default_target() {
        let body = r#"{ "info": { "version": "2.0.0a1" },
          "releases": { "2.0.0a1": [{ "yanked": false }] } }"#;
        let v = parse_versions("x", body).unwrap();
        assert_eq!(v.latest, None, "只有预发布时不能说「默认装 2.0.0a1」");
        assert!(v.has_prerelease, "要能提示用户存在预发布");
    }

    #[test]
    fn malformed_body_is_an_error_not_a_panic() {
        assert!(parse_versions("x", "not json").is_err());
        assert!(parse_versions("x", "{}").unwrap().versions.is_empty());
    }

    /// PEP 440 排序：预发布 < 正式 < post；dev 最低；rc 高于 b 高于 a
    #[test]
    fn pep440_ordering_rules() {
        let order = |a: &str, b: &str| parse_pep440(a).unwrap().cmp(&parse_pep440(b).unwrap());
        assert_eq!(order("1.0.0a1", "1.0.0"), Ordering::Less);
        assert_eq!(order("1.0.0b1", "1.0.0a2"), Ordering::Greater);
        assert_eq!(order("1.0.0rc1", "1.0.0b9"), Ordering::Greater);
        assert_eq!(
            order("1.0.0rc1", "1.0.0"),
            Ordering::Less,
            "rc 仍低于正式版"
        );
        assert_eq!(order("1.0.0.post1", "1.0.0"), Ordering::Greater);
        assert_eq!(order("1.0.0.dev1", "1.0.0a1"), Ordering::Less, "dev 最低");
        assert_eq!(order("1.2", "1.2.0"), Ordering::Equal, "短的补 0");
        assert_eq!(
            order("1.10.0", "1.9.0"),
            Ordering::Greater,
            "按数字比不是按字符串"
        );
        assert_eq!(
            order("1.2.1a1", "1.2.0"),
            Ordering::Greater,
            "1.2.1a1 高于 1.2.0"
        );
        assert_eq!(order("1!1.0.0", "2.0.0"), Ordering::Greater, "epoch 优先");
    }

    /// dev 的方向：同形态下「有 dev」低于「无 dev」（PEP 440：1.0.0a1.dev1 < 1.0.0a1），
    /// dev 序号之间按数字升序。之前 sort key 取负把两个方向都排反了
    #[test]
    fn pep440_dev_orders_below_and_ascending() {
        let order = |a: &str, b: &str| parse_pep440(a).unwrap().cmp(&parse_pep440(b).unwrap());
        assert_eq!(order("1.0.0a1.dev1", "1.0.0a1"), Ordering::Less);
        assert_eq!(order("1.0.0.dev1", "1.0.0.dev2"), Ordering::Less);
        assert_eq!(order("1.0.0.dev2", "1.0.0.dev10"), Ordering::Less);
    }

    /// pre 标签后可以带一个分隔点：`1.0.0-alpha.23` 的序号是 23 不是 0
    #[test]
    fn pep440_pre_separator_keeps_the_number() {
        let v = parse_pep440("1.0.0-alpha.23").unwrap();
        assert_eq!(v.pre, Some((PreKind::A, 23)));
        let order = |a: &str, b: &str| parse_pep440(a).unwrap().cmp(&parse_pep440(b).unwrap());
        assert_eq!(order("1.0.0-alpha.2", "1.0.0-alpha.10"), Ordering::Less);
    }

    #[test]
    fn pep440_accepts_upstream_spellings() {
        for raw in [
            "1.2.0",
            "1.2.1a1",
            "1.0.0a7",
            "2.0.0rc1",
            "1.0.0.post1",
            "1.0.0.dev1",
            "1.0.0-alpha.23",
            "1.2",
        ] {
            assert!(parse_pep440(raw).is_some(), "{raw} 应该能解析");
        }
        assert!(parse_pep440("").is_none());
        assert!(parse_pep440("weird-version").is_none());
    }

    /// 真机联网冒烟：默认 #[ignore]，避免 CI 依赖网络。
    /// 手动跑：cargo test -p ncd-appframework --lib fetch_neobot_app -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn fetch_neobot_app_from_pypi() {
        let v = fetch_versions("neobot-app", None).await.expect("查 PyPI");
        println!(
            "neobot-app: latest={:?} count={} has_prerelease={}",
            v.latest,
            v.versions.len(),
            v.has_prerelease
        );
        assert!(!v.versions.is_empty(), "至少有历史版本");
        assert!(v.latest.is_some(), "应该有一个正式版");
        // 版本号必须是我们自己的 PEP 440 解析器认得的东西
        for raw in &v.versions {
            assert!(parse_pep440(raw).is_some(), "{raw} 解析不了");
        }
    }

    #[test]
    fn non_pep440_spelling_still_listed_but_last() {
        let body = r#"{ "info": {}, "releases": { "1.0.0": [{}], "weird-version": [{}] } }"#;
        let v = parse_versions("x", body).unwrap();
        assert_eq!(
            v.versions,
            vec!["1.0.0", "weird-version"],
            "畸形的排最后但仍可选"
        );
    }
}
