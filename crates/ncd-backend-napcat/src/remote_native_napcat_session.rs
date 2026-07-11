//! 远端 NapCat Native:SSH 隧道 + 远端 napcat_{qq}.log tail
//!
//! WebUI 端口/token 只从日志解析;隧道远端端口优先用日志里的真实 port,
//! 没有日志时才回退 6099。reconcile attach 时必须立刻扫一遍已有日志,
//! 否则 UI 拿不到登录态/二维码。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use ncd_deploy::parse_napcat_webui_line;
use ncd_domain::BotConfig;
use ncd_domain::bot_config::is_remote_native_napcat_config;
use ncd_host::remote::{TunnelHandle, TunnelSpec};
use ncd_host::{Host, HostError, HostPath};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::warn;

use crate::remote_native_launch::{
    RemoteNapcatLayout, napcat_remote_log_path, probe_remote_napcat_layout,
};
use ncd_deploy::{EventBusSink, NativeRuntimeEventSink};
use ncd_domain::domain_event::DomainEvent;
use ncd_domain::ids::BotId;
use ncd_traits::events::{BroadcastEventBus, EventBus};

const REMOTE_NAPCAT_WEBUI_PORT_FALLBACK: u16 = 6099;
const LOG_TAIL_POLL_SECS: u64 = 2;

struct SessionInner {
    tunnel: Option<TunnelHandle>,
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
        inner.tunnel = None;
    }
}

pub struct RemoteNativeNapcatSessionRegistry {
    sessions: Mutex<HashMap<BotId, Arc<RemoteNativeNapcatSession>>>,
}

impl Default for RemoteNativeNapcatSessionRegistry {
    fn default() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
        }
    }
}

impl RemoteNativeNapcatSessionRegistry {
    pub fn new() -> Self {
        Self::default()
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

        let (home, layout) = match probe_remote_napcat_layout(host.as_ref()).await {
            Ok(p) => p,
            Err(e) => {
                warn!(
                    target: "ncd_runtime::remote_native_napcat_session",
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

        // reconcile / 二次 attach 时日志里通常已有 WebUI 行;先扫全量拿最新 port+token
        let existing_bytes = host
            .read_file(&HostPath::from_posix(&log_path))
            .await
            .unwrap_or_default();
        let seed = scan_latest_webui(&existing_bytes);
        let remote_webui_port = seed
            .as_ref()
            .map(|(port, _)| *port)
            .unwrap_or(REMOTE_NAPCAT_WEBUI_PORT_FALLBACK);

        let mut tunnel = None;
        let mut local_port = None;
        match open_loopback_tunnel(host.as_ref(), remote_webui_port).await {
            Ok(handle) => {
                local_port = Some(handle.local_port());
                tunnel = Some(handle);
            }
            Err(e) => {
                warn!(
                    target: "ncd_runtime::remote_native_napcat_session",
                    bot_id = %bot_id,
                    remote_port = remote_webui_port,
                    err = %e,
                    "NapCat 远端 Native: WebUI 隧道建立失败"
                );
            }
        }

        // 立刻发布,避免等 2s poll 才出二维码/登录态
        // 只在需要 owned token 时 clone,不整包 clone seed
        if let Some(port) = local_port {
            if let Some((_, token)) = seed.as_ref() {
                bus.publish(DomainEvent::napcat_webui_available(
                    bot_id.clone(),
                    port,
                    token.clone(),
                ));
            }
        } else if seed.is_some() {
            warn!(
                target: "ncd_runtime::remote_native_napcat_session",
                bot_id = %bot_id,
                "NapCat 远端 Native: 日志已有 WebUI token,但本地 SSH 隧道未建立"
            );
        }

        let sink = Arc::new(EventBusSink::new(bus.clone()));
        let host_log = Arc::clone(&host);
        let bot_log = bot_id.clone();
        let lp = local_port;
        let initial_size = existing_bytes.len();
        let seed_published = local_port.is_some() && seed.is_some();

        let log_task = tokio::spawn(async move {
            let mut missing_tunnel_warned = false;
            let mut last_size: usize = initial_size;
            let mut webui_published = seed_published;
            loop {
                tokio::time::sleep(Duration::from_secs(LOG_TAIL_POLL_SECS)).await;
                let bytes = match host_log.read_file(&HostPath::from_posix(&log_path)).await {
                    Ok(b) => b,
                    Err(_) => continue,
                };
                if bytes.len() < last_size {
                    // 日志被 rotate/截断,整文件重扫
                    last_size = 0;
                    webui_published = false;
                }
                let slice = if bytes.len() > last_size {
                    &bytes[last_size..]
                } else {
                    continue;
                };
                last_size = bytes.len();
                let text = String::from_utf8_lossy(slice);
                let mut latest = None;
                for line in text.lines() {
                    sink.publish_log_line(&bot_log, line, "stdout");
                    if let Some(pair) = parse_napcat_webui_line(line) {
                        latest = Some(pair);
                    }
                }
                if webui_published {
                    continue;
                }
                if let Some((_remote_port, token)) = latest {
                    if let Some(port) = lp {
                        bus.publish(DomainEvent::napcat_webui_available(
                            bot_log.clone(),
                            port,
                            token,
                        ));
                        webui_published = true;
                    } else if !missing_tunnel_warned {
                        warn!(
                            target: "ncd_runtime::remote_native_napcat_session",
                            bot_id = %bot_log,
                            "NapCat 远端 Native: 已发现 WebUI token,但本地 SSH 隧道未建立,跳过登录状态轮询"
                        );
                        missing_tunnel_warned = true;
                    }
                }
            }
        });

        let session = Arc::new(RemoteNativeNapcatSession {
            inner: Mutex::new(SessionInner {
                tunnel,
                log_task: Some(log_task),
            }),
        });
        self.sessions.lock().await.insert(bot_id, session);
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

#[cfg(test)]
mod scan_latest_webui_tests {
    use super::scan_latest_webui;

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
