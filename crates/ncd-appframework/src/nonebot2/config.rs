//! NoneBot2 核心与 OneBot v11 配置。驱动 / 插件扩展键保留在 custom 中。
//! 对照 nonebot2 2.5.0@6e1efbea 与 adapter-onebot@2f5930d4；别名优先于通用键。

use ncd_domain::{AppConfigDocument, AppConfigFormat, AppConfigIssue};
use ncd_host::{Host, HostPath};
use ncd_traits::AppFrameworkError;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::adapter::apply_with_backup_ex;
use crate::config_doc::{DocumentSnapshot, IssueSink, read_documents, revision_of};
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
const ENV_API_TIMEOUT: &str = "API_TIMEOUT";
const ENV_SESSION_EXPIRE_TIMEOUT: &str = "SESSION_EXPIRE_TIMEOUT";
const ENV_ONEBOT_SECRET: &str = "ONEBOT_SECRET";
const ENV_ONEBOT_API_ROOTS: &str = "ONEBOT_API_ROOTS";
const ONEBOT_V11_ALIASES: [(&str, &str); 4] = [
    (ENV_ONEBOT_ACCESS_TOKEN, "ONEBOT_V11_ACCESS_TOKEN"),
    (ENV_ONEBOT_SECRET, "ONEBOT_V11_SECRET"),
    (ENV_ONEBOT_WS_URLS, "ONEBOT_V11_WS_URLS"),
    (ENV_ONEBOT_API_ROOTS, "ONEBOT_V11_API_ROOTS"),
];

