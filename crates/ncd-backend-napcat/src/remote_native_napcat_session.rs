//! 远端 NapCat Native:SSH 隧道 + 远端 napcat_{qq}.log tail
//!
//! WebUI 端口/token 只从日志解析;隧道远端端口优先用日志里的真实 port,
//! 没有日志时不瞎开 6099(多实例会连错口)。reconcile attach 时必须立刻扫
//! 一遍已有日志,否则 UI 拿不到登录态/二维码。
//!
//! 多实例时 NapCat 会从 6099 起 +1;进程重启也会换 token。log task 必须在
//! (remote_port, token) 变化或本地转发失效时重建隧道并重发 NapCatWebuiAvailable。
//! 重建失败时清 endpoint/poller(via hook),避免 UI 继续打死后端口。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use ncd_domain::BotConfig;
use ncd_domain::bot_config::is_remote_native_napcat_config;
use ncd_host::remote::{TunnelHandle, TunnelSpec};
use ncd_host::{Host, HostCommand, HostError, HostPath};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio::time::timeout;
use tracing::warn;

use crate::remote_native_launch::{
    RemoteNapcatLayout, napcat_remote_log_path, probe_remote_napcat_layout,
};
use ncd_deploy::{EventBusSink, NapcatLogNoiseFilter, NativeRuntimeEventSink, parse_napcat_webui_line};
use ncd_domain::domain_event::DomainEvent;
use ncd_domain::ids::BotId;
use ncd_traits::events::{BroadcastEventBus, EventBus};

const LOG_TAIL_POLL_SECS: u64 = 2;
/// 每隔多少次 poll 做一次本机转发健康检查(约 6s)
const TUNNEL_HEALTH_EVERY_POLLS: u32 = 3;
const TUNNEL_HEALTH_TIMEOUT: Duration = Duration::from_secs(2);
/// 连续健康失败多少次才强制重建,避免慢 WebUI 抖动
const TUNNEL_HEALTH_FAIL_THRESHOLD: u32 = 3;

/// 本机 loopback 隧道当前状态;与 log_task 共享
#[derive(Default)]
struct TunnelSlot {
    handle: Option<TunnelHandle>,
    /// Desktop 本机可达口
    local_port: Option<u16>,
    /// 隧道远端目标口(Bot 真实 WebUI);None 表示无隧道
    remote_port: Option<u16>,
}

impl TunnelSlot {
    fn clear(&mut self) {
        self.handle = None;
        self.local_port = None;
        self.remote_port = None;
    }

    fn set_open(&mut self, handle: TunnelHandle, remote_port: u16) {
        self.local_port = Some(handle.local_port());
        self.remote_port = Some(remote_port);
        self.handle = Some(handle);
    }

    fn take_handle(&mut self) -> Option<TunnelHandle> {
        self.local_port = None;
        self.remote_port = None;
        self.handle.take()
    }
}

/// 上次成功对外发布的 (local, remote, token)
type PublishedWebui = (u16, u16, String);

/// 是否需要按新发现的远端 WebUI 口重建隧道
fn should_retunnel(slot_remote: Option<u16>, has_local_port: bool, discovered_remote: u16) -> bool {
    !has_local_port || slot_remote != Some(discovered_remote)
}

/// 是否需要再发 NapCatWebuiAvailable(本机口 / 远端口 / token 任一变化)
fn should_republish(
    last: Option<&PublishedWebui>,
    local_port: u16,
    remote_port: u16,
    token: &str,
) -> bool {
    match last {
        Some((lp, rp, t)) if *lp == local_port && *rp == remote_port && t == token => false,
        _ => true,
    }
}

/// 健康失败计数:连续失败达到阈值才强制 retunnel
fn health_force_retunnel(consecutive_fails: u32, threshold: u32) -> bool {
    consecutive_fails >= threshold
}

/// 本 tick 对隧道要做的动作(纯决策,便于单测)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TunnelAction {
    Keep,
    Retunnel,
}

fn decide_tunnel_action(
    slot_remote: Option<u16>,
    has_local_port: bool,
    discovered_remote: u16,
    force_health: bool,
) -> TunnelAction {
    if force_health || should_retunnel(slot_remote, has_local_port, discovered_remote) {
        TunnelAction::Retunnel
    } else {
        TunnelAction::Keep
    }
}

