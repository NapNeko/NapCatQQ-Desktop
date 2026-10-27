use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use ncd_domain::SecretError;
use ncd_domain::assistant::{AssistantProvider, AssistantSessionState};
use ncd_traits::assistant::{
    AssistantContext, AssistantInteraction, AssistantToolConnection, AssistantToolHost,
};
use tokio_util::sync::CancellationToken;

use super::*;

#[derive(Default)]
struct Secrets {
    values: Mutex<HashMap<String, String>>,
    unavailable: AtomicBool,
}

impl SecretStore for Secrets {
    fn get(&self, key: &str) -> Result<Option<String>, SecretError> {
        if self.unavailable.load(Ordering::Acquire) {
            return Err(SecretError::Unavailable);
        }
        Ok(lock(&self.values).get(key).cloned())
    }
    fn put(&self, key: &str, value: &str) -> Result<(), SecretError> {
        if self.unavailable.load(Ordering::Acquire) {
            return Err(SecretError::Unavailable);
        }
        lock(&self.values).insert(key.to_owned(), value.to_owned());
        Ok(())
    }
    fn delete(&self, key: &str) -> Result<(), SecretError> {
        if self.unavailable.load(Ordering::Acquire) {
            return Err(SecretError::Unavailable);
        }
        lock(&self.values).remove(key);
        Ok(())
    }
}

struct Host;
#[async_trait]
impl AssistantToolHost for Host {
    async fn context(&self, _: &str) -> Result<AssistantContext, String> {
        Ok(AssistantContext {
            bot_name: "测试环境".into(),
            summary: "仅本地测试".into(),
        })
    }
    async fn open(
        &self,
        _: &str,
        _: &str,
        _: Arc<dyn AssistantInteraction>,
        _: CancellationToken,
    ) -> Result<AssistantToolConnection, String> {
        Ok(AssistantToolConnection {
            url: "http://127.0.0.1:1/mcp".into(),
            bearer_token: "fixture-mcp-token".into(),
        })
    }
    async fn close(&self, _: &str) {}
    fn redact(&self, text: &str) -> String {
        text.replace("fixture-mcp-token", "[REDACTED]")
    }
}

async fn fixture() -> (tempfile::TempDir, Arc<AssistantManager>, Arc<Secrets>) {
    let root = tempfile::tempdir().unwrap();
    let secrets = Arc::new(Secrets::default());
    let manager = Arc::new(
        AssistantManager::new(root.path().to_owned(), Arc::new(Host))
            .with_secret_store(secrets.clone()),
    );
    let mut settings = manager.settings().await.unwrap();
    settings.builtin.base_url = "https://provider-a.invalid".into();
    settings.builtin.model = "fixture-model".into();
    manager.save_settings(settings).await.unwrap();
    (root, manager, secrets)
}

