//! 正在跑的可取消动作:task_id → 取消令牌
//!
//! 组件动作和 Desktop 自更新共用一张表:取消命令按 task_id 找令牌,
//! 轻量模式据它判断还有没有活在跑。锁只包一次 HashMap 读写,用同步锁,
//! 这样 guard 在 Drop 里就能把条目摘掉。

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use tokio_util::sync::CancellationToken;

#[derive(Clone, Default)]
pub struct ActiveTasks {
    inner: Arc<Mutex<HashMap<String, CancellationToken>>>,
}

impl ActiveTasks {
    fn map(&self) -> MutexGuard<'_, HashMap<String, CancellationToken>> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn insert(&self, task_id: impl Into<String>, token: CancellationToken) {
        self.map().insert(task_id.into(), token);
    }

    pub fn remove(&self, task_id: &str) {
        self.map().remove(task_id);
    }

    /// 登记一条,返回的 guard 丢掉时自动摘除;runner 中途 return 也不会留下死条目
    pub fn register(
        &self,
        task_id: impl Into<String>,
        token: CancellationToken,
    ) -> ActiveTaskGuard {
        let task_id = task_id.into();
        self.insert(task_id.clone(), token);
        ActiveTaskGuard {
            tasks: self.clone(),
            task_id,
        }
    }

    /// 找到就取消;没登记(已跑完 / 还在排队)返回 false
    pub fn cancel(&self, task_id: &str) -> bool {
        let token = self.map().get(task_id).cloned();
        match token {
            Some(t) => {
                t.cancel();
                true
            }
            None => false,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.map().is_empty()
    }
}

pub struct ActiveTaskGuard {
    tasks: ActiveTasks,
    task_id: String,
}

impl Drop for ActiveTaskGuard {
    fn drop(&mut self) {
        self.tasks.remove(&self.task_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guard_removes_entry_on_drop() {
        let tasks = ActiveTasks::default();
        let token = CancellationToken::new();
        {
            let _guard = tasks.register("t1", token.clone());
            assert!(!tasks.is_empty());
            assert!(tasks.cancel("t1"));
            assert!(token.is_cancelled());
        }
        assert!(tasks.is_empty());
        assert!(!tasks.cancel("t1"));
    }
}
