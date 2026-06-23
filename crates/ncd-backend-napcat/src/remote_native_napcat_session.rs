//! 远端 NapCat Native:SSH 隧道 6099 + 远端 napcat_{qq}.log tail(WebUI 仅从日志解析 token)

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

use ncd_domain::domain_event::DomainEvent;
use ncd_domain::ids::BotId;
use ncd_deploy::{EventBusSink, NativeRuntimeEventSink};
use ncd_traits::events::{BroadcastEventBus, EventBus};
use crate::remote_native_launch::{napcat_remote_log_path, probe_remote_napcat_layout, RemoteNapcatLayout};

const REMOTE_NAPCAT_WEBUI_PORT: u16 = 6099;
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

    /// fallback_token 已废弃:勿用 Docker secret 冒充 NapCat 进程 token
    pub async fn start_session(
        &self,
        bot_id: BotId,
        config: BotConfig,
        host: Arc<dyn Host>,
        bus: Arc<BroadcastEventBus>,
    ) {
        let _ = ();
        if !is_remote_native_napcat_config(&config) {
            return;
        }

        self.shutdown_bot(&bot_id).await;

        let qq_id = config.bot.qq_id;
        let mut tunnel = None;
        let mut local_port = None;

        match open_loopback_tunnel(host.as_ref(), REMOTE_NAPCAT_WEBUI_PORT).await {
            Ok(handle) => {
                local_port = Some(handle.local_port());
                tunnel = Some(handle);
            }
            Err(e) => {
                warn!(
                    target: "ncd_runtime::remote_native_napcat_session",
                    bot_id = %bot_id,
                    err = %e,
                    "NapCat 远端 Native: WebUI 隧道建立失败"
                );
            }
        }

        let (home, layout) = match probe_remote_napcat_layout(host.as_ref()).await {
            Ok(p) => p,
            Err(e) => {
                warn!(
                    target: "ncd_runtime::remote_native_napcat_session",
                    bot_id = %bot_id,
                    err = %e,
                    "NapCat 远端 Native: 布局探测失败，跳过日志 tail"
                );
                let session = Arc::new(RemoteNativeNapcatSession {
                    inner: Mutex::new(SessionInner {
                        tunnel,
                        log_task: None,
                    }),
                });
                self.sessions.lock().await.insert(bot_id, session);
                return;
            }
        };

        let install_base = match layout {
            RemoteNapcatLayout::System => HostPath::from_posix("/"),
            RemoteNapcatLayout::Rootless => HostPath::from_posix(format!("{home}/Napcat")),
        };
        let log_path = napcat_remote_log_path(&install_base, qq_id);

        let sink = Arc::new(EventBusSink::new(bus.clone()));
        let host_log = Arc::clone(&host);
        let bot_log = bot_id.clone();
        let lp = local_port;

        let log_task = tokio::spawn(async move {
            let mut webui_published = false;
            let mut last_size: usize = 0;
            loop {
                tokio::time::sleep(Duration::from_secs(LOG_TAIL_POLL_SECS)).await;
                let bytes = match host_log.read_file(&HostPath::from_posix(&log_path)).await {
                    Ok(b) => b,
                    Err(_) => continue,
                };
                if bytes.len() < last_size {
                    last_size = 0;
                }
                let slice = if bytes.len() > last_size {
                    &bytes[last_size..]
                } else {
                    continue;
                };
                last_size = bytes.len();
                let text = String::from_utf8_lossy(slice);
                for line in text.lines() {
                    sink.publish_log_line(&bot_log, line, "stdout");
                    if webui_published {
                        continue;
                    }
                    if let Some((_remote_port, token)) = parse_napcat_webui_line(line) {
                        let port = lp.unwrap_or(REMOTE_NAPCAT_WEBUI_PORT);
                        bus.publish(DomainEvent::napcat_webui_available(
                            bot_log.clone(),
                            port,
                            token,
                        ));
                        webui_published = true;
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

async fn open_loopback_tunnel(host: &dyn Host, remote_port: u16) -> Result<TunnelHandle, HostError> {
    let spec = TunnelSpec {
        local_host: "127.0.0.1".to_string(),
        local_port: 0,
        remote_host: "127.0.0.1".to_string(),
        remote_port,
    };
    host.open_tunnel(spec).await
}