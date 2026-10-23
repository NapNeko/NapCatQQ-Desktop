//! MCP 输出脱敏与资源上限；日志和上游文本仍应当作不可信数据。

use std::sync::OnceLock;

use regex::Regex;
use serde_json::{Value, json};

const REDACTED: &str = "[REDACTED]";
const MAX_TEXT: usize = 16 * 1024;
const OUTPUT_BUDGET: usize = 256 * 1024;
const MAX_NODES: usize = 8192;

fn normalized_key(key: &str) -> String {
    key.chars()
        .filter(char::is_ascii_alphanumeric)
        .flat_map(char::to_lowercase)
        .collect()
}

fn sensitive_key(key: &str) -> bool {
    let key = normalized_key(key);
    if matches!(key.as_str(), "key" | "auth" | "bkn" | "gtk") {
        return true;
    }
    [
        "token",
        "secret",
        "password",
        "passwd",
        "cookie",
        "authorization",
        "credential",
        "privatekey",
        "apikey",
        "clientkey",
        "decryptkey",
        "encryptionkey",
        "passphrase",
        "csrf",
        "ticket",
        "rkey",
        "skey",
    ]
    .iter()
    .any(|needle| key.contains(needle))
}

pub(crate) fn sanitize_text(text: &str) -> String {
    static PATTERNS: OnceLock<Option<Vec<Regex>>> = OnceLock::new();
    let patterns = PATTERNS.get_or_init(|| {
        [
            r"(?is)-----BEGIN [A-Z ]*PRIVATE KEY-----.*?(?:-----END [A-Z ]*PRIVATE KEY-----|$)",
            r"(?im)\b(?:cookie|set-cookie|authorization|proxy-authorization)\s*:\s*[^\r\n]+(?:\r?\n[ \t]+[^\r\n]*)*",
            r"(?i)\b(?:Bearer|Basic)\s+[A-Za-z0-9_./+=-]+",
            r#"(?i)(?:[a-z0-9_-]*(?:token|secret|password|passwd|passphrase|cookie|authorization|credential|(?:api|client|decrypt|encryption)[_-]?key|csrf|ticket|rkey|skey)[a-z0-9_-]*|\b(?:key|auth|bkn|g_tk))[\"']?\s*[:=]\s*(?:\"(?:\\.|[^\"\\])*(?:\"|\\?$)|'(?:\\.|[^'\\])*(?:'|\\?$)|[^\s,;&}\]]+)"#,
            r"(?i)\b(?:https?|wss?|socks5h?)://[^\s/@]+@",
            r"\beyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+",
        ].iter().map(|pattern| Regex::new(pattern).ok()).collect()
    });
    let Some(patterns) = patterns else {
        return REDACTED.to_owned();
    };
    let mut out: String = text.chars().take(MAX_TEXT).collect();
    for pattern in patterns {
        out = pattern.replace_all(&out, REDACTED).into_owned();
    }
    if text.chars().count() > MAX_TEXT {
        out.push_str("…[truncated]");
    }
    out
}

