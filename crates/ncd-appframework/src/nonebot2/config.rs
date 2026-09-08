//! NoneBot2 窄类型化配置：只覆盖对接和常用运行键，插件/适配器列表由商店 Tab 写 toml。

use ncd_domain::{AppConfigDocument, AppConfigFormat, AppConfigIssue};
use ncd_host::{Host, HostPath};
use ncd_traits::AppFrameworkError;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::adapter::apply_with_backup;
use crate::config_doc::{
    DocumentSnapshot, IssueSink, read_documents, revision_of,
};
use crate::env_file::EnvFile;

use super::manifest::{
    ENV_DRIVER, ENV_HOST, ENV_ONEBOT_ACCESS_TOKEN, ENV_PORT, NONEBOT2_ENV_FILE,
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
        if let Some(v) = env.get(ENV_HOST) {
            out.host = v;
        }
        if let Some(v) = env.get(ENV_PORT)
            && let Ok(p) = v.parse()
        {
            out.port = p;
        }
        if let Some(v) = env.get(ENV_DRIVER) {
            out.driver = v;
        }
        if let Some(v) = env.get(ENV_LOG_LEVEL) {
            out.log_level = v;
        }
        if let Some(v) = env.get(ENV_ONEBOT_ACCESS_TOKEN) {
            out.onebot_access_token = v;
        }
        if let Some(v) = env.get(ENV_SUPERUSERS) {
            out.superusers = parse_list(&v);
        }
        if let Some(v) = env.get(ENV_NICKNAME) {
            out.nickname = parse_list(&v);
        }
        if let Some(v) = env.get(ENV_COMMAND_START) {
            out.command_start = parse_list(&v);
        }
        if let Some(v) = env.get(ENV_COMMAND_SEP) {
            out.command_sep = parse_list(&v);
        }
        out.custom = env
            .entries()
            .into_iter()
            .filter(|e| !SYSTEM_KEYS.contains(&e.key.as_str()))
            .map(|e| NoneBot2EnvEntry {
                key: e.key,
                value: e.value,
                comment: e.comment,
            })
            .collect();
        out
    }

    pub fn apply_to(&self, env: &mut EnvFile) {
        env.set(ENV_HOST, &self.host);
        env.set(ENV_PORT, &self.port.to_string());
        env.set(ENV_DRIVER, &self.driver);
        env.set(ENV_LOG_LEVEL, &self.log_level);
        env.set(ENV_ONEBOT_ACCESS_TOKEN, &self.onebot_access_token);
        env.set(ENV_SUPERUSERS, &render_list(&self.superusers));
        env.set(ENV_NICKNAME, &render_list(&self.nickname));
        env.set(ENV_COMMAND_START, &render_list(&self.command_start));
        env.set(ENV_COMMAND_SEP, &render_list(&self.command_sep));

        let keep: std::collections::BTreeSet<&str> = self
            .custom
            .iter()
            .map(|e| e.key.as_str())
            .collect();
        for entry in env.entries() {
            if !SYSTEM_KEYS.contains(&entry.key.as_str()) && !keep.contains(entry.key.as_str()) {
                env.remove(&entry.key);
            }
        }
        for c in &self.custom {
            if c.key.trim().is_empty() || SYSTEM_KEYS.contains(&c.key.as_str()) {
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

pub fn nonebot2_config_documents() -> Vec<AppConfigDocument> {
    let d = |id: &str, rel: &str, format: AppConfigFormat| AppConfigDocument {
        id: id.to_string(),
        label: rel.to_string(),
        rel_path: rel.to_string(),
        format,
        hot_reload: false,
    };
    vec![
        d(DOC_ENV, NONEBOT2_ENV_FILE, AppConfigFormat::DotEnv),
        d(DOC_ENV_PROD, NONEBOT2_ENV_PROD_FILE, AppConfigFormat::DotEnv),
        d(DOC_PYPROJECT, NONEBOT2_PYPROJECT, AppConfigFormat::Toml),
    ]
}

pub fn link_inputs_changed(before: &NoneBot2EnvProd, after: &NoneBot2EnvProd) -> bool {
    before.port != after.port || before.onebot_access_token != after.onebot_access_token
}

pub fn parse_nonebot2_config(snaps: &[DocumentSnapshot]) -> Result<NoneBot2InstanceConfig, AppFrameworkError> {
    let env_text = snaps
        .iter()
        .find(|s| s.doc.id == DOC_ENV_PROD)
        .and_then(|s| s.text.as_deref())
        .ok_or_else(|| {
            AppFrameworkError::Integration("NoneBot2 实例缺少 .env.prod，请先完成安装".to_string())
        })?;
    Ok(NoneBot2InstanceConfig {
        env_prod: NoneBot2EnvProd::from_env_file(&EnvFile::parse(env_text)),
    })
}

pub async fn read_nonebot2_config(
    host: &dyn Host,
    install_dir: &HostPath,
) -> Result<(NoneBot2InstanceConfig, Vec<DocumentSnapshot>), AppFrameworkError> {
    let snaps = read_documents(host, install_dir, &nonebot2_config_documents()).await?;
    let config = parse_nonebot2_config(&snaps)?;
    Ok((config, snaps))
}

pub async fn write_nonebot2_config(
    host: &dyn Host,
    install_dir: &HostPath,
    config: &NoneBot2InstanceConfig,
    current: &[DocumentSnapshot],
) -> Result<(NoneBot2InstanceConfig, Vec<DocumentSnapshot>), AppFrameworkError> {
    let issues = config.validate();
    if !issues.is_empty() {
        return Err(AppFrameworkError::ConfigInvalid(issues));
    }
    let path = install_dir.join(NONEBOT2_ENV_PROD_FILE);
    let text = current
        .iter()
        .find(|s| s.doc.id == DOC_ENV_PROD)
        .and_then(|s| s.text.clone())
        .unwrap_or_default();
    let mut env = EnvFile::parse(&text);
    config.env_prod.apply_to(&mut env);
    let out = env.render();
    let unchanged = current
        .iter()
        .find(|s| s.doc.id == DOC_ENV_PROD)
        .is_some_and(|s| s.revision == revision_of(out.as_bytes()));
    if !unchanged {
        apply_with_backup(host, std::slice::from_ref(&path), || async {
            host.write_file(&path, out.as_bytes())
                .await
                .map_err(|e| AppFrameworkError::Integration(e.to_string()))
        })
        .await?;
    }
    read_nonebot2_config(host, install_dir).await
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
}
