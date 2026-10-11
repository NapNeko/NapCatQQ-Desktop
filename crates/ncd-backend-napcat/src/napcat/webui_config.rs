//! 解析 NapCat config/webui.json 里的 WebUI token
//!
//! Docker 镜像 entrypoint 只在 webui.json 不存在时才写入 WEBUI_TOKEN，之后容器认的
//! 是这个文件（启动时缓存成 initialWebUiToken），Desktop 自己记的那份可能早就对不上。

use serde::Deserialize;

/// 只取 token，其余字段（host/port/loginRate/...）忽略
#[derive(Debug, Deserialize)]
struct WebUiJsonFile {
    #[serde(default)]
    token: String,
}

/// 空白 / 缺字段 / 非 JSON 一律 None，由调用方回退。
/// `napcat` 是默认占位，NapCat 启动时会换成随机 token 再写回，读到说明还没启动完
pub fn parse_webui_json_token(text: &str) -> Option<String> {
    let file: WebUiJsonFile = serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()?;
    // 上游对原始 token 做 hash，不能去掉口令中的首尾空格。
    (!file.token.trim().is_empty() && file.token != "napcat").then_some(file.token)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_token_from_docker_entrypoint_shape() {
        let text = r#"{
    "host": "0.0.0.0",
    "prefix": "",
    "port": 6099,
    "token": "abc",
    "loginRate": 3
}"#;
        assert_eq!(parse_webui_json_token(text).as_deref(), Some("abc"));
    }

    #[test]
    fn blank_token_is_none() {
        assert_eq!(parse_webui_json_token(r#"{"token":"   "}"#), None);
    }

    #[test]
    fn token_whitespace_is_preserved_for_authentication() {
        assert_eq!(
            parse_webui_json_token("\u{feff}{\"token\":\" abc \"}").as_deref(),
            Some(" abc ")
        );
    }

    #[test]
    fn default_placeholder_token_is_none() {
        assert_eq!(parse_webui_json_token(r#"{"token":"napcat"}"#), None);
    }

    #[test]
    fn missing_token_is_none() {
        assert_eq!(parse_webui_json_token(r#"{"port":6099}"#), None);
    }

    #[test]
    fn invalid_json_is_none() {
        assert_eq!(parse_webui_json_token("cat: no such file"), None);
        assert_eq!(parse_webui_json_token(""), None);
    }
}
