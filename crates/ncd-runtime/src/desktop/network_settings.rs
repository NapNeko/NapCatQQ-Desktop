//! 网络客户端与设置、凭据一起提交，失败时保留此前的有效配置。

use std::path::Path;

use ncd_domain::{AppSettings, AppSettingsDto};
use ncd_traits::SecretStore;
use tokio::sync::RwLock;

use super::network::{
    NetworkManager, read_proxy_password, restore_proxy_password, save_proxy_password,
};
use super::update_app_settings;
use crate::SecretStoreImpl;

pub const GITHUB_PAT_SECRET_KEY: &str = "app:github_pat";

pub async fn save_desktop_settings(
    data_root: &Path,
    current: &RwLock<AppSettings>,
    network: &NetworkManager,
    dto: AppSettingsDto,
) -> Result<(AppSettings, bool), String> {
    let secrets = SecretStoreImpl::new(data_root.join("secrets"));
    save_desktop_settings_with(data_root, current, network, dto, &secrets, || {
        CredentialValues::read(&SecretStoreImpl::new(data_root.join("secrets")))
    })
    .await
}

async fn save_desktop_settings_with(
    data_root: &Path,
    current: &RwLock<AppSettings>,
    network: &NetworkManager,
    mut dto: AppSettingsDto,
    secrets: &dyn SecretStore,
    read_fresh: impl Fn() -> Result<CredentialValues, String>,
) -> Result<(AppSettings, bool), String> {
    dto.settings.normalize();
    let prepared = NetworkManager::prepare(&dto.settings.network, &dto.proxy_password)?;
    let previous = CredentialValues::read(secrets)?;
    let wanted = CredentialValues {
        password: (!dto.proxy_password.is_empty()).then_some(dto.proxy_password),
        pat: (!dto.github_pat.trim().is_empty()).then(|| dto.github_pat.trim().to_string()),
    };
    let credentials_changed = previous != wanted;
    let mut transaction = SecretTransaction {
        secrets,
        read_fresh,
        previous,
        password_attempted: false,
        pat_attempted: false,
        completed: false,
    };
    if let Err(error) = transaction.save(&wanted) {
        return Err(transaction.rollback(error));
    }
    let (saved, settings_changed) = update_app_settings(data_root, current, |settings| {
        let package = settings.snowluma_package;
        let changed = settings.network != dto.settings.network
            || settings.github_api_mode != dto.settings.github_api_mode;
        *settings = dto.settings;
        settings.snowluma_package = package;
        changed
    })
    .await
    .map_err(|error| transaction.rollback(error))?;
    network.apply(prepared);
    transaction.completed = true;
    Ok((saved, settings_changed || credentials_changed))
}

#[derive(Clone, PartialEq, Eq)]
struct CredentialValues {
    password: Option<String>,
    pat: Option<String>,
}

impl CredentialValues {
    fn read(secrets: &dyn SecretStore) -> Result<Self, String> {
        Ok(Self {
            password: read_proxy_password(secrets)?,
            pat: secrets
                .get(GITHUB_PAT_SECRET_KEY)
                .map_err(|_| "读取 GitHub PAT 失败".to_string())?,
        })
    }
}

struct SecretTransaction<'a, F: Fn() -> Result<CredentialValues, String>> {
    secrets: &'a dyn SecretStore,
    read_fresh: F,
    previous: CredentialValues,
    password_attempted: bool,
    pat_attempted: bool,
    completed: bool,
}

