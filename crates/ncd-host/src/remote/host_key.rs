//! Host key 校验策略
//!
//! 首次连接未知主机的处理由 HostKeyPolicy 控制:
//! - Strict { known_hosts_path }:严格模式,不在 known_hosts 里就拒
//! - Insecure:测试 / 容器 / 同 LAN 受信网络专用,生产禁用
//! - AcceptOnFirstUse:未知主机先拒绝并返回可确认错误,上层确认后再写入 known_hosts

use std::path::PathBuf;
use tokio::fs;

use crate::error::HostError;

/// Host key 校验策略
#[derive(Debug, Clone)]
pub enum HostKeyPolicy {
    /// 严格模式:必须在 known_hosts 中找到匹配
    Strict { known_hosts_path: PathBuf },
    /// 不校验(测试 / 受信网络专用,生产禁用)
    Insecure,
    /// 首次接受 + 持久化未知主机会返回 HostKeyUnknown,等待 UI 确认后写入
    AcceptOnFirstUse { known_hosts_path: PathBuf },
}

impl HostKeyPolicy {
    pub fn strict(known_hosts_path: impl Into<PathBuf>) -> Self {
        Self::Strict {
            known_hosts_path: known_hosts_path.into(),
        }
    }

    /// 默认的用户级 known_hosts(<data_root>/secrets/known_hosts)
    pub fn strict_in(data_root: &std::path::Path) -> Self {
        Self::Strict {
            known_hosts_path: data_root.join("secrets").join("known_hosts"),
        }
    }
    /// TOFU 模式的用户级 known_hosts(<data_root>/secrets/known_hosts)
    pub fn accept_on_first_use_in(data_root: &std::path::Path) -> Self {
        Self::AcceptOnFirstUse {
            known_hosts_path: data_root.join("secrets").join("known_hosts"),
        }
    }
}

/// known_hosts 查询结果未知与不匹配必须拆开:未知可走 TOFU 确认,不匹配应阻断
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostKeyCheck {
    Match,
    Unknown,
    Mismatch,
}

/// 解析 OpenSSH 风格 known_hosts 文件
///
/// 主机字段按 [known_hosts_host_matches] 比对,不支持 hostname hash(|1|...|...)
/// 与 wildcard:这份文件只由应用自己写入(format_host),不复用 OpenSSH 的历史文件
pub struct KnownHostsStore {
    path: PathBuf,
}

impl KnownHostsStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// 检查 host:port 是否有匹配条目,并区分未知主机与同主机 key 不一致
    /// 文件不存在视作未知主机
    pub async fn check(
        &self,
        host: &str,
        port: u16,
        key_kind: &str,
        key_b64: &str,
    ) -> Result<HostKeyCheck, HostError> {
        let content = match fs::read_to_string(&self.path).await {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(HostKeyCheck::Unknown),
            Err(e) => return Err(HostError::Io(e)),
        };

        // 同一主机多种算法(ed25519 / rsa / ecdsa)是 OpenSSH 常态。
        // 只有「同算法、不同公钥」才算 mismatch；别的算法当未知，允许再记一条。
        let mut saw_same_kind = false;
        for line in content.lines().filter_map(parse_known_hosts_line) {
            // 应用自己的文件不写 @cert-authority / @revoked,有也不当普通条目
            if line.marker.is_some() || !known_hosts_host_matches(line.hosts, host, port) {
                continue;
            }
            if line.kind == key_kind && line.key_b64 == key_b64 {
                return Ok(HostKeyCheck::Match);
            }
            if line.kind == key_kind {
                saw_same_kind = true;
            }
        }

        if saw_same_kind {
            Ok(HostKeyCheck::Mismatch)
        } else {
            Ok(HostKeyCheck::Unknown)
        }
    }

    /// 兼容旧调用:只有完全匹配才返回 true
    pub async fn matches(
        &self,
        host: &str,
        port: u16,
        key_kind: &str,
        key_b64: &str,
    ) -> Result<bool, HostError> {
        Ok(matches!(
            self.check(host, port, key_kind, key_b64).await?,
            HostKeyCheck::Match
        ))
    }

    /// 把新条目追加到 known_hosts(AcceptOnFirstUse 用,实装时调用)
    pub async fn append(
        &self,
        host: &str,
        port: u16,
        key_kind: &str,
        key_b64: &str,
    ) -> Result<(), HostError> {
        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                fs::create_dir_all(parent).await?;
            }
        }
        let line = format!("{} {} {}\n", format_host(host, port), key_kind, key_b64);
        let mut existing = match fs::read(&self.path).await {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(HostError::Io(e)),
        };
        existing.extend_from_slice(line.as_bytes());
        fs::write(&self.path, existing).await?;
        Ok(())
    }
}

