//! 普通操作按实例串行；全局恢复期间拒绝新操作，不让页面排在维护任务后。

use ncd_traits::AppFrameworkError;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, Weak};
use tokio::sync::{
    Mutex as AsyncMutex, OwnedMutexGuard, OwnedRwLockReadGuard, OwnedRwLockWriteGuard, RwLock,
};

#[derive(Default)]
pub(super) struct ConfigGate {
    maintenance: Arc<RwLock<()>>,
    instances: Mutex<HashMap<String, Weak<AsyncMutex<()>>>>,
}

pub(super) struct ConfigOperation {
    _maintenance: OwnedRwLockReadGuard<()>,
    _instance: OwnedMutexGuard<()>,
}

impl ConfigGate {
    pub async fn operation(&self, id: &str) -> Result<ConfigOperation, AppFrameworkError> {
        let maintenance = self
            .maintenance
            .clone()
            .try_read_owned()
            .map_err(|_| AppFrameworkError::Runtime("配置正在备份或恢复，请稍后重试".into()))?;
        let gate = {
            let mut instances = self
                .instances
                .lock()
                .expect("app configuration gates poisoned");
            instances.retain(|_, gate| gate.strong_count() > 0);
            if let Some(gate) = instances.get(id).and_then(Weak::upgrade) {
                gate
            } else {
                let gate = Arc::new(AsyncMutex::new(()));
                instances.insert(id.into(), Arc::downgrade(&gate));
                gate
            }
        };
        let instance = tokio::time::timeout(std::time::Duration::from_secs(8), gate.lock_owned())
            .await
            .map_err(|_| AppFrameworkError::Runtime("该实例有操作尚未结束，请稍后重试".into()))?;
        Ok(ConfigOperation {
            _maintenance: maintenance,
            _instance: instance,
        })
    }

    pub async fn lock(&self) -> OwnedRwLockWriteGuard<()> {
        self.maintenance.clone().write_owned().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn slow_instance_does_not_block_other_instances() {
        let gate = ConfigGate::default();
        let _slow = gate.operation("slow").await.unwrap();
        let _fast = tokio::time::timeout(Duration::from_millis(100), gate.operation("fast"))
            .await
            .unwrap()
            .unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(10), gate.operation("slow"))
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn restore_rejects_new_operations_instead_of_queueing_them() {
        let gate = ConfigGate::default();
        let restore = gate.lock().await;
        assert!(gate.operation("app").await.is_err());
        drop(restore);
        assert!(gate.operation("app").await.is_ok());
    }
}