impl<F: Fn() -> Result<CredentialValues, String>> SecretTransaction<'_, F> {
    fn save(&mut self, wanted: &CredentialValues) -> Result<(), String> {
        if self.previous.password != wanted.password {
            // SecretStore 写入返回错误时也可能已改变缓存，回滚要覆盖尝试过的项。
            self.password_attempted = true;
            save_proxy_password(self.secrets, wanted.password.as_deref().unwrap_or_default())?;
        }
        if self.previous.pat != wanted.pat {
            self.pat_attempted = true;
            write_pat(self.secrets, wanted.pat.as_deref())?;
        }
        if (self.password_attempted || self.pat_attempted) && (self.read_fresh)()? != *wanted {
            return Err("凭据保存校验失败".to_string());
        }
        Ok(())
    }

    fn rollback(&mut self, error: String) -> String {
        self.completed = true;
        if !self.password_attempted && !self.pat_attempted {
            return error;
        }
        let mut failed = false;
        if self.password_attempted {
            failed |=
                restore_proxy_password(self.secrets, self.previous.password.as_deref()).is_err();
        }
        if self.pat_attempted {
            failed |= write_pat(self.secrets, self.previous.pat.as_deref()).is_err();
        }
        // 必须重开存储检查持久化值，原实例缓存不能证明恢复成功。
        failed |= match (self.read_fresh)() {
            Ok(restored) => restored != self.previous,
            Err(_) => true,
        };
        if failed {
            format!("{error}；恢复原凭据失败，请重新保存凭据")
        } else {
            error
        }
    }
}

impl<F: Fn() -> Result<CredentialValues, String>> Drop for SecretTransaction<'_, F> {
    fn drop(&mut self) {
        if !self.completed && (self.password_attempted || self.pat_attempted) {
            // 等待设置写锁时调用可能被取消，不能遗留只有凭据保存成功的半次提交。
            let error = self.rollback("保存设置已取消".to_string());
            tracing::warn!(error = %error, "网络设置保存未完成");
        }
    }
}

