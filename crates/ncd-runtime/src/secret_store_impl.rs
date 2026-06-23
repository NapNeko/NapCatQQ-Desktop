use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit},
};
use keyring::Entry;
use rand::RngCore;

use ncd_domain::errors::SecretError;
use ncd_traits::SecretStore;

pub struct SecretStoreImpl {
    fallback_dir: PathBuf,
    service: String,
    fallback_cache: RwLock<HashMap<String, String>>,
    force_fallback: bool,
}

impl SecretStoreImpl {
    pub fn new(fallback_dir: impl Into<PathBuf>) -> Self {
        let fallback_dir = fallback_dir.into();
        let _ = fs::create_dir_all(&fallback_dir);
        let cache = Self::load_fallback(&fallback_dir).unwrap_or_default();

        Self {
            fallback_dir,
            service: "napcat-desktop".to_string(),
            fallback_cache: RwLock::new(cache),
            force_fallback: false,
        }
    }

    pub fn new_with_force_fallback(fallback_dir: impl Into<PathBuf>, force: bool) -> Self {
        let fallback_dir = fallback_dir.into();
        let _ = fs::create_dir_all(&fallback_dir);
        let cache = Self::load_fallback(&fallback_dir).unwrap_or_default();

        Self {
            fallback_dir,
            service: "napcat-desktop".to_string(),
            fallback_cache: RwLock::new(cache),
            force_fallback: force,
        }
    }

    fn get_or_create_key(dir: &Path) -> Result<[u8; 32], SecretError> {
        let key_path = dir.join(".key");
        if key_path.exists() {
            if let Ok(bytes) = fs::read(&key_path) {
                if bytes.len() == 32 {
                    let mut key = [0u8; 32];
                    key.copy_from_slice(&bytes);
                    return Ok(key);
                }
            }
        }

        let mut key = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut key);