/// 隧道失效时清 endpoint + dispose poller;由 BotManager 注入,backend 不依赖 runtime
pub type WebuiUnreachableHook = Arc<dyn Fn(BotId) + Send + Sync>;

struct SessionInner {
    tunnel_slot: Arc<Mutex<TunnelSlot>>,
    log_task: Option<JoinHandle<()>>,
}

pub struct RemoteNativeNapcatSession {
    inner: Mutex<SessionInner>,
}

impl RemoteNativeNapcatSession {
    pub async fn shutdown(&self) {
        let mut inner = self.inner.lock().await;
        if let Some(h) = inner.log_task.take() {
            h.abort();
        }
        inner.tunnel_slot.lock().await.clear();
    }
}

pub struct RemoteNativeNapcatSessionRegistry {
    sessions: Mutex<HashMap<BotId, Arc<RemoteNativeNapcatSession>>>,
    on_webui_unreachable: Mutex<Option<WebuiUnreachableHook>>,
}

impl Default for RemoteNativeNapcatSessionRegistry {
    fn default() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            on_webui_unreachable: Mutex::new(None),
        }
    }
}

impl RemoteNativeNapcatSessionRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// 隧道建不起来 / 健康检查连续失败时回调(清 poller + endpoint)
    pub async fn set_on_webui_unreachable(&self, hook: WebuiUnreachableHook) {
        *self.on_webui_unreachable.lock().await = Some(hook);
    }

    pub async fn shutdown_bot(&self, bot_id: &BotId) {
        if let Some(s) = self.sessions.lock().await.remove(bot_id) {
            s.shutdown().await;
        }
    }

    pub async fn shutdown_all(&self) {
        let mut guard = self.sessions.lock().await;
        for (_, s) in guard.drain() {
            s.shutdown().await;
        }
    }

    pub async fn start_session(
        &self,
        bot_id: BotId,
        config: BotConfig,
        host: Arc<dyn Host>,
        bus: Arc<BroadcastEventBus>,
    ) {
        if !is_remote_native_napcat_config(&config) {
            return;
        }

        self.shutdown_bot(&bot_id).await;

        let qq_id = config.bot.qq_id;
        let unreachable_hook = self.on_webui_unreachable.lock().await.clone();

        let (home, layout) = match probe_remote_napcat_layout(host.as_ref()).await {
            Ok(p) => p,
            Err(e) => {
                warn!(
                    target: "ncd_backend_napcat::remote_native_napcat_session",
                    bot_id = %bot_id,
                    err = %e,
                    "NapCat 远端 Native: 布局探测失败，跳过会话"
                );
                return;
            }
        };

        let install_base = match layout {
            RemoteNapcatLayout::System => HostPath::from_posix("/"),
            RemoteNapcatLayout::Rootless => HostPath::from_posix(format!("{home}/Napcat")),
        };
        let log_path = napcat_remote_log_path(&install_base, qq_id);

        let existing_bytes = host
            .read_file(&HostPath::from_posix(&log_path))
            .await
            .unwrap_or_default();
        let seed = scan_latest_webui(&existing_bytes);

        let mut initial_slot = TunnelSlot::default();
        // 没有日志证据时不要回退开 6099:多实例/刚 rotate 会连到别人的口或空口
        if let Some((remote_webui_port, _)) = seed.as_ref() {
            match ensure_loopback_tunnel(host.as_ref(), *remote_webui_port).await {
                Ok(handle) => {
                    initial_slot.set_open(handle, *remote_webui_port);
                }
                Err(e) => {
                    warn!(
                        target: "ncd_backend_napcat::remote_native_napcat_session",
                        bot_id = %bot_id,
                        remote_port = *remote_webui_port,
                        err = %e,
                        "NapCat 远端 Native: WebUI 隧道建立失败"
                    );
                }
            }
        }

        let mut last_published: Option<PublishedWebui> = None;
        if let Some(local) = initial_slot.local_port {
            if let Some((remote_port, token)) = seed.as_ref() {
                bus.publish(DomainEvent::napcat_webui_available_remote(
                    bot_id.clone(),
                    local,
                    *remote_port,
                    token.clone(),
                ));
                last_published = Some((local, *remote_port, token.clone()));
            }
        } else if seed.is_some() {
            warn!(
                target: "ncd_backend_napcat::remote_native_napcat_session",
                bot_id = %bot_id,
                "NapCat 远端 Native: 日志已有 WebUI token,但本地 SSH 隧道未建立"
            );
            notify_unreachable(&unreachable_hook, &bot_id);
        }

        let sink = Arc::new(EventBusSink::new(bus.clone()));
        let host_log = Arc::clone(&host);
        let bot_log = bot_id.clone();
        let initial_size = existing_bytes.len();
        let mut known_webui: Option<(u16, String)> = seed.clone();

        let tunnel_slot = Arc::new(Mutex::new(initial_slot));
        let tunnel_for_task = Arc::clone(&tunnel_slot);

        let log_task = tokio::spawn(async move {
            let mut missing_tunnel_warned = false;
            let mut last_size: usize = initial_size;
            let mut poll_i: u32 = 0;
            let mut health_fails: u32 = 0;
            let mut noise = NapcatLogNoiseFilter::new();
            loop {
                tokio::time::sleep(Duration::from_secs(LOG_TAIL_POLL_SECS)).await;
                poll_i = poll_i.wrapping_add(1);
                let bytes = match host_log.read_file(&HostPath::from_posix(&log_path)).await {
                    Ok(b) => b,
                    Err(_) => continue,
                };
                if bytes.len() < last_size {
                    last_size = 0;
                    last_published = None;
                    known_webui = None;
                    health_fails = 0;
                    noise = NapcatLogNoiseFilter::new();
                }

                if bytes.len() > last_size {
                    let slice = &bytes[last_size..];
                    last_size = bytes.len();
                    let text = String::from_utf8_lossy(slice);
                    let mut latest = None;
                    for line in text.lines() {
                        // process_line: L1+L2；WebUI 解析用 Keep 行或对 Drop 行仍可 parse 半成品
                        // 先 parse 原文清洗结果，避免 L2 Drop 掉带 WebUI 的异常行（实际 WebUI 不会 Drop）
                        if let Some(pair) = parse_napcat_webui_line(line) {
                            latest = Some(pair);
                        }
                        let Some(cleaned) = noise.process_line(line) else {
                            continue;
                        };
                        sink.publish_log_line(&bot_log, &cleaned, "stdout");
                    }
                    if let Some(pair) = latest {
                        known_webui = Some(pair);
                    }
                } else if known_webui.is_none() {
                    if let Some(pair) = scan_latest_webui(&bytes) {
                        known_webui = Some(pair);
                        last_size = bytes.len();
                    }
                }

                let Some((remote_port, token_ref)) = known_webui.as_ref() else {
                    continue;
                };
                let remote_port = *remote_port;
                let token = token_ref.as_str();

                if poll_i % TUNNEL_HEALTH_EVERY_POLLS == 0 {
                    let local = {
                        let slot = tunnel_for_task.lock().await;
                        slot.local_port
                    };
                    match local {
                        Some(lp) if local_forward_healthy(lp).await => {
                            health_fails = 0;
                        }
                        Some(_) | None => {
                            health_fails = health_fails.saturating_add(1);
                        }
                    }
                }
                let force_health =
                    health_force_retunnel(health_fails, TUNNEL_HEALTH_FAIL_THRESHOLD);

                let (action, had_local) = {
                    let slot = tunnel_for_task.lock().await;
                    let action = decide_tunnel_action(
                        slot.remote_port,
                        slot.local_port.is_some(),
                        remote_port,
                        force_health,
                    );
                    (action, slot.local_port.is_some())
                };

                if action == TunnelAction::Retunnel {
                    let old = {
                        let mut slot = tunnel_for_task.lock().await;
                        slot.take_handle()
                    };
                    drop(old);

                    match ensure_loopback_tunnel(host_log.as_ref(), remote_port).await {
                        Ok(handle) => {
                            let local = handle.local_port();
                            {
                                let mut slot = tunnel_for_task.lock().await;
                                slot.set_open(handle, remote_port);
                            }
                            health_fails = 0;
                            if should_republish(last_published.as_ref(), local, remote_port, token)
                            {
                                bus.publish(DomainEvent::napcat_webui_available_remote(
                                    bot_log.clone(),
                                    local,
                                    remote_port,
                                    token.to_string(),
                                ));
                                last_published = Some((local, remote_port, token.to_string()));
                            }
                            missing_tunnel_warned = false;
                        }
                        Err(e) => {
                            warn!(
                                target: "ncd_backend_napcat::remote_native_napcat_session",
                                bot_id = %bot_log,
                                remote_port,
                                err = %e,
                                "NapCat 远端 Native: WebUI 隧道建立/重建失败"
                            );
                            last_published = None;
                            health_fails = 0;
                            if had_local || !missing_tunnel_warned {
                                notify_unreachable(&unreachable_hook, &bot_log);
                            }
                            if !missing_tunnel_warned {
                                warn!(
                                    target: "ncd_backend_napcat::remote_native_napcat_session",
                                    bot_id = %bot_log,
                                    "NapCat 远端 Native: 已发现 WebUI token,但本地 SSH 隧道未建立,跳过登录状态轮询"
                                );
                                missing_tunnel_warned = true;
                            }
                        }
                    }
                    continue;
                }

                let local = {
                    let slot = tunnel_for_task.lock().await;
                    slot.local_port
                };
                if let Some(local) = local {
                    if should_republish(last_published.as_ref(), local, remote_port, token) {
                        bus.publish(DomainEvent::napcat_webui_available_remote(
                            bot_log.clone(),
                            local,
                            remote_port,
                            token.to_string(),
                        ));
                        last_published = Some((local, remote_port, token.to_string()));
                        missing_tunnel_warned = false;
                    }
                }
            }
        });

        let session = Arc::new(RemoteNativeNapcatSession {
            inner: Mutex::new(SessionInner {
                tunnel_slot,
                log_task: Some(log_task),
            }),
        });
        self.sessions.lock().await.insert(bot_id, session);
    }
}

