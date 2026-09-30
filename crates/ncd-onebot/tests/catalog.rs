//! 目录层的对外行为：用手写的小型夹具走一遍「解析 → 合并 → 对照」。

#![allow(clippy::unwrap_used, clippy::expect_used)]

use ncd_domain::bot_config::BackendType;
use ncd_domain::onebot_debug::{
    DebugActionCategory, DebugActionSafety, DebugCatalogSource, DebugParamDiffKind,
};
use ncd_onebot::catalog::{
    Catalog, inline_refs, param_diff, parse_napcat, parse_snowluma, safety_for,
};
use serde_json::{Value, json};

const NAPCAT_FIXTURE: &str = include_str!("fixtures/napcat_schemas.json");
const SNOWLUMA_FIXTURE: &str = include_str!("fixtures/snowluma_actions.json");

fn fixture(text: &str) -> Value {
    serde_json::from_str(text).expect("夹具应是合法 JSON")
}

fn napcat(source: DebugCatalogSource) -> Catalog {
    parse_napcat(&fixture(NAPCAT_FIXTURE), "fixture", source).expect("NapCat 夹具应能解析")
}

fn snowluma(source: DebugCatalogSource) -> Catalog {
    parse_snowluma(&fixture(SNOWLUMA_FIXTURE), "fixture", source).expect("SnowLuma 夹具应能解析")
}

#[test]
fn napcat_fixture_parses_into_six_actions() {
    let cat = napcat(DebugCatalogSource::Live);
    assert_eq!(cat.backend(), BackendType::NapCat);
    assert_eq!(cat.source(), DebugCatalogSource::Live);
    assert_eq!(cat.version(), "fixture");
    assert_eq!(cat.actions().len(), 6);
}

#[test]
fn napcat_get_login_info() {
    let cat = napcat(DebugCatalogSource::Live);
    let spec = cat.get("get_login_info").unwrap();
    assert_eq!(spec.safety, DebugActionSafety::ReadOnly);
    assert_eq!(spec.category, DebugActionCategory::Account);
    assert_eq!(spec.summary, "获取登录号信息");
    let returns = spec.returns_schema.as_ref().expect("应有返回值 schema");
    assert!(returns["properties"].get("user_id").is_some());
}

#[test]
fn napcat_send_group_msg() {
    let cat = napcat(DebugCatalogSource::Live);
    let spec = cat.get("send_group_msg").unwrap();
    assert_eq!(spec.safety, DebugActionSafety::SideEffect);
    // 上游给它挂的标签是「群组接口」，但它是消息动作
    assert_eq!(spec.category, DebugActionCategory::Message);
    let props = &spec.params_schema["properties"];
    assert_eq!(props["group_id"]["x-ncd-role"], "group_id");
    assert_eq!(props["message"]["x-ncd-role"], "message");
    // 发送动作里的 user_id 是私聊对象，不是群成员
    assert_eq!(props["user_id"]["x-ncd-role"], "user_id");
    assert_eq!(
        spec.examples[0],
        json!({"group_id": "123456", "message": "hello"})
    );
    assert!(!spec.stream);
}

#[test]
fn napcat_delete_msg_is_dangerous() {
    let cat = napcat(DebugCatalogSource::Live);
    let spec = cat.get("delete_msg").unwrap();
    assert_eq!(spec.safety, DebugActionSafety::Dangerous);
    assert_eq!(spec.category, DebugActionCategory::Message);
}

#[test]
fn napcat_get_group_member_info() {
    let cat = napcat(DebugCatalogSource::Live);
    let spec = cat.get("get_group_member_info").unwrap();
    assert_eq!(spec.category, DebugActionCategory::GroupInfo);
    assert_eq!(spec.safety, DebugActionSafety::ReadOnly);
    assert_eq!(
        spec.params_schema["properties"]["user_id"]["x-ncd-role"],
        "member_id"
    );
}

#[test]
fn napcat_upload_file_stream_is_stream() {
    let cat = napcat(DebugCatalogSource::Live);
    let spec = cat.get("upload_file_stream").unwrap();
    assert!(spec.stream);
    assert_eq!(spec.category, DebugActionCategory::Stream);
}

