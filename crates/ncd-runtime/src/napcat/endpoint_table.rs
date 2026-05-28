//! NapCat per-Bot 运行时端点表。
//!
//! 多 bot 时 NapCat WebUI 的 6099 端口会被先到的进程占住，后到的会自动 +1
//! 找下一个可用端口（甚至再 +1，直到能 bind）。webui.json 只有一份，写的是
//! 最后一个启动的 bot 的端口，并不是每个 bot 的真实端口。
//!
//! 真实 (port, token) 只能从每个 bot 进程自己的 stdout 抓那行
//! `WebUi User Panel Url: http://127.0.0.1:{port}/webui?token={token}`。
//! `runtime_backend::spawn_log_reader` 已经把这条信息抽取出来发成
//! `DomainEvent::NapCatWebuiAvailable { bot_id, port, token }`，本模块提供
//! 一个轻量内存表，让 `BotManager` 在需要的时候按 BotId 反查。
//!
//! 关键约束：
//! - 表内容由 BotManager 单点维护，写入路径在 handle_webui_available；
//!   清理路径在 dispose_poller。
//! - token 是 NapCat 启动时随机生成的，对应一把 webui credential 的种子，
//!   不持久化、不对外发。Snapshot 取出后只用于本进程内一次性 fetch_credential。

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::ids::BotId;

/// 单个 NapCat bot 的 WebUI 接入信息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NapCatEndpoint {
    /// stdout 抓到的真实监听端口。
    pub port: u16,
    /// stdout 抓到的 webui token，用于换 Bearer credential。
    pub token: String,
}

/// per-Bot 端点表。`Clone` 共享同一份内部 `RwLock`，方便嵌入需要 `Clone` 的
/// 容器（例如 `BotManager`）。
#[derive(Debug, Clone, Default)]
pub struct NapCatEndpointTable {
    inner: Arc<RwLock<HashMap<BotId, NapCatEndpoint>>>,
}

impl NapCatEndpointTable {
    /// 新建空表。
    pub fn new() -> Self {
        Self::default()
    }

    /// 插入或覆盖 `bot_id` 对应的端点。
    /// NapCat 进程重启时 stdout 会再印一次 URL，本方法直接覆盖旧值。
    pub async fn insert(&self, bot_id: BotId, endpoint: NapCatEndpoint) {
        self.inner.write().await.insert(bot_id, endpoint);
    }

    /// 移除 `bot_id` 对应的端点；不存在时返回 `None`。
    /// 由 BotProcessExited / delete_bot / shutdown_all 调用。
    pub async fn remove(&self, bot_id: &BotId) -> Option<NapCatEndpoint> {
        self.inner.write().await.remove(bot_id)
    }

    /// 取出 `bot_id` 当前的端点快照（克隆字符串），不持锁返回。
    pub async fn snapshot(&self, bot_id: &BotId) -> Option<NapCatEndpoint> {
        self.inner.read().await.get(bot_id).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn insert_then_snapshot_returns_clone() {
        let table = NapCatEndpointTable::new();
        let id = BotId::new("10001");
        table
            .insert(
                id.clone(),
                NapCatEndpoint {
                    port: 6099,
                    token: "tok".into(),
                },
            )
            .await;

        let got = table.snapshot(&id).await.expect("snapshot present");
        assert_eq!(got.port, 6099);
        assert_eq!(got.token, "tok");
    }

    #[tokio::test]
    async fn insert_overwrites_previous_endpoint() {
        // NapCat 进程重启场景：同一 bot 的端口和 token 都会换。
        let table = NapCatEndpointTable::new();
        let id = BotId::new("10001");
        table
            .insert(
                id.clone(),
                NapCatEndpoint {
                    port: 6099,
                    token: "old".into(),
                },
            )
            .await;
        table
            .insert(
                id.clone(),
                NapCatEndpoint {
                    port: 6100,
                    token: "new".into(),
                },
            )
            .await;

        let got = table.snapshot(&id).await.expect("snapshot present");
        assert_eq!(got.port, 6100);
        assert_eq!(got.token, "new");
    }

    #[tokio::test]
    async fn remove_returns_old_value_and_clears_entry() {
        let table = NapCatEndpointTable::new();
        let id = BotId::new("10001");
        table
            .insert(
                id.clone(),
                NapCatEndpoint {
                    port: 6099,
                    token: "tok".into(),
                },
            )
            .await;

        let removed = table.remove(&id).await.expect("present before remove");
        assert_eq!(removed.port, 6099);
        assert!(table.snapshot(&id).await.is_none());
    }

    #[tokio::test]
    async fn remove_missing_is_idempotent() {
        let table = NapCatEndpointTable::new();
        let id = BotId::new("never-inserted");
        assert!(table.remove(&id).await.is_none());
        assert!(table.remove(&id).await.is_none());
    }

    #[tokio::test]
    async fn clone_shares_state_across_handles() {
        // BotManager::Clone 会复制 NapCatEndpointTable，必须共享同一份内部 map。
        let a = NapCatEndpointTable::new();
        let b = a.clone();
        let id = BotId::new("10001");
        a.insert(
            id.clone(),
            NapCatEndpoint {
                port: 6099,
                token: "tok".into(),
            },
        )
        .await;

        let got = b.snapshot(&id).await.expect("clone shares state");
        assert_eq!(got.port, 6099);
    }
}
