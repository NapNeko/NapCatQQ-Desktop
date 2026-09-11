//! 窄类型化：只暴露认领到的那条 OneBot 行 + 只读 dashboard.port。

use ncd_domain::AppConfigIssue;
use ncd_host::{Host, HostPath};
use ncd_traits::AppFrameworkError;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use super::ai::{
    self, AstrBotAiSettings, AstrBotKbBind, AstrBotPlatformGates, AstrBotProviderModel,
    AstrBotProviderSource, AstrBotSttSettings, AstrBotSubagentConfig, AstrBotTtsSettings,
    AstrBotWebSearchSettings,
};
use super::config_json::{
    cmd_config_documents, load_cmd_config, read_cmd_config_snapshots, save_cmd_config,
};
use super::manifest::{ASTRBOT_DEFAULT_DASHBOARD_PORT, ASTRBOT_DEFAULT_PORT};
use super::platform::{
    self, Claim, apply_claimed_fields, claim_for_probe, claim_for_upsert, collect_port_issues,
    dashboard_port, ncd_platform_id, other_platform_types, platforms, read_aiocqhttp_host,
    read_aiocqhttp_port, read_aiocqhttp_token, seed_row,
};
use crate::adopt::write_project_sidecar;
use crate::config_doc::{DocumentSnapshot, IssueSink};
use ncd_domain::AppInstance;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AstrBotOneBotRow {
    pub id: String,
    pub enable: bool,
    pub ws_reverse_host: String,
    pub ws_reverse_port: u16,
    pub ws_reverse_token: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AstrBotInstanceConfig {
    pub onebot: AstrBotOneBotRow,
    pub dashboard_port: u16,
    /// 其它 platform type，只读提示用
    #[serde(default)]
    pub other_platforms: Vec<String>,
    /// 文件里已经有可认领的 aiocqhttp
    #[serde(default)]
    pub claimed: bool,
    #[serde(default)]
    pub sources: Vec<AstrBotProviderSource>,
    #[serde(default)]
    pub models: Vec<AstrBotProviderModel>,
    #[serde(default)]
    pub ai: AstrBotAiSettings,
    #[serde(default)]
    pub stt: AstrBotSttSettings,
    #[serde(default)]
    pub tts: AstrBotTtsSettings,
    #[serde(default)]
    pub websearch: AstrBotWebSearchSettings,
    #[serde(default)]
    pub kb: AstrBotKbBind,
    #[serde(default)]
    pub gates: AstrBotPlatformGates,
    #[serde(default)]
    pub subagent: AstrBotSubagentConfig,
}

impl Default for AstrBotOneBotRow {
    fn default() -> Self {
        Self {
            id: String::new(),
            enable: true,
            ws_reverse_host: "0.0.0.0".into(),
            ws_reverse_port: ASTRBOT_DEFAULT_PORT,
            ws_reverse_token: String::new(),
        }
    }
}

impl Default for AstrBotInstanceConfig {
    fn default() -> Self {
        Self {
            onebot: AstrBotOneBotRow::default(),
            dashboard_port: ASTRBOT_DEFAULT_DASHBOARD_PORT,
            other_platforms: Vec::new(),
            claimed: false,
            sources: Vec::new(),
            models: Vec::new(),
            ai: AstrBotAiSettings::default(),
            stt: AstrBotSttSettings::default(),
            tts: AstrBotTtsSettings::default(),
            websearch: AstrBotWebSearchSettings::default(),
            kb: AstrBotKbBind::default(),
            gates: AstrBotPlatformGates::default(),
            subagent: AstrBotSubagentConfig::default(),
        }
    }
}

impl AstrBotInstanceConfig {
    pub fn from_root(root: &Value, instance_id: &str, listen_port: u16) -> Self {
        let rows = platforms(root);
        let want_id = ncd_platform_id(instance_id);
        let idx = rows
            .iter()
            .position(|row| platform::row_id(row) == Some(want_id.as_str()) && platform::is_aiocqhttp(row))
            .or_else(|| claim_for_probe(rows));
        let onebot = if let Some(i) = idx {
            let row = &rows[i];
            AstrBotOneBotRow {
                id: row
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                enable: platform::is_enabled(row),
                ws_reverse_host: read_aiocqhttp_host(row),
                ws_reverse_port: read_aiocqhttp_port(row).unwrap_or(listen_port.max(1)),
                ws_reverse_token: read_aiocqhttp_token(row).unwrap_or_default(),
            }
        } else {
            AstrBotOneBotRow {
                id: ncd_platform_id(instance_id),
                enable: true,
                ws_reverse_host: "0.0.0.0".into(),
                ws_reverse_port: if listen_port > 0 {
                    listen_port
                } else {
                    ASTRBOT_DEFAULT_PORT
                },
                ws_reverse_token: String::new(),
            }
        };
        Self {
            onebot,
            dashboard_port: dashboard_port(root),
            other_platforms: other_platform_types(rows, idx),
            claimed: idx.is_some(),
            sources: ai::sources_from_root(root),
            models: ai::models_from_root(root),
            ai: ai::ai_from_root(root),
            stt: ai::stt_from_root(root),
            tts: ai::tts_from_root(root),
            websearch: ai::websearch_from_root(root),
            kb: ai::kb_from_root(root),
            gates: ai::gates_from_root(root),
            subagent: ai::subagent_from_root(root),
        }
    }

    pub fn validate(&self) -> Vec<AppConfigIssue> {
        let mut sink = IssueSink::default();
        if self.onebot.ws_reverse_port == 0 {
            sink.push("onebot/ws_reverse_port", "端口不能为 0");
        }
        if self.onebot.ws_reverse_host.trim().is_empty() {
            sink.push("onebot/ws_reverse_host", "主机不能为空");
        }
        if self.dashboard_port > 0 && self.onebot.ws_reverse_port == self.dashboard_port {
            sink.push(
                "onebot/ws_reverse_port",
                format!("OneBot 口不能与 WebUI 口相同（{}）", self.dashboard_port),
            );
        }
        sink.extend(ai::validate_ai(&self.sources, &self.models));
        sink.into_vec()
    }
}

pub fn link_inputs_changed(before: &AstrBotInstanceConfig, after: &AstrBotInstanceConfig) -> bool {
    before.onebot.ws_reverse_port != after.onebot.ws_reverse_port
        || before.onebot.ws_reverse_token != after.onebot.ws_reverse_token
}

pub fn astrbot_config_documents() -> Vec<ncd_domain::AppConfigDocument> {
    cmd_config_documents()
}

pub async fn read_astrbot_config(
    host: &dyn Host,
    install_dir: &HostPath,
    instance_id: &str,
    listen_port: u16,
) -> Result<(AstrBotInstanceConfig, Vec<DocumentSnapshot>), AppFrameworkError> {
    let snaps = read_cmd_config_snapshots(host, install_dir).await?;
    let text = snaps.first().and_then(|s| s.text.as_deref()).unwrap_or("");
    let root = if text.is_empty() {
        platform::default_empty_root()
    } else {
        super::config_json::parse_cmd_config(text)?
    };
    Ok((
        AstrBotInstanceConfig::from_root(&root, instance_id, listen_port),
        snaps,
    ))
}

pub async fn write_astrbot_config(
    host: &dyn Host,
    instance: &AppInstance,
    cfg: &AstrBotInstanceConfig,
    _current: &[DocumentSnapshot],
) -> Result<(AstrBotInstanceConfig, Vec<DocumentSnapshot>), AppFrameworkError> {
    let issues = cfg.validate();
    if !issues.is_empty() {
        return Err(AppFrameworkError::ConfigInvalid(issues));
    }
    let install_dir = HostPath::from_posix(&instance.install_dir);
    let mut root = load_cmd_config(host, &install_dir).await?;
    let mut sink = IssueSink::default();
    collect_port_issues(
        &root,
        instance.id.as_str(),
        cfg.onebot.ws_reverse_port,
        &mut sink,
    );
    let extra = sink.into_vec();
    if !extra.is_empty() {
        return Err(AppFrameworkError::ConfigInvalid(extra));
    }

    apply_onebot_to_root(&mut root, instance.id.as_str(), cfg)?;

    let mut sources = cfg.sources.clone();
    let mut models = cfg.models.clone();
    ai::restore_extras(&root, &mut sources, &mut models);
    ai::apply_ai_patch(
        &mut root,
        &sources,
        &models,
        &cfg.ai,
        &cfg.stt,
        &cfg.tts,
        &cfg.websearch,
        &cfg.kb,
        &cfg.gates,
        &cfg.subagent,
    )
    .map_err(AppFrameworkError::Integration)?;

    save_cmd_config(
        host,
        &install_dir,
        &root,
        write_project_sidecar(instance),
    )
    .await?;
    read_astrbot_config(host, &install_dir, instance.id.as_str(), instance.port).await
}

pub fn apply_onebot_to_root(
    root: &mut Value,
    instance_id: &str,
    cfg: &AstrBotInstanceConfig,
) -> Result<Claim, AppFrameworkError> {
    let claim = claim_for_upsert(
        platforms(root),
        instance_id,
        cfg.onebot.ws_reverse_port,
    )?;
    if !cfg.claimed && matches!(claim, Claim::Index(_)) {
        return Err(AppFrameworkError::Validation(
            platform::AMBIGUOUS_AIOCQHTTP.into(),
        ));
    }
    let rows = platform::platforms_mut(root)?;
    match claim {
        Claim::Index(i) => {
            let row = &mut rows[i];
            apply_claimed_fields(
                row,
                instance_id,
                cfg.onebot.ws_reverse_port,
                &cfg.onebot.ws_reverse_token,
            );
            if let Some(obj) = row.as_object_mut() {
                obj.insert("enable".into(), Value::Bool(cfg.onebot.enable));
                obj.insert(
                    super::manifest::KEY_WS_REVERSE_HOST.into(),
                    Value::String(cfg.onebot.ws_reverse_host.clone()),
                );
            }
        }
        Claim::Append => {
            let mut row = seed_row(
                instance_id,
                cfg.onebot.ws_reverse_port,
                &cfg.onebot.ws_reverse_token,
            );
            if let Some(obj) = row.as_object_mut() {
                obj.insert("enable".into(), Value::Bool(cfg.onebot.enable));
                obj.insert(
                    super::manifest::KEY_WS_REVERSE_HOST.into(),
                    Value::String(cfg.onebot.ws_reverse_host.clone()),
                );
            }
            rows.push(row);
        }
    }
    Ok(claim)
}

pub fn claimed_platform_row<'a>(root: &'a Value, instance_id: &str, port: u16) -> Option<&'a Value> {
    let rows = platforms(root);
    let want_id = ncd_platform_id(instance_id);
    let idx = rows
        .iter()
        .position(|row| platform::row_id(row) == Some(want_id.as_str()) && platform::is_aiocqhttp(row))
        .or_else(|| match claim_for_upsert(rows, instance_id, port).ok()? {
            Claim::Index(i) => Some(i),
            Claim::Append => None,
        })?;
    Some(&rows[idx])
}

