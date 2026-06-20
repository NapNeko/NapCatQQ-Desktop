//! 远端 Bot 磁盘日志增量 tail → bot_log_appended（SnowLuma bot_{qq}.log 等）。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use ncd_deploy::strip_ansi_escapes;
use ncd_host::{Host, HostPath};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use crate::events::BroadcastEventBus;
use crate::ids::BotId;
use crate::native_deployment_adapter::EventBusSink;
use ncd_deploy::NativeRuntimeEventSink;

const POLL_SECS: u64 = 2;

struct FollowInner {
    task: Option<JoinHandle<()>>,
}

pub struct RemoteBotLogFollow {
    inner: Mutex<FollowInner>,
}

impl RemoteBotLogFollow {
    pub async fn stop(&self) {
        let mut g = self.inner.lock().await;
        if let Some(t) = g.task.take() {
            t.abort();
        }
    }
}

pub struct RemoteBotLogFollowRegistry {
    by_bot: Mutex<HashMap<BotId, Arc<RemoteBotLogFollow>>>,
}

impl Default for RemoteBotLogFollowRegistry {
    fn default() -> Self {
        Self {
            by_bot: Mutex::new(HashMap::new()),
        }
    }
}

impl RemoteBotLogFollowRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn stop_bot(&self, bot_id: &BotId) {
        if let Some(f) = self.by_bot.lock().await.remove(bot_id) {
            f.stop().await;
        }
    }

    pub async fn shutdown_all(&self) {
        let mut g = self.by_bot.lock().await;
        for (_, f) in g.drain() {
            f.stop().await;
        }
    }

    pub async fn start_bot_log(
        &self,
        bot_id: BotId,
        host: Arc<dyn Host>,
        log_path: String,
        bus: Arc<BroadcastEventBus>,
    ) {
        self.stop_bot(&bot_id).await;

        let sink = Arc::new(EventBusSink::new(bus));
        let bid = bot_id.clone();
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
                    sink.publish_log_line(&bid, &line, "stdout");
                }
            }
        });

        let follow = Arc::new(RemoteBotLogFollow {
            inner: Mutex::new(FollowInner { task: Some(task) }),
        });
        self.by_bot.lock().await.insert(bot_id, follow);
    }
}