#[test]
fn napcat_action_without_payload_gets_empty_params_schema() {
    let cat = napcat(DebugCatalogSource::Live);
    let spec = cat.get("get_status").unwrap();
    assert_eq!(
        spec.params_schema,
        json!({"type": "object", "properties": {}})
    );
    assert!(spec.examples.is_empty());
}

#[test]
fn snowluma_fixture_parses_into_five_actions() {
    let cat = snowluma(DebugCatalogSource::Live);
    assert_eq!(cat.backend(), BackendType::SnowLuma);
    assert_eq!(cat.actions().len(), 5);
}

#[test]
fn snowluma_get_login_info_falls_back_to_returns_for_summary() {
    let cat = snowluma(DebugCatalogSource::Live);
    let spec = cat.get("get_login_info").unwrap();
    assert_eq!(spec.safety, DebugActionSafety::ReadOnly);
    assert_eq!(spec.category, DebugActionCategory::Account);
    assert!(spec.returns_text.is_some());
    assert_eq!(spec.summary, "当前登录账号的 QQ 号与昵称");
    assert!(spec.returns_schema.is_some());
}

#[test]
fn snowluma_set_group_ban() {
    let cat = snowluma(DebugCatalogSource::Live);
    let spec = cat.get("set_group_ban").unwrap();
    assert_eq!(spec.safety, DebugActionSafety::Dangerous);
    assert_eq!(spec.category, DebugActionCategory::GroupAdmin);
    let props = &spec.params_schema["properties"];
    assert_eq!(props["user_id"]["x-ncd-role"], "member_id");
    assert_eq!(props["user_id"]["x-role"], "member_id");
    assert_eq!(props["group_id"]["x-ncd-role"], "group_id");
}

#[test]
fn snowluma_send_group_msg_infers_role_for_message() {
    let cat = snowluma(DebugCatalogSource::Live);
    let spec = cat.get("send_group_msg").unwrap();
    assert_eq!(spec.category, DebugActionCategory::Message);
    // 上游没给 message 的 role，靠名字补
    assert_eq!(
        spec.params_schema["properties"]["message"]["x-ncd-role"],
        "message"
    );
}

#[test]
fn snowluma_fetch_custom_face_is_read_only_face() {
    let cat = snowluma(DebugCatalogSource::Live);
    let spec = cat.get("fetch_custom_face").unwrap();
    assert_eq!(spec.safety, DebugActionSafety::ReadOnly);
    assert_eq!(spec.category, DebugActionCategory::Face);
}

#[test]
fn snowluma_download_file_stream_is_stream() {
    let cat = snowluma(DebugCatalogSource::Live);
    let spec = cat.get("download_file_stream").unwrap();
    assert!(spec.stream);
    assert_eq!(spec.category, DebugActionCategory::Stream);
    assert_eq!(
        spec.params_schema["properties"]["file"]["x-ncd-role"],
        "file"
    );
}

#[test]
fn merge_keeps_live_presence_and_fills_docs_from_snapshot() {
    let snapshot = napcat(DebugCatalogSource::Snapshot);
    // live：少了 delete_msg，且不带示例（模拟上游 schemas 里没有 payloadExample）
    let mut live_json = fixture(NAPCAT_FIXTURE);
    let live_obj = live_json.as_object_mut().unwrap();
    live_obj.remove("delete_msg");
    for entry in live_obj.values_mut() {
        entry.as_object_mut().unwrap().remove("payloadExample");
    }
    let live = parse_napcat(&live_json, "live-1", DebugCatalogSource::Live).unwrap();
    assert!(live.get("send_group_msg").unwrap().examples.is_empty());

    let merged = Catalog::merge(Some(live), &snapshot);
    assert_eq!(merged.source(), DebugCatalogSource::Live);
    assert_eq!(merged.actions().len(), 6);

    let send = merged.get("send_group_msg").unwrap();
    assert!(send.supported);
    assert_eq!(
        send.examples[0],
        json!({"group_id": "123456", "message": "hello"})
    );

    let delete = merged.get("delete_msg").unwrap();
    assert!(!delete.supported, "live 里没有的接口应保留但标为不支持");
    assert_eq!(delete.safety, DebugActionSafety::Dangerous);
}

