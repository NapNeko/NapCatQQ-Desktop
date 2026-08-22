//! apt / dnf / yum 安装过程 stdout 行解析,供流式进度与日志摘要使用
//!
//! Debian 在非 TTY 下多为 Get: / Fetched / Setting up;RHEL 系为
//! Downloading / Installing / Complete解析只做启发式,不追求精确包计数

/// 包管理器输出族(由行内容推断,不依赖事先知道是 apt 还是 dnf)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PkgMgrFamily {
    Apt,
    Dnf,
    Other,
}

/// 粗粒度阶段,用于映射建议进度百分比
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PkgPhase {
    UpdateIndex,
    Fetch,
    Unpack,
    Configure,
    Error,
    Noise,
    Other,
}

/// 单行解析结果
#[derive(Debug, Clone)]
pub struct PkgLineParse {
    pub family: PkgMgrFamily,
    pub phase: PkgPhase,
    /// 给 UI / 任务队列的短摘要(已截断)
    pub summary: String,
    /// 建议进度 0–100;None 表示只记日志,不推高百分比
    pub suggest_percent: Option<u8>,
}

/// 截断过长行,避免任务队列 / InfoBar 撑爆
pub fn truncate_pkg_line(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max_chars).collect();
    out.push('…');
    out
}

/// 解析一行包管理器输出空行返回 None
pub fn parse_pkg_mgr_line(line: &str) -> Option<PkgLineParse> {
    let t = line.trim();
    if t.is_empty() {
        return None;
    }

    let lower = t.to_ascii_lowercase();

    if t.starts_with("NCD:") {
        let msg = t.strip_prefix("NCD:").unwrap_or(t).trim();
        return Some(PkgLineParse {
            family: PkgMgrFamily::Apt,
            phase: PkgPhase::Other,
            summary: truncate_pkg_line(msg, 160),
            suggest_percent: None,
        });
    }

    if t.starts_with("E:") || t.starts_with("Err:") {
        return Some(PkgLineParse {
            family: PkgMgrFamily::Apt,
            phase: PkgPhase::Error,
            summary: truncate_pkg_line(t, 200),
            suggest_percent: None,
        });
    }

    if t.starts_with("Get:") || t.starts_with("Ign:") || t.starts_with("Hit:") {
        let pct = if t.starts_with("Get:") {
            Some(18u8)
        } else {
            Some(12)
        };
        return Some(PkgLineParse {
            family: PkgMgrFamily::Apt,
            phase: if t.starts_with("Ign:") {
                PkgPhase::Noise
            } else {
                PkgPhase::Fetch
            },
            summary: truncate_pkg_line(t, 160),
            suggest_percent: pct,
        });
    }

    if t.starts_with("Fetched") || lower.contains("reading package lists") {
        return Some(PkgLineParse {
            family: PkgMgrFamily::Apt,
            phase: PkgPhase::Fetch,
            summary: truncate_pkg_line(t, 160),
            suggest_percent: Some(42),
        });
    }

    if lower.contains("unpacking ") || t.contains("Unpacking ") {
        return Some(PkgLineParse {
            family: PkgMgrFamily::Apt,
            phase: PkgPhase::Unpack,
            summary: truncate_pkg_line(t, 160),
            suggest_percent: Some(68),
        });
    }

    if lower.contains("setting up ") || t.contains("Setting up ") {
        return Some(PkgLineParse {
            family: PkgMgrFamily::Apt,
            phase: PkgPhase::Configure,
            summary: truncate_pkg_line(t, 160),
            suggest_percent: Some(82),
        });
    }

    if lower.contains("preparing to unpack") || lower.contains("processing triggers") {
        return Some(PkgLineParse {
            family: PkgMgrFamily::Apt,
            phase: PkgPhase::Unpack,
            summary: truncate_pkg_line(t, 160),
            suggest_percent: Some(60),
        });
    }

    if lower.contains("metadata cache created")
        || lower.contains("updating subscription management")
        || lower.contains("determining fastest mirrors")
    {
        return Some(PkgLineParse {
            family: PkgMgrFamily::Dnf,
            phase: PkgPhase::UpdateIndex,
            summary: truncate_pkg_line(t, 160),
            suggest_percent: Some(15),
        });
    }

    if lower.contains("downloading packages") || lower.starts_with("downloading ") {
        return Some(PkgLineParse {
            family: PkgMgrFamily::Dnf,
            phase: PkgPhase::Fetch,
            summary: truncate_pkg_line(t, 160),
            suggest_percent: Some(35),
        });
    }

    if lower.contains("installing:")
        || lower.contains("install  ")
        || (lower.contains("installing ") && !lower.contains("installing group"))
    {
        return Some(PkgLineParse {
            family: PkgMgrFamily::Dnf,
            phase: PkgPhase::Configure,
            summary: truncate_pkg_line(t, 160),
            suggest_percent: Some(72),
        });
    }

    if lower.contains("complete!") || t == "Complete." {
        return Some(PkgLineParse {
            family: PkgMgrFamily::Dnf,
            phase: PkgPhase::Other,
            summary: truncate_pkg_line(t, 120),
            suggest_percent: Some(90),
        });
    }

    if lower.contains("error:")
        || lower.contains("cannot find")
        || lower.contains("no package")
        || lower.contains("failed to download")
    {
        return Some(PkgLineParse {
            family: PkgMgrFamily::Dnf,
            phase: PkgPhase::Error,
            summary: truncate_pkg_line(t, 200),
            suggest_percent: None,
        });
    }

    if t.contains("docker-ce") || t.contains("docker-compose-plugin") {
        return Some(PkgLineParse {
            family: PkgMgrFamily::Other,
            phase: PkgPhase::Configure,
            summary: truncate_pkg_line(t, 160),
            suggest_percent: Some(58),
        });
    }

    None
}

/// 结合行号做兜底百分比(与 [parse_pkg_mgr_line] 未命中时兼容旧逻辑)
pub fn fallback_percent_from_line_no(line_no: u32, line: &str) -> u8 {
    if let Some(p) = parse_pkg_mgr_line(line) {
        if let Some(pct) = p.suggest_percent {
            return pct;
        }
    }
    8 + (line_no % 40).min(25) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apt_get_line() {
        let p = parse_pkg_mgr_line(
            "Get:1 http://archive.ubuntu.com/ubuntu jammy/main amd64 xvfb amd64 2:21.1.4-2ubuntu1 [866 kB]",
        )
        .unwrap();
        assert_eq!(p.family, PkgMgrFamily::Apt);
        assert_eq!(p.phase, PkgPhase::Fetch);
        assert!(p.suggest_percent.unwrap() >= 10);
    }

    #[test]
    fn apt_setting_up() {
        let p = parse_pkg_mgr_line("Setting up novnc (1:1.0.0-3) ...").unwrap();
        assert_eq!(p.phase, PkgPhase::Configure);
        assert!(p.suggest_percent.unwrap() > 70);
    }

    #[test]
    fn dnf_installing() {
        let p = parse_pkg_mgr_line("Installing: docker-ce;docker-ce-cli").unwrap();
        assert_eq!(p.family, PkgMgrFamily::Dnf);
        assert!(p.suggest_percent.unwrap() > 50);
    }

    #[test]
    fn empty_is_none() {
        assert!(parse_pkg_mgr_line("   ").is_none());
    }
}
