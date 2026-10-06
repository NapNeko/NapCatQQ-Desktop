//! 调试台向外要的两样东西：Bot 的配置和运行态（由 BotManager 提供），
//! 以及把事件批次推给前端的出口（由 Tauri 那一层提供）。
//!
//! 窄接口而不是直接持有 `BotManager<R, S>`：调试台只读这几项，测试里一个假实现就能驱动，
//! 也不会让 DebugManager 带上 BotManager 的两个泛型参数。

use ncd_domain::bot_actor::{BotActorSnapshot, BotActorState};
use ncd_domain::bot_config::BotConfig;
use ncd_domain::ids::BotId;
use ncd_domain::onebot_debug::DebugEventBatch;

/// 调试台眼里的一个 Bot：配置 + 运行态 + 上游报告的登录状态。
/// `Debug` 手写：配置里有各个网络服务的令牌，只打印认得出是哪个 Bot 的几项
#[derive(Clone)]
pub struct DebugBotView {
    pub config: BotConfig,
    pub snapshot: BotActorSnapshot,
    /// 上游报告的 QQ 登录状态；探不到（或后端不报）时为空
    pub online: Option<bool>,
}

impl std::fmt::Debug for DebugBotView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DebugBotView")
            .field("bot_id", &self.bot_id())
            .field("name", &self.config.bot.name)
            .field("backend", &self.config.bot.backend_type)
            .field("state", &self.snapshot.state)
            .field("revision", &self.snapshot.revision)
            .field("online", &self.online)
            .finish_non_exhaustive()
    }
}

impl DebugBotView {
    /// 与 BotManager 一致的 BotId：QQ 号的十进制串
    pub fn bot_id(&self) -> BotId {
        BotId::new(self.config.bot.qq_id.to_string())
    }

    pub fn running(&self) -> bool {
        self.snapshot.state == BotActorState::Running
    }
}

#[async_trait::async_trait]
pub trait DebugBotPort: Send + Sync {
    async fn list_bots(&self) -> Vec<DebugBotView>;
    async fn bot(&self, bot_id: &BotId) -> Option<DebugBotView>;
    /// Desktop 能直接连的 NapCat WebUI（本机进程口或隧道本地口）+ WebUI token
    async fn napcat_webui(&self, bot_id: &BotId) -> Option<(u16, String)>;
    /// SnowLuma WebUI（本机 daemon / 远端隧道 / Docker 隧道）端口 + 密码
    async fn snowluma_webui(&self, bot_id: &BotId) -> Result<(u16, String), String>;
    async fn recover_webui(&self, _bot_id: &BotId) -> Result<(), String> {
        Ok(())
    }
}

/// 事件批次的出口。返回 false 表示对面已经走了（窗口关了），调用方据此摘掉它
pub trait DebugEventSink: Send + Sync {
    fn send(&self, batch: &DebugEventBatch) -> bool;

    /// 开这个出口的页面（窗口标签）和开的时刻。网页重载后旧页面的出口照样 `send` 成功
    /// （新页面里只是找不到回调），空批次探不出来，只能在「页面开始加载」时按窗口摘掉更早的，
    /// 见 [`super::DebugManager::page_loading`]。不属于某个页面的出口返回 `None`
    fn opened_by(&self) -> Option<(&str, std::time::Instant)> {
        None
    }
}
