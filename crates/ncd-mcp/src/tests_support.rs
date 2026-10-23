//! 测试共用：空 Bot 表的 DebugManager。真实 Bot 的完整链路在 ncd-runtime 的调试台测试里，
//! 这里只需要「管理器本身健康、Bot 一个都不认识」

use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex};

use ncd_domain::RuntimeTarget;
use ncd_domain::ids::BotId;
use ncd_runtime::onebot_debug::{DebugBotView, DebugManager};
use ncd_runtime::{DebugBotPort, HostResolveError, HostResolver};

pub(crate) struct FakeBots {
    pub bots: StdMutex<Vec<DebugBotView>>,
}

/// MCP 单元测试不接触真实主机，也不依赖 Windows 本机实现。
pub(crate) struct NoHostResolver;

#[async_trait::async_trait]
impl HostResolver for NoHostResolver {
    async fn resolve(
        &self,
        _target: &RuntimeTarget,
    ) -> Result<Arc<dyn ncd_host::Host>, HostResolveError> {
        Err(HostResolveError::message("测试不提供真实主机"))
    }
}

#[async_trait::async_trait]
impl DebugBotPort for FakeBots {
    async fn list_bots(&self) -> Vec<DebugBotView> {
        self.bots.lock().unwrap().clone()
    }

    async fn bot(&self, bot_id: &BotId) -> Option<DebugBotView> {
        self.bots
            .lock()
            .unwrap()
            .iter()
            .find(|v| &v.bot_id() == bot_id)
            .cloned()
    }

    async fn napcat_webui(&self, _bot_id: &BotId) -> Option<(u16, String)> {
        None
    }

    async fn snowluma_webui(&self, _bot_id: &BotId) -> Result<(u16, String), String> {
        Err("没有 SnowLuma WebUI".to_owned())
    }
}

pub(crate) fn test_debug_manager(root: PathBuf) -> Arc<DebugManager> {
    Arc::new(DebugManager::new(
        Arc::new(FakeBots {
            bots: StdMutex::new(Vec::new()),
        }),
        Arc::new(NoHostResolver),
        root,
    ))
}
