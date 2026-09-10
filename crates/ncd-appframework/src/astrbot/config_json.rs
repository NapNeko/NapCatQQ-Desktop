//! 保序读写 `data/cmd_config.json`；只改调用方指定的指针。

use ncd_domain::{AppConfigDocument, AppConfigFormat};
use ncd_host::{Host, HostPath};
use ncd_traits::AppFrameworkError;
use serde_json::Value;

use super::manifest::ASTRBOT_CMD_CONFIG;
use crate::adapter::apply_with_backup_ex;
use crate::config_doc::{DocumentSnapshot, read_documents, render_json_pretty, revision_of};

pub const DOC_CMD_CONFIG: &str = "cmd_config";

pub fn cmd_config_document() -> AppConfigDocument {
    AppConfigDocument {
        id: DOC_CMD_CONFIG.to_string(),
        label: ASTRBOT_CMD_CONFIG.to_string(),
        rel_path: ASTRBOT_CMD_CONFIG.to_string(),
        format: AppConfigFormat::Json,
        hot_reload: false,
    }
}

pub fn cmd_config_documents() -> Vec<AppConfigDocument> {
    vec![cmd_config_document()]
}

pub fn cmd_config_path(install_dir: &HostPath) -> HostPath {
    install_dir.join(ASTRBOT_CMD_CONFIG)
}

pub fn parse_cmd_config(text: &str) -> Result<Value, AppFrameworkError> {
    let trimmed = text.trim_start_matches('\u{feff}').trim();
    if trimmed.is_empty() {
        return Ok(super::platform::default_empty_root());
    }
    serde_json::from_str(trimmed)
        .map_err(|e| AppFrameworkError::Integration(format!("cmd_config.json 无法解析: {e}")))
}

pub async fn load_cmd_config(
    host: &dyn Host,
    install_dir: &HostPath,
) -> Result<Value, AppFrameworkError> {
    let path = cmd_config_path(install_dir);
    if !host
        .exists(&path)
        .await
        .map_err(|e| AppFrameworkError::Host(e.to_string()))?
    {
        return Ok(super::platform::default_empty_root());
    }
    let bytes = host
        .read_file(&path)
        .await
        .map_err(|e| AppFrameworkError::Host(e.to_string()))?;
    parse_cmd_config(&String::from_utf8_lossy(&bytes))
}

pub async fn save_cmd_config(
    host: &dyn Host,
    install_dir: &HostPath,
    root: &Value,
    write_sidecar: bool,
) -> Result<String, AppFrameworkError> {
    let path = cmd_config_path(install_dir);
    if let Some(parent) = path.parent()
        && !host
            .exists(&parent)
            .await
            .map_err(|e| AppFrameworkError::Host(e.to_string()))?
    {
        host.create_dir_all(&parent)
            .await
            .map_err(|e| AppFrameworkError::Host(e.to_string()))?;
    }
    let text = render_json_pretty(root)?;
    let bytes = text.as_bytes().to_vec();
    apply_with_backup_ex(host, std::slice::from_ref(&path), write_sidecar, || async {
        host.write_file(&path, &bytes)
            .await
            .map_err(|e| AppFrameworkError::Integration(e.to_string()))
    })
    .await?;
    Ok(revision_of(&bytes))
}

pub async fn read_cmd_config_snapshots(
    host: &dyn Host,
    install_dir: &HostPath,
) -> Result<Vec<DocumentSnapshot>, AppFrameworkError> {
    read_documents(host, install_dir, &cmd_config_documents()).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_strips_bom_and_keeps_unknown_keys() {
        let v = parse_cmd_config("\u{feff}{\"platform\":[],\"keep\":true}\n").unwrap();
        assert_eq!(v["keep"], true);
        assert!(v["platform"].as_array().unwrap().is_empty());
    }

    #[test]
    fn render_keeps_unknown_key_insertion_order() {
        let mut root = parse_cmd_config("{\"zzz\":true,\"platform\":[],\"aaa\":1}").unwrap();
        super::super::platform::upsert_claimed_row(&mut root, "a1", 6199, "t").unwrap();
        let out = crate::config_doc::render_json_pretty(&root).unwrap();
        let zzz = out.find("\"zzz\"").expect("zzz");
        let aaa = out.find("\"aaa\"").expect("aaa");
        assert!(zzz < aaa, "{out}");
        assert!(out.contains("ncd-app:a1"));
    }
}
