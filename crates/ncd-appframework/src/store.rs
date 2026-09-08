//! 应用端商店条目：框架无关的市场 / 已装视图。
//!
//! Karin 的 npm/git/app 细节仍在 `KarinPlugin*` 里；这里做一层映射，好让 NoneBot
//! 适配器 / 插件和 Karin 插件走同一套 Adapter / 任务方法。

use ncd_domain::AppStoreResource;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::karin::plugin::{
    KarinPluginAppFile, KarinPluginAuthor, KarinPluginInstalled, KarinPluginKind,
    KarinPluginMarketEntry, KarinPluginRepo,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub enum AppStoreFlavor {
    Npm,
    Git,
    App,
    Pypi,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AppStoreMarketEntry {
    pub resource: AppStoreResource,
    /// 装更卸 / 启停用的稳定键：Karin 包名，NoneBot `module_name`
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub homepage: String,
    #[serde(default)]
    pub time: String,
    /// PyPI `project_link` 或 npm 包名
    #[serde(default)]
    pub package: String,
    #[serde(default)]
    pub module_name: String,
    pub flavor: AppStoreFlavor,
    #[serde(default)]
    pub is_official: bool,
    #[serde(default)]
    pub valid: bool,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub supported_adapters: Vec<String>,
    #[serde(default)]
    pub authors: Vec<KarinPluginAuthor>,
    #[serde(default)]
    pub repos: Vec<KarinPluginRepo>,
    #[serde(default)]
    pub files: Vec<KarinPluginAppFile>,
    #[serde(default)]
    pub allow_build: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AppStoreInstalled {
    pub id: String,
    pub name: String,
    pub resource: AppStoreResource,
    pub flavor: AppStoreFlavor,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub version: Option<String>,
    pub enabled: bool,
    #[serde(default)]
    pub package: String,
}

impl AppStoreFlavor {
    pub fn from_karin(kind: KarinPluginKind) -> Self {
        match kind {
            KarinPluginKind::Npm => Self::Npm,
            KarinPluginKind::Git => Self::Git,
            KarinPluginKind::App => Self::App,
        }
    }

    pub fn to_karin(self) -> Option<KarinPluginKind> {
        match self {
            Self::Npm => Some(KarinPluginKind::Npm),
            Self::Git => Some(KarinPluginKind::Git),
            Self::App => Some(KarinPluginKind::App),
            Self::Pypi => None,
        }
    }
}

impl AppStoreMarketEntry {
    pub fn from_karin(entry: KarinPluginMarketEntry) -> Self {
        let author = entry
            .authors
            .first()
            .map(|a| a.name.clone())
            .unwrap_or_default();
        Self {
            resource: AppStoreResource::Plugin,
            id: entry.name.clone(),
            name: entry.name.clone(),
            description: entry.description,
            version: String::new(),
            author,
            homepage: entry.home,
            time: entry.time,
            package: entry.name.clone(),
            module_name: entry.name.clone(),
            flavor: AppStoreFlavor::from_karin(entry.kind),
            is_official: false,
            valid: true,
            tags: Vec::new(),
            supported_adapters: Vec::new(),
            authors: entry.authors,
            repos: entry.repos,
            files: entry.files,
            allow_build: entry.allow_build,
        }
    }

    pub fn to_karin(&self) -> Option<KarinPluginMarketEntry> {
        let kind = self.flavor.to_karin()?;
        Some(KarinPluginMarketEntry {
            name: self.id.clone(),
            kind,
            description: self.description.clone(),
            time: self.time.clone(),
            home: self.homepage.clone(),
            authors: self.authors.clone(),
            repos: self.repos.clone(),
            files: self.files.clone(),
            allow_build: self.allow_build.clone(),
        })
    }
}

impl AppStoreInstalled {
    pub fn from_karin(entry: KarinPluginInstalled) -> Self {
        Self {
            id: entry.name.clone(),
            name: entry.name.clone(),
            resource: AppStoreResource::Plugin,
            flavor: AppStoreFlavor::from_karin(entry.kind),
            version: entry.version,
            enabled: entry.enabled,
            package: entry.name,
        }
    }

    pub fn to_karin(&self) -> Option<KarinPluginInstalled> {
        Some(KarinPluginInstalled {
            name: self.id.clone(),
            kind: self.flavor.to_karin()?,
            version: self.version.clone(),
            enabled: self.enabled,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn karin_round_trip_keeps_install_flavor() {
        let raw = KarinPluginMarketEntry {
            name: "@karinjs/plugin-basic".into(),
            kind: KarinPluginKind::Npm,
            description: "d".into(),
            time: "2026-01-01 00:00:00".into(),
            home: "https://example".into(),
            authors: vec![KarinPluginAuthor {
                name: "k".into(),
                home: String::new(),
            }],
            repos: Vec::new(),
            files: Vec::new(),
            allow_build: vec!["win32".into()],
        };
        let store = AppStoreMarketEntry::from_karin(raw.clone());
        assert_eq!(store.id, "@karinjs/plugin-basic");
        assert_eq!(store.flavor, AppStoreFlavor::Npm);
        assert_eq!(store.to_karin().unwrap().allow_build, raw.allow_build);
    }
}
