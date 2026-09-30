//! 两个后端的文档都没有可靠给出的东西，靠名字和已知清单补：
//! 动作的副作用等级、分类（名字启发式），以及参数的「角色」（前端据此给选择器）。

use ncd_domain::onebot_debug::{DebugActionCategory, DebugActionSafety};
use serde_json::Value;

/// 会让账号 / 群 / 数据发生不可逆变化的动作，调用前前端要二次确认。
/// 名字按原样比对（含 `_` 前缀），别名要单独列
pub const DANGEROUS: &[&str] = &[
    "bot_exit",
    "set_restart",
    "set_group_leave",
    "set_group_kick",
    "set_group_kick_members",
    "set_group_ban",
    "set_group_whole_ban",
    "delete_msg",
    "delete_friend",
    "clean_cache",
    "delete_group_file",
    "delete_group_folder",
    "set_group_admin",
    "_del_group_notice",
    "delete_flash_file",
    "delete_essence_msg",
    // 以下是对照两个后端的真实目录后补的破坏类动作：删文件夹 / 表情 / 相册 / 说说，
    // 取消待办，改群成员权限与邀请策略，禁言（匿名 / 空间）。名字都能在至少一份上游目录里找到
    "delete_group_file_folder",
    "delete_custom_face",
    "del_group_album_media",
    "delete_qzone_msg",
    "cancel_group_todo",
    "set_group_member_permissions",
    "set_group_anonymous_ban",
    "set_qzone_ban",
    "set_group_member_invite_policy",
];

/// 名字看着像有副作用、其实只读的动作（OCR、分词、翻译、取状态）。
/// 排在前缀规则之前，免得被当成 `SideEffect`
pub const READ_ONLY_EXTRA: &[&str] = &[
    ".ocr_image",
    "ocr_image",
    ".get_word_slices",
    "translate_en2zh",
    "nc_get_packet_status",
    "nc_get_user_status",
    "nc_get_rkey",
];

/// 安全等级判定用的只读前缀
const SAFETY_READ_PREFIXES: &[&str] = &["get_", "fetch_", "can_", "check_", "list_", "search_"];

/// 群相关动作里区分「查」和「管」用的前缀，比安全判定少 `list_` / `search_`：
/// 群里没有叫这两个名字的管理动作，收窄只是和文档口径保持一致
const CATEGORY_READ_PREFIXES: &[&str] = &["get_", "fetch_", "can_", "check_"];

/// 去掉动作名开头的 `.` / `_`，只用于判前缀：`_get_group_notice` 也是只读的
fn strip_marker(name: &str) -> &str {
    name.trim_start_matches(['.', '_'])
}

fn has_prefix(name: &str, prefixes: &[&str]) -> bool {
    let bare = strip_marker(name);
    prefixes.iter().any(|p| bare.starts_with(p))
}

/// 动作的副作用等级。
///
/// 判定顺序：危险清单 → 只读补充清单 → 上游文档给的 `readOnly` 标记 → 名字前缀。
/// 危险清单排最前，即便上游把它标成只读也要确认
pub fn safety_for(name: &str, read_only_hint: Option<bool>) -> DebugActionSafety {
    if DANGEROUS.contains(&name) {
        return DebugActionSafety::Dangerous;
    }
    if READ_ONLY_EXTRA.contains(&name) {
        return DebugActionSafety::ReadOnly;
    }
    match read_only_hint {
        Some(true) => return DebugActionSafety::ReadOnly,
        Some(false) => return DebugActionSafety::SideEffect,
        None => {}
    }
    if has_prefix(name, SAFETY_READ_PREFIXES) {
        DebugActionSafety::ReadOnly
    } else {
        DebugActionSafety::SideEffect
    }
}

/// 只凭动作名猜分类。上游没给分类，或给的是「扩展」这种没信息量的标签时用。
///
/// 顺序有讲究：越具体的关键词越靠前（`upload_file_stream` 要落到 Stream 而不是 File，
/// `set_msg_emoji_like` 要落到 Face 而不是 Message / Friend）
pub fn category_from_name(name: &str) -> DebugActionCategory {
    let has = |words: &[&str]| words.iter().any(|w| name.contains(w));

    if has(&["_request"]) {
        DebugActionCategory::Request
    } else if has(&["face", "emoji"]) {
        DebugActionCategory::Face
    } else if has(&["_stream"]) {
        DebugActionCategory::Stream
    } else if has(&["group_file", "group_folder", "_file", "fileset", "flash"]) {
        DebugActionCategory::File
    } else if has(&["forward", "_msg", "poke", "record", "image"]) {
        DebugActionCategory::Message
    } else if has(&["friend", "stranger", "user", "like"]) {
        DebugActionCategory::Friend
    } else if has(&["group"]) {
        group_category(name)
    } else if has(&[
        "login",
        "status",
        "version",
        "cookies",
        "csrf",
        "credentials",
        "restart",
        "cache",
        "rkey",
        "online",
        // 退出 / 重启这类进程级动作也归账号
        "exit",
        "quit",
    ]) {
        DebugActionCategory::Account
    } else {
        DebugActionCategory::Extension
    }
}

