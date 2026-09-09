//! NoneBot2 窄类型化配置：只覆盖对接和常用运行键，插件/适配器列表由商店 Tab 写 toml。

use ncd_domain::{AppConfigDocument, AppConfigFormat, AppConfigIssue};
use ncd_host::{Host, HostPath};
use ncd_traits::AppFrameworkError;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::adapter::apply_with_backup_ex;
use crate::config_doc::{
    DocumentSnapshot, IssueSink, read_documents, revision_of,
};
use crate::env_file::EnvFile;

use super::env_layout::{self, NoneBotEnvLayout};
use super::manifest::{
    ENV_DRIVER, ENV_HOST, ENV_ONEBOT_ACCESS_TOKEN, ENV_ONEBOT_WS_URLS, ENV_PORT, NONEBOT2_ENV_FILE,
    NONEBOT2_ENV_PROD_FILE, NONEBOT2_PYPROJECT,
};

pub const DOC_ENV: &str = "env";
pub const DOC_ENV_PROD: &str = "env_prod";
pub const DOC_PYPROJECT: &str = "pyproject";

const ENV_LOG_LEVEL: &str = "LOG_LEVEL";
const ENV_SUPERUSERS: &str = "SUPERUSERS";
const ENV_NICKNAME: &str = "NICKNAME";
const ENV_COMMAND_START: &str = "COMMAND_START";
const ENV_COMMAND_SEP: &str = "COMMAND_SEP";

