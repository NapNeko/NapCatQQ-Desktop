//! 系统凭据存储不可用时直接失败，供 MCP 等授权凭据使用。

use keyring::Entry;
use ncd_domain::SecretError;
use ncd_traits::SecretStore;

/// 不访问本地降级文件，避免把服务授权凭据与解密密钥保存在同一目录。
pub struct SystemSecretStore;

impl SecretStore for SystemSecretStore {
    fn get(&self, key: &str) -> Result<Option<String>, SecretError> {
        read_entry(&entry(key)?)
    }

    fn put(&self, key: &str, value: &str) -> Result<(), SecretError> {
        write_entry(&entry(key)?, value)
    }

    fn delete(&self, key: &str) -> Result<(), SecretError> {
        delete_entry(&entry(key)?)
    }
}

fn entry(key: &str) -> Result<Entry, SecretError> {
    Entry::new("napcat-desktop", key).map_err(|_| SecretError::Unavailable)
}

fn read_entry(entry: &Entry) -> Result<Option<String>, SecretError> {
    match entry.get_password() {
        Ok(value) => Ok(Some(value)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(_) => Err(SecretError::Unavailable),
    }
}

fn write_entry(entry: &Entry, value: &str) -> Result<(), SecretError> {
    entry
        .set_password(value)
        .map_err(|_| SecretError::Unavailable)
}

fn delete_entry(entry: &Entry) -> Result<(), SecretError> {
    match entry.delete_password() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(_) => Err(SecretError::Unavailable),
    }
}

#[cfg(test)]
mod tests {
    use keyring::mock::MockCredential;

    use super::*;

    fn fail_next(entry: &Entry) {
        entry
            .get_credential()
            .downcast_ref::<MockCredential>()
            .unwrap()
            .set_error(keyring::Error::Invalid(
                "store".into(),
                "unavailable".into(),
            ));
    }

    #[test]
    fn system_store_distinguishes_missing_from_unavailable() {
        let entry = Entry::new_with_credential(Box::<MockCredential>::default());
        assert!(read_entry(&entry).unwrap().is_none());
        write_entry(&entry, "old-test-token").unwrap();
        fail_next(&entry);
        assert!(matches!(read_entry(&entry), Err(SecretError::Unavailable)));
        assert_eq!(
            read_entry(&entry).unwrap().as_deref(),
            Some("old-test-token")
        );
    }

    #[test]
    fn failed_rotation_never_reports_success_or_masks_old_credential() {
        let entry = Entry::new_with_credential(Box::<MockCredential>::default());
        write_entry(&entry, "old-test-token").unwrap();
        fail_next(&entry);
        assert!(matches!(
            write_entry(&entry, "new-test-token"),
            Err(SecretError::Unavailable)
        ));
        assert_eq!(
            read_entry(&entry).unwrap().as_deref(),
            Some("old-test-token")
        );
        write_entry(&entry, "new-test-token").unwrap();
        assert_eq!(
            read_entry(&entry).unwrap().as_deref(),
            Some("new-test-token")
        );
        fail_next(&entry);
        assert!(matches!(
            delete_entry(&entry),
            Err(SecretError::Unavailable)
        ));
        delete_entry(&entry).unwrap();
        assert!(read_entry(&entry).unwrap().is_none());
    }
}