/// 群相关动作：查询类归 GroupInfo，其余（设置 / 踢 / 禁言 …）归 GroupAdmin
pub fn group_category(name: &str) -> DebugActionCategory {
    if has_prefix(name, CATEGORY_READ_PREFIXES) {
        DebugActionCategory::GroupInfo
    } else {
        DebugActionCategory::GroupAdmin
    }
}

/// 猜参数的角色，前端据此换成群 / 成员 / 消息选择器。猜不出返回 `None`。
///
/// `siblings` 是同一个动作的全部参数名：`user_id` 在带 `group_id` 的动作里
/// 通常是「群成员」，而 `send_*` / `*_msg` 里的 `user_id` 是私聊对象，不是群成员
pub fn infer_role(action: &str, field: &str, siblings: &[&str]) -> Option<&'static str> {
    match field {
        "group_id" => Some("group_id"),
        "user_id" => {
            let is_send = action.starts_with("send_") || action.ends_with("_msg");
            if siblings.contains(&"group_id") && !is_send {
                Some("member_id")
            } else {
                Some("user_id")
            }
        }
        "message_id" => Some("message_id"),
        "message" => Some("message"),
        "face_id" | "emoji_id" => Some("face_id"),
        // 同叫 `file`，在图片 / 语音 / 视频类动作里是对应媒体，别当成通用文件
        "file" => Some(if action.contains("image") {
            "image"
        } else if action.contains("record") {
            "record"
        } else if action.contains("video") {
            "video"
        } else {
            "file"
        }),
        "time" | "timestamp" => Some("timestamp"),
        _ => None,
    }
}

