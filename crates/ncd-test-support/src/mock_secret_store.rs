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
