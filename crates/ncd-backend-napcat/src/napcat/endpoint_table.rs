//! NapCat per-Bot 运行时端点表
//!
//! 多 bot 时 NapCat WebUI 的 6099 端口会被先到的进程占住,后到的会自动 +1
//! 找下一个可用端口(甚至再 +1,直到能 bind)webui.json 只有一份,写的是
//! 最后一个启动的 bot 的端口,并不是每个 bot 的真实端口
//!
//! 真实 (port, token) 只能从每个 bot 进程自己的 stdout 抓那行
//! WebUi User Panel Url: http://127.0.0.1:{port}/webui?token={token}
//! runtime_backend::spawn_log_reader 已经把这条信息抽取出来发成
//! DomainEvent::NapCatWebuiAvailable { bot_id, port, token },本模块提供
//! 一个轻量内存表,让 BotManager 在需要的时候按 BotId 反查
//!
//! 远端场景额外记 host_port:SSH 隧道本机口只能给 Desktop login_poller 用,
//! ncd-watch 跑在远端,必须探 Bot 进程本机监听口(通常 6099),不能写隧道口。
//!
//! 关键约束:
//! - 表内容由 BotManager 单点维护,写入路径在 handle_webui_available;
//!   清理路径在 dispose_poller
//! - token 是 NapCat 启动时随机生成的,对应一把 webui credential 的种子,
//!   不持久化,不对外发Snapshot 取出后只用于本进程内一次性 fetch_credential

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;

use ncd_domain::ids::BotId;

/// 单个 NapCat bot 的 WebUI 接入信息
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NapCatEndpoint {
    /// Desktop 本机可达端口(本机=进程口;远端=SSH 隧道本地口)
    pub port: u16,
    /// 远端 Host 上进程实际监听口;供 ncd-watch 写 notify.json
    /// None 表示与 port 相同(本机)
    pub host_port: Option<u16>,
    /// stdout 抓到的 webui token,用于换 Bearer credential
    pub token: String,
    /// 最近一次可信登录探测；None 表示尚未探测或连续探测失败。
    pub online: Option<bool>,
}

impl NapCatEndpoint {
    /// watch / 远端本机探活用的端口
    pub fn watch_port(&self) -> u16 {
        self.host_port.filter(|p| *p > 0).unwrap_or(self.port)
    }
}

/// per-Bot 端点表Clone 共享同一份内部 RwLock,方便嵌入需要 Clone 的
/// 容器(例如 BotManager)
#[derive(Debug, Clone, Default)]
pub struct NapCatEndpointTable {
    inner: Arc<RwLock<HashMap<BotId, NapCatEndpoint>>>,
}

impl NapCatEndpointTable {
    /// 新建空表
    pub fn new() -> Self {
        Self::default()
    }

    /// 插入或覆盖 bot_id 对应的端点
    /// NapCat 进程重启时 stdout 会再印一次 URL,本方法直接覆盖旧值
    pub async fn insert(&self, bot_id: BotId, endpoint: NapCatEndpoint) {
        self.inner.write().await.insert(bot_id, endpoint);
    }

    /// 移除 bot_id 对应的端点;不存在时返回 None
    /// 由 BotProcessExited / delete_bot / shutdown_all 调用
    pub async fn remove(&self, bot_id: &BotId) -> Option<NapCatEndpoint> {
        self.inner.write().await.remove(bot_id)
    }

    /// 取出 bot_id 当前的端点快照(克隆字符串),不持锁返回
    pub async fn snapshot(&self, bot_id: &BotId) -> Option<NapCatEndpoint> {
        self.inner.read().await.get(bot_id).cloned()
    }

    /// 更新最近一次登录探测，不存在的端点说明会话已回收，忽略迟到事件。
    pub async fn set_online(&self, bot_id: &BotId, online: Option<bool>) {
        if let Some(endpoint) = self.inner.write().await.get_mut(bot_id) {
            endpoint.online = online;
        }
    }

    /// 列出当前全部端点(冷启动 hydrate / 前端补齐 WebUI 按钮用)
    pub async fn list_all(&self) -> Vec<(BotId, NapCatEndpoint)> {
        self.inner
            .read()
            .await
            .iter()
            .map(|(id, ep)| (id.clone(), ep.clone()))
            .collect()
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
                    host_port: None,
                    token: "tok".into(),
                    online: None,
                },
            )
            .await;
        let snap = table.snapshot(&id).await.unwrap();
        assert_eq!(snap.port, 6099);
        assert_eq!(snap.token, "tok");
        assert_eq!(snap.watch_port(), 6099);
    }

    #[tokio::test]
    async fn watch_port_prefers_host_port() {
        let ep = NapCatEndpoint {
            port: 58408,
            host_port: Some(6099),
            token: "t".into(),
            online: Some(true),
        };
        assert_eq!(ep.watch_port(), 6099);
    }

    #[tokio::test]
    async fn remove_clears_entry() {
        let table = NapCatEndpointTable::new();
        let id = BotId::new("10001");
        table
            .insert(
                id.clone(),
                NapCatEndpoint {
                    port: 1,
                    host_port: None,
                    token: "a".into(),
                    online: None,
                },
            )
            .await;
        assert!(table.remove(&id).await.is_some());
        assert!(table.snapshot(&id).await.is_none());
    }

    #[tokio::test]
    async fn list_all_returns_all_entries() {
        let table = NapCatEndpointTable::new();
        let a = BotId::new("10001");
        let b = BotId::new("10002");
        table
            .insert(
                a.clone(),
                NapCatEndpoint {
                    port: 6099,
                    host_port: None,
                    token: "a".into(),
                    online: Some(true),
                },
            )
            .await;
        table
            .insert(
                b.clone(),
                NapCatEndpoint {
                    port: 6100,
                    host_port: Some(6100),
                    token: "b".into(),
                    online: Some(false),
                },
            )
            .await;
        let mut all = table.list_all().await;
        all.sort_by(|x, y| x.0.as_str().cmp(y.0.as_str()));
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].0, a);
        assert_eq!(all[0].1.port, 6099);
        assert_eq!(all[1].0, b);
        assert_eq!(all[1].1.port, 6100);
    }

    #[tokio::test]
    async fn clone_shares_inner_map() {
        // BotManager::Clone 会复制 NapCatEndpointTable,必须共享同一份内部 map
        let a = NapCatEndpointTable::new();
        let b = a.clone();
        let id = BotId::new("1");
        a.insert(
            id.clone(),
            NapCatEndpoint {
                port: 9,
                host_port: Some(6099),
                token: "x".into(),
                online: None,
            },
        )
        .await;
        assert_eq!(b.snapshot(&id).await.unwrap().watch_port(), 6099);
    }

    #[tokio::test]
    async fn set_online_updates_existing_endpoint_only() {
        let table = NapCatEndpointTable::new();
        let id = BotId::new("10001");
        table
            .insert(
                id.clone(),
                NapCatEndpoint {
                    port: 6099,
                    host_port: None,
                    token: "tok".into(),
                    online: None,
                },
            )
            .await;

        table.set_online(&id, Some(true)).await;
        table.set_online(&BotId::new("missing"), Some(false)).await;

        assert_eq!(table.snapshot(&id).await.unwrap().online, Some(true));
        assert_eq!(table.list_all().await.len(), 1);
    }
}
