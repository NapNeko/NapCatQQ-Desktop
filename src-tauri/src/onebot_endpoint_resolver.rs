// 从本机 Running Bot 的 connect.httpServers 解析 OneBot HTTP 端点

use async_trait::async_trait;
use ncd_domain::BotActorState;
use ncd_domain::ids::BotId;
use ncd_runtime::{
    LocalHttpServerCandidate, OneBotEndpointResolver, OneBotMessenger,
    resolve_local_onebot_messenger,
};

use crate::AppBotManager;

pub struct BotManagerOneBotEndpointResolver {
    bot_manager: std::sync::Arc<AppBotManager>,
}

impl BotManagerOneBotEndpointResolver {
    pub fn new(bot_manager: std::sync::Arc<AppBotManager>) -> Self {
        Self { bot_manager }
    }
}

#[async_trait]
impl OneBotEndpointResolver for BotManagerOneBotEndpointResolver {
    async fn resolve(
        &self,
        messenger_bot_id: &str,
        exclude_bot_id: &BotId,
    ) -> Option<OneBotMessenger> {
        let bot_id = {
            let messenger = messenger_bot_id.trim();
            if messenger.is_empty() {
                return None;
            }
            BotId::new(messenger)
        };
        let snap = self.bot_manager.get_snapshot(&bot_id).await.ok()?;
        let cfg = self.bot_manager.get_bot_config(&bot_id).await.ok()??;
        let servers: Vec<LocalHttpServerCandidate> = cfg
            .connect
            .http_servers
            .iter()
            .map(|s| LocalHttpServerCandidate {
                enable: s.base.enable,
                host: s.host.clone(),
                port: s.port,
                token: s.base.token.clone(),
            })
            .collect();
        resolve_local_onebot_messenger(
            messenger_bot_id,
            exclude_bot_id.as_str(),
            snap.state == BotActorState::Running,
            &servers,
        )
        .ok()
    }
}
