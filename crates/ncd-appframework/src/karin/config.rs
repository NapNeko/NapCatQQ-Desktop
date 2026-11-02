//! Karin 对接地址读取与备份文档清单。

use ncd_domain::{AppConfigDocument, AppConfigFormat};

use super::manifest::KARIN_ENV_FILE;

pub const KARIN_CONFIG_DIR: &str = "@karinjs/config";

pub const DOC_ENV: &str = "env";
pub const DOC_CONFIG: &str = "config";
pub const DOC_ADAPTER: &str = "adapter";
pub const DOC_GROUPS: &str = "groups";
pub const DOC_PRIVATES: &str = "privates";
pub const DOC_RENDER: &str = "render";
pub const DOC_REDIS: &str = "redis";

/// 只收 enable 的正向客户端,默认占位 `ws://127.0.0.1:7778` 是关着的
pub fn outbound_onebot_ws_urls(text: &str) -> Vec<String> {
    let Ok(root) = serde_json::from_str::<serde_json::Value>(text) else {
        return Vec::new();
    };
    root.pointer("/onebot/ws_client")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter(|c| c.get("enable").and_then(serde_json::Value::as_bool) == Some(true))
        .filter_map(|c| c.get("url").and_then(serde_json::Value::as_str))
        .map(|url| url.trim().to_string())
        .filter(|u| !u.is_empty())
        .collect()
}

fn doc(
    id: &str,
    label: &str,
    rel_path: String,
    format: AppConfigFormat,
    hot_reload: bool,
) -> AppConfigDocument {
    AppConfigDocument {
        id: id.to_string(),
        label: label.to_string(),
        rel_path,
        format,
        hot_reload,
        reload_note: (id == DOC_ENV)
            .then(|| "日志级别、鉴权密钥等支持热重载；HTTP 监听地址、端口等启动项需重启。".into()),
    }
}

/// 随实例备份的配置文件。
pub fn karin_config_documents() -> Vec<AppConfigDocument> {
    let json = |name: &str| format!("{KARIN_CONFIG_DIR}/{name}.json");
    vec![
        doc(
            DOC_ENV,
            ".env",
            KARIN_ENV_FILE.to_string(),
            AppConfigFormat::DotEnv,
            true,
        ),
        doc(
            DOC_CONFIG,
            "config.json",
            json("config"),
            AppConfigFormat::Json,
            true,
        ),
        doc(
            DOC_ADAPTER,
            "adapter.json",
            json("adapter"),
            AppConfigFormat::Json,
            true,
        ),
        doc(
            DOC_GROUPS,
            "groups.json",
            json("groups"),
            AppConfigFormat::Json,
            true,
        ),
        doc(
            DOC_PRIVATES,
            "privates.json",
            json("privates"),
            AppConfigFormat::Json,
            true,
        ),
        doc(
            DOC_RENDER,
            "render.json",
            json("render"),
            AppConfigFormat::Json,
            true,
        ),
        doc(
            DOC_REDIS,
            "redis.json",
            json("redis"),
            AppConfigFormat::Json,
            false,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outbound_ws_urls_skip_disabled_placeholder() {
        let disabled = r#"{"onebot":{"ws_client":[{"enable":false,"url":"ws://127.0.0.1:7778"}]}}"#;
        assert!(outbound_onebot_ws_urls(disabled).is_empty());
        let on = r#"{"onebot":{"ws_client":[{"enable":true,"url":"ws://127.0.0.1:3001"}]}}"#;
        assert_eq!(
            outbound_onebot_ws_urls(on),
            vec!["ws://127.0.0.1:3001".to_string()]
        );
    }
}
