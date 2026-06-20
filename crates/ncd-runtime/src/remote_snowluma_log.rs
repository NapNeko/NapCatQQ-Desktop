//! 远端 SnowLuma：按 SSH 主机共享 daemon.log 增量 → snowluma_daemon_log。
//!
//! 与 [RemoteBotLogFollowRegistry]（per-bot bot_{qq}.log）分工，避免在 bot_manager 里重复轮询逻辑。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use ncd_deploy::strip_ansi_escapes;
use ncd_host::{Host, HostPath};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use crate::events::{BroadcastEventBus, DomainEvent, EventBus};

const POLL_SECS: u64 = 2;

struct FollowInner {
    task: Option<JoinHandle<()>>,
}

pub struct RemoteSnowLumaDaemonLogFollow {
    inner: Mutex<FollowInner>,
}

impl RemoteSnowLumaDaemonLogFollow {
    pub async fn stop(&self) {
        let mut g = self.inner.lock().await;
        if let Some(t) = g.task.take() {
            t.abort();
        }
    }
}

pub struct RemoteSnowLumaLogRegistry {
    by_server: Mutex<HashMap<String, Arc<RemoteSnowLumaDaemonLogFollow>>>,
}

impl Default for RemoteSnowLumaLogRegistry {
    fn default() -> Self {
        Self {
            by_server: Mutex::new(HashMap::new()),
        }
    }
}

impl RemoteSnowLumaLogRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn stop_server(&self, server_id: &str) {
        if let Some(f) = self.by_server.lock().await.remove(server_id) {
            f.stop().await;
        }
    }

    pub async fn shutdown_all(&self) {
        let mut g = self.by_server.lock().await;
        for (_, f) in g.drain() {
            f.stop().await;
        }
    }

    /// 同一 server_id 只保留一个 follow（多 Bot 共用远端 daemon）。
    pub async fn start_daemon_follow_for_server(
        &self,
        server_id: &str,
        host: Arc<dyn Host>,
        log_path: String,
        bus: Arc<BroadcastEventBus>,
    ) {
        self.stop_server(server_id).await;

        let sid = server_id.to_string();
        let task = tokio::spawn(async move {
            let mut last_size: usize = 0;
            loop {
                tokio::time::sleep(Duration::from_secs(POLL_SECS)).await;
                let bytes = match host.read_file(&HostPath::from_posix(&log_path)).await {
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
                    let line = strip_ansi_escapes(line);
                    if line.is_empty() {
                        continue;
                    }
                    bus.publish(DomainEvent::snowluma_daemon_log(line));
                }
            }
        });

        let follow = Arc::new(RemoteSnowLumaDaemonLogFollow {
            inner: Mutex::new(FollowInner { task: Some(task) }),
        });
        self.by_server.lock().await.insert(sid, follow);
    }
}