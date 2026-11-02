//! 云崽连接配置读取、鉴权头写入与备份清单。

use ncd_domain::{AppConfigDocument, AppConfigFormat};
use ncd_host::{Host, HostPath};
use ncd_traits::AppFrameworkError;
use serde_yaml::{Mapping, Value};

use super::manifest::{
    YUNZAI_AUTH_HEADER, YUNZAI_CONFIG_DIR, YUNZAI_CONFIG_NAMES, YUNZAI_DEFAULT_CONFIG_DIR,
    YUNZAI_DEFAULT_PORT,
};

pub struct ServerConnection {
    pub port: u16,
    pub headers: Vec<(String, String)>,
}

impl ServerConnection {
    pub fn access_token(&self) -> Option<String> {
        self.headers
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(YUNZAI_AUTH_HEADER))
            .map(|(_, value)| {
                value
                    .strip_prefix("Bearer ")
                    .unwrap_or(value)
                    .trim()
                    .to_string()
            })
            .filter(|token| !token.is_empty())
    }
}

pub async fn read_effective_config(
    host: &dyn Host,
    root: &HostPath,
    name: &str,
) -> Result<Mapping, AppFrameworkError> {
    let user = read_optional(host, &root.join(config_rel(name))).await?;
    let default = read_optional(host, &root.join(default_config_rel(name))).await?;
    let user = parse_mapping(user.as_deref(), &config_rel(name))?;
    let default = parse_mapping(default.as_deref(), &default_config_rel(name)).unwrap_or_default();
    Ok(merged(&default, &user))
}

pub async fn read_server_connection(
    host: &dyn Host,
    root: &HostPath,
) -> Result<ServerConnection, AppFrameworkError> {
    let map = read_effective_config(host, root, "server").await?;
    let headers = get(&map, "auth")
        .and_then(Value::as_mapping)
        .into_iter()
        .flatten()
        .filter_map(|(key, value)| Some((key_str(key)?, scalar_string(value)?)))
        .collect();
    Ok(ServerConnection {
        port: as_u16(get(&map, "port"))
            .filter(|port| *port > 0)
            .unwrap_or(YUNZAI_DEFAULT_PORT),
        headers,
    })
}

pub fn config_rel(name: &str) -> String {
    format!("{YUNZAI_CONFIG_DIR}/{name}.yaml")
}

pub fn default_config_rel(name: &str) -> String {
    format!("{YUNZAI_DEFAULT_CONFIG_DIR}/{name}.yaml")
}

/// 随实例备份的配置文件。
pub fn yunzai_config_documents() -> Vec<AppConfigDocument> {
    YUNZAI_CONFIG_NAMES
        .iter()
        .map(|name| AppConfigDocument {
            id: (*name).to_string(),
            label: format!("{name}.yaml"),
            rel_path: config_rel(name),
            format: AppConfigFormat::Yaml,
            hot_reload: !matches!(*name, "server" | "redis" | "db" | "milky" | "satori"),
            reload_note: (!matches!(*name, "server" | "redis" | "db" | "milky" | "satori")).then(
                || {
                    "启用文件监听时支持自动热重载；file_watch 等启动项需重启，请查看实例日志确认。"
                        .into()
                },
            ),
        })
        .collect()
}

fn key_str(k: &Value) -> Option<String> {
    match k {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn get<'a>(map: &'a Mapping, key: &str) -> Option<&'a Value> {
    map.iter()
        .find(|(k, _)| key_str(k).as_deref() == Some(key))
        .map(|(_, v)| v)
}

fn scalar_string(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn as_u64(v: Option<&Value>) -> Option<u64> {
    match v? {
        Value::Number(n) => n.as_u64().or_else(|| n.as_f64().map(|f| f.max(0.0) as u64)),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

fn as_u16(v: Option<&Value>) -> Option<u16> {
    as_u64(v)
        .filter(|n| *n <= u16::MAX as u64)
        .map(|n| n as u16)
}

fn parse_mapping(text: Option<&str>, label: &str) -> Result<Mapping, AppFrameworkError> {
    let Some(text) = text else {
        return Ok(Mapping::new());
    };
    if text.trim().is_empty() {
        return Ok(Mapping::new());
    }
    match serde_yaml::from_str::<Value>(text) {
        Ok(Value::Mapping(m)) => Ok(m),
        Ok(Value::Null) => Ok(Mapping::new()),
        Ok(_) => Err(AppFrameworkError::Validation(format!(
            "{label} 顶层不是键值表"
        ))),
        Err(e) => Err(AppFrameworkError::Validation(format!(
            "{label} 不是合法的 YAML：{e}"
        ))),
    }
}

/// 上游 `getAllCfg`：默认那份打底，用户那份的顶层键整条盖上去
fn merged(default: &Mapping, user: &Mapping) -> Mapping {
    let mut out = default.clone();
    for (k, v) in user {
        let key = key_str(k);
        // 默认那份写成数字、用户那份写成字符串的同名键也算同一个
        let existing = out.keys().find(|dk| key_str(dk) == key).cloned();
        out.insert(existing.unwrap_or_else(|| k.clone()), v.clone());
    }
    out
}

async fn read_optional(
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

/// auth 里 Authorization 那一条换成新 token（空就删掉）；`keep_other` 为假时别的头一并去掉
pub fn auth_value(current: Option<&Value>, token: &str, keep_other: bool) -> Value {
    let mut auth = match current {
        Some(Value::Mapping(m)) if keep_other => m.clone(),
        _ => Mapping::new(),
    };
    let existing: Vec<Value> = auth
        .keys()
        .filter(|k| key_str(k).is_some_and(|s| s.eq_ignore_ascii_case(YUNZAI_AUTH_HEADER)))
        .cloned()
        .collect();
    for k in existing {
        auth.remove(&k);
    }
    if !token.is_empty() {
        auth.insert(
            Value::String(YUNZAI_AUTH_HEADER.into()),
            Value::String(format!("Bearer {token}")),
        );
    }
    if auth.is_empty() {
        Value::Null
    } else {
        Value::Mapping(auth)
    }
}

/// `http://localhost:2536` 里的端口跟着换；用户改成别的样子（没写端口 / 反代地址）就不碰
pub fn url_with_port(url: &str, old_port: u16, new_port: u16) -> Option<String> {
    let marker = format!(":{old_port}");
    let (scheme_end, rest) = url.split_once("://")?;
    let host_part = rest.split('/').next().unwrap_or(rest);
    if !host_part.ends_with(&marker) {
        return None;
    }
    let new_host = format!(
        "{}:{new_port}",
        &host_part[..host_part.len() - marker.len()]
    );
    Some(format!(
        "{scheme_end}://{new_host}{}",
        &rest[host_part.len()..]
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn connection_reads_only_server_config_and_keeps_upstream_merge_rules() {
        let host = crate::config_backup::tests::TestHost::new("local");
        host.put(
            "/yunzai/config/default_config/server.yaml",
            b"port: 2536\nauth:\n  Authorization: Bearer default\n  X-Default: enabled\n",
        );
        host.put(
            "/yunzai/config/config/server.yaml",
            b"port: 29001\nauth:\n  authorization: Bearer custom\n",
        );
        host.put("/yunzai/config/config/bot.yaml", b"broken: [");
        let connection = read_server_connection(host.as_ref(), &HostPath::from_posix("/yunzai"))
            .await
            .unwrap();
        assert_eq!(connection.port, 29001);
        assert_eq!(connection.access_token().as_deref(), Some("custom"));
        assert_eq!(connection.headers.len(), 1, "auth uses a shallow override");
    }
}
