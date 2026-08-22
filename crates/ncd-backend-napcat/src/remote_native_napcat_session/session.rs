//! 远端 NapCat Native: SSH 隧道 + 远端 napcat_{qq}.log tail
//!
//! WebUI 端口/token 只从日志解析;隧道远端端口优先用日志里的真实 port。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use ncd_domain::BotConfig;
use ncd_domain::bot_config::is_remote_native_napcat_config;
use ncd_host::{Host, HostPath};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::warn;

use super::launch::{RemoteNapcatLayout, napcat_remote_log_path, probe_remote_napcat_layout};
use ncd_deploy::{
    EventBusSink, NapcatLogNoiseFilter, NativeRuntimeEventSink, parse_napcat_webui_line,
};
use ncd_domain::domain_event::DomainEvent;
use ncd_domain::ids::BotId;
use ncd_traits::events::{BroadcastEventBus, EventBus};

use super::decision::{
    PublishedWebui, TunnelAction, decide_tunnel_action, health_force_retunnel, should_republish,
};
use super::tunnel_io::{
    TunnelSlot, ensure_loopback_tunnel, local_forward_healthy, notify_unreachable,
    scan_latest_webui,
};

const LOG_TAIL_POLL_SECS: u64 = 2;
const TUNNEL_HEALTH_EVERY_POLLS: u32 = 3;
const TUNNEL_HEALTH_FAIL_THRESHOLD: u32 = 3;

/// 隧道失效时清 endpoint + dispose poller;由 BotManager 注入
pub use super::tunnel_io::WebuiUnreachableHook;

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
