use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackendKind {
    Local,
    RemoteSsh,
}

impl BackendKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::RemoteSsh => "remote_ssh",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BotFlavor {
    NapCat,
    SnowLuma,
}

impl BotFlavor {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NapCat => "napcat",
            Self::SnowLuma => "snowluma",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeTarget {
    Local,
    Server(String),
}

impl RuntimeTarget {
    pub const fn local() -> Self {
        Self::Local
    }

    pub fn server(value: impl Into<String>) -> Self {
        Self::Server(value.into())
    }

    pub fn is_local(&self) -> bool {
        matches!(self, Self::Local)
    }

    pub fn server_id(&self) -> Option<&str> {
        match self {
            Self::Local => None,
            Self::Server(id) => Some(id.as_str()),
        }
    }
}

impl Serialize for RuntimeTarget {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Local => serializer.serialize_str("local"),
            Self::Server(id) => serializer.serialize_str(id),
        }
    }
}

impl<'de> Deserialize<'de> for RuntimeTarget {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct RuntimeTargetVisitor;

        impl<'de> de::Visitor<'de> for RuntimeTargetVisitor {
            type Value = RuntimeTarget;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a runtime target string")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(RuntimeTarget::from(value))
            }

            fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
            where
                E: de::Error,
            {
                Ok(RuntimeTarget::from(value))
            }
        }

        deserializer.deserialize_str(RuntimeTargetVisitor)
    }
}

impl From<String> for RuntimeTarget {
    fn from(value: String) -> Self {
        if value.eq_ignore_ascii_case("local") {
            Self::Local
        } else {
            Self::Server(value)
        }
    }
}

impl From<&str> for RuntimeTarget {
    fn from(value: &str) -> Self {
        Self::from(value.to_owned())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SchemaVersion(pub u16);

impl SchemaVersion {
    pub const V1: Self = Self(1);
    pub const V2: Self = Self(2);
    pub const V3: Self = Self(3);

    pub const fn new(value: u16) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u16 {
        self.0
    }
}

impl From<u16> for SchemaVersion {
    fn from(value: u16) -> Self {
        Self::new(value)
    }
}

impl From<SchemaVersion> for u16 {
    fn from(value: SchemaVersion) -> Self {
        value.get()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_bot_flavor_and_runtime_target() {
        assert_eq!(
            serde_json::to_string(&BotFlavor::NapCat).unwrap(),
            "\"napcat\""
        );
        assert_eq!(
            serde_json::to_string(&RuntimeTarget::server("remote-a")).unwrap(),
            "\"remote-a\""
        );
        assert_eq!(
            serde_json::to_string(&RuntimeTarget::Local).unwrap(),
            "\"local\""
        );
    }
}
