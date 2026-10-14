//! 公共下载源的镜像列表与 URL 改写
//!
//! GitHub URL 展开成直连 + GitHub 镜像，Node 官方 dist 使用 Node 二进制镜像。镜像前缀
//! 是 reverse-proxy 模式(完整原始 URL 加在前缀后面,如 https://gh-proxy.com/https://github.com/...)
//! 仅为已知公共主机展开，不把认证、签名链接或其它下载源转发给第三方。

use reqwest::Url;

/// 内置候选镜像前缀(不含末尾 /)，空串表示直连，实际选择由吞吐量测速决定。
pub const DEFAULT_MIRROR_PREFIXES: &[&str] = &[
    "",                        // 0. 直连
    "https://gh.ddlc.top",     // 1. ddlc
    "https://gh-proxy.com",    // 2. gh-proxy
    "https://ghfast.top",      // 3. ghfast
    "https://cors.isteed.cc",  // 4. isteed
    "https://ghproxy.cc",      // 5. ghproxy
    "https://github.akams.cn", // 6. akams
];

const NODE_BINARY_MIRROR: &str = "https://registry.npmmirror.com/-/binary/node";

/// 展开已知公共下载源，保留候选顺序并去重。
///
/// 默认按主机选择 GitHub / Node 镜像；显式 prefixes 仍按反代前缀拼接，
/// 非法前缀被跳过。未知主机、认证或签名 URL 只返回直连地址。
pub fn build_mirror_urls(original: &str, prefixes: Option<&[&str]>) -> Vec<String> {
    let Ok(url) = Url::parse(original) else {
        return vec![original.to_string()];
    };
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.port().is_some()
    {
        return vec![original.to_string()];
    }

    let is_github = matches!(
        url.host_str(),
        Some(
            "github.com"
                | "raw.githubusercontent.com"
                | "objects.githubusercontent.com"
                | "release-assets.githubusercontent.com"
                | "codeload.github.com"
        )
    );
    let node_dist_path = (url.host_str() == Some("nodejs.org"))
        .then(|| url.path().strip_prefix("/dist/"))
        .flatten()
        .filter(|path| !path.is_empty());
    if !is_github && node_dist_path.is_none() {
        return vec![original.to_string()];
    }

    if prefixes.is_none() {
        if let Some(path) = node_dist_path {
            return vec![original.to_string(), format!("{NODE_BINARY_MIRROR}/{path}")];
        }
    }

    let prefixes = prefixes.unwrap_or(DEFAULT_MIRROR_PREFIXES);
    let mut out = Vec::with_capacity(prefixes.len());
    for prefix in prefixes {
        let prefix = prefix.trim();
        let candidate = if prefix.is_empty() {
            original.to_string()
        } else {
            let Ok(prefix_url) = Url::parse(prefix) else {
                continue;
            };
            if !matches!(prefix_url.scheme(), "http" | "https")
                || prefix_url.host_str().is_none()
                || !prefix_url.username().is_empty()
                || prefix_url.password().is_some()
                || prefix_url.query().is_some()
                || prefix_url.fragment().is_some()
            {
                continue;
            }
            format!("{}/{original}", prefix_url.as_str().trim_end_matches('/'))
        };
        if !out.contains(&candidate) {
            out.push(candidate);
        }
    }
    if out.is_empty() {
        out.push(original.to_string());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_mirror_urls_uses_default_when_none() {
        let urls = build_mirror_urls("https://github.com/foo/bar/releases/x.zip", None);
        assert_eq!(urls.len(), DEFAULT_MIRROR_PREFIXES.len());
        assert_eq!(urls[0], "https://github.com/foo/bar/releases/x.zip");
        assert!(urls[1].starts_with("https://gh.ddlc.top/https://github.com/"));
    }

    #[test]
    fn build_mirror_urls_strips_trailing_slash() {
        let urls = build_mirror_urls(
            "https://github.com/x.zip",
            Some(&["", "https://m.com/", "https://n.com"]),
        );
        assert_eq!(urls[0], "https://github.com/x.zip");
        assert_eq!(urls[1], "https://m.com/https://github.com/x.zip");
        assert_eq!(urls[2], "https://n.com/https://github.com/x.zip");
    }

    #[test]
    fn node_dist_uses_node_binary_mirror() {
        let original = "https://nodejs.org/dist/v22.15.0/node-v22.15.0-win-x64.zip";
        assert_eq!(
            build_mirror_urls(original, None),
            vec![
                original,
                "https://registry.npmmirror.com/-/binary/node/v22.15.0/node-v22.15.0-win-x64.zip"
            ]
        );
    }

    #[test]
    fn private_signed_and_unrecognized_sources_stay_direct() {
        for original in [
            "https://github.com.evil.example/archive.zip",
            "https://github.com@private.example/archive.zip",
            "https://private.example/archive.zip",
            "https://127.0.0.1/archive.zip",
            "https://nodejs.org/download/archive.zip",
            "https://user:secret@github.com/foo/bar/releases/x.zip",
            "https://github.com/foo/bar/releases/x.zip?token=secret",
            "https://objects.githubusercontent.com/payload?X-Amz-Signature=secret",
            "https://github.com/foo/bar/releases/x.zip#secret",
            "https://github.com:8443/foo/bar/releases/x.zip",
        ] {
            assert_eq!(build_mirror_urls(original, None), vec![original]);
            assert_eq!(
                build_mirror_urls(original, Some(&["https://mirror.example"])),
                vec![original]
            );
        }
    }

    #[test]
    fn invalid_prefixes_are_skipped_and_equivalent_candidates_deduplicated() {
        let original = "https://github.com/foo/bar/releases/x.zip";
        assert_eq!(
            build_mirror_urls(
                original,
                Some(&[
                    "",
                    " ",
                    "https://mirror.example/",
                    "https://mirror.example",
                    "https://mirror.example?token=secret",
                    "https://user:secret@mirror.example",
                    "file:///tmp/mirror",
                    "not a URL",
                ])
            ),
            vec![
                original.to_string(),
                format!("https://mirror.example/{original}")
            ]
        );
        assert_eq!(
            build_mirror_urls(original, Some(&["not a URL"])),
            vec![original]
        );
    }
}