fn notify_unreachable(hook: &Option<WebuiUnreachableHook>, bot_id: &BotId) {
    if let Some(h) = hook {
        h(bot_id.clone());
    }
}

fn scan_latest_webui(bytes: &[u8]) -> Option<(u16, String)> {
    let text = String::from_utf8_lossy(bytes);
    let mut latest = None;
    for line in text.lines() {
        if let Some(pair) = parse_napcat_webui_line(line) {
            latest = Some(pair);
        }
    }
    latest
}

/// 远端 127.0.0.1:port 是否有人听;避免 bind 了本地口但 direct-tcpip ConnectFailed
async fn remote_loopback_listening(host: &dyn Host, remote_port: u16) -> bool {
    let script = format!(
        "bash -c 'echo >/dev/tcp/127.0.0.1/{remote_port}' 2>/dev/null \
         || (command -v ss >/dev/null 2>&1 && ss -lnt 2>/dev/null | grep -Eq ':{remote_port}([[:space:]]|$)') \
         || (command -v netstat >/dev/null 2>&1 && netstat -lnt 2>/dev/null | grep -Eq ':{remote_port}([[:space:]]|$)')"
    );
    let cmd = HostCommand::new("sh")
        .arg("-c")
        .arg(script)
        .timeout(Duration::from_secs(5));
    match host.run_to_string(cmd).await {
        Ok(out) => out.success(),
        Err(_) => false,
    }
}

