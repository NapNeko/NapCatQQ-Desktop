//! 凭据同步层：统一管理 keyring ↔ Host 的密码同步
//!
//! 职责：
//! 1. 统一读取密码（ssh + sudo 优先级）
//! 2. 同步密码到 Host（缓存 + 隔离连接）
//! 3. 密码变更时触发缓存失效

use crate::server_manager::{AuthMethod, ServerCredentialStore, ServerProfile};
use ncd_host::Host;
use ncd_host::remote::SshCredentials;
use std::sync::Arc;

/// 凭据同步层：桥接 keyring 持久化与 Host 内存缓存
pub struct CredentialSyncLayer {
    credentials: Arc<dyn ServerCredentialStore>,
}

impl CredentialSyncLayer {
    pub fn new(credentials: Arc<dyn ServerCredentialStore>) -> Self {
        Self { credentials }
    }

    /// 获取 SSH 认证凭据（登录用）
    pub fn ssh_credentials(&self, profile: &ServerProfile) -> Result<SshCredentials, String> {
        match profile.auth_method {
            AuthMethod::Password => {
                let password = self
                    .credentials
                    .get_password(&profile.id)
                    .ok_or_else(|| "密码认证但未保存密码".to_string())?;
                Ok(SshCredentials::password(&profile.username, password))
            }
            AuthMethod::Key => {
                let key_path = profile
                    .private_key_path
                    .as_ref()
                    .ok_or_else(|| "密钥认证但未配置密钥路径".to_string())?;
                let passphrase = self.credentials.get_password(&profile.id);
                Ok(SshCredentials::key_file(
                    &profile.username,
                    key_path,
                    passphrase,
                ))
            }
        }
    }

    /// 获取提权密码（sudo 用）
    ///
    /// 优先级：sudo 专用槽 > ssh 登录密码
    pub fn elevation_password(&self, server_id: &str) -> Option<String> {
        self.credentials
            .get_sudo_password(server_id)
            .or_else(|| self.credentials.get_password(server_id))
    }

    /// 同步提权密码到 Host 实例
    pub async fn sync_elevation_to_host(&self, server_id: &str, host: &dyn Host) {
        host.set_elevation_password(self.elevation_password(server_id))
            .await;
    }

    /// 记住 sudo 密码（弹框勾选"记住"时调用）
    pub fn remember_sudo(&self, server_id: &str, password: &str) -> Result<(), String> {
        self.credentials.set_sudo_password(server_id, password)
    }

    /// 同步 SSH 密码到 sudo 槽（setup_key_auth 自动迁移）
    ///
    /// 逻辑：如果 ssh 槽有密码但 sudo 槽为空，则复制到 sudo 槽
    pub fn migrate_ssh_to_sudo(&self, server_id: &str) -> Result<(), String> {
        if let Some(ssh_pwd) = self.credentials.get_password(server_id) {
            if self.credentials.get_sudo_password(server_id).is_none() {
                self.credentials.set_sudo_password(server_id, &ssh_pwd)?;
            }
        }
        Ok(())
    }

    /// 密码变更后的缓存失效钩子（返回是否需要重连）
    pub fn on_password_changed(&self, _server_id: &str, changed_slot: PasswordSlot) -> bool {
        // SSH 密码变更必须重连，sudo 密码变更可以热更新
        matches!(changed_slot, PasswordSlot::Ssh)
    }

    /// 获取底层凭据存储（给 ServerManager 暴露，用于 remember_sudo_password）
    pub fn credentials(&self) -> &Arc<dyn ServerCredentialStore> {
        &self.credentials
    }
}

#[derive(Debug, Clone, Copy)]
pub enum PasswordSlot {
    Ssh,
    Sudo,
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_in_result)]
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    // 测试用的内存凭据存储
    #[derive(Default)]
    struct InMemoryCredentialStore {
        passwords: Mutex<HashMap<String, String>>,
        sudo_passwords: Mutex<HashMap<String, String>>,
    }

    impl ServerCredentialStore for InMemoryCredentialStore {
        fn get_password(&self, server_id: &str) -> Option<String> {
            self.passwords.lock().unwrap().get(server_id).cloned()
        }

        fn set_password(&self, server_id: &str, password: &str) -> Result<(), String> {
            self.passwords
                .lock()
                .unwrap()
                .insert(server_id.to_string(), password.to_string());
            Ok(())
        }

        fn delete_password(&self, server_id: &str) -> Result<(), String> {
            self.passwords.lock().unwrap().remove(server_id);
            Ok(())
        }

        fn get_sudo_password(&self, server_id: &str) -> Option<String> {
            self.sudo_passwords
                .lock()
                .unwrap()
                .get(server_id)
                .cloned()
        }

        fn set_sudo_password(&self, server_id: &str, password: &str) -> Result<(), String> {
            self.sudo_passwords
                .lock()
                .unwrap()
                .insert(server_id.to_string(), password.to_string());
            Ok(())
        }

        fn delete_sudo_password(&self, server_id: &str) -> Result<(), String> {
            self.sudo_passwords.lock().unwrap().remove(server_id);
            Ok(())
        }
    }

    #[test]
    fn elevation_password_priority() {
        let store = Arc::new(InMemoryCredentialStore::default());
        store.set_password("s1", "ssh-pwd").unwrap();
        store.set_sudo_password("s1", "sudo-pwd").unwrap();

        let sync = CredentialSyncLayer::new(store);

        // sudo 专用槽优先
        assert_eq!(
            sync.elevation_password("s1"),
            Some("sudo-pwd".to_string())
        );
    }

    #[test]
    fn elevation_password_fallback_to_ssh() {
        let store = Arc::new(InMemoryCredentialStore::default());
        store.set_password("s1", "ssh-pwd").unwrap();

        let sync = CredentialSyncLayer::new(store);

        // 无 sudo 槽时回退到 ssh
        assert_eq!(
            sync.elevation_password("s1"),
            Some("ssh-pwd".to_string())
        );
    }

    #[test]
    fn migrate_ssh_to_sudo_only_if_empty() {
        let store = Arc::new(InMemoryCredentialStore::default());
        store.set_password("s1", "ssh-pwd").unwrap();

        let sync = CredentialSyncLayer::new(store.clone());
        sync.migrate_ssh_to_sudo("s1").unwrap();

        assert_eq!(
            store.get_sudo_password("s1"),
            Some("ssh-pwd".to_string())
        );

        // 再次迁移不覆盖
        store.set_sudo_password("s1", "manual-sudo").unwrap();
        sync.migrate_ssh_to_sudo("s1").unwrap();
        assert_eq!(
            store.get_sudo_password("s1"),
            Some("manual-sudo".to_string())
        );
    }

    #[test]
    fn on_password_changed_ssh_requires_reconnect() {
        let store = Arc::new(InMemoryCredentialStore::default());
        let sync = CredentialSyncLayer::new(store);

        assert!(sync.on_password_changed("s1", PasswordSlot::Ssh));
        assert!(!sync.on_password_changed("s1", PasswordSlot::Sudo));
    }
}
