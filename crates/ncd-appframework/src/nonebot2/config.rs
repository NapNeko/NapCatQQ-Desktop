//! NoneBot2 环境文件合并、对接字段读写与备份清单。

use ncd_domain::{AppConfigDocument, AppConfigFormat};
use ncd_host::{Host, HostPath};
use ncd_traits::AppFrameworkError;

use crate::env_file::EnvFile;

use super::env_layout::{self, NoneBotEnvLayout};
use super::manifest::{
    ENV_ONEBOT_ACCESS_TOKEN, ENV_ONEBOT_WS_URLS, ENV_PORT, NONEBOT2_ENV_FILE,
    NONEBOT2_ENV_PROD_FILE, NONEBOT2_PYPROJECT,
};

pub const DOC_ENV: &str = "env";
pub const DOC_ENV_PROD: &str = "env_prod";
pub const DOC_PYPROJECT: &str = "pyproject";

const ENV_ONEBOT_SECRET: &str = "ONEBOT_SECRET";
const ENV_ONEBOT_API_ROOTS: &str = "ONEBOT_API_ROOTS";
const ONEBOT_V11_ALIASES: [(&str, &str); 4] = [
    (ENV_ONEBOT_ACCESS_TOKEN, "ONEBOT_V11_ACCESS_TOKEN"),
    (ENV_ONEBOT_SECRET, "ONEBOT_V11_SECRET"),
    (ENV_ONEBOT_WS_URLS, "ONEBOT_V11_WS_URLS"),
    (ENV_ONEBOT_API_ROOTS, "ONEBOT_V11_API_ROOTS"),
];

fn config_doc(id: &str, rel: &str, format: AppConfigFormat) -> AppConfigDocument {
    AppConfigDocument {
        id: id.to_string(),
        label: rel.to_string(),
        rel_path: rel.to_string(),
        format,
        hot_reload: false,
        reload_note: None,
    }
}

pub fn nonebot2_config_documents() -> Vec<AppConfigDocument> {
    vec![
        config_doc(DOC_ENV, NONEBOT2_ENV_FILE, AppConfigFormat::DotEnv),
        config_doc(
            DOC_ENV_PROD,
            NONEBOT2_ENV_PROD_FILE,
            AppConfigFormat::DotEnv,
        ),
        config_doc(DOC_PYPROJECT, NONEBOT2_PYPROJECT, AppConfigFormat::Toml),
    ]
}

/// 官方：`.env` 总会加载；存在 `.env.{ENVIRONMENT}` 时再叠一层。没有 overlay 就不列幽灵文件。
pub fn nonebot2_config_documents_for(layout: &NoneBotEnvLayout) -> Vec<AppConfigDocument> {
    let mut docs = vec![config_doc(
        DOC_ENV,
        NONEBOT2_ENV_FILE,
        AppConfigFormat::DotEnv,
    )];
    if layout.is_overlay() {
        docs.push(config_doc(
            DOC_ENV_PROD,
            &layout.write_rel,
            AppConfigFormat::DotEnv,
        ));
    }
    docs.push(config_doc(
        DOC_PYPROJECT,
        NONEBOT2_PYPROJECT,
        AppConfigFormat::Toml,
    ));
    docs
}

/// 官方优先级：`.env` 先加载，`.env.{ENVIRONMENT}` 覆盖同名键。
pub fn merge_env_texts(base: Option<&str>, overlay: Option<&str>) -> EnvFile {
    let mut env = EnvFile::parse_python(base.unwrap_or(""));
    if let Some(over) = overlay.filter(|s| !s.is_empty()) {
        let extra = EnvFile::parse_python(over);
        for entry in extra.entries_ci() {
            // Python dotenv 合并时大小写不敏感，overlay 的拼写和值共同覆盖 base。
            for existing in env.entries() {
                if existing.key.eq_ignore_ascii_case(&entry.key) {
                    env.remove(&existing.key);
                }
            }
            env.set_with_comment(&entry.key, &entry.value, &entry.comment);
        }
    }
    env
}

fn read_onebot_env(env: &EnvFile, key: &str) -> Option<String> {
    ONEBOT_V11_ALIASES
        .iter()
        .find(|(name, _)| *name == key)
        .and_then(|(_, alias)| env.get_ci(alias))
        .or_else(|| env.get_ci(key))
}

/// 同步 v11 专用键与通用键；专用键可能来自 base，必须在 overlay 中一起覆盖。
pub(super) fn write_onebot_env(env: &mut EnvFile, key: &str, value: &str) {
    if let Some((_, alias)) = ONEBOT_V11_ALIASES.iter().find(|(name, _)| *name == key) {
        env.set_ci(alias, value);
    }
    env.set_ci(key, value);
}

pub fn read_listen_port(base: Option<&str>, overlay: Option<&str>) -> Option<u16> {
    merge_env_texts(base, overlay)
        .get_ci(ENV_PORT)
        .and_then(|v| v.parse().ok())
        .filter(|p| *p > 0)
}

pub fn read_access_token_from_texts(base: Option<&str>, overlay: Option<&str>) -> Option<String> {
    let merged = merge_env_texts(base, overlay);
    read_onebot_env(&merged, ENV_ONEBOT_ACCESS_TOKEN).filter(|v| !v.trim().is_empty())
}

