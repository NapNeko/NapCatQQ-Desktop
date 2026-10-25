//! 本机 ACP Agent 的有界 JSONL 通信与进程生命周期。

mod io_tasks;
mod process_group;
mod wire;

#[cfg(test)]
mod tests;

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::PathBuf,
    process::Stdio,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};

use serde_json::{Value, json};
use tokio::{
    process::Command,
    sync::{mpsc, oneshot},
    time,
};
use tokio_util::sync::CancellationToken;

use io_tasks::{drain_stderr, read_stdout, supervise, write_stdin};
use process_group::ProcessGroup;
use wire::RequestId;

const MAX_PENDING_REQUESTS: usize = 32;
const MAX_AGENT_REQUESTS: usize = 8;
const EVENT_QUEUE_CAPACITY: usize = 65;
const WRITE_QUEUE_CAPACITY: usize = 16;
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_STDERR_LINE_BYTES: usize = 64 * 1024;
const MAX_STDERR_TOTAL_BYTES: usize = 8 * 1024 * 1024;
const BLOCKED_PROCESS_ENV: &[&str] = &[
    "NODE_OPTIONS",
    "NODE_PATH",
    "BUN_OPTIONS",
    "PYTHONSTARTUP",
    "PYTHONPATH",
    "PYTHONHOME",
    "LD_PRELOAD",
    "DYLD_INSERT_LIBRARIES",
];

pub struct AcpLaunch {
    pub executable: PathBuf,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub cwd: PathBuf,
}

pub enum AcpEvent {
    Notification {
        method: String,
        params: Value,
    },
    Request {
        id: Value,
        method: String,
        params: Value,
    },
    Closed {
        reason: String,
    },
}

type PendingReply = oneshot::Sender<Result<Value, String>>;

struct Shared {
    closed: AtomicBool,
    next_id: AtomicU64,
    pending: Mutex<HashMap<i64, PendingReply>>,
    agent_requests: Mutex<HashSet<RequestId>>,
    event_gate: Mutex<()>,
    events: mpsc::Sender<AcpEvent>,
    stop: CancellationToken,
    stopped: CancellationToken,
    stdout_done: CancellationToken,
}

impl Shared {
    fn close(&self, reason: &str) {
        if self.closed.swap(true, Ordering::AcqRel) {
            return;
        }
        let pending = std::mem::take(
            &mut *self
                .pending
                .lock()
                .unwrap_or_else(|error| error.into_inner()),
        );
        for (_, sender) in pending {
            let _ = sender.send(Err(reason.to_owned()));
        }
        self.agent_requests
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clear();
        self.stop.cancel();
        let _gate = self
            .event_gate
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        // 数据事件始终留一个空位，保证满队列也能投递关闭原因。
        let _ = self.events.try_send(AcpEvent::Closed {
            reason: reason.to_owned(),
        });
    }

    fn publish(&self, event: AcpEvent) -> Result<(), String> {
        let gate = self
            .event_gate
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if self.closed.load(Ordering::Acquire) {
            return Err("Agent 连接已关闭".into());
        }
        if self.events.capacity() <= 1 || self.events.try_send(event).is_err() {
            drop(gate);
            self.close("Agent 事件过多或接收端已关闭");
            return Err("Agent 事件队列不可用".into());
        }
        Ok(())
    }
}

struct Outbound {
    bytes: Vec<u8>,
    request_id: Option<i64>,
    written: Option<oneshot::Sender<Result<(), String>>>,
}

pub struct AcpProcess {
    shared: Arc<Shared>,
    outgoing: mpsc::Sender<Outbound>,
}

struct PendingGuard {
    shared: Arc<Shared>,
    outgoing: mpsc::Sender<Outbound>,
    id: i64,
}

impl Drop for PendingGuard {
    fn drop(&mut self) {
        let removed = self
            .shared
            .pending
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(&self.id)
            .is_some();
        if removed && !self.shared.closed.load(Ordering::Acquire) {
            let cancellation = wire::encode(json!({
                "jsonrpc":"2.0", "method":"$/cancel_request", "params":{"requestId":self.id}
            }));
            let sent = cancellation.and_then(|bytes| {
                self.outgoing
                    .try_send(Outbound {
                        bytes,
                        request_id: None,
                        written: None,
                    })
                    .map_err(|_| "无法取消 Agent 请求".to_string())
            });
            if sent.is_err() {
                self.shared.close("Agent 请求取消失败");
            }
        }
    }
}

