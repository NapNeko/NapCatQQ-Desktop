//! 不建立连接或初始化运行时的 Bot 日志查询。

use ncd_domain::{BackendType, BotConfig, BotId, LogSnapshot, RuntimeScenario};
use ncd_traits::ConfigStore;
use ncd_traits::runtime_backend::TailOpts;

use super::{BotManager, BotManagerError};
use crate::LocalBotConfigRepo;

impl<S: ConfigStore + 'static> BotManager<LocalBotConfigRepo<S>, S> {
    /// 缓存未初始化时直接报告错误，不借查询触发旧口令迁移。
    pub async fn get_cached_bot_config(
        &self,
        bot_id: &BotId,
    ) -> Result<Option<BotConfig>, BotManagerError> {
        let qq_id = bot_id
            .as_str()
            .parse()
            .map_err(|_| BotManagerError::BotNotFound(bot_id.clone()))?;
        self.repo
            .get_cached(qq_id)
            .await
            .map_err(BotManagerError::from)
    }

    /// 只读已注入的本机后端缓存；远端日志尚无本机缓存时返回 None。
    /// 不能复用 tail_log 的完整路由，它可能建立 SSH 连接、探库存和补建凭据。
    pub async fn tail_cached_log(
        &self,
        bot_id: &BotId,
        lines: usize,
    ) -> Result<Option<LogSnapshot>, BotManagerError> {
        let config = self
            .get_cached_bot_config(bot_id)
            .await?
            .ok_or_else(|| BotManagerError::BotNotFound(bot_id.clone()))?;
        let RuntimeScenario::LocalNative { backend } = RuntimeScenario::from_config(&config)?
        else {
            return Ok(None);
        };
        if !self.actors.read().await.contains_key(bot_id) {
            return Ok(None);
        }
        let backend = match backend {
            BackendType::NapCat => &self.backend,
            BackendType::SnowLuma => match self.snowluma_backend.as_ref() {
                Some(backend) => backend,
                None => return Ok(None),
            },
        };
        backend
            .tail_log(
                bot_id.clone(),
                TailOpts {
                    lines: lines.clamp(1, 200),
                },
            )
            .await
            .map(Some)
            .map_err(BotManagerError::from)
    }
}