#[test]
fn older_settings_gain_builtin_defaults_without_losing_explicit_external_choice() {
    let legacy: ncd_domain::assistant::AssistantSettings =
        serde_json::from_str(r#"{"defaultProvider":"dsh","providers":[]}"#).unwrap();
    let settings = super::super::normalized_settings(legacy);
    assert_eq!(settings.default_provider, Some(AssistantProvider::Dsh));
    assert_eq!(settings.builtin.max_steps, 6);
    let legacy: ncd_domain::assistant::AssistantSettings =
        serde_json::from_str(r#"{"defaultProvider":null,"providers":[]}"#).unwrap();
    assert_eq!(
        super::super::normalized_settings(legacy).default_provider,
        Some(AssistantProvider::Builtin)
    );
}

#[tokio::test]
async fn local_ollama_can_be_configured_without_credentials_or_a_secret_store() {
    let root = tempfile::tempdir().unwrap();
    let manager = Arc::new(AssistantManager::new(
        root.path().to_owned(),
        Arc::new(Host),
    ));
    let mut settings = manager.settings().await.unwrap();
    settings.builtin.provider_preset = AssistantModelPreset::Ollama;
    settings.builtin.base_url = "http://localhost:11434/v1".into();
    settings.builtin.model = "local-model".into();
    manager.save_settings(settings.clone()).await.unwrap();
    let status = manager.model_status().await.unwrap();
    assert!(status.configured);
    assert!(!status.has_api_key);
    assert!(status.error.is_none());
    assert_eq!(
        manager
            .credentials
            .read_for_model(&settings.builtin)
            .await
            .unwrap(),
        ""
    );
    let session = manager
        .create_session(AssistantProvider::Builtin, "test-bot".into())
        .await
        .unwrap();
    assert_eq!(session.info.state, AssistantSessionState::Ready);
    manager.shutdown().await;
}

#[test]
fn optional_ollama_key_is_limited_to_explicit_local_presets() {
    for base_url in [
        "http://localhost:11434/v1",
        "http://127.0.0.1:11434",
        "https://[::1]/v1",
    ] {
        let mut profile = AssistantBuiltinSettings {
            provider_preset: AssistantModelPreset::Ollama,
            base_url: base_url.into(),
            ..Default::default()
        };
        assert!(!requires_api_key(&profile));
        profile.provider_preset = AssistantModelPreset::Custom;
        assert!(requires_api_key(&profile));
    }
    let remote = AssistantBuiltinSettings {
        provider_preset: AssistantModelPreset::Ollama,
        base_url: "https://ollama.example/v1".into(),
        ..Default::default()
    };
    assert!(requires_api_key(&remote));
}

#[tokio::test]
async fn switching_api_format_retains_the_same_origins_credential() {
    let (_root, manager, secrets) = fixture().await;
    manager.set_api_key("same-origin-key".into()).await.unwrap();
    for format in [
        ncd_domain::assistant::AssistantApiFormat::OpenAiResponses,
        ncd_domain::assistant::AssistantApiFormat::AnthropicMessages,
        ncd_domain::assistant::AssistantApiFormat::GeminiGenerateContent,
    ] {
        let mut settings = manager.settings().await.unwrap();
        settings.builtin.api_format = format;
        manager.save_settings(settings).await.unwrap();
        assert!(manager.model_status().await.unwrap().configured);
        assert_eq!(lock(&secrets.values).len(), 1);
    }
}

#[tokio::test]
async fn stale_credential_form_cannot_write_or_clear_another_origins_key() {
    let (_root, manager, secrets) = fixture().await;
    manager
        .set_api_key("existing-origin-a-key".into())
        .await
        .unwrap();
    let session = manager
        .create_session(AssistantProvider::Builtin, "test-bot".into())
        .await
        .unwrap();
    let mut settings = manager.settings().await.unwrap();
    settings.builtin.base_url = "https://provider-b.invalid/v1".into();
    manager.save_settings(settings).await.unwrap();
    let error = manager
        .set_api_key_for_origin(
            "new-key-from-stale-form".into(),
            "https://provider-a.invalid".into(),
        )
        .await
        .unwrap_err();
    assert!(!error.contains("new-key-from-stale-form"));
    assert!(
        manager
            .clear_api_key_for_origin("https://provider-a.invalid".into())
            .await
            .is_err()
    );
    assert_eq!(
        lock(&secrets.values).values().cloned().collect::<Vec<_>>(),
        vec!["existing-origin-a-key"]
    );
    assert_eq!(
        manager
            .get_session(&session.info.id)
            .await
            .unwrap()
            .info
            .state,
        AssistantSessionState::Ready
    );
    manager
        .set_api_key_for_origin(
            "origin-b-key".into(),
            "https://provider-b.invalid:443".into(),
        )
        .await
        .unwrap();
    assert_eq!(lock(&secrets.values).len(), 2);
    manager.shutdown().await;
}

#[tokio::test]
async fn credential_is_bound_to_origin_and_never_serialized_in_settings_or_sessions() {
    let (root, manager, secrets) = fixture().await;
    let value = "fixture-private-model-key";
    assert!(!manager.model_status().await.unwrap().has_api_key);
    assert!(manager.set_api_key(value.into()).await.unwrap().configured);
    let session = manager
        .create_session(AssistantProvider::Builtin, "test-bot".into())
        .await
        .unwrap();
    assert_eq!(session.info.state, AssistantSessionState::Ready);
    let settings = manager.settings().await.unwrap();
    assert!(!serde_json::to_string(&settings).unwrap().contains(value));
    assert!(!serde_json::to_string(&session).unwrap().contains(value));
    let primary = std::fs::read_to_string(root.path().join("config/assistant.json")).unwrap();
    assert!(!primary.contains(value));
    assert_eq!(lock(&secrets.values).values().next().unwrap(), value);
    let mut changed = settings;
    changed.builtin.base_url = "https://provider-b.invalid".into();
    manager.save_settings(changed).await.unwrap();
    assert!(!manager.model_status().await.unwrap().has_api_key);
    manager.shutdown().await;
}

#[tokio::test]
async fn same_origin_normalizes_default_port_and_paths_but_clear_does_not_delete_another_origins_key()
 {
    let (_root, manager, secrets) = fixture().await;
    manager.set_api_key("origin-a-key".into()).await.unwrap();
    let mut settings = manager.settings().await.unwrap();
    settings.builtin.base_url = "https://PROVIDER-A.invalid:443/v1".into();
    manager.save_settings(settings.clone()).await.unwrap();
    assert!(manager.model_status().await.unwrap().has_api_key);
    settings.builtin.base_url = "https://provider-b.invalid/v1".into();
    manager.save_settings(settings.clone()).await.unwrap();
    manager.set_api_key("origin-b-key".into()).await.unwrap();
    manager.clear_api_key().await.unwrap();
    assert_eq!(lock(&secrets.values).len(), 1);
    assert_eq!(
        lock(&secrets.values).values().next().unwrap(),
        "origin-a-key"
    );
}

#[tokio::test]
async fn unavailable_secret_store_has_no_plaintext_fallback_and_does_not_replace_existing_key() {
    let (root, manager, secrets) = fixture().await;
    manager
        .set_api_key("original-model-key".into())
        .await
        .unwrap();
    secrets.unavailable.store(true, Ordering::Release);
    let result = manager
        .set_api_key("replacement-private-value".into())
        .await
        .unwrap_err();
    assert!(!result.contains("replacement-private-value"));
    assert!(manager.model_status().await.unwrap().error.is_some());
    assert_eq!(
        lock(&secrets.values).values().next().unwrap(),
        "original-model-key"
    );
    let settings = std::fs::read_to_string(root.path().join("config/assistant.json")).unwrap();
    assert!(
        !settings.contains("replacement-private-value") && !settings.contains("original-model-key")
    );
}

#[test]
fn profile_validation_is_local_and_rejects_credential_urls_and_unbounded_budgets() {
    for base_url in [
        "https://user:password@example.invalid/v1",
        "https://example.invalid/v1?api_key=fixture",
        "http://example.invalid/v1",
    ] {
        let profile = AssistantBuiltinSettings {
            base_url: base_url.into(),
            ..Default::default()
        };
        assert!(validate_profile(&profile, false).is_err());
    }
    for key in ["", "has space", "line\nbreak", "tab\tkey"] {
        assert!(validate_api_key(key).is_err());
    }
    let profile = AssistantBuiltinSettings {
        max_steps: 100,
        ..Default::default()
    };
    assert!(validate_profile(&profile, false).is_err());
    assert!(validate_profile(&AssistantBuiltinSettings::default(), false).is_ok());
}

#[tokio::test]
async fn failed_key_write_restores_admission_after_stopping_builtin_session() {
    let (_root, manager, secrets) = fixture().await;
    manager
        .set_api_key("original-model-key".into())
        .await
        .unwrap();
    let session = manager
        .create_session(AssistantProvider::Builtin, "test-bot".into())
        .await
        .unwrap();
    secrets.unavailable.store(true, Ordering::Release);
    assert!(
        manager
            .set_api_key("replacement-model-key".into())
            .await
            .is_err()
    );
    assert!(!manager.updating_credentials.load(Ordering::Acquire));
    assert_eq!(
        manager
            .get_session(&session.info.id)
            .await
            .unwrap()
            .info
            .state,
        AssistantSessionState::Disconnected
    );
    assert_eq!(
        lock(&secrets.values).values().next().unwrap(),
        "original-model-key"
    );
    secrets.unavailable.store(false, Ordering::Release);
    assert!(
        manager
            .set_api_key("replacement-model-key".into())
            .await
            .unwrap()
            .configured
    );
    assert!(!manager.updating_credentials.load(Ordering::Acquire));
}

#[tokio::test]
async fn failed_key_delete_restores_admission_and_preserves_original_credential() {
    let (_root, manager, secrets) = fixture().await;
    manager
        .set_api_key("original-model-key".into())
        .await
        .unwrap();
    let session = manager
        .create_session(AssistantProvider::Builtin, "test-bot".into())
        .await
        .unwrap();
    secrets.unavailable.store(true, Ordering::Release);
    assert!(manager.clear_api_key().await.is_err());
    assert!(!manager.updating_credentials.load(Ordering::Acquire));
    assert_eq!(
        manager
            .get_session(&session.info.id)
            .await
            .unwrap()
            .info
            .state,
        AssistantSessionState::Disconnected
    );
    assert_eq!(
        lock(&secrets.values).values().next().unwrap(),
        "original-model-key"
    );
    secrets.unavailable.store(false, Ordering::Release);
    manager.clear_api_key().await.unwrap();
    assert!(lock(&secrets.values).is_empty());
    assert!(!manager.updating_credentials.load(Ordering::Acquire));
}

#[test]
fn credential_update_guard_resets_flag_on_early_return() {
    let updating = AtomicBool::new(false);
    let fail = || -> Result<(), ()> {
        let _guard = CredentialUpdate::begin(&updating);
        assert!(updating.load(Ordering::Acquire));
        Err(())
    };
    assert!(fail().is_err());
    assert!(!updating.load(Ordering::Acquire));
}