pub fn read_access_token_from_root(root: &Value, instance_id: &str, port: u16) -> Option<String> {
    let rows = platforms(root);
    let want_id = ncd_platform_id(instance_id);
    let idx = rows
        .iter()
        .position(|row| platform::row_id(row) == Some(want_id.as_str()) && platform::is_aiocqhttp(row))
        .or_else(|| {
            if port == 0 {
                return None;
            }
            match claim_for_upsert(rows, instance_id, port).ok()? {
                Claim::Index(i) => Some(i),
                Claim::Append => None,
            }
        })
        .or_else(|| claim_for_probe(rows))?;
    read_aiocqhttp_token(&rows[idx])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_root_reads_claimed_row_and_other_types() {
        let root = serde_json::json!({
            "dashboard": { "port": 6185 },
            "platform": [
                {
                    "id": "ncd-app:a1",
                    "type": "aiocqhttp",
                    "enable": true,
                    "ws_reverse_host": "0.0.0.0",
                    "ws_reverse_port": 6201,
                    "ws_reverse_token": "tok"
                },
                { "id": "lark", "type": "lark", "enable": true }
            ]
        });
        let cfg = AstrBotInstanceConfig::from_root(&root, "a1", 6201);
        assert!(cfg.claimed);
        assert_eq!(cfg.onebot.ws_reverse_port, 6201);
        assert_eq!(cfg.onebot.ws_reverse_token, "tok");
        assert_eq!(cfg.dashboard_port, 6185);
        assert_eq!(cfg.other_platforms, vec!["lark".to_string()]);
    }

    #[test]
    fn validate_rejects_port_clash_with_dashboard() {
        let cfg = AstrBotInstanceConfig {
            onebot: AstrBotOneBotRow {
                ws_reverse_port: 6185,
                ..AstrBotOneBotRow::default()
            },
            dashboard_port: 6185,
            ..AstrBotInstanceConfig::default()
        };
        assert!(!cfg.validate().is_empty());
    }

    #[test]
    fn from_root_two_aiocqhttp_stays_unclaimed_even_if_listen_is_default() {
        let root = serde_json::json!({
            "dashboard": { "port": 6185 },
            "platform": [
                { "id": "one", "type": "aiocqhttp", "enable": true, "ws_reverse_port": 6199 },
                { "id": "two", "type": "aiocqhttp", "enable": true, "ws_reverse_port": 6200 }
            ]
        });
        let cfg = AstrBotInstanceConfig::from_root(&root, "a1", 6199);
        assert!(!cfg.claimed);
        assert_eq!(cfg.onebot.ws_reverse_port, 6199);
        assert!(cfg.other_platforms.is_empty());
    }

    #[test]
    fn from_root_empty_is_unclaimed_suggestion() {
        let root = serde_json::json!({
            "dashboard": { "port": 6185 },
            "platform": []
        });
        let cfg = AstrBotInstanceConfig::from_root(&root, "a1", 0);
        assert!(!cfg.claimed);
        assert_eq!(cfg.onebot.id, "ncd-app:a1");
        assert_eq!(cfg.onebot.ws_reverse_port, 6199);
    }
}