fn write_pat(secrets: &dyn SecretStore, value: Option<&str>) -> Result<(), String> {
    match value {
        Some(value) => secrets
            .put(GITHUB_PAT_SECRET_KEY, value)
            .map_err(|_| "保存 GitHub PAT 失败".into()),
        None => match secrets.delete(GITHUB_PAT_SECRET_KEY) {
            Ok(()) | Err(ncd_domain::errors::SecretError::NotFound) => Ok(()),
            Err(_) => Err("清除 GitHub PAT 失败".into()),
        },
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use ncd_domain::{DownloadSourceMode, NetworkSettings, ProxyMode, SecretError};
    use ncd_network::client::NetworkProvider;

    use super::super::network::PROXY_PASSWORD_SECRET_KEY;
    use super::super::read_app_settings_file;
    use super::*;

    #[derive(Clone, Copy)]
    enum WriteFault {
        FailAfterCaching,
        LosePersistence,
    }

    struct StoreState {
        cached: CredentialValues,
        persisted: CredentialValues,
        writes: usize,
        faults: Vec<(usize, WriteFault)>,
    }

    struct MemorySecrets(Mutex<StoreState>);

    impl MemorySecrets {
        fn new(password: Option<&str>, pat: Option<&str>) -> Self {
            let values = CredentialValues {
                password: password.map(str::to_string),
                pat: pat.map(str::to_string),
            };
            Self(Mutex::new(StoreState {
                cached: values.clone(),
                persisted: values,
                writes: 0,
                faults: Vec::new(),
            }))
        }

        fn fault_at(&self, write: usize, fault: WriteFault) {
            self.0.lock().unwrap().faults.push((write, fault));
        }

        fn read_fresh(&self) -> Result<CredentialValues, String> {
            Ok(self.0.lock().unwrap().persisted.clone())
        }

        fn writes(&self) -> usize {
            self.0.lock().unwrap().writes
        }

        fn write(&self, key: &str, value: Option<String>) -> Result<(), SecretError> {
            let mut state = self.0.lock().unwrap();
            state.writes += 1;
            let slot = match key {
                PROXY_PASSWORD_SECRET_KEY => &mut state.cached.password,
                GITHUB_PAT_SECRET_KEY => &mut state.cached.pat,
                _ => panic!("unexpected secret key"),
            };
            *slot = value.clone();
            match state
                .faults
                .iter()
                .find(|(write, _)| *write == state.writes)
                .map(|(_, fault)| *fault)
            {
                Some(WriteFault::FailAfterCaching) => return Err(SecretError::Unavailable),
                Some(WriteFault::LosePersistence) => return Ok(()),
                None => {}
            }
            let slot = match key {
                PROXY_PASSWORD_SECRET_KEY => &mut state.persisted.password,
                GITHUB_PAT_SECRET_KEY => &mut state.persisted.pat,
                _ => panic!("unexpected secret key"),
            };
            *slot = value;
            Ok(())
        }
    }

    impl SecretStore for MemorySecrets {
        fn get(&self, key: &str) -> Result<Option<String>, SecretError> {
            let state = self.0.lock().unwrap();
            match key {
                PROXY_PASSWORD_SECRET_KEY => Ok(state.cached.password.clone()),
                GITHUB_PAT_SECRET_KEY => Ok(state.cached.pat.clone()),
                _ => panic!("unexpected secret key"),
            }
        }

        fn put(&self, key: &str, value: &str) -> Result<(), SecretError> {
            self.write(key, Some(value.to_string()))
        }

        fn delete(&self, key: &str) -> Result<(), SecretError> {
            self.write(key, None)
        }
    }

    fn initial_settings() -> AppSettings {
        AppSettings {
            network: NetworkSettings {
                proxy_mode: ProxyMode::Direct,
                ..NetworkSettings::default()
            },
            ..AppSettings::default()
        }
    }

    fn draft(settings: &AppSettings, password: &str, pat: &str) -> AppSettingsDto {
        let mut settings = settings.clone();
        settings.network.github_download_mode = DownloadSourceMode::Direct;
        AppSettingsDto {
            settings,
            proxy_password: password.to_string(),
            github_pat: pat.to_string(),
            network_error: None,
        }
    }

    #[tokio::test]
    async fn credential_failure_restores_both_secrets_before_changing_settings() {
        let temp = tempfile::tempdir().unwrap();
        let initial = initial_settings();
        let current = RwLock::new(initial.clone());
        update_app_settings(temp.path(), &current, |_| {})
            .await
            .unwrap();
        let manager = NetworkManager::new(&initial.network, "").unwrap();
        let secrets = MemorySecrets::new(Some("old-password"), Some("old-pat"));
        let previous = secrets.read_fresh().unwrap();
        secrets.fault_at(2, WriteFault::FailAfterCaching);

        let error = save_desktop_settings_with(
            temp.path(),
            &current,
            &manager,
            draft(&initial, "new-password", "new-pat"),
            &secrets,
            || secrets.read_fresh(),
        )
        .await
        .unwrap_err();

        assert!(error.contains("保存 GitHub PAT 失败"));
        assert!(!error.contains("恢复原凭据失败"));
        assert!(secrets.read_fresh().unwrap() == previous);
        assert_eq!(secrets.writes(), 4);
        assert_eq!(*current.read().await, initial);
        assert_eq!(manager.snapshot().settings, initial.network);
        assert_eq!(read_app_settings_file(temp.path()), Some(initial));
    }

    #[tokio::test]
    async fn failed_settings_write_restores_credentials_and_active_client() {
        let temp = tempfile::tempdir().unwrap();
        let invalid_root = temp.path().join("not-a-directory");
        std::fs::write(&invalid_root, "occupied").unwrap();
        let initial = initial_settings();
        let current = RwLock::new(initial.clone());
        let manager = NetworkManager::new(&initial.network, "").unwrap();
        let secrets = MemorySecrets::new(Some("old-password"), None);
        let previous = secrets.read_fresh().unwrap();

        let error = save_desktop_settings_with(
            &invalid_root,
            &current,
            &manager,
            draft(&initial, "new-password", ""),
            &secrets,
            || secrets.read_fresh(),
        )
        .await
        .unwrap_err();

        assert!(error.contains("写入 app-settings.json 失败"));
        assert!(!error.contains("恢复原凭据失败"));
        assert!(secrets.read_fresh().unwrap() == previous);
        assert_eq!(*current.read().await, initial);
        assert_eq!(manager.snapshot().settings, initial.network);
    }

    #[tokio::test]
    async fn fresh_read_detects_rollback_that_only_changed_the_cache() {
        let temp = tempfile::tempdir().unwrap();
        let invalid_root = temp.path().join("not-a-directory");
        std::fs::write(&invalid_root, "occupied").unwrap();
        let initial = initial_settings();
        let current = RwLock::new(initial.clone());
        let manager = NetworkManager::new(&initial.network, "").unwrap();
        let secrets = MemorySecrets::new(Some("old-password"), None);
        secrets.fault_at(2, WriteFault::LosePersistence);

        let error = save_desktop_settings_with(
            &invalid_root,
            &current,
            &manager,
            draft(&initial, "new-password", ""),
            &secrets,
            || secrets.read_fresh(),
        )
        .await
        .unwrap_err();

        assert!(error.contains("恢复原凭据失败"));
        assert!(!error.contains("old-password"));
        assert!(!error.contains("new-password"));
        assert_eq!(*current.read().await, initial);
        assert_eq!(manager.snapshot().settings, initial.network);
    }

    #[tokio::test]
    async fn successful_save_keeps_password_whitespace_and_applies_network() {
        let temp = tempfile::tempdir().unwrap();
        let initial = initial_settings();
        let current = RwLock::new(initial.clone());
        let manager = NetworkManager::new(&initial.network, "").unwrap();
        let secrets = MemorySecrets::new(None, None);

        let (saved, changed) = save_desktop_settings_with(
            temp.path(),
            &current,
            &manager,
            draft(&initial, "  password with spaces  ", ""),
            &secrets,
            || secrets.read_fresh(),
        )
        .await
        .unwrap();

        assert!(changed);
        assert_eq!(
            secrets.read_fresh().unwrap().password.as_deref(),
            Some("  password with spaces  ")
        );
        assert_eq!(secrets.writes(), 1);
        assert_eq!(manager.snapshot().settings, saved.network);
        assert_eq!(read_app_settings_file(temp.path()), Some(saved));
        let file = std::fs::read_to_string(temp.path().join("config/app-settings.json")).unwrap();
        assert!(!file.contains("password with spaces"));
    }

    #[tokio::test]
    async fn unchanged_credentials_are_not_written() {
        let temp = tempfile::tempdir().unwrap();
        let initial = initial_settings();
        let current = RwLock::new(initial.clone());
        let manager = NetworkManager::new(&initial.network, "").unwrap();
        let secrets = MemorySecrets::new(Some("same-password"), Some("same-pat"));
        secrets.fault_at(1, WriteFault::FailAfterCaching);
        save_desktop_settings_with(
            temp.path(),
            &current,
            &manager,
            draft(&initial, "same-password", "same-pat"),
            &secrets,
            || secrets.read_fresh(),
        )
        .await
        .unwrap();
        assert_eq!(secrets.writes(), 0);
    }

    #[test]
    fn unfinished_credential_transaction_restores_previous_values() {
        let secrets = MemorySecrets::new(Some("old-password"), None);
        let previous = secrets.read_fresh().unwrap();
        {
            let mut transaction = SecretTransaction {
                secrets: &secrets,
                read_fresh: || secrets.read_fresh(),
                previous: previous.clone(),
                password_attempted: false,
                pat_attempted: false,
                completed: false,
            };
            transaction
                .save(&CredentialValues {
                    password: Some("new-password".to_string()),
                    pat: None,
                })
                .unwrap();
        }
        assert!(secrets.read_fresh().unwrap() == previous);
    }
}
