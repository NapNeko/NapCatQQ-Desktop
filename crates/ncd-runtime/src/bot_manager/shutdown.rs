//! 桌面退出和完整关闭的资源清理。

use super::{BatchResult, BotManager};
use crate::bot_actor::{BotActorHandle, BotActorSnapshot};
use ncd_domain::ids::BotId;
use ncd_traits::{BotConfigRepo, ConfigStore};
use std::collections::HashSet;
use tracing::info;

impl<R: BotConfigRepo + 'static, S: ConfigStore + 'static> BotManager<R, S> {
    /// 桌面进程退出:仅停止本机 Bot;远端(Docker / 直接运行)不停远端进程,只拆掉本机隧道/日志会话
    pub async fn exit_desktop(&self) -> BatchResult {
        info!(
            target: "ncd_runtime::bot_manager",
            "桌面退出：停止本机 Bot，远端保持运行"
        );
        crate::launch::router::cancel_remote_snowluma_initialization(
            &self.remote_snowluma_init_locks,
            None,
        )
        .await;
        let configs = self.repo.list().await.unwrap_or_default();
        let snapshots: Vec<BotActorSnapshot> = {
            let actors = self.actors.read().await;
            actors.values().map(|h| h.snapshot()).collect()
        };

        let local_active_ids: Vec<BotId> = snapshots
            .iter()
            .filter(|s| s.state.is_active())
            .filter(|s| {
                configs
                    .iter()
                    .find(|c| c.bot.qq_id.to_string() == s.bot_id.as_str())
                    .is_some_and(|c| c.bot.runtime_target.is_local())
            })
            .map(|s| s.bot_id.clone())
            .collect();

        let mut result = BatchResult {
            succeeded: Vec::new(),
            failed: Vec::new(),
        };

        for bot_id in &local_active_ids {
            match self.stop_bot(bot_id).await {
                Ok(_) => result.succeeded.push(bot_id.clone()),
                Err(err) => result.failed.push((bot_id.clone(), err)),
            }
        }

        if let Some(daemon) = self.snowluma_daemon.as_ref() {
            daemon.shutdown().await;
        }

        self.remote_snowluma_tunnels.shutdown_all().await;
        self.windows_bots.close_all().await;
        let daemons: Vec<_> = self
            .remote_snowluma_daemons
            .lock()
            .await
            .values()
            .cloned()
            .collect();
        for daemon in daemons {
            daemon.detach_local_sessions().await;
        }

        self.remote_runtime_sessions().shutdown_all().await;

        result
    }

    /// 应用退出收口:尝试停止所有运行中的 Bot 并 shutdown 它们的 Actor
    /// 用法:Tauri WindowEvent::CloseRequested 时调用,避免 QQ.exe 残留
    /// 行为:
    /// - 对所有处于 active 状态的 Bot 调用 stop_bot(内部走 kill_process_tree)
    /// - 不论 stop 是否成功,都会 shutdown 对应的 actor 释放邮箱
    /// - 任何错误只记录到返回值,不会阻塞其它 Bot 的清理
    pub async fn shutdown_all(&self) -> BatchResult {
        info!(target: "ncd_runtime::bot_manager", "应用退出：正在停止所有运行中的 Bot");
        // 远端初始化按主机共享，不能取消迁移任务持有的初始化。
        if !self.has_protocol_reservations() {
            crate::launch::router::cancel_remote_snowluma_initialization(
                &self.remote_snowluma_init_locks,
                None,
            )
            .await;
        }
        let snapshots: Vec<BotActorSnapshot> = {
            let actors = self.actors.read().await;
            actors.values().map(|h| h.snapshot()).collect()
        };

        let mut result = BatchResult {
            succeeded: Vec::new(),
            failed: Vec::new(),
        };
        let mut mutation_guards = Vec::new();
        let mut shutdown_ids = HashSet::new();
        for snapshot in &snapshots {
            if self.is_protocol_reserved(&snapshot.bot_id) {
                continue;
            }
            match self.guard_protocol_mutation(&snapshot.bot_id) {
                Ok(guard) => {
                    shutdown_ids.insert(snapshot.bot_id.clone());
                    mutation_guards.push(guard);
                }
                Err(error) => result.failed.push((snapshot.bot_id.clone(), error)),
            }
        }
        let active_ids: Vec<BotId> = snapshots
            .iter()
            .filter(|s| shutdown_ids.contains(&s.bot_id) && s.state.is_active())
            .map(|s| s.bot_id.clone())
            .collect();

        for bot_id in &active_ids {
            match self.stop_bot(bot_id).await {
                Ok(_) => result.succeeded.push(bot_id.clone()),
                Err(err) => result.failed.push((bot_id.clone(), err)),
            }
        }

        // 保留迁移 Actor；其余 Actor 在计数租约释放前不能被迁移任务认领。
        let handles: Vec<BotActorHandle> = {
            let actors = self.actors.read().await;
            actors
                .iter()
                .filter(|(id, _)| shutdown_ids.contains(*id))
                .map(|(_, handle)| handle.clone())
                .collect()
        };
        for handle in handles {
            let _ = handle.shutdown().await;
        }
        {
            let mut actors = self.actors.write().await;
            actors.retain(|id, _| !shutdown_ids.contains(id));
        }

        // 迁移中的登录轮询仍供目标在线检查使用。
        {
            let mut pollers = self.login_pollers.write().await;
            for id in &shutdown_ids {
                if let Some(poller) = pollers.remove(id) {
                    poller.dispose();
                }
            }
        }

        // SnowLuma daemon 优雅关闭
        if let Some(daemon) = self.snowluma_daemon.as_ref() {
            daemon.shutdown().await;
        }

        for id in &shutdown_ids {
            self.remote_runtime_sessions().shutdown_bot(id).await;
        }

        result
    }
}
