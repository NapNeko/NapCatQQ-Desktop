//! 本地 SSH 密钥对生成。
//!
//! 用于"密码登录 → 自动配置免密"流程：本地生成一对 ed25519 密钥，私钥落盘到
//! 数据目录，公钥推到远端 authorized_keys。生成走纯 Rust 的 ssh-key crate，
//! 不依赖系统 ssh-keygen 二进制（Windows 上不一定有）。

use ssh_key::rand_core::OsRng;
use ssh_key::{Algorithm, LineEnding, PrivateKey};

/// 一对新生成的 ed25519 密钥的可落盘表示。
pub struct GeneratedKeyPair {
    /// OpenSSH 格式私钥 PEM（写进 <data_root>/ssh_keys/<id>，权限 600）。
    pub private_openssh: String,
    /// authorized_keys 单行公钥（ssh-ed25519 AAAA... comment）。
    pub public_line: String,
}

/// 生成一对 ed25519 密钥。comment 写进公钥尾部，方便用户在远端
/// authorized_keys 里认出这是哪台 Desktop 加的。
pub fn generate_ed25519(comment: &str) -> Result<GeneratedKeyPair, String> {
    let mut private = PrivateKey::random(&mut OsRng, Algorithm::Ed25519)
        .map_err(|e| format!("生成 ed25519 密钥失败: {e}"))?;
    private.set_comment(comment);

    let private_openssh = private
        .to_openssh(LineEnding::LF)
        .map_err(|e| format!("编码私钥失败: {e}"))?
        .to_string();

    let public_line = private
        .public_key()
        .to_openssh()
        .map_err(|e| format!("编码公钥失败: {e}"))?;

    Ok(GeneratedKeyPair {
        private_openssh,
        public_line,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_valid_openssh_pair() {
        let pair = generate_ed25519("napcatqq-desktop@test").unwrap();
        assert!(pair.private_openssh.contains("OPENSSH PRIVATE KEY"));
        assert!(pair.public_line.starts_with("ssh-ed25519 "));
        assert!(pair.public_line.contains("napcatqq-desktop@test"));
    }

    #[test]
    fn two_calls_produce_different_keys() {
        let a = generate_ed25519("x").unwrap();
        let b = generate_ed25519("x").unwrap();
        assert_ne!(a.public_line, b.public_line);
    }
}
