//! 随包快照：真实的上游产物必须能解析，且和 R2 里手写的规则（危险清单、分类）对得上。
//!
//! 快照文件由 `pnpm run catalog:snapshots` 重新生成；这里的断言是给「重新生成后有没有把
//! 什么弄坏」把关的，所以用的是真实数据而不是夹具。

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::BTreeSet;

use ncd_domain::bot_config::BackendType;
use ncd_domain::onebot_debug::DebugCatalogSource;
use ncd_onebot::catalog::{Catalog, DANGEROUS, parse_napcat, parse_snowluma, snapshot};
use serde_json::Value;

const NAPCAT_JSON: &str = include_str!("../src/catalog/snapshot/napcat.json");
const SNOWLUMA_JSON: &str = include_str!("../src/catalog/snapshot/snowluma.json");

fn parse(text: &str) -> (Value, String) {
    let value: Value = serde_json::from_str(text).expect("快照应是合法 JSON");
    let version = value["version"]
        .as_str()
        .expect("快照应带版本号")
        .to_owned();
    (value, version)
}

fn napcat() -> Catalog {
    let (value, version) = parse(NAPCAT_JSON);
    parse_napcat(&value, &version, DebugCatalogSource::Snapshot).expect("NapCat 快照应能解析")
}

fn snowluma() -> Catalog {
    let (value, version) = parse(SNOWLUMA_JSON);
    parse_snowluma(&value, &version, DebugCatalogSource::Snapshot).expect("SnowLuma 快照应能解析")
}

#[test]
fn both_snapshots_parse_with_enough_actions() {
    for (label, catalog) in [("NapCat", napcat()), ("SnowLuma", snowluma())] {
        assert!(
            catalog.actions().len() >= 150,
            "{label} 快照只有 {} 个动作",
            catalog.actions().len()
        );
        assert_eq!(catalog.source(), DebugCatalogSource::Snapshot);
        assert_ne!(catalog.version(), "invalid");
    }
}

#[test]
fn loader_serves_the_same_catalogs() {
    assert_eq!(
        snapshot(BackendType::NapCat).actions().len(),
        napcat().actions().len()
    );
    assert_eq!(
        snapshot(BackendType::SnowLuma).actions().len(),
        snowluma().actions().len()
    );
    assert_eq!(
        snapshot(BackendType::NapCat).version(),
        napcat().version(),
        "版本号应取自快照文件"
    );
}

#[test]
fn every_dangerous_name_exists_in_at_least_one_snapshot() {
    let (nc, sl) = (napcat(), snowluma());
    let unknown: BTreeSet<&str> = DANGEROUS
        .iter()
        .copied()
        .filter(|name| nc.get(name).is_none() && sl.get(name).is_none())
        .collect();
    assert!(
        unknown.is_empty(),
        "DANGEROUS 里有两份快照都没有的动作名（拼写错了或上游已删除）：{unknown:?}"
    );
}

#[test]
fn every_action_has_a_safety_and_a_name() {
    // `safety` 是枚举、不会缺；这里防的是快照里混进空名字，或安全等级全被判成同一个值
    for catalog in [napcat(), snowluma()] {
        assert!(catalog.actions().iter().all(|a| !a.name.is_empty()));
        let levels: BTreeSet<String> = catalog
            .actions()
            .iter()
            .map(|a| format!("{:?}", a.safety))
            .collect();
        assert_eq!(
            levels.len(),
            3,
            "{:?} 的安全等级应三种都有，实际 {levels:?}",
            catalog.backend()
        );
    }
}

#[test]
fn send_group_msg_exists_in_both_and_pairs_up() {
    let (mut nc, mut sl) = (napcat(), snowluma());
    assert!(nc.get("send_group_msg").is_some());
    assert!(sl.get("send_group_msg").is_some());

    let (nc_copy, sl_copy) = (nc.clone(), sl.clone());
    nc.annotate_other(&sl_copy);
    sl.annotate_other(&nc_copy);

    let from_nc = nc
        .get("send_group_msg")
        .unwrap()
        .other_backend
        .as_ref()
        .expect("annotate_other 后应有对照信息");
    assert!(from_nc.present);
    assert_eq!(from_nc.backend, BackendType::SnowLuma);

    let from_sl = sl
        .get("send_group_msg")
        .unwrap()
        .other_backend
        .as_ref()
        .expect("annotate_other 后应有对照信息");
    assert!(from_sl.present);
    assert_eq!(from_sl.backend, BackendType::NapCat);
}

#[test]
fn napcat_get_login_info_carries_docs_from_the_openapi() {
    let nc = napcat();
    let spec = nc.get("get_login_info").unwrap();
    let example = spec.return_example.as_ref().expect("应有返回示例");
    assert!(example.get("user_id").is_some(), "{example}");
    // `response` 是 `$ref: OB11User`，解析后应已展开成自包含的 schema
    let returns = spec.returns_schema.as_ref().expect("应有返回值 schema");
    assert!(returns["properties"].get("user_id").is_some());
    assert!(!spec.error_examples.is_empty());
    assert!(!spec.examples.is_empty());
}

#[test]
fn napcat_schemas_are_self_contained() {
    // 展开后不该还留着指向 `defs` 的引用，否则前端表单会看到一个不认识的 `$ref`
    let nc = napcat();
    let dangling: Vec<&str> = nc
        .actions()
        .iter()
        .filter(|a| {
            let text = serde_json::to_string(&(&a.params_schema, &a.returns_schema)).unwrap();
            text.contains("#/components/schemas/")
        })
        .map(|a| a.name.as_str())
        .collect();
    assert!(
        dangling.is_empty(),
        "{} 个动作的 schema 里还有未展开的引用：{dangling:?}",
        dangling.len()
    );
}

#[test]
fn snowluma_snapshot_keeps_upstream_docs() {
    let sl = snowluma();
    let spec = sl.get("get_login_info").unwrap();
    assert!(spec.returns_text.is_some());
    // 上游有 4 个相册动作既没写 summary 也没写返回说明，属于文档本身的缺口；
    // 这里只防「摘要整体丢了」（比如生成脚本改坏了字段名）
    let without_summary = sl.actions().iter().filter(|a| a.summary.is_empty()).count();
    assert!(
        without_summary * 10 < sl.actions().len(),
        "{without_summary} 个动作没有摘要"
    );
}
