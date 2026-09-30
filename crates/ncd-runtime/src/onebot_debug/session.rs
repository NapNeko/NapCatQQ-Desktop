//! 每个 Bot 一份的连接状态：内部通道的客户端、各通道的协议客户端和隧道、最近一次的通道状态、
//! 现取的动作目录。
//!
//! 客户端和隧道都按「建它时用的地址 / 令牌」记一个指纹：用户改了端口或令牌、NapCat 重启换了
//! WebUI 口之后，旧的连接不能再用，下一次取用时发现指纹对不上就整组丢掉重建。
//!
//! 会话没人用超过 30 分钟（也没有接收器在收事件）就由清扫整个丢掉：本机的 `-L` 隧道口
//! 一直开着，等于在本机留了一个直通远端 OneBot 服务的口子。

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use ncd_backend_napcat::NapCatDebugClient;
use ncd_backend_snowluma::SnowLumaDebugClient;
use ncd_domain::onebot_debug::DebugChannelStatus;
use ncd_host::remote::TunnelHandle;
use ncd_onebot::catalog::Catalog;
use ncd_onebot::client::{HttpActionClient, WsClient};
// 用 tokio 的时钟：空闲回收、目录失败冷却在暂停时钟的测试里才能快进
use tokio::time::Instant;

use super::plan::ChannelPlan;

/// 内部通道状态的键，与 `DebugChannelId::Internal.label_key()` 一致
pub(crate) const INTERNAL_KEY: &str = "internal";
/// 现取目录失败后多久内不再重试：否则每次打开目录都要重新登录一遍、再记一条告警
pub(crate) const CATALOG_RETRY_AFTER: Duration = Duration::from_secs(30);

pub(crate) struct BotSession {
    pub napcat: Option<Arc<NapCatDebugClient>>,
    pub snowluma: Option<Arc<SnowLumaDebugClient>>,
    /// 内部通道探测到调试接口不存在（404）。「自动」挑通道时跳过它
    pub internal_too_old: bool,
    /// 键都是 `DebugChannelId::label_key`
    pub http: HashMap<String, Arc<HttpActionClient>>,
    pub ws: HashMap<String, Arc<WsClient>>,
    /// Drop 即关隧道
    pub tunnels: HashMap<String, OpenTunnel>,
    /// 最近一次探测 / 调用得出的通道状态
    pub status: HashMap<String, DebugChannelStatus>,
    /// 现取的目录与快照合并后的结果，本会话内只取一次
    pub catalog: Option<Arc<Catalog>>,
    /// 最近一次现取目录失败的时间（「太老」另有标记，不算在这里）
    pub catalog_failed_at: Option<Instant>,
    /// 最近一次有人用它的时间；空闲回收按它判断
    pub last_activity: Instant,
    /// 内部客户端对应的 WebUI（端口, 口令指纹）
    internal_endpoint: Option<(u16, u64)>,
    /// 上面这些关于上游的判断是在 Bot 快照的哪个 revision 下得出的
    bot_revision: u64,
    /// 各通道的 http / ws / 隧道是按哪份规划建的
    signatures: HashMap<String, u64>,
}

impl BotSession {
    /// `bot_revision` 是建会话时 Bot 快照的 revision
    pub fn new(bot_revision: u64) -> Self {
        Self {
            napcat: None,
            snowluma: None,
            internal_too_old: false,
            http: HashMap::new(),
            ws: HashMap::new(),
            tunnels: HashMap::new(),
            status: HashMap::new(),
            catalog: None,
            catalog_failed_at: None,
            last_activity: Instant::now(),
            internal_endpoint: None,
            bot_revision,
            signatures: HashMap::new(),
        }
    }

    pub fn touch(&mut self) {
        self.last_activity = Instant::now();
    }

    /// 超过 `idle` 没人用了
    pub fn idle_longer_than(&self, idle: Duration, now: Instant) -> bool {
        now.saturating_duration_since(self.last_activity) > idle
    }

    /// WebUI 端点换了（Bot 重启、换口、改了令牌）：旧客户端的凭据作废，
    /// 「太老」的判断和现取的目录也可能随上游升级而变，一并清掉
    pub fn sync_internal_endpoint(&mut self, port: u16, secret: &str) {
        let key = (port, fingerprint(secret));
        if self.internal_endpoint == Some(key) {
            return;
        }
        self.internal_endpoint = Some(key);
        self.napcat = None;
        self.snowluma = None;
        self.forget_upstream();
    }

    /// Bot 的状态变过（停了又开、重启）：上游可能在这期间升级了，
    /// 「太老」、现取的目录和取目录失败的冷却都重新来过。客户端留着，端点变没变另有指纹管
    pub fn sync_bot_revision(&mut self, revision: u64) {
        if self.bot_revision == revision {
            return;
        }
        self.bot_revision = revision;
        self.forget_upstream();
    }

    /// 现取目录最近失败过、还在冷却期内
    pub fn catalog_cooling_down(&self) -> bool {
        self.catalog_failed_at
            .is_some_and(|at| at.elapsed() < CATALOG_RETRY_AFTER)
    }

    fn forget_upstream(&mut self) {
        self.internal_too_old = false;
        self.catalog = None;
        self.catalog_failed_at = None;
        self.status.remove(INTERNAL_KEY);
    }

