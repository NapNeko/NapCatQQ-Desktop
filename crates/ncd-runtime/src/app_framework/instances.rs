//! 应用实例表：`data_root/config/app-instances.json`
//!
//! 与 bot.json 同一套 ConfigStore 原子写 + .bak 轮转；内存缓存一份，所有改动走
//! [AppInstanceStore::update] 保证读改写不撕裂。

use std::path::{Path, PathBuf};

use ncd_domain::{AppInstance, AppInstanceId};
use ncd_traits::{AppFrameworkError, ConfigStore};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::config_store_impl::LocalConfigStore;

pub const APP_INSTANCES_FILE: &str = "app-instances.json";
const FILE_VERSION: u32 = 1;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AppInstancesFile {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    instances: Vec<AppInstance>,
}

pub struct AppInstanceStore {
    store: LocalConfigStore,
    path: PathBuf,
    cache: RwLock<Vec<AppInstance>>,
}

impl AppInstanceStore {
    /// 同步读一次磁盘（启动期调用）；文件不存在视为空表
    pub fn load(data_root: &Path) -> Result<Self, AppFrameworkError> {
        let store = LocalConfigStore::new(data_root);
        let path = store.config_dir().join(APP_INSTANCES_FILE);
        let instances = match store.read_json(&path) {
            Ok(value) => {
                serde_json::from_value::<AppInstancesFile>(value)
                    .map_err(|e| AppFrameworkError::Store(format!("{APP_INSTANCES_FILE}: {e}")))?
                    .instances
            }
            Err(ncd_domain::errors::ConfigError::NotFound(_)) => Vec::new(),
            Err(e) => return Err(AppFrameworkError::Store(e.to_string())),
        };
        Ok(Self {
            store,
            path,
            cache: RwLock::new(instances),
        })
    }

    /// 文件损坏时的兜底：空表起步（write_json_atomic 会把旧文件轮转成 .bak，不丢原件）
    pub fn empty(data_root: &Path) -> Self {
        let store = LocalConfigStore::new(data_root);
        let path = store.config_dir().join(APP_INSTANCES_FILE);
        Self {
            store,
            path,
            cache: RwLock::new(Vec::new()),
        }
    }

    pub async fn list(&self) -> Vec<AppInstance> {
        self.cache.read().await.clone()
    }

    pub async fn get(&self, id: &AppInstanceId) -> Option<AppInstance> {
        self.cache.read().await.iter().find(|i| &i.id == id).cloned()
    }

    pub async fn require(&self, id: &AppInstanceId) -> Result<AppInstance, AppFrameworkError> {
        self.get(id)
            .await
            .ok_or_else(|| AppFrameworkError::InstanceNotFound(id.as_str().to_string()))
    }

    /// 新建或整体替换（按 id）
    pub async fn upsert(&self, instance: AppInstance) -> Result<AppInstance, AppFrameworkError> {
        let mut cache = self.cache.write().await;
        match cache.iter_mut().find(|i| i.id == instance.id) {
            Some(slot) => *slot = instance.clone(),
            None => cache.push(instance.clone()),
        }
        self.persist(&cache)?;
        Ok(instance)
    }

    /// 读改写一体；实例不存在返回 InstanceNotFound
    pub async fn update<F>(&self, id: &AppInstanceId, f: F) -> Result<AppInstance, AppFrameworkError>
    where
        F: FnOnce(&mut AppInstance),
    {
        let mut cache = self.cache.write().await;
        let slot = cache
            .iter_mut()
            .find(|i| &i.id == id)
            .ok_or_else(|| AppFrameworkError::InstanceNotFound(id.as_str().to_string()))?;
        f(slot);
        let updated = slot.clone();
        self.persist(&cache)?;
        Ok(updated)
    }

    pub async fn remove(&self, id: &AppInstanceId) -> Result<Option<AppInstance>, AppFrameworkError> {
        let mut cache = self.cache.write().await;
        let pos = cache.iter().position(|i| &i.id == id);
        let removed = pos.map(|p| cache.remove(p));
        if removed.is_some() {
            self.persist(&cache)?;
        }
        Ok(removed)
    }

    fn persist(&self, instances: &[AppInstance]) -> Result<(), AppFrameworkError> {
        let payload = serde_json::to_value(AppInstancesFile {
            version: FILE_VERSION,
            instances: instances.to_vec(),
        })
        .map_err(|e| AppFrameworkError::Store(e.to_string()))?;
        self.store
            .write_json_atomic(&self.path, &payload)
            .map_err(|e| AppFrameworkError::Store(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ncd_domain::{AppFrameworkId, AppInstanceState, AppPlacement};

    fn sample(id: &str) -> AppInstance {
        AppInstance {
            id: AppInstanceId::new(id),
            framework_id: AppFrameworkId::new("karin"),
            display_name: "Karin".into(),
            placement: AppPlacement::LocalNative,
            host_id: "local".into(),
            install_dir: "/c/apps/karin/x".into(),
            port: 7777,
            state: AppInstanceState::NotInstalled,
            link: None,
            installed_version: None,
            last_error: None,
            created_at_ms: 1,
        }
    }

    #[tokio::test]
    async fn round_trips_through_disk() {
        let temp = ncd_test_support::TempWorkspace::new().unwrap();
        let store = AppInstanceStore::load(temp.path()).unwrap();
        assert!(store.list().await.is_empty());

        store.upsert(sample("a")).await.unwrap();
        store
            .update(&AppInstanceId::new("a"), |i| {
                i.state = AppInstanceState::Installed;
                i.installed_version = Some("1.17.0".into());
            })
            .await
            .unwrap();

        let reloaded = AppInstanceStore::load(temp.path()).unwrap();
        let list = reloaded.list().await;
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].state, AppInstanceState::Installed);
        assert_eq!(list[0].installed_version.as_deref(), Some("1.17.0"));

        assert!(reloaded.remove(&AppInstanceId::new("a")).await.unwrap().is_some());
        assert!(reloaded.remove(&AppInstanceId::new("a")).await.unwrap().is_none());
        assert!(AppInstanceStore::load(temp.path()).unwrap().list().await.is_empty());
    }

    #[tokio::test]
    async fn update_missing_is_not_found() {
        let temp = ncd_test_support::TempWorkspace::new().unwrap();
        let store = AppInstanceStore::load(temp.path()).unwrap();
        let err = store.update(&AppInstanceId::new("nope"), |_| {}).await.unwrap_err();
        assert!(matches!(err, AppFrameworkError::InstanceNotFound(_)));
    }
}
