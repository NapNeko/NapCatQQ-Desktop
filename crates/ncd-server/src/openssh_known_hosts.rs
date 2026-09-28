//! 读取本机 OpenSSH `~/.ssh/known_hosts`，给 Desktop 档案做指纹种子。
//!
//! 连接仍走应用自己的 `<data_root>/secrets/known_hosts`。这里只在添加档案时
//! 把用户已经用 `ssh` 信任过的明文条目抄过去，避免导入后第一次连接被 TOFU 拦住。
//! 不解析 `|1|` 哈希主机名，也不认通配符（对不上就不抄，仍走确认框）。
//!
//! OpenSSH 记指纹用的是 HostName 替换之后真正去连的主机名，只有配了 HostKeyAlias
//! 才记别名；非 22 端口记成 `[host]:port`。所以只按连接用的 host 和端口找，
//! 按档案显示名或 config 里的 Host 别名找，可能把另一台机器的公钥抄成这台的。

use std::path::Path;

use ncd_host::remote::{
    HostKeyCheck, KnownHostsStore, known_hosts_host_matches, parse_known_hosts_line,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenSshHostKey {
    pub key_kind: String,
    pub key_b64: String,
}

/// 从 OpenSSH known_hosts 文本里取出 `host` 在 `port` 上的明文公钥。对不上返回空列表。
/// 用户 `@revoked` 过的公钥一律不抄，不管那行写的是哪台主机：ssh 见到吊销的 key 照样拒，
/// 这里通配符和哈希主机名都不解析，宁可多排除几把、退回确认框
pub fn lookup_openssh_host_keys(content: &str, host: &str, port: u16) -> Vec<OpenSshHostKey> {
    let lines: Vec<_> = content.lines().filter_map(parse_known_hosts_line).collect();
    let revoked: Vec<(&str, &str)> = lines
        .iter()
        .filter(|l| l.marker == Some("@revoked"))
        .map(|l| (l.kind, l.key_b64))
        .collect();
    let mut out = Vec::new();
    for line in &lines {
        if line.marker.is_some() || line.hosts.starts_with("|1|") {
            continue;
        }
        if revoked.contains(&(line.kind, line.key_b64)) {
            continue;
        }
        if !known_hosts_host_matches(line.hosts, host, port) {
            continue;
        }
        let key = OpenSshHostKey {
            key_kind: line.kind.to_string(),
            key_b64: line.key_b64.to_string(),
        };
        if !out.contains(&key) {
            out.push(key);
        }
    }
    out
}

/// 把 OpenSSH 里已有的指纹写入应用 known_hosts。同算法不同公钥不覆盖。
/// 文件不存在或读不了就当没有。返回实际追加的条数。
pub async fn seed_app_known_hosts(
    store: &KnownHostsStore,
    openssh_path: &Path,
    connect_host: &str,
    port: u16,
) -> usize {
    let Ok(content) = tokio::fs::read_to_string(openssh_path).await else {
        return 0;
    };
    let keys = lookup_openssh_host_keys(&content, connect_host, port);
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn lookup_goes_by_connect_host_not_alias() {
        let content = "160.30.231.138 ssh-ed25519 AAAAed\n\
                       kunming-4-8 ssh-rsa AAAArsa\n\
                       other.example ssh-ed25519 AAAAskip\n";
        let keys = lookup_openssh_host_keys(content, "160.30.231.138", 22);
        assert_eq!(
            keys,
            vec![OpenSshHostKey {
                key_kind: "ssh-ed25519".into(),
                key_b64: "AAAAed".into(),
            }]
        );
        let listed_together = "kunming-4-8,160.30.231.138 ssh-rsa AAAArsa\n";
        assert_eq!(
            lookup_openssh_host_keys(listed_together, "160.30.231.138", 22).len(),
            1
        );
    }

    #[test]
    fn lookup_keeps_ports_apart() {
        let content = "[192.0.2.11]:2222 ssh-ed25519 AAAAport\n\
                       192.0.2.11 ssh-ed25519 AAAAplain\n";
        let on_2222 = lookup_openssh_host_keys(content, "192.0.2.11", 2222);
        assert_eq!(on_2222.len(), 1);
        assert_eq!(on_2222[0].key_b64, "AAAAport");
        let on_22 = lookup_openssh_host_keys(content, "192.0.2.11", 22);
        assert_eq!(on_22.len(), 1);
        assert_eq!(on_22[0].key_b64, "AAAAplain");
    }

    #[test]
    fn lookup_ignores_case_and_honours_negation() {
        let content = "Box.Example ssh-ed25519 AAAAcase\n\
                       box.example,!box.example ssh-rsa AAAAneg\n";
        let keys = lookup_openssh_host_keys(content, "box.example", 22);
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].key_b64, "AAAAcase");
    }

    #[test]
    fn lookup_skips_hashed_and_markers() {
        let content = "|1|abc|def ssh-ed25519 AAAAhash\n\
                       @cert-authority *.example ssh-ed25519 AAAAcert\n\
                       192.0.2.10 ssh-ed25519 AAAAok\n";
        let keys = lookup_openssh_host_keys(content, "192.0.2.10", 22);
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].key_b64, "AAAAok");
    }

    #[test]
    fn lookup_never_copies_revoked_keys() {
        let content = "192.0.2.10 ssh-ed25519 AAAAbad\n\
                       192.0.2.10 ssh-rsa AAAAgood\n\
                       @revoked * ssh-ed25519 AAAAbad\n";
        let keys = lookup_openssh_host_keys(content, "192.0.2.10", 22);
        assert_eq!(
            keys,
            vec![OpenSshHostKey {
                key_kind: "ssh-rsa".into(),
                key_b64: "AAAAgood".into(),
            }]
        );
    }

    #[tokio::test]
    async fn seed_appends_unknown_and_skips_known() {
        let dir = tempdir().unwrap();
        let openssh = dir.path().join("openssh");
        std::fs::write(
            &openssh,
            "192.0.2.10 ssh-ed25519 AAAAed\n192.0.2.10 ssh-rsa AAAArsa\n",
        )
        .unwrap();
        let app_path = dir.path().join("app_known_hosts");
        let store = KnownHostsStore::new(&app_path);
        let n = seed_app_known_hosts(&store, &openssh, "192.0.2.10", 22).await;
        assert_eq!(n, 2);
        let again = seed_app_known_hosts(&store, &openssh, "192.0.2.10", 22).await;
        assert_eq!(again, 0);
        assert_eq!(
            store
                .check("192.0.2.10", 22, "ssh-ed25519", "AAAAed")
                .await
                .unwrap(),
            HostKeyCheck::Match
        );
    }

    #[tokio::test]
    async fn seed_without_openssh_file_is_a_no_op() {
        let dir = tempdir().unwrap();
        let store = KnownHostsStore::new(dir.path().join("app_known_hosts"));
        let n = seed_app_known_hosts(&store, &dir.path().join("missing"), "192.0.2.10", 22).await;
        assert_eq!(n, 0);
    }
}
