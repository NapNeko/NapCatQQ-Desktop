//! 调用参数进内存 / 进文件之前的瘦身：事件流里的调用记录和调用历史共用这一套。
//!
//! 参数是用户或上游框架给的原样 JSON，发一张图片就可能带几 MB 的 base64。事件缓冲按条数
//! 封顶（5000 条），历史也按条数封顶，不瘦身的话几十次发图就能把内存吃到 GB 级。

use serde_json::{Map, Value, json};

/// 参数里单个字符串超过这个长度就换成占位文字
pub(super) const PARAM_STRING_LIMIT: usize = 64 * 1024;
/// 一条记录的参数（字符串瘦身之后）整体序列化后的上限
pub(super) const PARAM_TOTAL_LIMIT: usize = 256 * 1024;
/// 整体超限时，顶层这么短的标量原样留着：群号、QQ 号、消息类型这些，聊天视图靠它们认会话
const KEPT_SCALAR_LIMIT: usize = 256;

/// 参数里超过 64 KiB 的字符串叶子换成 `<已省略 N 字节>`（N 是原字节数）。返回是否动过
pub(super) fn cap_params(value: &mut Value) -> bool {
    match value {
        Value::String(text) if text.len() > PARAM_STRING_LIMIT => {
            *text = omitted(text.len());
            true
        }
        Value::Array(items) => items.iter_mut().fold(false, |acc, v| cap_params(v) || acc),
        Value::Object(map) => map.values_mut().fold(false, |acc, v| cap_params(v) || acc),
        _ => false,
    }
}

/// 先按 [`cap_params`] 换掉超长字符串；整体仍超过 256 KiB 的（几百个中等大小的字段），
/// 顶层的短标量留着，其余字段各换成一段占位；还是超的（字段本身就有成千上万个），
/// 整个参数换成一个只说明原大小的对象。返回是否动过 —— 历史条目的 `params_truncated`
/// 靠它标，前端据此拦「把占位文字当参数发出去」
pub(super) fn cap_record_params(params: &mut Value) -> bool {
    let mut changed = cap_params(params);
    let Some(total) = serialized_len(params) else {
        return changed;
    };
    if total <= PARAM_TOTAL_LIMIT {
        return changed;
    }
    if let Value::Object(map) = params {
        for value in map.values_mut() {
            if !is_short_scalar(value) {
                let len = serialized_len(value).unwrap_or(0);
                *value = Value::String(omitted(len));
                changed = true;
            }
        }
        if changed && serialized_len(params).is_some_and(|len| len <= PARAM_TOTAL_LIMIT) {
            return true;
        }
    }
    let mut summary = Map::new();
    summary.insert(
        "_omitted".to_owned(),
        json!(format!("<参数共 {total} 字节，已省略>")),
    );
    *params = Value::Object(summary);
    true
}

fn is_short_scalar(value: &Value) -> bool {
    match value {
        Value::Null | Value::Bool(_) | Value::Number(_) => true,
        Value::String(text) => text.len() <= KEPT_SCALAR_LIMIT,
        Value::Array(_) | Value::Object(_) => false,
    }
}

fn omitted(bytes: usize) -> String {
    format!("<已省略 {bytes} 字节>")
}

/// 只数字节不分配：回包可能有几百 KiB，为了量一下大小不值得再拷一份
struct ByteCounter(usize);

impl std::io::Write for ByteCounter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0 += buf.len();
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// JSON 紧凑序列化后的字节数
pub(super) fn serialized_len(value: &Value) -> Option<usize> {
    let mut counter = ByteCounter(0);
    serde_json::to_writer(&mut counter, value).ok()?;
    Some(counter.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_strings_anywhere_become_placeholders() {
        let mut params = json!({
            "file": "a".repeat(PARAM_STRING_LIMIT + 1),
            "edge": "b".repeat(PARAM_STRING_LIMIT),
            "nested": [{"deep": "c".repeat(100_000)}],
            "n": 1,
        });
        cap_record_params(&mut params);
        assert_eq!(params["file"], "<已省略 65537 字节>");
        assert_eq!(params["nested"][0]["deep"], "<已省略 100000 字节>");
        assert_eq!(
            params["edge"].as_str().map(str::len),
            Some(PARAM_STRING_LIMIT)
        );
        assert_eq!(params["n"], 1);
    }

    #[test]
    fn many_medium_fields_keep_the_short_scalars() {
        let mut params = json!({"group_id": 123, "message_type": "group"});
        for n in 0..10 {
            params[format!("chunk{n}")] = json!("x".repeat(40_000));
        }
        params["message"] = json!([{"type": "image", "data": {"file": "y".repeat(40_000)}}]);
        cap_record_params(&mut params);
        assert!(serialized_len(&params).unwrap() <= PARAM_TOTAL_LIMIT);
        assert_eq!(params["group_id"], 123);
        assert_eq!(params["message_type"], "group");
        assert_eq!(params["chunk0"], "<已省略 40002 字节>");
        assert!(params["message"].as_str().unwrap().starts_with("<已省略 "));
    }

    #[test]
    fn a_huge_number_of_fields_collapses_to_a_summary() {
        let mut map = Map::new();
        for n in 0..40_000 {
            map.insert(format!("k{n}"), json!(n));
        }
        let mut params = Value::Object(map);
        let before = serialized_len(&params).unwrap();
        cap_record_params(&mut params);
        assert_eq!(
            params,
            json!({"_omitted": format!("<参数共 {before} 字节，已省略>")})
        );
        // 非对象的超大参数同样收成摘要
        let mut list = json!(vec![1_u32; 200_000]);
        cap_record_params(&mut list);
        assert!(list.get("_omitted").is_some());
    }

    #[test]
    fn small_params_are_untouched() {
        let original =
            json!({"group_id": 1, "message": [{"type": "text", "data": {"text": "hi"}}]});
        let mut params = original.clone();
        cap_record_params(&mut params);
        assert_eq!(params, original);
    }
}
