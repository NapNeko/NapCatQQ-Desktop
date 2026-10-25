//! 内置模型配置校验与按接口 origin 隔离的系统凭据。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use ncd_domain::assistant::{AssistantBuiltinSettings, AssistantModelStatus, AssistantProvider};
use ncd_traits::SecretStore;
use sha2::{Digest, Sha256};

use super::AssistantManager;
use super::session::lock;

#[derive(Default)]
pub(super) struct ModelCredentials {
    store: Mutex<Option<Arc<dyn SecretStore>>>,
}

impl ModelCredentials {
    pub fn install(&self, store: Arc<dyn SecretStore>) {
        *lock(&self.store) = Some(store);
    }

    fn store(&self) -> Result<Arc<dyn SecretStore>, String> {
        lock(&self.store)
            .clone()
            .ok_or_else(|| "系统凭据存储未接入，无法使用内置模型".into())
    }

    pub async fn read(&self, profile: &AssistantBuiltinSettings) -> Result<Option<String>, String> {
        let key = credential_key(profile)?;
        let store = self.store()?;
        let value = tokio::task::spawn_blocking(move || store.get(&key))
            .await
            .map_err(|_| "读取模型凭据失败".to_owned())?
            .map_err(|_| "系统凭据存储不可用，无法读取模型 API Key".to_owned())?;
        if let Some(value) = &value {
            validate_api_key(value)?;
        }
        Ok(value)
    }
}

impl AssistantManager {
    pub fn with_secret_store(self, store: Arc<dyn SecretStore>) -> Self {
        self.credentials.install(store);
        self
    }

    pub async fn model_status(&self) -> Result<AssistantModelStatus, String> {
        let profile = self.settings().await?.builtin;
        if profile.base_url.trim().is_empty() {
            return Ok(status(&profile, false, None));
        }
        Ok(match self.credentials.read(&profile).await {
            Ok(key) => status(&profile, key.is_some(), None),
            Err(error) => status(&profile, false, Some(error)),
        })
    }

    pub async fn set_api_key(&self, api_key: String) -> Result<AssistantModelStatus, String> {
        self.ensure_loaded().await?;
        validate_api_key(&api_key)?;
        let _save = self.settings_gate.lock().await;
        let _admission = self.admission.lock().await;
        let _updating = CredentialUpdate::begin(&self.updating_credentials);
        let profile = lock(&self.settings).builtin.clone();
        let key = credential_key(&profile)?;
        let store = self.credentials.store()?;
        self.stop_builtin_for_credentials().await?;
        tokio::task::spawn_blocking(move || store.put(&key, &api_key))
            .await
            .map_err(|_| "保存模型凭据失败".to_owned())?
            .map_err(|_| "系统凭据存储不可用，API Key 未保存".to_owned())?;
        Ok(status(&profile, true, None))
    }

    pub async fn clear_api_key(&self) -> Result<AssistantModelStatus, String> {
        self.ensure_loaded().await?;
        let _save = self.settings_gate.lock().await;
        let _admission = self.admission.lock().await;
        let _updating = CredentialUpdate::begin(&self.updating_credentials);
        let profile = lock(&self.settings).builtin.clone();
        let key = credential_key(&profile)?;
        let store = self.credentials.store()?;
        self.stop_builtin_for_credentials().await?;
        tokio::task::spawn_blocking(move || store.delete(&key))
            .await
            .map_err(|_| "清除模型凭据失败".to_owned())?
            .map_err(|_| "系统凭据存储不可用，API Key 未清除".to_owned())?;
        Ok(status(&profile, false, None))
    }

    async fn stop_builtin_for_credentials(&self) -> Result<(), String> {
        let sessions = lock(&self.sessions).values().cloned().collect::<Vec<_>>();
        for session in sessions {
            if session.snapshot().info.provider == AssistantProvider::Builtin {
                // 等待本轮结算和 checkpoint，系统凭据变更成功后不会再提交旧 Key 请求。
                self.stop_session(&session, false, true).await?;
            }
        }
        Ok(())
    }
}

struct CredentialUpdate<'a>(&'a AtomicBool);

impl<'a> CredentialUpdate<'a> {
    fn begin(updating: &'a AtomicBool) -> Self {
        updating.store(true, Ordering::Release);
        Self(updating)
    }
}

impl Drop for CredentialUpdate<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

fn status(
    profile: &AssistantBuiltinSettings,
    has_api_key: bool,
    error: Option<String>,
) -> AssistantModelStatus {
    let error = error
        .or_else(|| validate_profile(profile, true).err())
        .or_else(|| (!has_api_key).then(|| "请先保存模型 API Key".into()));
    AssistantModelStatus {
        configured: error.is_none() && has_api_key,
        has_api_key,
        error,
    }
}

fn credential_key(profile: &AssistantBuiltinSettings) -> Result<String, String> {
    let endpoint =
        super::builtin::endpoint_url(&profile.base_url).map_err(|error| error.to_string())?;
    let origin = endpoint.origin().ascii_serialization();
    Ok(format!(
        "assistant.model.{}",
        hex::encode(Sha256::digest(origin.as_bytes()))
    ))
}

pub(super) fn validate_profile(
    profile: &AssistantBuiltinSettings,
    required: bool,
) -> Result<(), String> {
    if !(1..=12).contains(&profile.max_steps)
        || !(128..=8192).contains(&profile.max_output_tokens)
        || !(10..=180).contains(&profile.request_timeout_secs)
    {
        return Err("内置模型处理上限无效：步骤 1–12，输出 128–8192，超时 10–180 秒".into());
    }
    if !profile.base_url.is_empty() {
        super::builtin::endpoint_url(&profile.base_url).map_err(|error| error.to_string())?;
    } else if required {
        return Err("请先配置模型接口地址".into());
    }
    if profile.model.len() > 256
        || profile.model.chars().any(char::is_control)
        || profile.model.trim() != profile.model
        || profile.model.to_ascii_lowercase().starts_with("sk-")
    {
        return Err("模型名称无效，请填写模型标识而非凭据".into());
    }
    if required && profile.model.is_empty() {
        return Err("请先填写模型名称".into());
    }
    Ok(())
}

fn validate_api_key(key: &str) -> Result<(), String> {
    if key.is_empty() || key.len() > 4096 || !key.bytes().all(|byte| byte.is_ascii_graphic()) {
        return Err("API Key 不能为空，也不能包含空白、控制字符或过长内容".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests;
