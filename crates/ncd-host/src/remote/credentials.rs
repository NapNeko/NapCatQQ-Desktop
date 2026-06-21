//! SSH 凭证类型
//!
//! 红线:凭证不进版本库,只在内存或受保护的 SecretStore 中流转SshKey
//! 提供从文件路径或字节加载,但禁止 Display / Debug 暴露密钥内容

use std::fmt;
use std::path::PathBuf;

/// SSH 私钥来源
#[derive(Clone)]
pub enum SshKey {
    /// 从文件路径加载(运行时延迟读取,避免长期持有内存中)
    Path { path: PathBuf, passphrase: Option<String> },
    /// 直接持有 PEM / OpenSSH 字节
    Pem { bytes: Vec<u8>, passphrase: Option<String> },
}

impl fmt::Debug for SshKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // 路径可以打印,但 passphrase 永远 redact
            Self::Path { path, passphrase } => f
                .debug_struct("SshKey::Path")
                .field("path", path)
                .field("passphrase", &passphrase.as_ref().map(|_| "<redacted>"))
                .finish(),
            Self::Pem { bytes, passphrase } => f
                .debug_struct("SshKey::Pem")
                .field("bytes", &format!("<{} bytes>", bytes.len()))
                .field("passphrase", &passphrase.as_ref().map(|_| "<redacted>"))
                .finish(),
        }
    }
}

/// SSH 认证凭证
#[derive(Debug, Clone)]
pub enum SshCredentials {
    /// 密码认证(简单场景或测试,生产推荐用 Key)
    Password { username: String, password: String },
    /// 私钥认证
    Key { username: String, key: SshKey },
}

impl SshCredentials {
    pub fn username(&self) -> &str {
        match self {
            Self::Password { username, .. } => username,
            Self::Key { username, .. } => username,
        }
    }

    /// 用密码连接的便捷构造器
    pub fn password(username: impl Into<String>, password: impl Into<String>) -> Self {
        Self::Password {
            username: username.into(),
            password: password.into(),
        }
    }

    /// 用私钥文件连接的便捷构造器
    pub fn key_file(
        username: impl Into<String>,
        path: impl Into<PathBuf>,
        passphrase: Option<String>,
    ) -> Self {
        Self::Key {
            username: username.into(),
            key: SshKey::Path {
                path: path.into(),
                passphrase,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_redacts_passphrase() {
        let key = SshKey::Path {
            path: PathBuf::from("/home/user/.ssh/id_ed25519"),
            passphrase: Some("super-secret".to_string()),
        };
        let dbg = format!("{key:?}");
        assert!(dbg.contains("/home/user/.ssh/id_ed25519"));
        assert!(!dbg.contains("super-secret"));
        assert!(dbg.contains("<redacted>"));
    }

    #[test]
    fn debug_redacts_pem_bytes_size_only() {
        let key = SshKey::Pem {
            bytes: vec![0u8; 256],
            passphrase: None,
        };
        let dbg = format!("{key:?}");
        assert!(dbg.contains("256 bytes"));
        assert!(!dbg.contains('\u{0}'));
    }

    #[test]
    fn credentials_username_extraction() {
        let pw = SshCredentials::password("alice", "secret");
        assert_eq!(pw.username(), "alice");

        let k = SshCredentials::key_file("bob", "/tmp/k", None);
        assert_eq!(k.username(), "bob");
    }
}