const SYSTEM_KEYS: [&str; 14] = [
    ENV_DRIVER,
    ENV_HOST,
    ENV_PORT,
    ENV_LOG_LEVEL,
    ENV_ONEBOT_ACCESS_TOKEN,
    ENV_SUPERUSERS,
    ENV_NICKNAME,
    ENV_COMMAND_START,
    ENV_COMMAND_SEP,
    ENV_API_TIMEOUT,
    ENV_SESSION_EXPIRE_TIMEOUT,
    ENV_ONEBOT_SECRET,
    ENV_ONEBOT_WS_URLS,
    ENV_ONEBOT_API_ROOTS,
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
#[serde(default)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct NoneBot2EnvProd {
    pub host: String,
    pub port: u16,
    pub driver: String,
    pub log_level: String,
    pub onebot_access_token: String,
    pub onebot_secret: String,
    pub onebot_ws_urls: Vec<String>,
    /// 原始 JSON 对象，键为 Bot self_id，值为 HTTP API 地址。
    pub onebot_api_roots: String,
    /// 保留 dotenv 数值写法，避免保存其他字段时改变精度。
    pub api_timeout: String,
    /// Pydantic 同时接受秒数、HH:MM:SS 与 ISO 8601 时长。
    pub session_expire_timeout: String,
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
            onebot_secret: String::new(),
            onebot_ws_urls: Vec::new(),
            onebot_api_roots: "{}".into(),
            api_timeout: "30.0".into(),
            session_expire_timeout: "PT2M".into(),
            superusers: Vec::new(),
            nickname: Vec::new(),
            command_start: vec!["/".into()],
            command_sep: vec![".".into()],
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
        if let Some(v) = read_onebot_env(env, ENV_ONEBOT_ACCESS_TOKEN) {
            out.onebot_access_token = v;
        }
        if let Some(v) = read_onebot_env(env, ENV_ONEBOT_SECRET) {
            out.onebot_secret = v;
        }
        if let Some(v) = read_onebot_env(env, ENV_ONEBOT_WS_URLS) {
            out.onebot_ws_urls = parse_list(&v);
        }
        if let Some(v) = read_onebot_env(env, ENV_ONEBOT_API_ROOTS) {
            out.onebot_api_roots = v;
        }
        if let Some(v) = env.get_ci(ENV_API_TIMEOUT) {
            out.api_timeout = v;
        }
        if let Some(v) = env.get_ci(ENV_SESSION_EXPIRE_TIMEOUT) {
            out.session_expire_timeout = v;
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
            .entries_ci()
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
        write_onebot_env(env, ENV_ONEBOT_ACCESS_TOKEN, &self.onebot_access_token);
        write_onebot_env(env, ENV_ONEBOT_SECRET, &self.onebot_secret);
        write_onebot_env(env, ENV_ONEBOT_WS_URLS, &render_list(&self.onebot_ws_urls));
        let roots = serde_json::from_str::<serde_json::Value>(&self.onebot_api_roots)
            .map(|value| value.to_string())
            .unwrap_or_else(|_| self.onebot_api_roots.clone());
        write_onebot_env(env, ENV_ONEBOT_API_ROOTS, &roots);
        env.set_ci(ENV_API_TIMEOUT, &self.api_timeout);
        if let Some(duration) = super::duration::normalize_duration(&self.session_expire_timeout) {
            env.set_ci(ENV_SESSION_EXPIRE_TIMEOUT, &duration);
        }
        env.set_ci(ENV_SUPERUSERS, &render_list(&self.superusers));
        env.set_ci(ENV_NICKNAME, &render_list(&self.nickname));
        env.set_ci(ENV_COMMAND_START, &render_list(&self.command_start));
        env.set_ci(ENV_COMMAND_SEP, &render_list(&self.command_sep));

        let keep: std::collections::BTreeSet<String> = self
            .custom
            .iter()
            .map(|e| e.key.to_ascii_lowercase())
            .collect();
        for entry in env.entries() {
            if !is_system_env_key(&entry.key)
                && !entry.key.eq_ignore_ascii_case("ENVIRONMENT")
                && !keep.contains(&entry.key.to_ascii_lowercase())
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
            let actual_key = env
                .entries_ci()
                .into_iter()
                .find(|entry| entry.key.eq_ignore_ascii_case(&c.key))
                .map(|entry| entry.key)
                .unwrap_or_else(|| c.key.clone());
            env.set_ci(&c.key, &c.value);
            env.set_with_comment(&actual_key, &c.value, &c.comment);
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
        if !self
            .env_prod
            .api_timeout
            .trim()
            .parse::<f64>()
            .is_ok_and(f64::is_finite)
            || !valid_env_value(&self.env_prod.api_timeout)
        {
            sink.push("env_prod/api_timeout", "API_TIMEOUT 须为有限秒数");
        }
        if super::duration::normalize_duration(&self.env_prod.session_expire_timeout).is_none() {
            sink.push(
                "env_prod/session_expire_timeout",
                "请输入有效时长；负数请用秒数或 -P 开头的 ISO 时长",
            );
        }
        if !valid_env_value(&self.env_prod.onebot_secret) {
            sink.push("env_prod/onebot_secret", "值不能包含换行");
        }
        for (i, url) in self.env_prod.onebot_ws_urls.iter().enumerate() {
            if !valid_env_value(url) || !valid_url(url, &["ws", "wss"]) {
                sink.push(
                    format!("env_prod/onebot_ws_urls/{i}"),
                    "须以 ws:// 或 wss:// 开头",
                );
            }
        }
        match serde_json::from_str::<std::collections::BTreeMap<String, String>>(
            &self.env_prod.onebot_api_roots,
        ) {
            Ok(roots) if roots.values().all(|url| valid_url(url, &["http", "https"])) => {}
            _ => sink.push(
                "env_prod/onebot_api_roots",
                "须为 self_id 到 HTTP / HTTPS 地址的 JSON 对象",
            ),
        }
        let mut seen = std::collections::BTreeSet::new();
        for (i, c) in self.env_prod.custom.iter().enumerate() {
            if !valid_env_key(&c.key) {
                sink.push(
                    format!("env_prod/custom/{i}/key"),
                    "键名只能是字母数字下划线",
                );
            }
            if is_system_env_key(&c.key) || c.key.eq_ignore_ascii_case("ENVIRONMENT") {
                sink.push(
                    format!("env_prod/custom/{i}/key"),
                    "请在对应系统字段中修改此键",
                );
            } else if !seen.insert(c.key.to_ascii_lowercase()) {
                sink.push(
                    format!("env_prod/custom/{i}/key"),
                    "键名重复（不区分大小写）",
                );
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

fn is_system_env_key(key: &str) -> bool {
    SYSTEM_KEYS.iter().any(|k| key.eq_ignore_ascii_case(k))
        || ONEBOT_V11_ALIASES
            .iter()
            .any(|(_, alias)| key.eq_ignore_ascii_case(alias))
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

pub fn link_inputs_changed(before: &NoneBot2EnvProd, after: &NoneBot2EnvProd) -> bool {
    before.port != after.port || before.onebot_access_token != after.onebot_access_token
}

pub fn parse_nonebot2_config(
    snaps: &[DocumentSnapshot],
) -> Result<NoneBot2InstanceConfig, AppFrameworkError> {
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
    let mut env = EnvFile::parse_python(&text);
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
    if key != key.trim() {
        return false;
    }
    let mut chars = key.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {
            chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        }
        _ => false,
    }
}

fn valid_url(value: &str, schemes: &[&str]) -> bool {
    reqwest::Url::parse(value).is_ok_and(|url| {
        schemes.contains(&url.scheme()) && url.host_str().is_some_and(|host| !host.is_empty())
    })
}

fn valid_env_value(value: &str) -> bool {
    !value.contains('\n') && !value.contains('\r')
}

fn parse_list(raw: &str) -> Vec<String> {
    let t = raw.trim();
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
        let parsed = NoneBot2EnvProd::from_env_file(&EnvFile::parse_python(text));
        assert_eq!(parsed.port, 8081);
        assert_eq!(parsed.superusers, vec!["10001".to_string()]);
        assert_eq!(parsed.custom[0].key, "WEATHER__API_KEY");
        let mut env = EnvFile::parse_python(text);
        parsed.apply_to(&mut env);
        let again = NoneBot2EnvProd::from_env_file(&EnvFile::parse_python(&env.render()));
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
        assert_eq!(parsed.onebot_ws_urls, vec!["ws://x".to_string()]);
        assert!(
            !parsed
                .custom
                .iter()
                .any(|e| e.key.eq_ignore_ascii_case("ENVIRONMENT"))
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
    #[test]
    fn core_and_onebot_v11_fields_round_trip_without_losing_extensions() {
        let text = concat!(
            "API_TIMEOUT=0.125\nSESSION_EXPIRE_TIMEOUT=P1DT2H\n",
            "ONEBOT_SECRET=secret\n",
            "ONEBOT_WS_URLS='[\"ws://127.0.0.1:3001\",\"wss://example.com/ws\"]'\n",
            "ONEBOT_API_ROOTS='{\"10001\":\"https://example.com/api\"}'\n",
            "# plugin setting\nCUSTOM_NESTED__KEY='{\"enabled\":true}'\n"
        );
        let parsed = NoneBot2EnvProd::from_env_file(&EnvFile::parse_python(text));
        assert_eq!(parsed.api_timeout, "0.125");
        assert_eq!(parsed.session_expire_timeout, "P1DT2H");
        assert_eq!(parsed.onebot_secret, "secret");
        assert_eq!(parsed.onebot_ws_urls.len(), 2);
        assert_eq!(parsed.custom.len(), 1);
        let mut env = EnvFile::parse_python(text);
        parsed.apply_to(&mut env);
        let again = NoneBot2EnvProd::from_env_file(&EnvFile::parse_python(&env.render()));
        assert_eq!(again, parsed);
        assert_eq!(again.custom[0].comment, "plugin setting");
    }

    #[test]
    fn onebot_v11_aliases_win_and_are_updated_on_save_and_relink() {
        let text = concat!(
            "ONEBOT_ACCESS_TOKEN=general\n",
            "onebot_v11_access_token=v11\n",
            "ONEBOT_WS_URLS='[\"ws://general\"]'\n",
            "onebot_v11_ws_urls='[\"wss://v11\"]'\n",
            "ONEBOT_V12_ACCESS_TOKEN=v12\n"
        );
        let mut env = EnvFile::parse_python(text);
        let mut parsed = NoneBot2EnvProd::from_env_file(&env);
        assert_eq!(parsed.onebot_access_token, "v11");
        assert_eq!(parsed.onebot_ws_urls, vec!["wss://v11"]);
        assert_eq!(parsed.custom[0].key, "ONEBOT_V12_ACCESS_TOKEN");
        parsed.onebot_access_token = "changed".into();
        parsed.apply_to(&mut env);
        assert_eq!(
            env.get_ci("ONEBOT_V11_ACCESS_TOKEN").as_deref(),
            Some("changed")
        );
        write_onebot_env(&mut env, ENV_ONEBOT_ACCESS_TOKEN, "relinked");
        assert_eq!(
            read_onebot_env(&env, ENV_ONEBOT_ACCESS_TOKEN).as_deref(),
            Some("relinked")
        );
        assert_eq!(
            read_access_token_from_texts(Some(&env.render()), None).as_deref(),
            Some("relinked")
        );
        assert_eq!(
            read_outbound_ws_urls_from_texts(Some(&env.render()), None),
            vec!["wss://v11"]
        );
    }

    #[test]
    fn missing_runtime_keys_use_official_defaults_and_overlay_ignores_case() {
        let parsed = NoneBot2EnvProd::from_env_file(&EnvFile::parse_python(""));
        assert_eq!(parsed.command_start, vec!["/"]);
        assert_eq!(parsed.command_sep, vec!["."]);
        assert_eq!(parsed.api_timeout, "30.0");
        assert_eq!(parsed.session_expire_timeout, "PT2M");
        let merged = merge_env_texts(
            Some("CUSTOM_KEY=base\nPORT=1\n"),
            Some("custom_key=overlay\nport=2\n"),
        );
        let parsed = NoneBot2EnvProd::from_env_file(&merged);
        assert_eq!(parsed.port, 2);
        assert_eq!(parsed.custom.len(), 1);
        assert_eq!(parsed.custom[0].value, "overlay");
    }

    #[test]
    fn rejects_invalid_new_fields_and_system_keys_in_custom() {
        let mut cfg = NoneBot2InstanceConfig {
            env_prod: NoneBot2EnvProd::default(),
        };
        cfg.env_prod.api_timeout = "NaN".into();
        cfg.env_prod.onebot_ws_urls = vec!["https://example.com".into()];
        cfg.env_prod.onebot_api_roots = r#"{"10001": 42}"#.into();
        cfg.env_prod.custom.push(NoneBot2EnvEntry {
            key: "ONEBOT_V11_SECRET".into(),
            value: "x".into(),
            comment: String::new(),
        });
        let issues = cfg.validate();
        assert!(issues.iter().any(|i| i.path == "env_prod/api_timeout"));
        assert!(issues.iter().any(|i| i.path == "env_prod/onebot_ws_urls/0"));
        assert!(issues.iter().any(|i| i.path == "env_prod/onebot_api_roots"));
        assert!(issues.iter().any(|i| i.path == "env_prod/custom/0/key"));
    }

    #[test]
    fn overlay_writes_override_aliases_in_base() {
        let base = "ONEBOT_V11_ACCESS_TOKEN=base-token\n";
        let mut overlay = EnvFile::parse_python("PORT=8080\n");
        write_onebot_env(&mut overlay, ENV_ONEBOT_ACCESS_TOKEN, "new-token");
        assert_eq!(
            read_access_token_from_texts(Some(base), Some(&overlay.render())).as_deref(),
            Some("new-token")
        );
    }

    #[test]
    fn rejects_empty_url_hosts_and_accepts_case_insensitive_schemes() {
        let mut cfg = NoneBot2InstanceConfig {
            env_prod: NoneBot2EnvProd::default(),
        };
        cfg.env_prod.onebot_ws_urls = vec!["ws://".into()];
        cfg.env_prod.onebot_api_roots = r#"{"1":"https://"}"#.into();
        assert_eq!(cfg.validate().len(), 2);
        cfg.env_prod.onebot_ws_urls = vec!["WSS://example.com/ws".into()];
        cfg.env_prod.onebot_api_roots = r#"{"1":"HTTPS://example.com/api"}"#.into();
        assert!(cfg.validate().is_empty());
        cfg.env_prod.custom.push(NoneBot2EnvEntry {
            key: " PORT ".into(),
            value: "9000".into(),
            comment: String::new(),
        });
        assert!(
            cfg.validate()
                .iter()
                .any(|i| i.path == "env_prod/custom/0/key")
        );
    }

    #[test]
    fn hash_prefixed_secrets_are_literal_python_dotenv_values() {
        let env = EnvFile::parse_python("ONEBOT_SECRET=#secret\nCUSTOM=#abc\n");
        let parsed = NoneBot2EnvProd::from_env_file(&env);
        assert_eq!(parsed.onebot_secret, "#secret");
        assert_eq!(parsed.custom[0].value, "#abc");
        let mut out = env;
        parsed.apply_to(&mut out);
        assert_eq!(
            NoneBot2EnvProd::from_env_file(&EnvFile::parse_python(&out.render())).onebot_secret,
            "#secret"
        );
    }

    #[test]
    fn ambiguous_negative_clock_is_rejected_without_reinterpreting_the_value() {
        let mut cfg = NoneBot2InstanceConfig {
            env_prod: NoneBot2EnvProd::default(),
        };
        cfg.env_prod.session_expire_timeout = "-1 day, 02:03:04.000005".into();
        assert!(
            cfg.validate()
                .iter()
                .any(|issue| issue.path == "env_prod/session_expire_timeout")
        );
        assert_eq!(
            cfg.env_prod.session_expire_timeout,
            "-1 day, 02:03:04.000005"
        );
        cfg.env_prod.session_expire_timeout = "-79015.999995".into();
        assert!(cfg.validate().is_empty());
        cfg.env_prod.session_expire_timeout = "-PT79015.999995S".into();
        assert!(cfg.validate().is_empty());
    }
}
