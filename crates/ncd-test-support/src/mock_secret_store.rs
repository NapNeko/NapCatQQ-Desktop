use std::collections::HashMap;
use std::sync::Mutex;

use ncd_domain::errors::SecretError;
use ncd_traits::SecretStore;

#[derive(Debug, Default)]
pub struct MockSecretStore {
    state: Mutex<State>,
}

#[derive(Debug, Default)]
struct State {
    secrets: HashMap<String, String>,
    fail_next_put: bool,
}

impl MockSecretStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn fail_next_put(&self) {
        self.state.lock().unwrap().fail_next_put = true;
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.state.lock().unwrap().secrets.contains_key(key)
    }

    pub fn len(&self) -> usize {
        self.state.lock().unwrap().secrets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl SecretStore for MockSecretStore {
    fn get(&self, key: &str) -> Result<Option<String>, SecretError> {
        Ok(self.state.lock().unwrap().secrets.get(key).cloned())
    }

    fn put(&self, key: &str, value: &str) -> Result<(), SecretError> {
        let mut state = self.state.lock().unwrap();
        if std::mem::take(&mut state.fail_next_put) {
            return Err(SecretError::Unavailable);
        }
        state.secrets.insert(key.to_owned(), value.to_owned());
        Ok(())
    }

    fn delete(&self, key: &str) -> Result<(), SecretError> {
        self.state.lock().unwrap().secrets.remove(key);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_write_preserves_the_secret_and_is_consumed_once() {
        let store = MockSecretStore::new();
        let secrets: &dyn SecretStore = &store;
        secrets.put("token", "old").unwrap();
        store.fail_next_put();
        assert!(matches!(
            secrets.put("token", "new"),
            Err(SecretError::Unavailable)
        ));
        assert_eq!(secrets.get("token").unwrap().as_deref(), Some("old"));
        secrets.put("token", "new").unwrap();
        assert_eq!(secrets.get("token").unwrap().as_deref(), Some("new"));
        secrets.delete("token").unwrap();
        assert!(store.is_empty());
    }
}
