//! 本地已安装的 core 版本快照。BootstrapSnapshot 的子结构。
//!
//! 解析失败一律返回 None：UI 层"显示未安装"是足够的语义，不需要把解析
//! 错误暴露给用户。装配位置在 src-tauri/src/bootstrap.rs::detect_local_versions。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// 本地已安装的 core 版本快照。
///
/// 字段语义：
/// - Some("4.18.1")：已安装且解析到版本号
/// - None：未安装 / 安装文件不存在 / 解析失败（fallback，不抛错）
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct LocalVersionSnapshot {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub napcat: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snowluma: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 字节级 round-trip：默认值（两字段 None）应当序列化为空对象 {}，
    /// 反序列化回默认值；这是 #[serde(default)] + skip_serializing_if
    /// 组合的契约：BootstrapSnapshot 老缓存少这两个字段时也能反序列化。
    #[test]
    fn default_round_trips_as_empty_object() {
        let value = LocalVersionSnapshot::default();
        let serialized = serde_json::to_string(&value).expect("serialize default");
        assert_eq!(serialized, "{}");

        let decoded: LocalVersionSnapshot =
            serde_json::from_str(&serialized).expect("deserialize default");
        assert_eq!(decoded, value);
    }

    #[test]
    fn populated_round_trips_byte_stable() {
        let value = LocalVersionSnapshot {
            napcat: Some("4.18.1".to_string()),
            snowluma: Some("0.3.2".to_string()),
        };
        let serialized = serde_json::to_string(&value).expect("serialize populated");
        assert_eq!(
            serialized,
            r#"{"napcat":"4.18.1","snowluma":"0.3.2"}"#
        );

        let decoded: LocalVersionSnapshot =
            serde_json::from_str(&serialized).expect("deserialize populated");
        assert_eq!(decoded, value);
    }

    /// 老缓存里没有这两个字段，反序列化应当回落到 None；保证 schema
    /// 演进期间历史 BootstrapSnapshot 缓存仍可读。
    #[test]
    fn missing_fields_default_to_none() {
        let decoded: LocalVersionSnapshot =
            serde_json::from_str("{}").expect("deserialize empty");
        assert_eq!(decoded.napcat, None);
        assert_eq!(decoded.snowluma, None);
    }
}
