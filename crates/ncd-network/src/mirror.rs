//! GitHub 镜像列表与 URL 改写
//!
//! build_mirror_urls 把原始 GitHub URL 展开成 6+1 候选(直连 + 6 镜像)镜像前缀
//! 是 reverse-proxy 模式(完整原始 URL 加在前缀后面,如 https://gh-proxy.com/https://github.com/...)
//! 不是所有镜像都支持 github.com / raw.githubusercontent.com / objects.githubusercontent.com,
//! race 阶段失败的 mirror 自然淘汰

/// 内置候选镜像前缀(不含末尾 /)空串表示直连前两个最稳,作为 race 初始 racer
pub const DEFAULT_MIRROR_PREFIXES: &[&str] = &[
    "",                          // 0. 直连
    "https://gh.ddlc.top",       // 1. ddlc(国内带宽最优)
    "https://gh-proxy.com",      // 2. gh-proxy
    "https://ghfast.top",        // 3. ghfast
    "https://cors.isteed.cc",    // 4. isteed
    "https://ghproxy.cc",        // 5. ghproxy
    "https://github.akams.cn",   // 6. akams
];

/// 把原始 URL 展开成 race 用的镜像 URL 列表prefixes None 时用 DEFAULT_MIRROR_PREFIXES
/// 直连(空串前缀)在最前,race 失败后 fallback 到直连
pub fn build_mirror_urls(original: &str, prefixes: Option<&[&str]>) -> Vec<String> {
    let prefixes = prefixes.unwrap_or(DEFAULT_MIRROR_PREFIXES);
    let mut out = Vec::with_capacity(prefixes.len());
    for p in prefixes {
        if p.is_empty() {
            out.push(original.to_string());
        } else {
            out.push(format!("{}/{original}", p.trim_end_matches('/')));
        }
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
}
