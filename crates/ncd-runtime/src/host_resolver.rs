//! HostResolver:把 BotConfig 的 RuntimeTarget 解析成一个可用的 Arc<dyn Host>。
//!
//! 为什么要这层抽象:bot 启动时要知道"在哪台机器上跑"。Local 用共享的本机
//! host;Server(id) 要去 ServerManager 取已连接(或现连)的 RemoteLinuxHost。
//! ServerManager 在 src-tauri 侧 wiring,而 BotManager 在 ncd-runtime,直接持有
//! ServerManager 会让 BotManager 绑死一种获取方式且难测。抽成 trait 后:
//! - 生产:TauriHostResolver 包 ServerManager + 本机 host
//! - 测试:mock resolver 直接返回 MockHost
//!
//! 放 ncd-runtime(它已依赖 ncd-host),src-tauri 实装 trait 注入 BotManager。

use std::sync::Arc;

use async_trait::async_trait;
use ncd_domain::RuntimeTarget;
use ncd_host::Host;

/// 把 RuntimeTarget 解析成可用的 host。失败返回人话错误(连不上远端 / server_id
/// 不存在等),由 BotManager 转成启动失败事件。
#[async_trait]
pub trait HostResolver: Send + Sync {
    /// 解析 target -> host。Local 给本机 host;Server(id) 连远端。
    /// 实装应做连接复用(同一 server_id 多次解析共用一条连接)。
    async fn resolve(&self, target: &RuntimeTarget) -> Result<Arc<dyn Host>, String>;

    /// 强制刷新（默认实现回退到 resolve）。调用方希望拿到一个"新鲜"的 host 实例，
    /// 用于替换自己持有的旧引用（例如 Holder 观测到传输失败后想换一个活连接）。
    /// 本地/Stub 实装通常直接回退到 resolve（无缓存可刷）。
    async fn refresh(&self, target: &RuntimeTarget) -> Result<Arc<dyn Host>, String> {
        self.resolve(target).await
    }
}

/// 始终返回固定本机 host 的 resolver。用于:还没接入远端的过渡期、纯本机
/// 部署场景、以及测试。Server(id) 也返回本机 host(过渡期不报错,行为等同
/// 旧版"一切本机")——真正的远端解析由 TauriHostResolver 覆盖。
pub struct LocalOnlyHostResolver {
    local: Arc<dyn Host>,
}

impl LocalOnlyHostResolver {
    pub fn new(local: Arc<dyn Host>) -> Self {
        Self { local }
    }
}

#[async_trait]
impl HostResolver for LocalOnlyHostResolver {
    async fn resolve(&self, _target: &RuntimeTarget) -> Result<Arc<dyn Host>, String> {
        Ok(Arc::clone(&self.local))
    }
}
