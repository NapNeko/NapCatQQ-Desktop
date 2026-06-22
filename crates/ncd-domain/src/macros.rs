/// serde `default` 属性用: bool 字段缺失时默认 true
pub(crate) fn default_true() -> bool {
    true
}

/// 生成基于字符串的 enum，自动实现 Serialize / Deserialize / From<String>。
///
/// enum 自动追加 `Unknown(String)` 作为兜底变体，未知输入不会反序列化失败。
/// 序列化时 `Unknown` 回写原始字符串，round-trip 无损。
///
/// 调用方负责 derive `Debug, Clone, PartialEq, Eq`、TS 和 Default（如需）。
macro_rules! string_enum {
    (
        $(#[$meta:meta])*
        pub enum $name:ident {
            $(
                $(#[$vmeta:meta])*
                $variant:ident => $str:expr
            ),+
            $(,)?
        }
    ) => {
        $(#[$meta])*
        pub enum $name {
            $(
                $(#[$vmeta])*
                $variant,
            )+
            /// 未知值兜底, 序列化回原始字符串保证 round-trip 无损
            Unknown(String),
        }

        impl serde::Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                match self {
                    $(Self::$variant => serializer.serialize_str($str),)+
                    Self::Unknown(s) => serializer.serialize_str(s),
                }
            }
        }

        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                let s = String::deserialize(deserializer)?;
                Ok(Self::from(s))
            }
        }

        impl From<String> for $name {
            fn from(s: String) -> Self {
                match s.as_str() {
                    $($str => Self::$variant,)+
                    _ => Self::Unknown(s),
                }
            }
        }

        impl From<&str> for $name {
            fn from(s: &str) -> Self {
                Self::from(s.to_owned())
            }
        }
    };
}
