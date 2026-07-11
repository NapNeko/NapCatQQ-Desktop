//! 远端 Bot 磁盘日志增量 → bot_log_appended（SnowLuma bot_{qq}.log 等）
//!
//! 大文件（Bugly crash maps 可达数十 MB）禁止每 2s SFTP 整读。
//! 用 wc + 远端 tail 按偏移读增量；文件 rotate 变小则重置。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use ncd_backend_snowluma::SnowLumaLogNoiseFilter;
use ncd_host::{Host, HostCommand};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use crate::events::BroadcastEventBus;
use crate::native_deployment_adapter::EventBusSink;
use ncd_deploy::NativeRuntimeEventSink;
use ncd_domain::ids::BotId;

const POLL_SECS: u64 = 2;
/// 单次增量最多读这么多字节，防止异常暴涨一次打爆通道
const MAX_CHUNK: u64 = 512 * 1024;

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
