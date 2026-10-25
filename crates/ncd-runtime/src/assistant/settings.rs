//! 模型和外部 Agent 配置保持兼容，读取状态不触发模型请求。

use ncd_domain::assistant::*;

use super::session::lock;
use super::{AssistantManager, providers};

impl AssistantManager {
    pub async fn settings(&self) -> Result<AssistantSettings, String> {
        self.ensure_loaded().await?;
        Ok(lock(&self.settings).clone())
    }

    pub async fn save_settings(&self, settings: AssistantSettings) -> Result<(), String> {
        self.ensure_loaded().await?;
        providers::validate_settings(&settings)?;
        let settings = normalized_settings(settings);
        let _save = self.settings_gate.lock().await;
        let storage = self.storage.clone();
        let saved = settings.clone();
        tokio::task::spawn_blocking(move || storage.save_settings(&saved))
            .await
            .map_err(|_| "保存助手设置失败")??;
        *lock(&self.settings) = settings;
        Ok(())
    }

    pub async fn providers(&self) -> Result<Vec<AssistantProviderStatus>, String> {
        let settings = self.settings().await?;
        let status = self.model_status().await?;
        let mut providers = vec![AssistantProviderStatus {
            provider: AssistantProvider::Builtin,
            label: "内置 Agent".into(),
            enabled: true,
            installed: true,
            executable_path: None,
            version: None,
            error: status.error,
            documentation_url: String::new(),
        }];
        providers.extend(providers::discover(&settings).await);
        Ok(providers)
    }
}

pub(super) fn normalized_settings(settings: AssistantSettings) -> AssistantSettings {
    let providers = AssistantSettings::default()
        .providers
        .into_iter()
        .map(|fallback| {
            settings
                .providers
                .iter()
                .find(|config| config.provider == fallback.provider)
                .cloned()
                .unwrap_or(fallback)
        })
        .collect();
    AssistantSettings {
        default_provider: settings
            .default_provider
            .or(Some(AssistantProvider::Builtin)),
        builtin: settings.builtin,
        providers,
    }
}