pub(crate) fn sanitize_output(value: Value) -> Value {
    fn walk(value: Value, depth: usize, budget: &mut usize, nodes: &mut usize) -> Value {
        if depth > 32 || *budget == 0 || *nodes == 0 {
            return json!({"_mcp_truncated": true});
        }
        *nodes -= 1;
        match value {
            Value::Object(map) => {
                let mut result = serde_json::Map::new();
                let named_secret = ["name", "key", "header"]
                    .iter()
                    .filter_map(|field| map.get(*field).and_then(Value::as_str))
                    .any(sensitive_key);
                for (key, value) in map {
                    if *budget == 0 || *nodes == 0 {
                        result.insert("_mcp_truncated".to_owned(), Value::Bool(true));
                        break;
                    }
                    let sensitive = sensitive_key(&key)
                        || (named_secret && matches!(key.as_str(), "value" | "values"));
                    let key: String = sanitize_text(&key).chars().take(256).collect();
                    *budget = budget.saturating_sub(key.len());
                    let value = if sensitive {
                        Value::String(REDACTED.to_owned())
                    } else {
                        walk(value, depth + 1, budget, nodes)
                    };
                    result.insert(key, value);
                }
                Value::Object(result)
            }
            Value::Array(items) => {
                let mut result = Vec::new();
                for item in items {
                    if *budget == 0 || *nodes == 0 {
                        result.push(json!({"_mcp_truncated": true}));
                        break;
                    }
                    result.push(walk(item, depth + 1, budget, nodes));
                }
                Value::Array(result)
            }
            Value::String(text) => {
                // 调试台会把参数存成 JSON 字符串；只过滤外层字段会漏掉其中的凭据。
                let clean = match serde_json::from_str::<Value>(&text) {
                    Ok(nested @ (Value::Object(_) | Value::Array(_))) => {
                        walk(nested, depth + 1, budget, nodes).to_string()
                    }
                    _ => sanitize_text(&text),
                };
                let allowed = clean.len().min(*budget);
                let mut end = allowed;
                while !clean.is_char_boundary(end) {
                    end -= 1;
                }
                *budget = budget.saturating_sub(end);
                if end < clean.len() {
                    *budget = 0;
                    Value::String(format!("{}…[truncated]", &clean[..end]))
                } else {
                    Value::String(clean)
                }
            }
            other => {
                *budget = budget.saturating_sub(16);
                other
            }
        }
    }
    let mut budget = OUTPUT_BUDGET;
    let mut nodes = MAX_NODES;
    let result = walk(value, 0, &mut budget, &mut nodes);
    // JSON 转义、键和标点也占字节；硬上限不能只靠字符串预算估算。
    if serde_json::to_vec(&result).map_or(true, |bytes| bytes.len() > OUTPUT_BUDGET) {
        json!({"_mcp_truncated": true, "reason": "结果超过256 KiB，请缩小查询范围或分页"})
    } else {
        result
    }
}

pub(crate) fn forbidden_action(action: &str) -> bool {
    let action = action.to_ascii_lowercase();
    sensitive_key(&action)
        || action.contains("upload")
        || action.contains("download")
        || matches!(
            action.as_str(),
            "get_file"
                | "get_image"
                | "get_record"
                | "get_record_stream"
                | "get_file_stream"
                | "clean_cache"
                | "get_config"
                | "set_config"
                | "request_decrypt_key"
                | "get_clientkey"
                | "send_packet"
                | "create_flash_task"
                | "clean_stream_temp_file"
                | "receive_online_file"
                | "send_online_file"
        )
}

