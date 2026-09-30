//! 随包的动作目录快照：连不上上游、或上游太旧没有调试接口时，界面仍能查文档。
//!
//! 两份 JSON 由 `scripts/onebot-catalog/build-snapshots.mjs` 从上游产物生成，
//! 编进二进制里，不在运行时读盘。

use std::sync::LazyLock;

use ncd_domain::bot_config::BackendType;
use ncd_domain::onebot_debug::DebugCatalogSource;
use serde_json::Value;

use super::model::{Catalog, CatalogError};
use super::{parse_napcat, parse_snowluma};

const NAPCAT_JSON: &str = include_str!("snapshot/napcat.json");
const SNOWLUMA_JSON: &str = include_str!("snapshot/snowluma.json");

/// 快照坏掉时目录上显示的版本号，界面据此能看出「这不是一份正常的快照」
const INVALID_VERSION: &str = "invalid";

static NAPCAT: LazyLock<Catalog> =
    LazyLock::new(|| load_or_empty(BackendType::NapCat, NAPCAT_JSON));
static SNOWLUMA: LazyLock<Catalog> =
    LazyLock::new(|| load_or_empty(BackendType::SnowLuma, SNOWLUMA_JSON));

/// 某个后端随包的目录快照。首次调用时解析（NapCat 那份要展开 `$ref`，不算便宜），之后复用。
///
/// 快照是构建期生成的，正常不会坏；万一坏了（脚本出过错又被提交了），
/// 记一条 error 日志并返回一个空目录，调试台照样能开，只是没有离线文档
pub fn snapshot(backend: BackendType) -> &'static Catalog {
    match backend {
        BackendType::NapCat => &NAPCAT,
        BackendType::SnowLuma => &SNOWLUMA,
    }
}

#[derive(Debug, thiserror::Error)]
enum SnapshotError {
    #[error("快照不是合法的 JSON：{0}")]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Catalog(#[from] CatalogError),
}

fn load_or_empty(backend: BackendType, text: &str) -> Catalog {
    match parse_snapshot(backend, text) {
        Ok(catalog) => catalog,
        Err(err) => {
            tracing::error!(?backend, error = %err, "随包的动作目录快照解析失败，改用空目录");
            Catalog::empty(backend, INVALID_VERSION)
        }
    }
}

fn parse_snapshot(backend: BackendType, text: &str) -> Result<Catalog, SnapshotError> {
    let value: Value = serde_json::from_str(text)?;
    let version = value
        .get("version")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let catalog = match backend {
        BackendType::NapCat => parse_napcat(&value, version, DebugCatalogSource::Snapshot)?,
        BackendType::SnowLuma => parse_snowluma(&value, version, DebugCatalogSource::Snapshot)?,
    };
    Ok(catalog)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_snapshots_parse_and_are_not_tiny() {
        for (backend, text) in [
            (BackendType::NapCat, NAPCAT_JSON),
            (BackendType::SnowLuma, SNOWLUMA_JSON),
        ] {
            let catalog = parse_snapshot(backend, text)
                .unwrap_or_else(|err| panic!("{backend:?} 快照应能解析：{err}"));
            assert!(
                catalog.actions().len() >= 150,
                "{backend:?} 只有 {} 个动作",
                catalog.actions().len()
            );
            assert_ne!(catalog.version(), "unknown", "{backend:?} 快照缺少版本号");
            assert_eq!(catalog.source(), DebugCatalogSource::Snapshot);
        }
    }

    #[test]
    fn snapshot_returns_the_cached_catalog_for_each_backend() {
        let napcat = snapshot(BackendType::NapCat);
        let snowluma = snapshot(BackendType::SnowLuma);
        assert_eq!(napcat.backend(), BackendType::NapCat);
        assert_eq!(snowluma.backend(), BackendType::SnowLuma);
        assert!(std::ptr::eq(napcat, snapshot(BackendType::NapCat)));
        assert!(!napcat.actions().is_empty());
        assert!(!snowluma.actions().is_empty());
    }

    #[test]
    fn broken_snapshot_falls_back_to_an_empty_catalog() {
        for text in ["", "not json", "[]", r#"{"actions": 3}"#] {
            for backend in [BackendType::NapCat, BackendType::SnowLuma] {
                let catalog = load_or_empty(backend, text);
                assert_eq!(catalog.backend(), backend, "{text:?}");
                assert_eq!(catalog.version(), INVALID_VERSION, "{text:?}");
                assert!(catalog.actions().is_empty(), "{text:?}");
            }
        }
    }
}
