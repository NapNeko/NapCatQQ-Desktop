//! 测试共用：空 Bot 表的 DebugManager。真实 Bot 的完整链路在 ncd-runtime 的调试台测试里，
//! 这里只需要「管理器本身健康、Bot 一个都不认识」

use std::path::PathBuf;
use std::sync::{Arc, Mutex as StdMutex};

use ncd_domain::ids::BotId;
use ncd_runtime::DebugBotPort;
use ncd_runtime::onebot_debug::{DebugBotView, DebugManager};

pub(crate) struct FakeBots {
    pub bots: StdMutex<Vec<DebugBotView>>,
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
    let local_host: Arc<dyn ncd_host::Host> = Arc::new(ncd_host::local::LocalWindowsHost::new());
    Arc::new(DebugManager::new(
        Arc::new(FakeBots {
            bots: StdMutex::new(Vec::new()),
        }),
        Arc::new(ncd_runtime::LocalOnlyHostResolver::new(local_host)),
        root,
    ))
}