fn format_host(host: &str, port: u16) -> String {
    if port == 22 {
        host.to_string()
    } else {
        format!("[{host}]:{port}")
    }
}

/// known_hosts 里的一行:`[@marker] <host[,host2]> <key-type> <base64-key> [comment]`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KnownHostsLine<'a> {
    /// `@cert-authority` / `@revoked`,普通行是 None
    pub marker: Option<&'a str>,
    pub hosts: &'a str,
    pub kind: &'a str,
    pub key_b64: &'a str,
}

/// 空行、注释、缺字段的行是 None
pub fn parse_known_hosts_line(raw: &str) -> Option<KnownHostsLine<'_>> {
    let line = raw.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let mut parts = line.split_whitespace();
    let first = parts.next()?;
    let (marker, hosts) = if first.starts_with('@') {
        (Some(first), parts.next()?)
    } else {
        (None, first)
    };
    Some(KnownHostsLine {
        marker,
        hosts,
        kind: parts.next()?,
        key_b64: parts.next()?,
    })
}

/// 一行的主机字段是否覆盖 host:port。照 ssh 的规矩:主机名不分大小写,
/// 22 端口认裸主机名、别的端口只认 `[host]:port`,同一行写了 `!host` 就不算。
/// 裸主机名要是也认别的端口,同一 IP 转发出去的几台机器会互相顶成「指纹变了」
pub fn known_hosts_host_matches(field: &str, host: &str, port: u16) -> bool {
    if host.is_empty() {
        return false;
    }
    let bracketed = format!("[{host}]:{port}");
    let is_this_host = |name: &str| {
        name.eq_ignore_ascii_case(&bracketed) || (port == 22 && name.eq_ignore_ascii_case(host))
    };
    let mut matched = false;
    for part in field.split(',').map(str::trim) {
        if let Some(negated) = part.strip_prefix('!') {
            if is_this_host(negated) {
                return false;
            }
        } else if is_this_host(part) {
            matched = true;
        }
    }
    matched
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn matches_returns_false_when_file_missing() {
        let dir = tempdir().unwrap();
        let store = KnownHostsStore::new(dir.path().join("nonexistent"));
        let m = store
            .matches("example.com", 22, "ssh-ed25519", "AAAA")
            .await
            .unwrap();
        assert!(!m);
    }

    #[tokio::test]
    async fn matches_finds_exact_entry_default_port() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("known_hosts");
        let content = "example.com ssh-ed25519 AAAAB3keyhere\n";
        fs::write(&path, content).await.unwrap();
        let store = KnownHostsStore::new(&path);
        assert!(
            store
                .matches("example.com", 22, "ssh-ed25519", "AAAAB3keyhere")
                .await
                .unwrap()
        );
        // 不同 key b64 不匹配
        assert!(
            !store
                .matches("example.com", 22, "ssh-ed25519", "AAAAdifferent")
                .await
                .unwrap()
        );
        // 不同 key 类型不匹配
        assert!(
            !store
                .matches("example.com", 22, "ssh-rsa", "AAAAB3keyhere")
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn matches_handles_nonstandard_port() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("known_hosts");
        fs::write(&path, "[example.com]:2222 ssh-ed25519 AAAAkey\n")
            .await
            .unwrap();
        let store = KnownHostsStore::new(&path);
        assert!(
            store
                .matches("example.com", 2222, "ssh-ed25519", "AAAAkey")
                .await
                .unwrap()
        );
        assert!(
            !store
                .matches("example.com", 22, "ssh-ed25519", "AAAAkey")
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn bare_port_22_entry_says_nothing_about_other_ports() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("known_hosts");
        fs::write(&path, "1.2.3.4 ssh-ed25519 AAAAport22\n")
            .await
            .unwrap();
        let store = KnownHostsStore::new(&path);
        // 同一 IP 转发出去的另一台机器:不能拿 22 端口那条判成指纹变了
        assert_eq!(
            store
                .check("1.2.3.4", 10022, "ssh-ed25519", "AAAAother")
                .await
                .unwrap(),
            HostKeyCheck::Unknown
        );
        assert_eq!(
            store
                .check("1.2.3.4", 22, "ssh-ed25519", "AAAAport22")
                .await
                .unwrap(),
            HostKeyCheck::Match
        );
    }

    #[test]
    fn host_field_ignores_case_and_honours_negation() {
        assert!(known_hosts_host_matches("Box.Example", "box.example", 22));
        assert!(known_hosts_host_matches(
            "[Box.Example]:2222",
            "box.example",
            2222
        ));
        assert!(!known_hosts_host_matches(
            "box.example",
            "box.example",
            2222
        ));
        assert!(!known_hosts_host_matches(
            "box.example,!box.example",
            "box.example",
            22
        ));
    }

    #[test]
    fn line_parser_splits_marker() {
        let line = parse_known_hosts_line("@revoked * ssh-ed25519 AAAA c").unwrap();
        assert_eq!(line.marker, Some("@revoked"));
        assert_eq!(line.hosts, "*");
        assert_eq!(line.kind, "ssh-ed25519");
        assert_eq!(line.key_b64, "AAAA");
        assert!(parse_known_hosts_line("  # x").is_none());
        assert!(parse_known_hosts_line("host ssh-ed25519").is_none());
    }

    #[tokio::test]
    async fn matches_skips_comments() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("known_hosts");
        let content = "# comment line\n\nexample.com ssh-ed25519 AAAAkey\n";
        fs::write(&path, content).await.unwrap();
        let store = KnownHostsStore::new(&path);
        assert!(
            store
                .matches("example.com", 22, "ssh-ed25519", "AAAAkey")
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn check_other_algorithm_is_unknown_not_mismatch() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("known_hosts");
        fs::write(&path, "example.com ssh-ed25519 AAAAed\n")
            .await
            .unwrap();
        let store = KnownHostsStore::new(&path);
        assert_eq!(
            store
                .check("example.com", 22, "ssh-rsa", "AAAArsa")
                .await
                .unwrap(),
            HostKeyCheck::Unknown
        );
        assert_eq!(
            store
                .check("example.com", 22, "ssh-ed25519", "AAAAother")
                .await
                .unwrap(),
            HostKeyCheck::Mismatch
        );
    }

    #[tokio::test]
    async fn append_creates_file_with_entry() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("subdir/known_hosts");
        let store = KnownHostsStore::new(&path);
        store
            .append("example.com", 22, "ssh-ed25519", "AAAAkey")
            .await
            .unwrap();
        let content = fs::read_to_string(&path).await.unwrap();
        assert!(content.contains("example.com ssh-ed25519 AAAAkey"));
        // 新加的条目应能 match
        assert!(
            store
                .matches("example.com", 22, "ssh-ed25519", "AAAAkey")
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn matches_finds_multi_host_entry() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("known_hosts");
        // 多 host 用逗号分隔(OpenSSH 风格)
        fs::write(
            &path,
            "alpha.example.com,beta.example.com ssh-ed25519 AAAAkey\n",
        )
        .await
        .unwrap();
        let store = KnownHostsStore::new(&path);
        assert!(
            store
                .matches("alpha.example.com", 22, "ssh-ed25519", "AAAAkey")
                .await
                .unwrap()
        );
        assert!(
            store
                .matches("beta.example.com", 22, "ssh-ed25519", "AAAAkey")
                .await
                .unwrap()
        );
        assert!(
            !store
                .matches("gamma.example.com", 22, "ssh-ed25519", "AAAAkey")
                .await
                .unwrap()
        );
    }
}
