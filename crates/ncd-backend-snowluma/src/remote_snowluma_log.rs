//! 远端 SnowLuma:按 SSH 主机共享 daemon.log 增量 → snowluma_daemon_log
//!
//! 与 RemoteBotLogFollowRegistry（per-bot bot_{qq}.log）分工。
//! 大文件禁止每轮 SFTP 整读。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use crate::snowluma::log_noise::SnowLumaLogNoiseFilter;
use ncd_host::{Host, HostCommand};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use ncd_domain::domain_event::DomainEvent;
use ncd_traits::events::{BroadcastEventBus, EventBus};

const POLL_SECS: u64 = 2;
const MAX_CHUNK: u64 = 512 * 1024;

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
            let mut last_size: u64 = 0;
            let mut filter = SnowLumaLogNoiseFilter::new();
            loop {
                tokio::time::sleep(Duration::from_secs(POLL_SECS)).await;
                let size = match remote_file_size(host.as_ref(), &log_path).await {
                    Some(s) => s,
                    None => continue,
                };
                if size < last_size {
                    last_size = 0;
                }
                if size == last_size {
                    continue;
                }
                let grow = size - last_size;
                let read_from = if last_size == 0 || grow > MAX_CHUNK {
                    size.saturating_sub(MAX_CHUNK)
                } else {
                    last_size
                };
                let chunk = match remote_read_from(host.as_ref(), &log_path, read_from).await {
                    Some(c) => c,
                    None => continue,
                };
                last_size = size;
                let text = String::from_utf8_lossy(&chunk);
                for line in text.lines() {
                    let Some(line) = filter.process_line(line) else {
                        continue;
                    };
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

async fn remote_file_size(host: &dyn Host, path: &str) -> Option<u64> {
    let quoted = shell_quote(path);
    let cmd = HostCommand::new("sh").arg("-c").arg(format!(
        "if [ -f {quoted} ]; then wc -c < {quoted}; else echo 0; fi"
    ));
    let out = host.run_to_string(cmd).await.ok()?;
    out.stdout.trim().parse().ok()
}

async fn remote_read_from(host: &dyn Host, path: &str, offset: u64) -> Option<Vec<u8>> {
    let quoted = shell_quote(path);
    let start = offset.saturating_add(1);
    let cmd = HostCommand::new("sh").arg("-c").arg(format!(
        "if [ -f {quoted} ]; then tail -c +{start} -- {quoted} | head -c {MAX_CHUNK}; fi"
    ));
    let out = host.run_to_string(cmd).await.ok()?;
    Some(out.stdout.into_bytes())
}

fn shell_quote(path: &str) -> String {
    format!("'{}'", path.replace('\'', "'\"'\"'"))
}
