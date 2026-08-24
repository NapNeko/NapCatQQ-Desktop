//! 读取本机 OpenSSH `~/.ssh/known_hosts`，给 Desktop 档案做指纹种子。
//!
//! 连接仍走应用自己的 `<data_root>/secrets/known_hosts`。这里只在添加档案时
//! 把用户已经用 `ssh` 信任过的明文条目抄过去，避免导入后第一次连接被 TOFU 拦住。
//! 不解析 `|1|` 哈希主机名（对不上就不抄，仍走确认框）。

use std::path::Path;

use ncd_host::remote::{HostKeyCheck, KnownHostsStore};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenSshHostKey {
    pub key_kind: String,
    pub key_b64: String,
}

/// 按连接用的 host / 别名 / 端口，从 OpenSSH known_hosts 取出明文公钥。
/// 文件不存在或对不上返回空列表，不报错。
pub fn lookup_openssh_host_keys(path: &Path, names: &[&str], port: u16) -> Vec<OpenSshHostKey> {
    let Ok(content) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for raw in content.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('@') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(hosts) = parts.next() else {
            continue;
        };
        if hosts.starts_with("|1|") {
            continue;
        }
        let Some(kind) = parts.next() else {
            continue;
        };
        let Some(b64) = parts.next() else {
            continue;
        };
        if !host_field_matches(hosts, names, port) {
            continue;
        }
        let key = OpenSshHostKey {
            key_kind: kind.to_string(),
            key_b64: b64.to_string(),
        };
        if !out.contains(&key) {
            out.push(key);
        }
    }
    out
}

/// 把 OpenSSH 里已有的指纹写入应用 known_hosts。同算法不同公钥不覆盖。
/// 返回实际追加的条数。
pub async fn seed_app_known_hosts(
    store: &KnownHostsStore,
    openssh_path: &Path,
    connect_host: &str,
    extra_names: &[&str],
    port: u16,
) -> usize {
    let mut names = Vec::with_capacity(extra_names.len() + 1);
    names.push(connect_host);
    for name in extra_names {
        if !name.is_empty() && *name != connect_host {
            names.push(name);
        }
    }
    let keys = lookup_openssh_host_keys(openssh_path, &names, port);
    let mut appended = 0;
    for key in keys {
        match store
            .check(connect_host, port, &key.key_kind, &key.key_b64)
            .await
        {
            Ok(HostKeyCheck::Unknown) => {
                if store
                    .append(connect_host, port, &key.key_kind, &key.key_b64)
                    .await
                    .is_ok()
                {
                    appended += 1;
                }
            }
            Ok(HostKeyCheck::Match | HostKeyCheck::Mismatch) => {}
            Err(_) => {}
        }
    }
    appended
}

fn host_field_matches(field: &str, names: &[&str], port: u16) -> bool {
    field.split(',').any(|part| {
        let part = part.trim();
        if part.is_empty() || part.starts_with('|') {
            return false;
        }
        names.iter().any(|name| {
            if name.is_empty() {
                return false;
            }
            part == *name
                || part == format!("[{name}]:{port}")
                || (port == 22 && part == format!("[{name}]:22"))
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn lookup_matches_ip_and_alias() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("known_hosts");
        std::fs::write(
            &path,
            "160.30.231.138 ssh-ed25519 AAAAed\n\
             kunming-4-8 ssh-rsa AAAArsa\n\
             other.example ssh-ed25519 AAAAskip\n",
        )
        .unwrap();
        let keys = lookup_openssh_host_keys(&path, &["160.30.231.138", "kunming-4-8"], 22);
        assert_eq!(keys.len(), 2);
        assert_eq!(keys[0].key_kind, "ssh-ed25519");
        assert_eq!(keys[1].key_kind, "ssh-rsa");
    }

    #[test]
    fn lookup_matches_nonstandard_port() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("known_hosts");
        std::fs::write(&path, "[192.0.2.11]:2222 ssh-ed25519 AAAAed\n").unwrap();
        let keys = lookup_openssh_host_keys(&path, &["192.0.2.11"], 2222);
        assert_eq!(keys.len(), 1);
        let none = lookup_openssh_host_keys(&path, &["192.0.2.11"], 22);
        assert!(none.is_empty());
    }

    #[test]
    fn lookup_skips_hashed_and_markers() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("known_hosts");
        std::fs::write(
            &path,
            "|1|abc|def ssh-ed25519 AAAAhash\n\
             @cert-authority *.example ssh-ed25519 AAAAcert\n\
             192.0.2.10 ssh-ed25519 AAAAok\n",
        )
        .unwrap();
        let keys = lookup_openssh_host_keys(&path, &["192.0.2.10"], 22);
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].key_b64, "AAAAok");
    }

    #[tokio::test]
    async fn seed_appends_unknown_and_skips_mismatch() {
        let dir = tempdir().unwrap();
        let openssh = dir.path().join("openssh");
        std::fs::write(
            &openssh,
            "192.0.2.10 ssh-ed25519 AAAAed\n192.0.2.10 ssh-rsa AAAArsa\n",
        )
        .unwrap();
        let app_path = dir.path().join("app_known_hosts");
        let store = KnownHostsStore::new(&app_path);
        let n = seed_app_known_hosts(&store, &openssh, "192.0.2.10", &["alpha"], 22).await;
        assert_eq!(n, 2);
        let again = seed_app_known_hosts(&store, &openssh, "192.0.2.10", &[], 22).await;
        assert_eq!(again, 0);
        assert_eq!(
            store
                .check("192.0.2.10", 22, "ssh-ed25519", "AAAAed")
                .await
                .unwrap(),
            HostKeyCheck::Match
        );
    }
}