/// 对本机转发口做极短探测:能 connect 且至少读到 1 字节才算通
async fn local_forward_healthy(local_port: u16) -> bool {
    let connect = timeout(
        TUNNEL_HEALTH_TIMEOUT,
        TcpStream::connect(("127.0.0.1", local_port)),
    )
    .await;
    let Ok(Ok(mut stream)) = connect else {
        return false;
    };
    let req = b"GET / HTTP/1.0\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";
    if timeout(TUNNEL_HEALTH_TIMEOUT, stream.write_all(req))
        .await
        .ok()
        .and_then(|r| r.ok())
        .is_none()
    {
        return false;
    }
    let mut buf = [0u8; 16];
    matches!(
        timeout(TUNNEL_HEALTH_TIMEOUT, stream.read(&mut buf)).await,
        Ok(Ok(n)) if n > 0
    )
}

async fn ensure_loopback_tunnel(
    host: &dyn Host,
    remote_port: u16,
) -> Result<TunnelHandle, HostError> {
    if !remote_loopback_listening(host, remote_port).await {
        // 目标口无人听,不是 SSH session 断线;勿用 remote_disconnected 以免误触发 host 自愈
        return Err(HostError::InvalidArgument {
            reason: format!("remote 127.0.0.1:{remote_port} not listening"),
        });
    }
    open_loopback_tunnel(host, remote_port).await
}