const SYSTEM_KEYS: [&str; 9] = [
    ENV_DRIVER,
    ENV_HOST,
    ENV_PORT,
    ENV_LOG_LEVEL,
    ENV_ONEBOT_ACCESS_TOKEN,
    ENV_SUPERUSERS,
    ENV_NICKNAME,
    ENV_COMMAND_START,
    ENV_COMMAND_SEP,
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct NoneBot2EnvEntry {
    pub key: String,
    pub value: String,
    #[serde(default)]
    pub comment: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct NoneBot2EnvProd {
    pub host: String,
    pub port: u16,
    pub driver: String,
    pub log_level: String,
    pub onebot_access_token: String,
    pub superusers: Vec<String>,
    pub nickname: Vec<String>,
    pub command_start: Vec<String>,
    pub command_sep: Vec<String>,
    #[serde(default)]
    pub custom: Vec<NoneBot2EnvEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct NoneBot2InstanceConfig {
    pub env_prod: NoneBot2EnvProd,
}

impl Default for NoneBot2EnvProd {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".into(),
            port: 8080,
            driver: "~fastapi".into(),
            log_level: "INFO".into(),
            onebot_access_token: String::new(),
            superusers: Vec::new(),
            nickname: Vec::new(),
            command_start: vec!["/".into(), String::new()],
            command_sep: vec![".".into(), " ".into()],
            custom: Vec::new(),
        }
    }
}

impl NoneBot2EnvProd {
    pub fn from_env_file(env: &EnvFile) -> Self {
        let mut out = Self::default();
        if let Some(v) = env.get_ci(ENV_HOST) {
            out.host = v;
        }
        if let Some(v) = env.get_ci(ENV_PORT)
            && let Ok(p) = v.parse()
        {
            out.port = p;
        }
        if let Some(v) = env.get_ci(ENV_DRIVER) {
            out.driver = v;
        }
        if let Some(v) = env.get_ci(ENV_LOG_LEVEL) {
            out.log_level = v;
        }
        if let Some(v) = env.get_ci(ENV_ONEBOT_ACCESS_TOKEN) {
            out.onebot_access_token = v;
        }
        if let Some(v) = env.get_ci(ENV_SUPERUSERS) {
            out.superusers = parse_list(&v);
        }
        if let Some(v) = env.get_ci(ENV_NICKNAME) {
            out.nickname = parse_list(&v);
        }
        if let Some(v) = env.get_ci(ENV_COMMAND_START) {
            out.command_start = parse_list(&v);
        }
        if let Some(v) = env.get_ci(ENV_COMMAND_SEP) {
            out.command_sep = parse_list(&v);
        }
        out.custom = env
            .entries()
            .into_iter()
            .filter(|e| !is_system_env_key(&e.key) && !e.key.eq_ignore_ascii_case("ENVIRONMENT"))
            .map(|e| NoneBot2EnvEntry {
                key: e.key,
                value: e.value,
                comment: e.comment,
            })
            .collect();
        out
    }

    pub fn apply_to(&self, env: &mut EnvFile) {
        env.set_ci(ENV_HOST, &self.host);
        env.set_ci(ENV_PORT, &self.port.to_string());
        env.set_ci(ENV_DRIVER, &self.driver);
        env.set_ci(ENV_LOG_LEVEL, &self.log_level);
        env.set_ci(ENV_ONEBOT_ACCESS_TOKEN, &self.onebot_access_token);
        env.set_ci(ENV_SUPERUSERS, &render_list(&self.superusers));
        env.set_ci(ENV_NICKNAME, &render_list(&self.nickname));
        env.set_ci(ENV_COMMAND_START, &render_list(&self.command_start));
        env.set_ci(ENV_COMMAND_SEP, &render_list(&self.command_sep));

        let keep: std::collections::BTreeSet<&str> = self
            .custom
            .iter()
            .map(|e| e.key.as_str())
            .collect();
        for entry in env.entries() {
            if !is_system_env_key(&entry.key)
                && !entry.key.eq_ignore_ascii_case("ENVIRONMENT")
                && !keep.contains(entry.key.as_str())
            {
                env.remove(&entry.key);
            }
        }
        for c in &self.custom {
            if c.key.trim().is_empty()
                || is_system_env_key(&c.key)
                || c.key.eq_ignore_ascii_case("ENVIRONMENT")
            {
                continue;
            }
            env.set_with_comment(&c.key, &c.value, &c.comment);
        }
    }
}

impl NoneBot2InstanceConfig {
    pub fn validate(&self) -> Vec<AppConfigIssue> {
        let mut sink = IssueSink::default();
        if self.env_prod.port == 0 {
            sink.push("env_prod/port", "端口不能为 0");
        }
        if self.env_prod.host.trim().is_empty() {
            sink.push("env_prod/host", "HOST 不能为空");
        }
        if !valid_env_value(&self.env_prod.host) {
            sink.push("env_prod/host", "值不能包含换行");
        }
        if !valid_env_value(&self.env_prod.driver) {
            sink.push("env_prod/driver", "值不能包含换行");
        }
        if !valid_env_value(&self.env_prod.log_level) {
            sink.push("env_prod/log_level", "值不能包含换行");
        }
        if !valid_env_value(&self.env_prod.onebot_access_token) {
            sink.push("env_prod/onebot_access_token", "值不能包含换行");
        }
        for (i, c) in self.env_prod.custom.iter().enumerate() {
            if !valid_env_key(&c.key) {
                sink.push(format!("env_prod/custom/{i}/key"), "键名只能是字母数字下划线");
            }
            if !valid_env_value(&c.value) {
                sink.push(format!("env_prod/custom/{i}/value"), "值不能包含换行");
            }
        }
        sink.into_vec()
    }
}

fn config_doc(id: &str, rel: &str, format: AppConfigFormat) -> AppConfigDocument {
    AppConfigDocument {
        id: id.to_string(),
        label: rel.to_string(),
        rel_path: rel.to_string(),
        format,
        hot_reload: false,
    }
}

pub fn nonebot2_config_documents() -> Vec<AppConfigDocument> {
    vec![
        config_doc(DOC_ENV, NONEBOT2_ENV_FILE, AppConfigFormat::DotEnv),
        config_doc(DOC_ENV_PROD, NONEBOT2_ENV_PROD_FILE, AppConfigFormat::DotEnv),
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
    let mut env = EnvFile::parse(base.unwrap_or(""));
    if let Some(over) = overlay.filter(|s| !s.is_empty()) {
        let extra = EnvFile::parse(over);
        for entry in extra.entries() {
            env.set_with_comment(&entry.key, &entry.value, &entry.comment);
        }
    }
    env
}

fn is_system_env_key(key: &str) -> bool {
    SYSTEM_KEYS.iter().any(|k| key.eq_ignore_ascii_case(k))
}

pub fn link_inputs_changed(before: &NoneBot2EnvProd, after: &NoneBot2EnvProd) -> bool {
    before.port != after.port || before.onebot_access_token != after.onebot_access_token
}

pub fn parse_nonebot2_config(snaps: &[DocumentSnapshot]) -> Result<NoneBot2InstanceConfig, AppFrameworkError> {
    let base = snaps
        .iter()
        .find(|s| s.doc.id == DOC_ENV)
        .and_then(|s| s.text.as_deref());
    let overlay = snaps
        .iter()
        .find(|s| s.doc.id == DOC_ENV_PROD)
        .and_then(|s| s.text.as_deref());
    if base.is_none() && overlay.is_none() {
        return Err(AppFrameworkError::Integration(
            "NoneBot2 实例没有 .env / .env.{ENVIRONMENT}".to_string(),
        ));
    }
    Ok(NoneBot2InstanceConfig {
        env_prod: NoneBot2EnvProd::from_env_file(&merge_env_texts(base, overlay)),
    })
}

pub fn read_listen_port(base: Option<&str>, overlay: Option<&str>) -> Option<u16> {
    merge_env_texts(base, overlay)
        .get_ci(ENV_PORT)
        .and_then(|v| v.parse().ok())
        .filter(|p| *p > 0)
}

pub fn read_access_token_from_texts(base: Option<&str>, overlay: Option<&str>) -> Option<String> {
    let merged = merge_env_texts(base, overlay);
    merged
        .get_ci(ENV_ONEBOT_ACCESS_TOKEN)
        .or_else(|| merged.get_ci("ONEBOT_V11_ACCESS_TOKEN"))
        .filter(|v| !v.trim().is_empty())
}

pub fn read_outbound_ws_urls_from_texts(base: Option<&str>, overlay: Option<&str>) -> Vec<String> {
    merge_env_texts(base, overlay)
        .get_ci(ENV_ONEBOT_WS_URLS)
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

pub async fn read_nonebot2_config(
    host: &dyn Host,
    install_dir: &HostPath,
) -> Result<(NoneBot2InstanceConfig, Vec<DocumentSnapshot>), AppFrameworkError> {
    let layout = load_env_layout(host, install_dir).await?;
    let snaps = read_documents(host, install_dir, &nonebot2_config_documents_for(&layout)).await?;
    parse_nonebot2_config(&snaps).map(|config| (config, snaps))
}

pub async fn write_nonebot2_config(
    host: &dyn Host,
    install_dir: &HostPath,
    config: &NoneBot2InstanceConfig,
    _current: &[DocumentSnapshot],
    write_sidecar: bool,
) -> Result<(NoneBot2InstanceConfig, Vec<DocumentSnapshot>), AppFrameworkError> {
    let issues = config.validate();
    if !issues.is_empty() {
        return Err(AppFrameworkError::ConfigInvalid(issues));
    }
    let layout = load_env_layout(host, install_dir).await?;
    let path = install_dir.join(&layout.write_rel);
    let text = read_optional_text(host, &path).await?.unwrap_or_default();
    let mut env = EnvFile::parse(&text);
    config.env_prod.apply_to(&mut env);
    let out = env.render();
    if revision_of(out.as_bytes()) != revision_of(text.as_bytes()) {
        apply_with_backup_ex(host, std::slice::from_ref(&path), write_sidecar, || async {
            host.write_file(&path, out.as_bytes())
                .await
                .map_err(|e| AppFrameworkError::Integration(e.to_string()))
        })
        .await?;
    }
    read_nonebot2_config(host, install_dir).await
}

async fn read_optional_text(
    host: &dyn Host,
    path: &HostPath,
) -> Result<Option<String>, AppFrameworkError> {
    if !host
        .exists(path)
        .await
        .map_err(|e| AppFrameworkError::Host(e.to_string()))?
    {
        return Ok(None);
    }
    let bytes = host
        .read_file(path)
        .await
        .map_err(|e| AppFrameworkError::Host(e.to_string()))?;
    Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
}

fn valid_env_key(key: &str) -> bool {
    let k = key.trim();
    let mut chars = k.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {
            chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        }
        _ => false,
    }
}

fn valid_env_value(value: &str) -> bool {
    !value.contains('\n') && !value.contains('\r')
}

fn parse_list(raw: &str) -> Vec<String> {
    // EnvFile::set 会把含 `"` 的 JSON 数组再包一层并转义；get 只剥外层引号
    let t = raw.trim().replace("\\\"", "\"");
    if t.is_empty() {
        return Vec::new();
    }
    if t.starts_with('[') {
        return serde_json::from_str::<Vec<serde_json::Value>>(&t)
            .map(|items| {
                items
                    .into_iter()
                    .map(|v| match v {
                        serde_json::Value::String(s) => s,
                        other => other.to_string(),
                    })
                    .collect()
            })
            .unwrap_or_else(|_| vec![t.to_string()]);
    }
    t.split(',')
        .map(|s| s.trim().trim_matches('"').to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn render_list(items: &[String]) -> String {
    serde_json::to_string(items).unwrap_or_else(|_| "[]".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_round_trip_keeps_custom_and_lists() {
        let text = "DRIVER=~fastapi\nPORT=8081\nSUPERUSERS=[\"10001\"]\nWEATHER__API_KEY=abc\n";
        let parsed = NoneBot2EnvProd::from_env_file(&EnvFile::parse(text));
        assert_eq!(parsed.port, 8081);
        assert_eq!(parsed.superusers, vec!["10001".to_string()]);
        assert_eq!(parsed.custom[0].key, "WEATHER__API_KEY");
        let mut env = EnvFile::parse(text);
        parsed.apply_to(&mut env);
        let again = NoneBot2EnvProd::from_env_file(&EnvFile::parse(&env.render()));
        assert_eq!(again.port, 8081);
        assert_eq!(again.superusers, vec!["10001".to_string()]);
        assert_eq!(again.custom[0].value, "abc");
    }

    #[test]
    fn reject_newline_in_custom_env() {
        let mut cfg = NoneBot2InstanceConfig {
            env_prod: NoneBot2EnvProd::default(),
        };
        cfg.env_prod.custom.push(NoneBot2EnvEntry {
            key: "FOO\nBAR".into(),
            value: "1".into(),
            comment: String::new(),
        });
        assert!(cfg.validate().iter().any(|i| i.path.contains("custom")));
    }

    #[test]
    fn link_inputs_watch_port_and_token() {
        let a = NoneBot2EnvProd::default();
        let mut b = a.clone();
        assert!(!link_inputs_changed(&a, &b));
        b.port = 9;
        assert!(link_inputs_changed(&a, &b));
        b = a.clone();
        b.onebot_access_token = "x".into();
        assert!(link_inputs_changed(&a, &b));
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
        let parsed = NoneBot2EnvProd::from_env_file(&merged);
        assert_eq!(parsed.port, 8080);
        assert_eq!(parsed.log_level, "DEBUG");
        assert!(parsed.custom.iter().any(|e| e.key == "ONEBOT_WS_URLS"));
        assert!(!parsed.custom.iter().any(|e| e.key.eq_ignore_ascii_case("ENVIRONMENT")));
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