pub(crate) fn contains_local_file(value: &Value) -> bool {
    match value {
        Value::String(text) => {
            let lower = text.to_ascii_lowercase();
            lower.contains("ncd-local-file://")
                || lower.contains("file://")
                || text.starts_with(['/', '\\'])
                || text.as_bytes().get(1) == Some(&b':')
                || lower.contains("[cq:")
        }
        Value::Array(items) => items.iter().any(contains_local_file),
        Value::Object(map) => {
            map.iter().any(|(key, value)| {
                let key = normalized_key(key);
                matches!(
                    key.as_str(),
                    "file"
                        | "files"
                        | "path"
                        | "paths"
                        | "filepath"
                        | "filename"
                        | "dbpath"
                        | "thumbpath"
                        | "image"
                        | "images"
                        | "record"
                        | "video"
                        | "audio"
                ) && !value.is_null()
                    || contains_local_file(value)
            }) || (map.contains_key("data")
                && map
                    .get("type")
                    .and_then(Value::as_str)
                    .is_some_and(|kind| !matches!(kind, "text" | "at" | "face" | "reply")))
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_nested_json_and_free_text_without_removing_status() {
        let result = sanitize_output(json!({
            "status": "running", "accessToken": "secret-a",
            "nested": [{"cookie": "secret-b", "params_text": "{\"password\":\"secret-c\"}"}],
            "log": "Authorization: Bearer abc.secret token=secret-d https://alice:secret-e@example.test/x?api_key=secret-f"
        }));
        let text = result.to_string();
        for secret in [
            "secret-a",
            "secret-b",
            "secret-c",
            "abc.secret",
            "secret-d",
            "secret-e",
            "secret-f",
        ] {
            assert!(!text.contains(secret), "{text}");
        }
        assert_eq!(result["status"], "running");
    }

    #[test]
    fn output_is_bounded_and_utf8_safe() {
        let result = sanitize_output(json!(
            (0..1000).map(|_| "中文".repeat(20_000)).collect::<Vec<_>>()
        ));
        assert!(result.to_string().len() < OUTPUT_BUDGET + 4096);
        assert!(result.to_string().contains("truncated"));
    }

    #[test]
    fn sensitive_actions_and_local_file_forms_are_rejected() {
        for action in [
            "get_cookies",
            "get_credentials",
            "nc_get_rkey",
            "get_file",
            "upload_file_stream",
        ] {
            assert!(forbidden_action(action));
        }
        for path in [
            "C:\\private.txt",
            "file:///etc/passwd",
            "ncd-local-file://x",
            "\\\\server\\share",
            "/etc/passwd",
            "[CQ:image,file=C:\\x]",
            "../secret.txt",
            "secret.txt",
        ] {
            assert!(contains_local_file(
                &json!({"message": [{"data": {"file": path}}]})
            ));
        }
        assert!(!contains_local_file(&json!({"message": "hello"})));
    }

    #[test]
    fn keys_and_json_escaping_cannot_bypass_output_limits() {
        let key = format!("token=secret-value{}", "x".repeat(300_000));
        let result = sanitize_output(json!({key: "value"})).to_string();
        assert!(!result.contains("secret-value"));
        assert!(result.len() <= OUTPUT_BUDGET);
        let result = sanitize_output(json!(
            (0..1000)
                .map(|_| "\u{0001}".repeat(8000))
                .collect::<Vec<_>>()
        ))
        .to_string();
        assert!(result.len() <= OUTPUT_BUDGET);
    }

    #[test]
    fn masks_entire_cookie_headers_and_multiline_private_keys() {
        let output = sanitize_text(
            "Cookie: first=secret-one; sid=secret-two\nSet-Cookie: session=secret-three; HttpOnly\n-----BEGIN RSA PRIVATE KEY-----\nsecret-four\n-----END RSA PRIVATE KEY-----\nstatus=ok",
        );
        for secret in ["secret-one", "secret-two", "secret-three", "secret-four"] {
            assert!(!output.contains(secret));
        }
        assert!(output.contains("status=ok"));
    }

    #[test]
    fn truncated_quotes_and_escaped_quotes_do_not_expose_secret_tails() {
        let output = sanitize_text(&format!(
            "password=\"first {}\"",
            "secret tail ".repeat(MAX_TEXT)
        ));
        assert!(!output.contains("first"));
        assert!(!output.contains("secret"));
        let output = sanitize_text(r#"password="first \"middle\" last"; status=ok"#);
        for secret in ["first", "middle", "last"] {
            assert!(!output.contains(secret), "{output}");
        }
        assert!(output.contains("status=ok"));
    }

    #[test]
    fn masks_username_only_userinfo_and_named_secret_records() {
        let output = sanitize_output(json!({
            "url": "https://sensitive-value@example.test/path",
            "clientkey": "client-secret",
            "headers": [{"name": "X-API-Key", "value": "header-secret"}],
            "settings": [{"key": "password", "value": "setting-secret"}],
            "log": "clientkey=log-secret"
        }))
        .to_string();
        for secret in [
            "sensitive-value",
            "client-secret",
            "header-secret",
            "setting-secret",
            "log-secret",
        ] {
            assert!(!output.contains(secret), "{output}");
        }
    }

    #[test]
    fn recognizes_plural_camelcase_and_complex_media_inputs() {
        for params in [
            json!({"images": ["relative.png"]}),
            json!({"files": ["relative.txt"]}),
            json!({"thumbPath": "relative.png"}),
            json!({"db_path": "relative.db"}),
            json!({"message": [{"type": "json", "data": {"data": "opaque"}}]}),
            json!({"message": [{"type": "node", "data": {"content": "opaque"}}]}),
        ] {
            assert!(contains_local_file(&params), "{params}");
        }
        assert!(forbidden_action("request_decrypt_key"));
        assert!(forbidden_action("get_clientkey"));
        assert!(forbidden_action("send_packet"));
    }
}
