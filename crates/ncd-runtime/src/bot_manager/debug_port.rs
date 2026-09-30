//! BotManager 给调试台的窄接口：Bot 列表（配置 + 运行态）和两个后端 WebUI 的可达端点。
//! 只读，不改任何 Bot 配置。

use std::collections::HashMap;

use async_trait::async_trait;
use ncd_domain::RuntimeScenario;
use ncd_domain::bot_actor::BotActorSnapshot;
use ncd_domain::bot_config::{BackendType, BotConfig};
use ncd_domain::ids::BotId;
use ncd_traits::{BotConfigRepo, ConfigStore};
use tracing::warn;

use super::BotManager;
use crate::onebot_debug::{DebugBotPort, DebugBotView};
use crate::snowluma_local_endpoint::local_snowluma_webui_endpoint;

impl<R: BotConfigRepo + 'static, S: ConfigStore + 'static> BotManager<R, S> {
    async fn debug_view(
        &self,
        config: BotConfig,
        snapshot: Option<BotActorSnapshot>,
    ) -> DebugBotView {
        let bot_id = BotId::new(config.bot.qq_id.to_string());
        // NapCat 的登录态来自 WebUI 端点表的探测；SnowLuma 这边没有同等的逐 Bot 字段
        let online = match config.bot.backend_type {
            BackendType::NapCat => self
                .napcat_endpoints
                .snapshot(&bot_id)
                .await
                .and_then(|ep| ep.online),
            BackendType::SnowLuma => None,
        };
        DebugBotView {
            config,
            // 配置在、Actor 还没建（刚导入、还没启动过）时按已停止处理
            snapshot: snapshot.unwrap_or_else(|| BotActorSnapshot::new(bot_id)),
            online,
        }
    }
}

#[async_trait]
impl<R: BotConfigRepo + 'static, S: ConfigStore + 'static> DebugBotPort for BotManager<R, S> {
    async fn list_bots(&self) -> Vec<DebugBotView> {
        let configs = match self.list_bot_configs().await {
            Ok(configs) => configs,
            Err(err) => {
                warn!(error = %err, "调试台：读取 Bot 配置失败");
                return Vec::new();
            }
        };
        let mut snapshots: HashMap<BotId, BotActorSnapshot> = self
            .list_snapshots()
            .await
            .into_iter()
            .map(|snapshot| (snapshot.bot_id.clone(), snapshot))
            .collect();
        let mut views = Vec::with_capacity(configs.len());
        for config in configs {
            let snapshot = snapshots.remove(&BotId::new(config.bot.qq_id.to_string()));
            views.push(self.debug_view(config, snapshot).await);
        }
        views
    }

    async fn bot(&self, bot_id: &BotId) -> Option<DebugBotView> {
        let config = match self.get_bot_config(bot_id).await {
            Ok(config) => config?,
            Err(err) => {
                warn!(bot_id = %bot_id, error = %err, "调试台：读取 Bot 配置失败");
                return None;
            }
        };
        let snapshot = self.get_snapshot(bot_id).await.ok();
        Some(self.debug_view(config, snapshot).await)
    }

    async fn napcat_webui(&self, bot_id: &BotId) -> Option<(u16, String)> {
        let endpoint = self.napcat_endpoints.snapshot(bot_id).await?;
        (endpoint.port > 0 && !endpoint.token.is_empty()).then_some((endpoint.port, endpoint.token))
    }

    async fn snowluma_webui(&self, bot_id: &BotId) -> Result<(u16, String), String> {
        let config = self
            .get_bot_config(bot_id)
            .await
            .map_err(|e| format!("读取 Bot 配置失败：{e}"))?
            .ok_or_else(|| "Bot 不存在".to_owned())?;
        if config.bot.backend_type != BackendType::SnowLuma {
            return Err("这不是 SnowLuma Bot".to_owned());
        }
        match RuntimeScenario::from_config(&config) {
            Ok(RuntimeScenario::RemoteDocker { .. }) => self
                .snowluma_docker_endpoints(bot_id)
                .await
                .map(|ep| (ep.webui_local_port, ep.webui_password))
                .ok_or_else(|| "SnowLuma Docker 隧道还没就绪".to_owned()),
            Ok(RuntimeScenario::RemoteNative { server_id, .. }) => self
                .snowluma_native_endpoints_for_server(&server_id)
                .await
                .map(|ep| (ep.webui_local_port, ep.webui_password))
                .ok_or_else(|| "SnowLuma 远端隧道还没就绪".to_owned()),
            Ok(RuntimeScenario::LocalNative { .. }) => {
                local_snowluma_webui_endpoint(self.store.root())
            }
            Err(err) => Err(format!("Bot 配置不完整：{err}")),
        }
    }
}