        fs::write(&key_path, key).map_err(|_| SecretError::Unavailable)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(metadata) = fs::metadata(&key_path) {
                let mut perms = metadata.permissions();
                perms.set_mode(0o600);
                let _ = fs::set_permissions(&key_path, perms);
            }
        }

        Ok(key)
    }

    fn load_fallback(dir: &Path) -> Result<HashMap<String, String>, SecretError> {
        let enc_path = dir.join("secrets.enc");
        if !enc_path.exists() {
            return Ok(HashMap::new());
        }

        let key = Self::get_or_create_key(dir)?;
        let file_bytes = fs::read(&enc_path).map_err(|_| SecretError::Unavailable)?;
        if file_bytes.len() < 12 {
            return Err(SecretError::Unavailable);
        }

        let nonce_bytes = &file_bytes[..12];
        let cipher_bytes = &file_bytes[12..];

        let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| SecretError::Unavailable)?;
        let nonce = Nonce::from_slice(nonce_bytes);

        let decrypted_bytes = cipher
            .decrypt(nonce, cipher_bytes)
            .map_err(|_| SecretError::Unavailable)?;

        let map: HashMap<String, String> =
            serde_json::from_slice(&decrypted_bytes).map_err(|_| SecretError::Unavailable)?;

        Ok(map)
    }

    fn save_fallback(&self, map: &HashMap<String, String>) -> Result<(), SecretError> {
        let key = Self::get_or_create_key(&self.fallback_dir)?;
        let json_bytes = serde_json::to_vec(map).map_err(|_| SecretError::Unavailable)?;

        let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| SecretError::Unavailable)?;
        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let encrypted_bytes = cipher
            .encrypt(nonce, json_bytes.as_slice())
            .map_err(|_| SecretError::Unavailable)?;

        let mut output = Vec::with_capacity(12 + encrypted_bytes.len());
        output.extend_from_slice(&nonce_bytes);
        output.extend_from_slice(&encrypted_bytes);

        let tmp_path = self.fallback_dir.join("secrets.enc.tmp");
        let enc_path = self.fallback_dir.join("secrets.enc");

        fs::write(&tmp_path, output).map_err(|_| SecretError::Unavailable)?;
        fs::rename(tmp_path, enc_path).map_err(|_| SecretError::Unavailable)?;

        Ok(())
    }

    fn put_fallback(&self, key: &str, value: &str) -> Result<(), SecretError> {
        let mut map = self
            .fallback_cache
            .write()
            .map_err(|_| SecretError::Unavailable)?;
        map.insert(key.to_string(), value.to_string());
        self.save_fallback(&map)
    }

    fn get_fallback(&self, key: &str) -> Result<Option<String>, SecretError> {
        let map = self
            .fallback_cache
            .read()
            .map_err(|_| SecretError::Unavailable)?;
        Ok(map.get(key).cloned())
    }

    fn delete_fallback(&self, key: &str) -> Result<bool, SecretError> {
        let mut map = self
            .fallback_cache
            .write()
            .map_err(|_| SecretError::Unavailable)?;
        if map.remove(key).is_some() {
            self.save_fallback(&map)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

impl SecretStore for SecretStoreImpl {
    fn get(&self, key: &str) -> Result<Option<String>, SecretError> {
        if self.force_fallback {
            return self.get_fallback(key);
        }

        match Entry::new(&self.service, key) {
            Ok(entry) => match entry.get_password() {
                Ok(pwd) => Ok(Some(pwd)),
                Err(keyring::Error::NoEntry) => {
                    // keyring 查找不到,安全降级在 fallback 中查找(可能旧数据存在降级存储中)
                    self.get_fallback(key)
                }
                Err(_) => {
                    // 底层平台报错,自愈降级在 fallback 中查找
                    self.get_fallback(key)
                }
            },
            Err(_) => self.get_fallback(key),
        }
    }

    fn put(&self, key: &str, value: &str) -> Result<(), SecretError> {
        if self.force_fallback {
            return self.put_fallback(key, value);
        }

        match Entry::new(&self.service, key) {
            Ok(entry) => {
                if entry.set_password(value).is_err() {
                    // keyring 设置密码报错,说明平台存储不可用,自愈降级本地加密文件
                    self.put_fallback(key, value)
                } else {
                    Ok(())
                }
            }
            Err(_) => self.put_fallback(key, value),
        }
    }

    fn delete(&self, key: &str) -> Result<(), SecretError> {
        let mut deleted_any = false;

        if !self.force_fallback {
            if let Ok(entry) = Entry::new(&self.service, key) {
                if entry.delete_password().is_ok() {
                    deleted_any = true;
                }
            }
        }

        if let Ok(deleted) = self.delete_fallback(key) {
            if deleted {
                deleted_any = true;
            }
        }

        if deleted_any {
            Ok(())
        } else {
            Err(SecretError::NotFound)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_secret_store_fallback_flow() {
        let dir = tempdir().unwrap();
        let store = SecretStoreImpl::new_with_force_fallback(dir.path().to_path_buf(), true);

        // 1. 获取不存在的 key
        assert_eq!(store.get("not_exist").unwrap(), None);

        // 2. 写入 credentials
        store.put("ssh_password", "my_secure_ssh_password").unwrap();

        // 3. 获取 credential
        assert_eq!(
            store.get("ssh_password").unwrap(),
            Some("my_secure_ssh_password".to_string())
        );

        // 4. 重建 store
        let store2 = SecretStoreImpl::new_with_force_fallback(dir.path().to_path_buf(), true);
        assert_eq!(
            store2.get("ssh_password").unwrap(),
            Some("my_secure_ssh_password".to_string())
        );

        // 5. 删除 credential
        store.delete("ssh_password").unwrap();
        assert_eq!(store.get("ssh_password").unwrap(), None);

        // 6. 再次删除不存在的,应当 NotFound
        assert!(matches!(
            store.delete("ssh_password"),
            Err(SecretError::NotFound)
        ));
    }
}
