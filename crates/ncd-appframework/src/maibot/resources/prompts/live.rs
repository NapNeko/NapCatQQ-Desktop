//! 跑着的时候走 WebUI `/api/webui/config/prompts/…`。上游每个回包都带版本列表和在用版本，不用再单独查。

use std::collections::BTreeMap;

use ncd_traits::AppFrameworkError;
use reqwest::Method;
use serde::Deserialize;
use serde_json::json;

use super::{
    ACTIVE_LANGUAGE, MaiBotPromptAction, MaiBotPromptCatalog, MaiBotPromptFile, MaiBotPromptInfo,
    MaiBotPromptLanguage, MaiBotPromptVersion, check_prompt,
};
use crate::maibot::api::MaiBotSession;
use crate::maibot::webui_client::{MaiBotWebUi, Request};

const BASE: &str = "/api/webui/config/prompts";

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct UpstreamCatalog {
    languages: Option<Vec<String>>,
    files: Option<BTreeMap<String, Vec<UpstreamInfo>>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct UpstreamInfo {
    name: Option<String>,
    display_name: Option<String>,
    description: Option<String>,
    advanced: Option<bool>,
    customized: Option<bool>,
    custom_version_count: Option<u32>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct UpstreamFile {
    content: Option<String>,
    customized: Option<bool>,
    active_version_id: Option<String>,
    versions: Option<Vec<UpstreamVersion>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct UpstreamVersion {
    id: Option<String>,
    label: Option<String>,
    created_at: Option<f64>,
    modified_at: Option<f64>,
    active: Option<bool>,
}

fn connect(s: &MaiBotSession) -> Result<MaiBotWebUi, AppFrameworkError> {
    MaiBotWebUi::connect(s.port, &s.token)
}

pub(super) async fn catalog(s: &MaiBotSession) -> Result<MaiBotPromptCatalog, AppFrameworkError> {
    let up: UpstreamCatalog = connect(s)?.get(BASE, &[]).await?;
    let mut files = up.files.unwrap_or_default();
    let languages = up
        .languages
        .unwrap_or_default()
        .into_iter()
        .map(|language| {
            let prompts = files
                .remove(&language)
                .unwrap_or_default()
                .into_iter()
                .filter_map(|p| {
                    Some(MaiBotPromptInfo {
                        name: p.name.filter(|n| !n.is_empty())?,
                        display_name: p.display_name.unwrap_or_default(),
                        description: p.description.unwrap_or_default(),
                        advanced: p.advanced.unwrap_or(false),
                        customized: p.customized.unwrap_or(false),
                        version_count: p.custom_version_count.unwrap_or(0),
                    })
                })
                .collect();
            MaiBotPromptLanguage { language, prompts }
        })
        .collect();
    Ok(MaiBotPromptCatalog {
        languages,
        active_language: ACTIVE_LANGUAGE.into(),
        live: true,
    })
}

async fn default_content(c: &MaiBotWebUi, language: &str, name: &str) -> Result<String, AppFrameworkError> {
    let path = format!("{BASE}/{language}/{name}/default");
    let up: UpstreamFile = c.get(&path, &[]).await?;
    Ok(up.content.unwrap_or_default())
}

pub(super) async fn file(s: &MaiBotSession, language: &str, name: &str) -> Result<MaiBotPromptFile, AppFrameworkError> {
    let c = connect(s)?;
    let path = format!("{BASE}/{language}/{name}");
    let up: UpstreamFile = c.get(&path, &[]).await?;
    let default = default_content(&c, language, name).await?;
    Ok(to_file(language, name, up, default))
}

pub(super) async fn version(
    s: &MaiBotSession,
    language: &str,
    name: &str,
    version_id: &str,
) -> Result<String, AppFrameworkError> {
    let path = format!("{BASE}/{language}/{name}/versions/{version_id}");
    let up: UpstreamFile = connect(s)?.get(&path, &[]).await?;
    Ok(up.content.unwrap_or_default())
}

pub(super) async fn act(s: &MaiBotSession, action: &MaiBotPromptAction) -> Result<MaiBotPromptFile, AppFrameworkError> {
    let c = connect(s)?;
    let (language, name) = action.target();
    let default = default_content(&c, language, name).await?;
    let file_path = format!("{BASE}/{language}/{name}");
    let up: UpstreamFile = match action {
        MaiBotPromptAction::Save { content, label, version_id, .. } => {
            check_prompt(content, &default).map_err(AppFrameworkError::Validation)?;
            // 上游：version_id 空就新建；给了就覆盖那个版本并设成在用
            let body = json!({
                "content": content,
                "version_id": version_id,
                "label": label,
                "create_version": version_id.is_none(),
            });
            c.call(Request::new(Method::PUT, &file_path).body(&body)).await?
        }
        MaiBotPromptAction::Activate { version_id, .. } => {
            let path = format!("{file_path}/versions/{version_id}/activate");
            c.call(Request::new(Method::POST, &path)).await?
        }
        MaiBotPromptAction::DeleteVersion { version_id, .. } => {
            let path = format!("{file_path}/versions/{version_id}");
            c.call(Request::new(Method::DELETE, &path)).await?
        }
        MaiBotPromptAction::Restore { .. } => c.call(Request::new(Method::DELETE, &file_path)).await?,
    };
    Ok(to_file(language, name, up, default))
}

fn to_file(language: &str, name: &str, up: UpstreamFile, default_content: String) -> MaiBotPromptFile {
    let versions = up
        .versions
        .unwrap_or_default()
        .into_iter()
        .filter_map(|v| {
            let id = v.id.filter(|i| !i.is_empty())?;
            Some(MaiBotPromptVersion {
                label: v.label.filter(|l| !l.is_empty()).unwrap_or_else(|| id.clone()),
                id,
                created_at: v.created_at.unwrap_or(0.0),
                modified_at: v.modified_at.unwrap_or(0.0),
                active: v.active.unwrap_or(false),
            })
        })
        .collect();
    MaiBotPromptFile {
        language: language.into(),
        name: name.into(),
        content: up.content.unwrap_or_default(),
        default_content,
        customized: up.customized.unwrap_or(false),
        active_version_id: up.active_version_id.filter(|v| !v.is_empty()),
        versions,
    }
}
