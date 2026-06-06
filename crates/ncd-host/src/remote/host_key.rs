//! Host key 校验策略。
//!
//! 首次连接未知主机的处理由 `HostKeyPolicy` 控制:
//! - `Strict { known_hosts_path }`:严格模式,不在 known_hosts 里就拒
//! - `Insecure`:测试 / 容器 / 同 LAN 受信网络专用,生产禁用
//! - `AcceptOnFirstUse`:未知主机先拒绝并返回可确认错误,上层确认后再写入 known_hosts

use std::path::PathBuf;
use tokio::fs;

use crate::error::HostError;

/// Host key 校验策略。
#[derive(Debug, Clone)]
pub enum HostKeyPolicy {
    /// 严格模式:必须在 known_hosts 中找到匹配
    Strict { known_hosts_path: PathBuf },
    /// 不校验(测试 / 受信网络专用,生产禁用)
    Insecure,
    /// 首次接受 + 持久化。未知主机会返回 HostKeyUnknown，等待 UI 确认后写入。
    AcceptOnFirstUse { known_hosts_path: PathBuf },
}

impl HostKeyPolicy {
    pub fn strict(known_hosts_path: impl Into<PathBuf>) -> Self {
        Self::Strict {
            known_hosts_path: known_hosts_path.into(),
        }
    }

    /// 默认的用户级 known_hosts(`<data_root>/secrets/known_hosts`)
    pub fn strict_in(data_root: &std::path::Path) -> Self {
        Self::Strict {
            known_hosts_path: data_root.join("secrets").join("known_hosts"),
        }
    }
    /// TOFU 模式的用户级 known_hosts(`<data_root>/secrets/known_hosts`)。
    pub fn accept_on_first_use_in(data_root: &std::path::Path) -> Self {
        Self::AcceptOnFirstUse {
            known_hosts_path: data_root.join("secrets").join("known_hosts"),
        }
    }
}

/// known_hosts 查询结果。未知与不匹配必须拆开:未知可走 TOFU 确认,不匹配应阻断。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostKeyCheck {
    Match,
    Unknown,
    Mismatch,
}

/// 解析 OpenSSH 风格 known_hosts 文件。
///
/// 简化版只支持精确 host:port 匹配,不支持 hostname hash(`|1|...|...`)
/// 与 wildcard。生产场景 known_hosts 由 ncd-update 派生写入,不复用 OpenSSH 的
/// 历史文件,故无需兼容旧风格。
pub struct KnownHostsStore {
    path: PathBuf,
}

impl KnownHostsStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// 检查 host:port 是否有匹配条目,并区分未知主机与同主机 key 不一致。
    /// 文件不存在视作未知主机。
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

        let target = format_host(host, port);
        let mut saw_host = false;
        for raw in content.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            // OpenSSH 行:`<host[,host2]> <key-type> <base64-key> [comment]`
            let mut parts = line.split_whitespace();
            let hosts = match parts.next() {
                Some(h) => h,
                None => continue,
            };
            let kind = parts.next().unwrap_or("");
            let b64 = parts.next().unwrap_or("");

            // 多 host 用逗号分隔
            let host_list: Vec<&str> = hosts.split(',').map(str::trim).collect();
            let host_match = host_list.iter().any(|h| matches_host(h, &target, host));
            if host_match && kind == key_kind && b64 == key_b64 {
                return Ok(HostKeyCheck::Match);
            }
            saw_host |= host_match;
        }

        if saw_host {
            Ok(HostKeyCheck::Mismatch)
        } else {
            Ok(HostKeyCheck::Unknown)
        }
    }

    /// 兼容旧调用:只有完全匹配才返回 true。
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

    /// 把新条目追加到 known_hosts(`AcceptOnFirstUse` 用,实装时调用)。
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

fn matches_host(entry: &str, target_full: &str, target_bare: &str) -> bool {
    // 完全匹配 (含端口)
    if entry == target_full {
        return true;
    }
    // 22 端口的简写形式:`example.com` 也认作 `example.com:22`
    if entry == target_bare {
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn matches_returns_false_when_file_missing() {
        let dir = tempdir().unwrap();
        let store = KnownHostsStore::new(dir.path().join("nonexistent"));
        let m = store.matches("example.com", 22, "ssh-ed25519", "AAAA").await.unwrap();
        assert!(!m);
    }

    #[tokio::test]
    async fn matches_finds_exact_entry_default_port() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("known_hosts");
        let content = "example.com ssh-ed25519 AAAAB3keyhere\n";
        fs::write(&path, content).await.unwrap();
        let store = KnownHostsStore::new(&path);
        assert!(store.matches("example.com", 22, "ssh-ed25519", "AAAAB3keyhere").await.unwrap());
        // 不同 key b64 不匹配
        assert!(!store.matches("example.com", 22, "ssh-ed25519", "AAAAdifferent").await.unwrap());
        // 不同 key 类型不匹配
        assert!(!store.matches("example.com", 22, "ssh-rsa", "AAAAB3keyhere").await.unwrap());
    }

    #[tokio::test]
    async fn matches_handles_nonstandard_port() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("known_hosts");
        fs::write(&path, "[example.com]:2222 ssh-ed25519 AAAAkey\n").await.unwrap();
        let store = KnownHostsStore::new(&path);
        assert!(store.matches("example.com", 2222, "ssh-ed25519", "AAAAkey").await.unwrap());
        assert!(!store.matches("example.com", 22, "ssh-ed25519", "AAAAkey").await.unwrap());
    }

    #[tokio::test]
    async fn matches_skips_comments() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("known_hosts");
        let content = "# comment line\n\nexample.com ssh-ed25519 AAAAkey\n";
        fs::write(&path, content).await.unwrap();
        let store = KnownHostsStore::new(&path);
        assert!(store.matches("example.com", 22, "ssh-ed25519", "AAAAkey").await.unwrap());
    }

    #[tokio::test]
    async fn append_creates_file_with_entry() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("subdir/known_hosts");
        let store = KnownHostsStore::new(&path);
        store.append("example.com", 22, "ssh-ed25519", "AAAAkey").await.unwrap();
        let content = fs::read_to_string(&path).await.unwrap();
        assert!(content.contains("example.com ssh-ed25519 AAAAkey"));
        // 新加的条目应能 match
        assert!(store.matches("example.com", 22, "ssh-ed25519", "AAAAkey").await.unwrap());
    }

    #[tokio::test]
    async fn matches_finds_multi_host_entry() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("known_hosts");
        // 多 host 用逗号分隔(OpenSSH 风格)
        fs::write(&path, "alpha.example.com,beta.example.com ssh-ed25519 AAAAkey\n").await.unwrap();
        let store = KnownHostsStore::new(&path);
        assert!(store.matches("alpha.example.com", 22, "ssh-ed25519", "AAAAkey").await.unwrap());
        assert!(store.matches("beta.example.com", 22, "ssh-ed25519", "AAAAkey").await.unwrap());
        assert!(!store.matches("gamma.example.com", 22, "ssh-ed25519", "AAAAkey").await.unwrap());
    }
}