#[test]
fn merge_without_live_uses_snapshot_as_is() {
    let snapshot = napcat(DebugCatalogSource::Snapshot);
    let merged = Catalog::merge(None, &snapshot);
    assert_eq!(merged.source(), DebugCatalogSource::Snapshot);
    assert_eq!(merged.actions().len(), 6);
    assert!(merged.actions().iter().all(|a| a.supported));
}

#[test]
fn annotate_other_reports_presence_and_param_differences() {
    let mut nc = napcat(DebugCatalogSource::Snapshot);
    let sl = snowluma(DebugCatalogSource::Snapshot);
    nc.annotate_other(&sl);

    let send = nc
        .get("send_group_msg")
        .unwrap()
        .other_backend
        .as_ref()
        .unwrap();
    assert!(send.present);
    assert_eq!(send.backend, BackendType::SnowLuma);
    for field in [
        "message_type",
        "user_id",
        "source",
        "news",
        "summary",
        "prompt",
    ] {
        assert!(
            send.diffs
                .iter()
                .any(|d| d.name == field && d.diff == DebugParamDiffKind::OnlyHere),
            "应报告 {field} 只在 NapCat 有: {:?}",
            send.diffs
        );
    }
    // 字符串 ID 对整数 ID、布尔对 布尔|字符串 是两个后端的惯常差别，不该报类型差异
    assert!(
        !send
            .diffs
            .iter()
            .any(|d| matches!(d.diff, DebugParamDiffKind::TypeDiffers { .. })),
        "{:?}",
        send.diffs
    );
    // NapCat 没把 group_id 标成必填，SnowLuma 标了
    assert!(send.diffs.iter().any(|d| d.name == "group_id"
        && d.diff
            == DebugParamDiffKind::RequiredDiffers {
                here: false,
                other: true
            }));

    let delete = nc
        .get("delete_msg")
        .unwrap()
        .other_backend
        .as_ref()
        .unwrap();
    assert!(!delete.present);
    assert!(delete.diffs.is_empty());

    let rows = nc.summaries();
    let send_row = rows.iter().find(|r| r.name == "send_group_msg").unwrap();
    assert_eq!(send_row.other_backend_present, Some(true));
    // 只是 NapCat 多了几个可选参数、必填与否不同，照 SnowLuma 的写法发过去照样能调
    assert!(!send.breaking);
    assert!(!send_row.param_diff);
    let login_row = rows.iter().find(|r| r.name == "get_login_info").unwrap();
    assert_eq!(login_row.other_backend_present, Some(true));
    assert!(!login_row.param_diff, "两边 get_login_info 都没有参数");
}

#[test]
fn debug_catalog_carries_backend_source_and_version() {
    let cat = snowluma(DebugCatalogSource::Snapshot);
    let dc = cat.to_debug_catalog();
    assert_eq!(dc.backend, BackendType::SnowLuma);
    assert_eq!(dc.source, DebugCatalogSource::Snapshot);
    assert_eq!(dc.snapshot_version, "fixture");
    assert_eq!(dc.actions.len(), 5);
}

#[test]
fn safety_for_public_table() {
    assert_eq!(safety_for("bot_exit", None), DebugActionSafety::Dangerous);
    assert_eq!(safety_for("get_x", None), DebugActionSafety::ReadOnly);
    assert_eq!(safety_for(".ocr_image", None), DebugActionSafety::ReadOnly);
    assert_eq!(safety_for("send_like", None), DebugActionSafety::SideEffect);
    assert_eq!(safety_for("foo", Some(true)), DebugActionSafety::ReadOnly);
}

#[test]
fn inline_refs_and_param_diff_are_reachable_from_the_crate_root() {
    let defs = json!({"Id": {"type": "string"}});
    let defs = defs.as_object().unwrap();
    let inlined = inline_refs(&json!({"properties": {"a": {"$ref": "#/defs/Id"}}}), defs);
    assert!(param_diff(&inlined, &json!({"properties": {}})).len() == 1);
}
