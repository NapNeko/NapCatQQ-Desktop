use std::collections::HashMap;
use std::fmt;
use std::sync::Mutex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MockSecretStoreError {
    InjectedFailure(String),
}

impl fmt::Display for MockSecretStoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InjectedFailure(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for MockSecretStoreError {}

#[derive(Debug, Default)]
pub struct MockSecretStore {
    secrets: Mutex<HashMap<String, String>>,
    next_put_failure: Mutex<Option<String>>,
    next_get_failure: Mutex<Option<String>>,
    next_delete_failure: Mutex<Option<String>>,
}

impl MockSecretStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_entries<I, K, V>(entries: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        let mut secrets = HashMap::new();
        for (key, value) in entries {
            secrets.insert(key.into(), value.into());
        }

        Self {
            secrets: Mutex::new(secrets),
            next_put_failure: Mutex::new(None),
            next_get_failure: Mutex::new(None),
            next_delete_failure: Mutex::new(None),
        }
    }

    pub fn fail_next_put(&self, message: impl Into<String>) {
        *self
            .next_put_failure
            .lock()
            .expect("mock secret store mutex poisoned") = Some(message.into());
    }

    pub fn fail_next_get(&self, message: impl Into<String>) {
        *self
            .next_get_failure
            .lock()
            .expect("mock secret store mutex poisoned") = Some(message.into());
    }

    pub fn fail_next_delete(&self, message: impl Into<String>) {
        *self
            .next_delete_failure
            .lock()
            .expect("mock secret store mutex poisoned") = Some(message.into());
    }

    pub fn put(
        &self,
        key: impl Into<String>,
        value: impl Into<String>,
    ) -> Result<(), MockSecretStoreError> {
        if let Some(message) = self
            .next_put_failure
            .lock()
            .expect("mock secret store mutex poisoned")
            .take()
        {
            return Err(MockSecretStoreError::InjectedFailure(message));
        }

        self.secrets
            .lock()
            .expect("mock secret store mutex poisoned")
            .insert(key.into(), value.into());
        Ok(())
    }

    pub fn get(&self, key: &str) -> Result<Option<String>, MockSecretStoreError> {
        if let Some(message) = self
            .next_get_failure
            .lock()
            .expect("mock secret store mutex poisoned")
            .take()
        {
            return Err(MockSecretStoreError::InjectedFailure(message));
        }

        Ok(self
            .secrets
            .lock()
            .expect("mock secret store mutex poisoned")
            .get(key)
            .cloned())
    }

    pub fn delete(&self, key: &str) -> Result<bool, MockSecretStoreError> {
        if let Some(message) = self
            .next_delete_failure
            .lock()
            .expect("mock secret store mutex poisoned")
            .take()
        {
            return Err(MockSecretStoreError::InjectedFailure(message));
        }

        Ok(self
            .secrets
            .lock()
            .expect("mock secret store mutex poisoned")
            .remove(key)
            .is_some())
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.secrets
            .lock()
            .expect("mock secret store mutex poisoned")
            .contains_key(key)
    }

    pub fn len(&self) -> usize {
        self.secrets
            .lock()
            .expect("mock secret store mutex poisoned")
            .len()
    }

    pub fn snapshot(&self) -> HashMap<String, String> {
        self.secrets
            .lock()
            .expect("mock secret store mutex poisoned")
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_and_reads_values() {
        let store = MockSecretStore::new();
        store.put("token", "abc").unwrap();
        assert_eq!(store.get("token").unwrap(), Some("abc".to_string()));
    }

    #[test]
    fn injects_failures_once() {
        let store = MockSecretStore::new();
        store.fail_next_put("boom");
        let error = store.put("token", "abc").unwrap_err();
        assert_eq!(
            error,
            MockSecretStoreError::InjectedFailure("boom".to_string())
        );
        assert_eq!(store.get("token").unwrap(), None);
    }
}


// ============================================================
// impl SecretStore(让 MockSecretStore 直接当 ncd-traits::SecretStore 实装用)
// ============================================================

use ncd_domain::errors::SecretError;
use ncd_traits::SecretStore;

/// 把 MockSecretStoreError 映射成 SecretError(注入失败 = 模拟存储不可用)。
impl From<MockSecretStoreError> for SecretError {
    fn from(_value: MockSecretStoreError) -> Self {
        SecretError::Unavailable
    }
}

impl SecretStore for MockSecretStore {
    fn get(&self, key: &str) -> Result<Option<String>, SecretError> {
        // 注意:这里调用的是 inherent method `get`(返回 MockSecretStoreError),
        // 通过 ? 自动转换到 SecretError
        Self::get(self, key).map_err(SecretError::from)
    }

    fn put(&self, key: &str, value: &str) -> Result<(), SecretError> {
        Self::put(self, key, value).map_err(SecretError::from)
    }

    fn delete(&self, key: &str) -> Result<(), SecretError> {
        // inherent delete 返回 Result<bool>,trait delete 返回 Result<()>
        // 这里把 bool 信息丢弃(trait 不区分 "key 之前存在" 与 "key 不存在")
        Self::delete(self, key)
            .map(|_| ())
            .map_err(SecretError::from)
    }
}

#[cfg(test)]
mod trait_impl_tests {
    use super::*;

    #[test]
    fn impl_works_via_trait_object() {
        let store: Box<dyn SecretStore> = Box::new(MockSecretStore::new());
        store.put("token", "abc").unwrap();
        let v = store.get("token").unwrap();
        assert_eq!(v.as_deref(), Some("abc"));
        store.delete("token").unwrap();
        let v = store.get("token").unwrap();
        assert_eq!(v, None);
    }

    #[test]
    fn injected_failure_maps_to_unavailable() {
        let store = MockSecretStore::new();
        store.fail_next_get("network down");
        let err = SecretStore::get(&store, "any").unwrap_err();
        assert!(matches!(err, SecretError::Unavailable));
    }
}
