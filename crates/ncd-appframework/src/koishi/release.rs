//! 装哪个整包：boilerplate 每个 Release 按平台出附件，但不是每版都齐（v1.16.1 就没打出 Windows 包）。
//! 从新到旧找第一个带本平台附件的正式版；一个都没有、或 API 不通，就装内置版本。
//!
//! 附件里只有 JS（没有原生模块），平台差别只在 `node_modules/.bin` 的 shim 上，
//! 本平台没附件时退一步拿别的平台的也能跑：桌面端起停和装插件都走 yarn，不碰 `.bin`。

use std::time::Duration;

use ncd_host::{Arch, Os};
use serde::Deserialize;

use super::manifest::{BOILERPLATE_REPO, PINNED_LINUX_TAG, PINNED_NODE_MAJOR, PINNED_WINDOWS_TAG};

const API_TIMEOUT: Duration = Duration::from_secs(10);

/// 附件名里的平台段（`windows-amd64` / `linux-arm64`）
pub fn platform_slug(os: Os, arch: Arch) -> &'static str {
    match (os, arch) {
        (Os::Windows, _) => "windows-amd64",
        (_, Arch::Aarch64) => "linux-arm64",
        _ => "linux-amd64",
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoilerplateAsset {
    pub tag: String,
    pub name: String,
    pub url: String,
}

#[derive(Deserialize)]
struct ReleaseRow {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<AssetRow>,
}

#[derive(Deserialize)]
struct AssetRow {
    name: String,
    browser_download_url: String,
}

fn asset_matches(name: &str, tag: &str, slug: &str) -> bool {
    name.starts_with(&format!("boilerplate-{tag}-{slug}-node")) && name.ends_with(".zip")
}

/// 从 Release 列表（新的在前）挑附件：先找本平台，整张表都没有再找任意平台
pub fn pick_asset(json: &str, slug: &str) -> Result<Option<BoilerplateAsset>, String> {
    let rows: Vec<ReleaseRow> =
        serde_json::from_str(json).map_err(|e| format!("解析 Release 列表失败: {e}"))?;
    let stable: Vec<&ReleaseRow> = rows
        .iter()
        .filter(|r| !r.draft && !r.prerelease && !r.tag_name.trim().is_empty())
        .collect();
    let find = |want: Option<&str>| {
        stable.iter().find_map(|r| {
            r.assets
                .iter()
                .find(|a| match want {
                    Some(slug) => asset_matches(&a.name, &r.tag_name, slug),
                    None => {
                        a.name.starts_with(&format!("boilerplate-{}-", r.tag_name))
                            && a.name.ends_with(".zip")
                    }
                })
                .map(|a| BoilerplateAsset {
                    tag: r.tag_name.clone(),
                    name: a.name.clone(),
                    url: a.browser_download_url.clone(),
                })
        })
    };
    Ok(find(Some(slug)).or_else(|| find(None)))
}

/// 内置版本的附件地址（不查 API 直接拼）
pub fn pinned_asset(slug: &str) -> BoilerplateAsset {
    let tag = if slug.starts_with("windows") {
        PINNED_WINDOWS_TAG
    } else {
        PINNED_LINUX_TAG
    };
    let name = format!("boilerplate-{tag}-{slug}-node{PINNED_NODE_MAJOR}.zip");
    BoilerplateAsset {
        tag: tag.to_string(),
        url: format!("https://github.com/{BOILERPLATE_REPO}/releases/download/{tag}/{name}"),
        name,
    }
}

/// 直连 api.github.com（国内中转只认它登记过的仓库）；拿不到就交给调用方回落
pub async fn fetch_latest_asset(slug: &str) -> Result<Option<BoilerplateAsset>, String> {
    let url = format!("https://api.github.com/repos/{BOILERPLATE_REPO}/releases?per_page=10");
    let resp = ncd_network::shared_client()
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .timeout(API_TIMEOUT)
        .send()
        .await
        .map_err(|e| format!("请求 Koishi 整包的 Release 失败: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!(
            "请求 Koishi 整包的 Release 失败: HTTP {}",
            resp.status()
        ));
    }
    let text = resp
        .text()
        .await
        .map_err(|e| format!("读取 Koishi 整包的 Release 失败: {e}"))?;
    pick_asset(&text, slug)
}

#[cfg(test)]
mod tests {
    use super::*;

    const RELEASES: &str = r#"[
        {"tag_name":"v1.17.0-beta","prerelease":true,"assets":[
            {"name":"boilerplate-v1.17.0-beta-windows-amd64-node22.zip","browser_download_url":"https://x/beta.zip"}]},
        {"tag_name":"v1.16.1","assets":[
            {"name":"boilerplate-v1.16.1-darwin-amd64-node20.zip","browser_download_url":"https://x/161-darwin.zip"},
            {"name":"boilerplate-v1.16.1-linux-amd64-node20.zip","browser_download_url":"https://x/161-linux.zip"},
            {"name":"boilerplate-v1.16.1-linux-arm64-node20.zip","browser_download_url":"https://x/161-arm.zip"}]},
        {"tag_name":"v1.16.0","assets":[
            {"name":"boilerplate-v1.16.0-linux-amd64-node20.zip","browser_download_url":"https://x/160-linux.zip"},
            {"name":"boilerplate-v1.16.0-windows-amd64-node20.zip","browser_download_url":"https://x/160-win.zip"}]}
    ]"#;

    #[test]
    fn picks_newest_release_that_has_this_platform() {
        let win = pick_asset(RELEASES, "windows-amd64").unwrap().unwrap();
        assert_eq!(win.tag, "v1.16.0", "v1.16.1 没有 Windows 包，预发布不算");
        assert_eq!(win.url, "https://x/160-win.zip");
        let arm = pick_asset(RELEASES, "linux-arm64").unwrap().unwrap();
        assert_eq!(arm.tag, "v1.16.1");
        assert_eq!(arm.name, "boilerplate-v1.16.1-linux-arm64-node20.zip");
    }

    #[test]
    fn falls_back_to_any_platform_then_to_none() {
        let only_linux = r#"[{"tag_name":"v2.0.0","assets":[
            {"name":"boilerplate-v2.0.0-linux-amd64-node22.zip","browser_download_url":"https://x/l.zip"}]}]"#;
        let got = pick_asset(only_linux, "windows-amd64").unwrap().unwrap();
        assert_eq!(got.url, "https://x/l.zip");
        assert_eq!(pick_asset("[]", "windows-amd64").unwrap(), None);
        assert!(pick_asset("{", "windows-amd64").is_err());
    }

    #[test]
    fn pinned_assets_follow_known_good_tags() {
        let win = pinned_asset(platform_slug(Os::Windows, Arch::X86_64));
        assert_eq!(
            win.url,
            "https://github.com/koishijs/boilerplate/releases/download/v1.16.0/boilerplate-v1.16.0-windows-amd64-node20.zip"
        );
        let arm = pinned_asset(platform_slug(Os::Linux, Arch::Aarch64));
        assert_eq!(arm.name, "boilerplate-v1.16.1-linux-arm64-node20.zip");
    }
}