    /// 通道规划变了（改端口、改令牌、改监听地址）：丢掉按旧规划建的连接、隧道和状态
    pub fn sync_plan(&mut self, plan: &ChannelPlan) {
        let key = plan.id.label_key();
        let signature = plan_signature(plan);
        if self.signatures.get(&key) == Some(&signature) {
            return;
        }
        self.http.remove(&key);
        self.ws.remove(&key);
        self.tunnels.remove(&key);
        self.status.remove(&key);
        self.signatures.insert(key, signature);
    }

    /// 缓存的状态还对得上当前规划才算数；对不上说明配置改过，状态是旧地址的
    pub fn status_for(&self, plan: &ChannelPlan) -> Option<&DebugChannelStatus> {
        let key = plan.id.label_key();
        if key != INTERNAL_KEY && self.signatures.get(&key) != Some(&plan_signature(plan)) {
            return None;
        }
        self.status.get(&key)
    }

    /// 成功时的状态：走隧道的写明本机口，直连的就是可用
    pub fn ok_status(&self, key: &str) -> DebugChannelStatus {
        match self.tunnels.get(key) {
            Some(tunnel) => DebugChannelStatus::Tunneled {
                local_port: tunnel.local_port(),
            },
            None => DebugChannelStatus::Available,
        }
    }

    /// 关掉这个会话持有的一切。WS 显式关闭，别处还拿着克隆在等回包的调用会立刻收到「已关闭」
    pub fn shutdown(&mut self) {
        for ws in self.ws.values() {
            ws.close();
        }
        self.ws.clear();
        self.http.clear();
        self.tunnels.clear();
    }
}

/// 会话持有的一条隧道。计入管理器的「开着的隧道」数，丢掉（随即关闭）时减掉
pub(crate) struct OpenTunnel {
    handle: TunnelHandle,
    open: Arc<AtomicUsize>,
}

impl OpenTunnel {
    pub fn new(handle: TunnelHandle, open: &Arc<AtomicUsize>) -> Self {
        open.fetch_add(1, Ordering::SeqCst);
        Self {
            handle,
            open: Arc::clone(open),
        }
    }

    pub fn local_port(&self) -> u16 {
        self.handle.local_port()
    }
}

impl Drop for OpenTunnel {
    fn drop(&mut self) {
        // 句柄紧接着随结构体一起被丢弃，它自己的 Drop 负责关隧道
        self.open.fetch_sub(1, Ordering::SeqCst);
    }
}

/// 口令 / 令牌只在进程内比对是否变化，不需要也不应该原样存第二份
fn fingerprint(secret: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    secret.hash(&mut hasher);
    hasher.finish()
}

fn plan_signature(plan: &ChannelPlan) -> u64 {
    let mut hasher = DefaultHasher::new();
    plan.reach.hash(&mut hasher);
    plan.token.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use ncd_domain::onebot_debug::DebugChannelId;

    use super::super::plan::Reach;
    use super::*;

    fn plan(port: u16, token: Option<&str>) -> ChannelPlan {
        ChannelPlan {
            id: DebugChannelId::Http { name: "h".into() },
            label: "HTTP · h".into(),
            can_call: true,
            can_receive: false,
            reach: Reach::Direct {
                host: "127.0.0.1".into(),
                port,
                path: "/".into(),
            },
            token: token.map(str::to_owned),
            rank_call: Some(3),
            rank_events: None,
        }
    }

    #[tokio::test]
    async fn plan_change_drops_clients_and_status() {
        let mut session = BotSession::new(1);
        let first = plan(3000, None);
        session.sync_plan(&first);
        session.http.insert(
            "http:h".into(),
            Arc::new(HttpActionClient::new("http://127.0.0.1:3000", None).unwrap()),
        );
        session
            .status
            .insert("http:h".into(), DebugChannelStatus::Available);
        assert_eq!(
            session.status_for(&first),
            Some(&DebugChannelStatus::Available)
        );

        // 同一份规划：什么都不动
        session.sync_plan(&first);
        assert!(session.http.contains_key("http:h"));

        // 令牌变了：旧状态不再算数，同步后连接也被丢掉
        let changed = plan(3000, Some("new"));
        assert_eq!(session.status_for(&changed), None);
        session.sync_plan(&changed);
        assert!(session.http.is_empty());
        assert!(session.status.is_empty());
    }

    #[test]
    fn internal_endpoint_change_resets_too_old_and_catalog() {
        let mut session = BotSession::new(1);
        session.sync_internal_endpoint(6099, "tok");
        session.internal_too_old = true;
        session
            .status
            .insert(INTERNAL_KEY.into(), DebugChannelStatus::UpstreamTooOld);

        session.sync_internal_endpoint(6099, "tok");
        assert!(session.internal_too_old, "端点没变不应重置");

        session.sync_internal_endpoint(6100, "tok");
        assert!(!session.internal_too_old);
        assert!(session.status.is_empty());
    }

    #[test]
    fn bot_revision_change_forgets_what_we_knew_about_the_upstream() {
        let mut session = BotSession::new(1);
        session.internal_too_old = true;
        session.catalog_failed_at = Some(Instant::now());
        assert!(session.catalog_cooling_down());

        session.sync_bot_revision(1);
        assert!(session.internal_too_old, "revision 没变不应重置");

        session.sync_bot_revision(4);
        assert!(!session.internal_too_old);
        assert!(!session.catalog_cooling_down());
    }
}