impl AcpProcess {
    pub async fn spawn(launch: AcpLaunch) -> Result<(Arc<Self>, mpsc::Receiver<AcpEvent>), String> {
        if !launch.executable.is_absolute() || !launch.cwd.is_absolute() {
            return Err("Agent 程序和工作目录必须使用绝对路径".into());
        }
        if launch.env.keys().any(|key| {
            BLOCKED_PROCESS_ENV
                .iter()
                .any(|blocked| key.eq_ignore_ascii_case(blocked))
        }) {
            return Err("不允许覆盖 Agent 的解释器注入环境变量".into());
        }
        let mut command = Command::new(&launch.executable);
        command
            .args(&launch.args)
            .envs(&launch.env)
            .current_dir(&launch.cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        // CLI 仍读取自己的配置；清掉能在协议握手前执行代码的继承变量。
        for key in BLOCKED_PROCESS_ENV {
            command.env_remove(key);
        }
        let group = ProcessGroup::prepare(&mut command)?;
        let mut child = command.spawn().map_err(|error| {
            format!(
                "无法启动 Agent（系统错误 {}）",
                error.raw_os_error().unwrap_or(0)
            )
        })?;
        if let Err(error) = group.attach(&child) {
            group.terminate();
            let _ = child.start_kill();
            let _ = time::timeout(SHUTDOWN_TIMEOUT, child.wait()).await;
            return Err(error);
        }
        let (stdin, stdout, stderr) =
            match (child.stdin.take(), child.stdout.take(), child.stderr.take()) {
                (Some(stdin), Some(stdout), Some(stderr)) => (stdin, stdout, stderr),
                _ => {
                    group.terminate();
                    let _ = child.start_kill();
                    let _ = time::timeout(SHUTDOWN_TIMEOUT, child.wait()).await;
                    return Err("无法建立 Agent 通信管道".into());
                }
            };
        let (events, receiver) = mpsc::channel(EVENT_QUEUE_CAPACITY);
        let (outgoing, writer_receiver) = mpsc::channel(WRITE_QUEUE_CAPACITY);
        let shared = Arc::new(Shared {
            closed: AtomicBool::new(false),
            next_id: AtomicU64::new(1),
            pending: Mutex::new(HashMap::new()),
            agent_requests: Mutex::new(HashSet::new()),
            event_gate: Mutex::new(()),
            events,
            stop: CancellationToken::new(),
            stopped: CancellationToken::new(),
            stdout_done: CancellationToken::new(),
        });
        tokio::spawn(read_stdout(stdout, Arc::clone(&shared)));
        tokio::spawn(write_stdin(stdin, writer_receiver, Arc::clone(&shared)));
        tokio::spawn(drain_stderr(stderr, Arc::clone(&shared)));
        tokio::spawn(supervise(child, group, Arc::clone(&shared)));
        Ok((Arc::new(Self { shared, outgoing }), receiver))
    }

    pub async fn request(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, String> {
        wire::validate_method(method)?;
        self.ensure_open()?;
        let raw_id = self
            .shared
            .next_id
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| {
                (id < 9_007_199_254_740_991).then_some(id + 1)
            })
            .map_err(|_| "Agent 请求编号已用尽".to_string())?;
        let id = i64::try_from(raw_id).map_err(|_| "Agent 请求编号无效".to_string())?;
        let bytes =
            wire::encode(json!({"jsonrpc":"2.0", "id":id, "method":method, "params":params}))?;
        let (sender, receiver) = oneshot::channel();
        {
            let mut pending = self
                .shared
                .pending
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            self.ensure_open()?;
            if pending.len() >= MAX_PENDING_REQUESTS {
                return Err("Agent 待处理请求过多".into());
            }
            pending.insert(id, sender);
        }
        let _guard = PendingGuard {
            shared: Arc::clone(&self.shared),
            outgoing: self.outgoing.clone(),
            id,
        };
        let operation = async {
            self.outgoing
                .send(Outbound {
                    bytes,
                    request_id: Some(id),
                    written: None,
                })
                .await
                .map_err(|_| "Agent 写入队列已关闭".to_string())?;
            receiver
                .await
                .map_err(|_| "Agent 请求未返回结果".to_string())?
        };
        match time::timeout(timeout, operation).await {
            Ok(result) => result,
            Err(_) => Err("Agent 请求超时".into()),
        }
    }

    pub async fn notify(&self, method: &str, params: Value) -> Result<(), String> {
        wire::validate_method(method)?;
        let bytes = wire::encode(json!({"jsonrpc":"2.0", "method":method, "params":params}))?;
        self.send_frame(bytes).await
    }

    pub async fn respond(&self, id: Value, result: Value) -> Result<(), String> {
        let parsed = RequestId::parse(&id)?;
        let bytes = wire::encode(json!({"jsonrpc":"2.0", "id":id, "result":result}))?;
        self.remove_agent_request(&parsed)?;
        self.send_frame(bytes).await
    }

    pub async fn respond_error(&self, id: Value, code: i32, message: &str) -> Result<(), String> {
        let parsed = RequestId::parse(&id)?;
        let bytes = wire::encode(
            json!({"jsonrpc":"2.0", "id":id, "error":{"code":code, "message":message}}),
        )?;
        self.remove_agent_request(&parsed)?;
        self.send_frame(bytes).await
    }

    pub async fn shutdown(&self) {
        self.shared.close("Agent 已停止");
        let _ = time::timeout(SHUTDOWN_TIMEOUT, self.shared.stopped.cancelled()).await;
    }

    pub fn is_closed(&self) -> bool {
        self.shared.closed.load(Ordering::Acquire)
    }

    fn ensure_open(&self) -> Result<(), String> {
        if self.is_closed() {
            Err("Agent 连接已关闭".into())
        } else {
            Ok(())
        }
    }

    fn remove_agent_request(&self, id: &RequestId) -> Result<(), String> {
        self.ensure_open()?;
        if !self
            .shared
            .agent_requests
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(id)
        {
            return Err("Agent 反向请求已处理或不存在".into());
        }
        Ok(())
    }

    async fn send_frame(&self, bytes: Vec<u8>) -> Result<(), String> {
        self.ensure_open()?;
        let (sender, receiver) = oneshot::channel();
        let result = time::timeout(WRITE_TIMEOUT, async {
            self.outgoing
                .send(Outbound {
                    bytes,
                    request_id: None,
                    written: Some(sender),
                })
                .await
                .map_err(|_| "Agent 写入队列已关闭".to_string())?;
            receiver
                .await
                .map_err(|_| "Agent 消息未能写入".to_string())?
        })
        .await;
        match result {
            Ok(Ok(())) => Ok(()),
            Ok(Err(error)) => {
                self.shared.close("Agent 消息写入失败");
                Err(error)
            }
            Err(_) => {
                self.shared.close("Agent 消息写入超时");
                Err("Agent 消息写入超时".into())
            }
        }
    }
}

impl Drop for AcpProcess {
    fn drop(&mut self) {
        self.shared.close("Agent 连接已释放");
    }
}
