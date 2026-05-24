use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BotId(String);

impl BotId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for BotId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<String> for BotId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl From<&str> for BotId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl AsRef<str> for BotId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BackendId(String);

impl BackendId {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for BackendId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<String> for BackendId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl From<&str> for BackendId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl AsRef<str> for BackendId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_ids_as_strings() {
        let bot_id = BotId::new("10001");
        let backend_id = BackendId::new("server-1");

        assert_eq!(serde_json::to_string(&bot_id).unwrap(), "\"10001\"");
        assert_eq!(serde_json::to_string(&backend_id).unwrap(), "\"server-1\"");
    }
}