pub fn read_outbound_ws_urls_from_texts(base: Option<&str>, overlay: Option<&str>) -> Vec<String> {
    read_onebot_env(&merge_env_texts(base, overlay), ENV_ONEBOT_WS_URLS)
        .map(|raw| parse_ws_url_list(&raw))
        .unwrap_or_default()
}

pub fn parse_ws_url_list(raw: &str) -> Vec<String> {
    let t = raw.trim();
    if t.is_empty() {
        return Vec::new();
    }
    if let Ok(arr) = serde_json::from_str::<Vec<String>>(t) {
        return arr
            .into_iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
    }
    if t.starts_with("ws://") || t.starts_with("wss://") {
        return vec![t.to_string()];
    }
    Vec::new()
}

pub async fn load_env_layout(
    host: &dyn Host,
    install_dir: &HostPath,
) -> Result<NoneBotEnvLayout, AppFrameworkError> {
    let base = read_optional_text(host, &install_dir.join(NONEBOT2_ENV_FILE)).await?;
    let environment = env_layout::environment_from_dotenv(base.as_deref().unwrap_or(""));
    let overlay_rel = env_layout::overlay_rel(&environment);
    let overlay_exists = host
        .exists(&install_dir.join(&overlay_rel))
        .await
        .map_err(|e| AppFrameworkError::Host(e.to_string()))?;
    Ok(env_layout::layout_from_base(base.as_deref(), |rel| {
        overlay_exists && rel == overlay_rel
    }))
}

async fn read_optional_text(
    host: &dyn Host,
    path: &HostPath,
) -> Result<Option<String>, AppFrameworkError> {
    let bytes = match host.read_file(path).await {
        Ok(bytes) => bytes,
        Err(ncd_host::HostError::PathNotFound { .. }) => return Ok(None),
        Err(error) => return Err(AppFrameworkError::Host(error.to_string())),
    };
    Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn optional_env_reads_once_and_preserves_permission_failures() {
        let host = crate::config_backup::tests::TestHost::new("remote:env");
        let path = HostPath::from_posix("/app/.env");
        assert!(
            read_optional_text(host.as_ref(), &path)
                .await
                .unwrap()
                .is_none()
        );
        host.put("/app/.env", b"ENVIRONMENT=prod\n");
        assert_eq!(
            read_optional_text(host.as_ref(), &path)
                .await
                .unwrap()
                .as_deref(),
            Some("ENVIRONMENT=prod\n")
        );
        assert_eq!(host.access_counts("/app/.env"), (0, 2));
        host.deny_read("/app/.env");
        assert!(read_optional_text(host.as_ref(), &path).await.is_err());
    }

    #[test]
    fn outbound_ws_urls_parse_json_array_and_single() {
        assert_eq!(
            parse_ws_url_list(r#"["ws://127.0.0.1:3001","wss://x:1/p"]"#),
            vec!["ws://127.0.0.1:3001".to_string(), "wss://x:1/p".to_string()]
        );
        assert_eq!(
            parse_ws_url_list("ws://127.0.0.1:3001"),
            vec!["ws://127.0.0.1:3001".to_string()]
        );
        assert!(parse_ws_url_list("not-a-url").is_empty());
        assert_eq!(
            read_outbound_ws_urls_from_texts(
                Some("ONEBOT_WS_URLS=[\"ws://127.0.0.1:3001\"]\n"),
                None
            ),
            vec!["ws://127.0.0.1:3001".to_string()]
        );
    }

    #[test]
    fn listen_port_prefers_overlay_then_base() {
        assert_eq!(
            read_listen_port(Some("PORT=13120\n"), Some("PORT=8080\n")),
            Some(8080)
        );
        assert_eq!(read_listen_port(Some("PORT=13120\n"), None), Some(13120));
        assert_eq!(read_listen_port(Some("port=13120\n"), None), Some(13120));
        assert_eq!(read_listen_port(Some("HOST=127.0.0.1\n"), None), None);
    }

    #[test]
    fn merge_follows_official_overlay() {
        let merged = merge_env_texts(
            Some("ENVIRONMENT=dev\nPORT=13120\nONEBOT_WS_URLS=[\"ws://x\"]\n"),
            Some("PORT=8080\nLOG_LEVEL=DEBUG\n"),
        );
        assert_eq!(merged.get_ci("PORT").as_deref(), Some("8080"));
        assert_eq!(merged.get_ci("LOG_LEVEL").as_deref(), Some("DEBUG"));
        assert_eq!(
            merged.get_ci("ONEBOT_WS_URLS").as_deref(),
            Some("[\"ws://x\"]")
        );
    }

    #[test]
    fn documents_omit_missing_overlay() {
        let only_env = NoneBotEnvLayout {
            environment: "prod".into(),
            write_rel: ".env".into(),
        };
        let docs = nonebot2_config_documents_for(&only_env);
        assert_eq!(docs.len(), 2);
        assert!(docs.iter().all(|d| d.rel_path != ".env.prod"));

        let with_overlay = NoneBotEnvLayout {
            environment: "dev".into(),
            write_rel: ".env.dev".into(),
        };
        let docs = nonebot2_config_documents_for(&with_overlay);
        assert_eq!(docs[1].rel_path, ".env.dev");
        assert_eq!(docs[1].id, DOC_ENV_PROD);
    }
}