async fn open_loopback_tunnel(
    host: &dyn Host,
    remote_port: u16,
) -> Result<TunnelHandle, HostError> {
    let spec = TunnelSpec {
        local_host: "127.0.0.1".to_string(),
        local_port: 0,
        remote_host: "127.0.0.1".to_string(),
        remote_port,
    };
    host.open_tunnel(spec).await
}

#[cfg(test)]
mod tests {
    use super::{
        TunnelAction, decide_tunnel_action, health_force_retunnel, scan_latest_webui,
        should_republish, should_retunnel,
    };

    #[test]
    fn empty_log_returns_none() {
        assert!(scan_latest_webui(b"").is_none());
        assert!(scan_latest_webui(b"noise without panel url\n").is_none());
    }

    #[test]
    fn keeps_last_webui_line_when_multiple() {
        let log = b"\
[info] [NapCat] [WebUi] WebUi User Panel Url: http://127.0.0.1:6099/webui?token=first\n\
noise\n\
[info] [NapCat] [WebUi] WebUi User Panel Url: http://127.0.0.1:6101/webui?token=second\n\
";
        let (port, token) = scan_latest_webui(log).expect("should parse");
        assert_eq!(port, 6101);
        assert_eq!(token, "second");
    }

    #[test]
    fn loose_url_fragment_also_works() {
        let log = b"panel http://127.0.0.1:6123/webui?token=loose_tok trailing\n";
        let (port, token) = scan_latest_webui(log).expect("loose parse");
        assert_eq!(port, 6123);
        assert_eq!(token, "loose_tok");
    }

    #[test]
    fn should_retunnel_when_no_local_port() {
        assert!(should_retunnel(None, false, 6100));
    }

    #[test]
    fn should_retunnel_when_remote_port_changed() {
        assert!(should_retunnel(Some(6099), true, 6100));
    }

    #[test]
    fn should_not_retunnel_when_same_remote_and_alive() {
        assert!(!should_retunnel(Some(6100), true, 6100));
    }

    #[test]
    fn should_republish_when_token_changes() {
        let last = (50000_u16, 6100_u16, "old".to_string());
        assert!(should_republish(Some(&last), 50000, 6100, "new"));
    }

    #[test]
    fn should_not_republish_when_all_same() {
        let last = (50000_u16, 6100_u16, "tok".to_string());
        assert!(!should_republish(Some(&last), 50000, 6100, "tok"));
    }

    #[test]
    fn should_republish_when_local_port_changes() {
        let last = (50000_u16, 6100_u16, "tok".to_string());
        assert!(should_republish(Some(&last), 50001, 6100, "tok"));
    }

    #[test]
    fn should_republish_when_remote_port_changes() {
        let last = (50000_u16, 6099_u16, "tok".to_string());
        assert!(should_republish(Some(&last), 50000, 6100, "tok"));
    }

    #[test]
    fn health_force_only_after_threshold() {
        assert!(!health_force_retunnel(2, 3));
        assert!(health_force_retunnel(3, 3));
    }

    #[test]
    fn decide_retunnel_on_force_health_even_if_port_same() {
        assert_eq!(
            decide_tunnel_action(Some(6100), true, 6100, true),
            TunnelAction::Retunnel
        );
    }

    #[test]
    fn decide_keep_when_healthy_same_port() {
        assert_eq!(
            decide_tunnel_action(Some(6100), true, 6100, false),
            TunnelAction::Keep
        );
    }

    #[test]
    fn decide_retunnel_when_no_local() {
        assert_eq!(
            decide_tunnel_action(None, false, 6100, false),
            TunnelAction::Retunnel
        );
    }
}