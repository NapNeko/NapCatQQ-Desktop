//! OneBot 11 回包信封：宽松解析，以及折算成 [`DebugCallOutcome`]。
//!
//! 两个上游的回包形状不完全一样：NapCat 有 `status / retcode / data / message / wording /
//! echo`，SnowLuma 的裸回包常常只有 `status / retcode / data / wording`。所以解析对缺字段
//! 一律给默认值，不报错 —— 调试台的目的就是把上游回了什么原样摆出来。

use std::time::Duration;

use ncd_domain::onebot_debug::{DebugCallOutcome, DebugChannelId};
use serde_json::Value;

/// 回包文本超过这个体积就不再整块塞进 IPC，只给前 [`RAW_PREVIEW_LIMIT`] 字节的预览
pub const RESPONSE_INLINE_LIMIT: usize = 5 * 1024 * 1024;
/// 截断时保留的预览字节数
pub const RAW_PREVIEW_LIMIT: usize = 256 * 1024;

/// 解析后的 OneBot 11 回包
#[derive(Debug, Clone, PartialEq)]
pub struct Ob11Reply {
    pub status: String,
    pub retcode: i64,
    pub data: Value,
    pub message: String,
    pub wording: String,
    pub echo: Value,
}

/// 宽松解析一个回包对象。
///
/// 缺失字段：`status` / `message` / `wording` 为空串，`data` / `echo` 为 `null`。
/// `retcode` 缺失或读不出数字时取 -1 而不是 0：拿到一个根本不是 OB11 的对象
/// （比如 WebUI 的 `{"code":-1,...}`）时，不能让它看起来像成功。
/// `retcode` 允许是字符串（`"0"`）或整数值的浮点数，个别实现会这么发。
pub fn parse_ob11_reply(v: &Value) -> Ob11Reply {
    Ob11Reply {
        status: text_field(v.get("status")),
        retcode: v.get("retcode").and_then(retcode_of).unwrap_or(-1),
        data: v.get("data").cloned().unwrap_or(Value::Null),
        message: text_field(v.get("message")),
        wording: text_field(v.get("wording")),
        echo: v.get("echo").cloned().unwrap_or(Value::Null),
    }
}

/// 字符串字段：字符串原样取，null / 缺失为空串，别的类型（数字、对象）转成 JSON 文本，
/// 免得把上游给的信息悄悄吞掉
fn text_field(v: Option<&Value>) -> String {
    match v {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
    }
}