/// 给参数 schema 顶层的每个属性标上 `x-ncd-role`。
///
/// 优先级：已有的 `x-ncd-role` → 上游自带的 `x-role`（SnowLuma 给的，比猜的准）→ [`infer_role`]。
/// 这样前端只认 `x-ncd-role` 一个字段，不用关心是哪个后端的目录
pub fn annotate_roles(action: &str, schema: &mut Value) {
    let Some(props) = schema.get_mut("properties").and_then(Value::as_object_mut) else {
        return;
    };
    let names: Vec<String> = props.keys().cloned().collect();
    let siblings: Vec<&str> = names.iter().map(String::as_str).collect();
    for (field, prop) in props.iter_mut() {
        let Some(prop) = prop.as_object_mut() else {
            continue;
        };
        if prop.contains_key("x-ncd-role") {
            continue;
        }
        let role = prop
            .get("x-role")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| infer_role(action, field, &siblings).map(str::to_owned));
        if let Some(role) = role {
            prop.insert("x-ncd-role".to_owned(), Value::String(role));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use serde_json::json;

    use super::*;

    #[test]
    fn safety_table() {
        let cases: &[(&str, Option<bool>, DebugActionSafety)] = &[
            ("bot_exit", None, DebugActionSafety::Dangerous),
            ("bot_exit", Some(true), DebugActionSafety::Dangerous),
            ("_del_group_notice", None, DebugActionSafety::Dangerous),
            ("get_x", None, DebugActionSafety::ReadOnly),
            ("_get_group_notice", None, DebugActionSafety::ReadOnly),
            (".get_word_slices", None, DebugActionSafety::ReadOnly),
            (".ocr_image", None, DebugActionSafety::ReadOnly),
            ("ocr_image", Some(false), DebugActionSafety::ReadOnly),
            ("send_like", None, DebugActionSafety::SideEffect),
            ("foo", Some(true), DebugActionSafety::ReadOnly),
            ("foo", Some(false), DebugActionSafety::SideEffect),
            ("get_thing", Some(false), DebugActionSafety::SideEffect),
            ("search_x", None, DebugActionSafety::ReadOnly),
            ("list_x", None, DebugActionSafety::ReadOnly),
            ("can_send_image", None, DebugActionSafety::ReadOnly),
            ("send_group_msg", None, DebugActionSafety::SideEffect),
        ];
        for (name, hint, want) in cases {
            assert_eq!(safety_for(name, *hint), *want, "{name} {hint:?}");
        }
    }

    #[test]
    fn destructive_actions_found_in_real_catalogs_are_dangerous() {
        for name in [
            "delete_group_file_folder",
            "delete_custom_face",
            "del_group_album_media",
            "delete_qzone_msg",
            "cancel_group_todo",
            "set_group_member_permissions",
            "set_group_anonymous_ban",
            "set_qzone_ban",
            "set_group_member_invite_policy",
        ] {
            // 上游即便标了 readOnly 也要确认
            assert_eq!(
                safety_for(name, Some(true)),
                DebugActionSafety::Dangerous,
                "{name}"
            );
            assert_eq!(
                safety_for(name, None),
                DebugActionSafety::Dangerous,
                "{name}"
            );
        }
    }

    #[test]
    fn dangerous_list_has_no_duplicates() {
        let unique: HashSet<&&str> = DANGEROUS.iter().collect();
        assert_eq!(unique.len(), DANGEROUS.len());
    }

    #[test]
    fn category_from_name_follows_documented_order() {
        use DebugActionCategory as C;
        let cases: &[(&str, DebugActionCategory)] = &[
            ("set_friend_add_request", C::Request),
            ("fetch_custom_face", C::Face),
            ("set_msg_emoji_like", C::Face),
            ("upload_file_stream", C::Stream),
            ("get_group_file_url", C::File),
            ("download_fileset", C::File),
            ("create_flash_task", C::File),
            ("send_group_msg", C::Message),
            ("get_forward_msg", C::Message),
            ("group_poke", C::Message),
            ("get_stranger_info", C::Friend),
            ("send_like", C::Friend),
            ("get_group_member_info", C::GroupInfo),
            ("_get_group_notice", C::GroupInfo),
            ("set_group_card", C::GroupAdmin),
            ("_del_group_notice", C::GroupAdmin),
            ("get_login_info", C::Account),
            ("get_csrf_token", C::Account),
            ("get_online_clients", C::Account),
            ("bot_exit", C::Account),
            ("quit_client", C::Account),
            ("get_mini_app_ark", C::Extension),
            (".get_word_slices", C::Extension),
        ];
        for (name, want) in cases {
            assert_eq!(category_from_name(name), *want, "{name}");
        }
    }

    #[test]
    fn infer_role_cases() {
        assert_eq!(infer_role("x", "group_id", &[]), Some("group_id"));
        assert_eq!(
            infer_role("set_group_ban", "user_id", &["group_id", "user_id"]),
            Some("member_id")
        );
        assert_eq!(
            infer_role("send_group_msg", "user_id", &["group_id", "user_id"]),
            Some("user_id")
        );
        assert_eq!(
            infer_role(
                "forward_group_single_msg",
                "user_id",
                &["group_id", "user_id"]
            ),
            Some("user_id")
        );
        assert_eq!(
            infer_role("get_stranger_info", "user_id", &["user_id"]),
            Some("user_id")
        );
        assert_eq!(
            infer_role("delete_msg", "message_id", &[]),
            Some("message_id")
        );
        assert_eq!(infer_role("send_msg", "message", &[]), Some("message"));
        assert_eq!(infer_role("x", "face_id", &[]), Some("face_id"));
        assert_eq!(infer_role("x", "emoji_id", &[]), Some("face_id"));
        assert_eq!(infer_role("upload_x", "file", &[]), Some("file"));
        assert_eq!(infer_role("get_image", "file", &[]), Some("image"));
        assert_eq!(infer_role("get_record", "file", &[]), Some("record"));
        assert_eq!(infer_role("send_video", "file", &[]), Some("video"));
        assert_eq!(infer_role("x", "time", &[]), Some("timestamp"));
        assert_eq!(infer_role("x", "timestamp", &[]), Some("timestamp"));
        assert_eq!(infer_role("x", "duration", &[]), None);
    }

    #[test]
    fn annotate_roles_prefers_upstream_role_over_guess() {
        let mut schema = json!({"type": "object", "properties": {
            "group_id": {"type": "integer", "x-role": "group_id"},
            "user_id": {"type": "integer", "x-role": "member_id"},
            "message": {"description": "no role upstream"},
            "duration": {"type": "integer", "x-role": "duration"},
            "count": {"type": "integer"},
            "raw": true,
        }});
        annotate_roles("set_group_ban", &mut schema);
        let props = &schema["properties"];
        assert_eq!(props["group_id"]["x-ncd-role"], "group_id");
        assert_eq!(props["user_id"]["x-ncd-role"], "member_id");
        assert_eq!(props["message"]["x-ncd-role"], "message");
        assert_eq!(props["duration"]["x-ncd-role"], "duration");
        assert!(props["count"].get("x-ncd-role").is_none());
        // x-role 原样保留
        assert_eq!(props["user_id"]["x-role"], "member_id");
    }

    #[test]
    fn annotate_roles_ignores_schemas_without_properties() {
        let mut schema = json!({"anyOf": []});
        annotate_roles("x", &mut schema);
        assert_eq!(schema, json!({"anyOf": []}));
    }
}