fn retcode_of(v: &Value) -> Option<i64> {
    match v {
        Value::Number(n) => n.as_i64().or_else(|| {
            // 整数值的浮点数（`0.0`）也认；带小数的不认
            n.as_f64().filter(|f| f.fract() == 0.0).map(|f| f as i64)
        }),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

/// 把一次调用的原始回包折算成前端看的结果。
///
/// `ok`：`status` 为 `ok`（不分大小写），或者回包压根没有 `status` 且 `retcode == 0`。
/// 体积超过 [`RESPONSE_INLINE_LIMIT`] 时 `truncated = true`、`data` 置空，`raw` 换成
/// 前 [`RAW_PREVIEW_LIMIT`] 字节的文本预览；`status / retcode / message` 仍取自完整回包，
/// 这样结果头部照常显示。
pub fn outcome_from(
    raw_text: &str,
    raw: Value,
    elapsed: Duration,
    channel: DebugChannelId,
) -> DebugCallOutcome {
    outcome_from_with_limit(raw_text, raw, elapsed, channel, RESPONSE_INLINE_LIMIT)
}

/// 按调用方指定的体积上限折算回包。
///
/// 媒体动作允许更大的 inline data，但仍由调用方明确选择，普通调试回包继续使用默认上限。
pub fn outcome_from_with_limit(
    raw_text: &str,
    raw: Value,
    elapsed: Duration,
    channel: DebugChannelId,
    inline_limit: usize,
) -> DebugCallOutcome {
    let reply = parse_ob11_reply(&raw);
    let ok =
        reply.status.eq_ignore_ascii_case("ok") || (reply.status.is_empty() && reply.retcode == 0);
    let size = raw_text.len();
    let truncated = size > inline_limit;
    let (data, raw) = if truncated {
        (
            Value::Null,
            Value::String(truncate_at_char_boundary(raw_text, RAW_PREVIEW_LIMIT).to_owned()),
        )
    } else {
        (reply.data, raw)
    };
    DebugCallOutcome {
        ok,
        status: reply.status,
        retcode: reply.retcode,
        data,
        message: reply.message,
        wording: reply.wording,
        raw,
        elapsed_ms: u32::try_from(elapsed.as_millis()).unwrap_or(u32::MAX),
        channel,
        size_bytes: u32::try_from(size).unwrap_or(u32::MAX),
        truncated,
    }
}

/// 取 `text` 的前 `max_bytes` 字节，且不切在多字节字符中间
/// （`str::floor_char_boundary` 要 1.91，工作区最低 1.85，所以自己找边界）
pub(crate) fn truncate_at_char_boundary(text: &str, max_bytes: usize) -> &str {
    if text.len() <= max_bytes {
        return text;
    }
    let mut end = max_bytes;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    text.get(..end).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn outcome(raw: Value) -> DebugCallOutcome {
        let text = raw.to_string();
        outcome_from(
            &text,
            raw,
            Duration::from_millis(42),
            DebugChannelId::Internal,
        )
    }

    #[test]
    fn napcat_success() {
        let raw = json!({
            "status": "ok", "retcode": 0,
            "data": {"user_id": 10001, "nickname": "bot"},
            "message": "", "wording": "", "echo": null,
        });
        let out = outcome(raw.clone());
        assert!(out.ok);
        assert_eq!(out.status, "ok");
        assert_eq!(out.retcode, 0);
        assert_eq!(out.data, json!({"user_id": 10001, "nickname": "bot"}));
        assert_eq!(out.raw, raw);
        assert_eq!(out.elapsed_ms, 42);
        assert_eq!(out.channel, DebugChannelId::Internal);
        assert_eq!(out.size_bytes as usize, raw.to_string().len());
        assert!(!out.truncated);
    }

    #[test]
    fn napcat_failed_with_retcode_400() {
        let raw = json!({
            "status": "failed", "retcode": 400, "data": null,
            "message": "参数错误", "wording": "缺少 group_id", "echo": null,
        });
        let out = outcome(raw);
        assert!(!out.ok);
        assert_eq!(out.status, "failed");
        assert_eq!(out.retcode, 400);
        assert_eq!(out.message, "参数错误");
        assert_eq!(out.wording, "缺少 group_id");
        assert_eq!(out.data, Value::Null);
    }

    #[test]
    fn snowluma_bare_reply_with_wording_only() {
        let raw =
            json!({"status": "failed", "retcode": 1404, "data": null, "wording": "账号不在线"});
        let reply = parse_ob11_reply(&raw);
        assert_eq!(reply.message, "");
        assert_eq!(reply.wording, "账号不在线");
        assert_eq!(reply.echo, Value::Null);
        let out = outcome(raw);
        assert!(!out.ok);
        assert_eq!(out.message, "");
        assert_eq!(out.wording, "账号不在线");
    }

    #[test]
    fn retcode_may_be_a_string() {
        let raw = json!({"status": "ok", "retcode": "0", "data": {}});
        assert_eq!(parse_ob11_reply(&raw).retcode, 0);
        assert_eq!(
            parse_ob11_reply(&json!({"retcode": " 1400 "})).retcode,
            1400
        );
        assert_eq!(parse_ob11_reply(&json!({"retcode": 0.0})).retcode, 0);
        assert!(outcome(raw).ok);
    }

    #[test]
    fn missing_status_with_retcode_zero_is_ok() {
        let out = outcome(json!({"retcode": 0, "data": {"a": 1}}));
        assert!(out.ok);
        assert_eq!(out.status, "");
        let out = outcome(json!({"retcode": 100, "data": null}));
        assert!(!out.ok);
    }

    #[test]
    fn status_is_case_insensitive() {
        assert!(outcome(json!({"status": "OK", "retcode": 0})).ok);
        // status 明确写了 failed 时，retcode 恰好为 0 也不算成功
        assert!(!outcome(json!({"status": "failed", "retcode": 0})).ok);
    }

    #[test]
    fn not_an_ob11_object_is_not_ok() {
        // WebUI 的错误信封 / 空对象：没有 status 也没有 retcode，不能被当成成功
        let out = outcome(json!({"code": -1, "message": "Unauthorized"}));
        assert!(!out.ok);
        assert_eq!(out.retcode, -1);
        assert_eq!(out.message, "Unauthorized");
        assert!(!outcome(json!({})).ok);
    }

    #[test]
    fn non_string_text_fields_are_kept_as_json_text() {
        let reply = parse_ob11_reply(&json!({"message": 5, "wording": {"a": 1}}));
        assert_eq!(reply.message, "5");
        assert_eq!(reply.wording, r#"{"a":1}"#);
    }

    #[test]
    fn oversized_reply_is_truncated_to_a_preview() {
        let filler = "x".repeat(6 * 1024 * 1024);
        let raw = json!({"status": "ok", "retcode": 0, "data": {"blob": filler}});
        let text = raw.to_string();
        let out = outcome_from(&text, raw, Duration::from_millis(1), DebugChannelId::Auto);
        assert!(out.truncated);
        assert!(out.ok, "截断不影响 ok / retcode");
        assert_eq!(out.retcode, 0);
        assert_eq!(out.data, Value::Null);
        assert_eq!(out.size_bytes as usize, text.len());
        let Value::String(preview) = &out.raw else {
            panic!("截断后 raw 应是字符串预览");
        };
        assert!(preview.len() <= RAW_PREVIEW_LIMIT);
        assert!(text.starts_with(preview.as_str()));
    }

    #[test]
    fn preview_never_cuts_inside_a_char() {
        // 每个字符 3 字节，256 KiB 不是 3 的倍数，必须回退到边界
        let text = "汉".repeat(RESPONSE_INLINE_LIMIT / 3 + 10);
        let out = outcome_from(
            &text,
            json!({"status": "ok", "retcode": 0}),
            Duration::ZERO,
            DebugChannelId::Auto,
        );
        assert!(out.truncated);
        let Value::String(preview) = &out.raw else {
            panic!("截断后 raw 应是字符串预览");
        };
        assert!(preview.len() <= RAW_PREVIEW_LIMIT);
        assert_eq!(preview.len() % 3, 0);
        assert!(RAW_PREVIEW_LIMIT - preview.len() < 3);
    }

    #[test]
    fn exactly_at_the_limit_is_not_truncated() {
        let text = "a".repeat(RESPONSE_INLINE_LIMIT);
        let out = outcome_from(
            &text,
            json!({"status": "ok", "retcode": 0, "data": 1}),
            Duration::ZERO,
            DebugChannelId::Auto,
        );
        assert!(!out.truncated);
        assert_eq!(out.data, json!(1));
    }

    #[test]
    fn elapsed_saturates_instead_of_wrapping() {
        let out = outcome_from(
            "{}",
            json!({}),
            Duration::from_secs(u64::from(u32::MAX)),
            DebugChannelId::Auto,
        );
        assert_eq!(out.elapsed_ms, u32::MAX);
    }
}
