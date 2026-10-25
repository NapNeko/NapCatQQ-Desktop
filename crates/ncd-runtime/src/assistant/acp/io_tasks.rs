//! ACP 管道的独立读写和退出收尾。

use std::{sync::Arc, time::Duration};

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStderr, ChildStdin, ChildStdout},
    sync::mpsc,
    time,
};
use tokio_util::sync::CancellationToken;

use super::{
    AcpEvent, MAX_AGENT_REQUESTS, MAX_STDERR_LINE_BYTES, MAX_STDERR_TOTAL_BYTES, Outbound,
    ProcessGroup, SHUTDOWN_TIMEOUT, Shared, WRITE_TIMEOUT,
    wire::{self, Incoming, RequestId},
};

pub(super) async fn read_stdout(stdout: ChildStdout, shared: Arc<Shared>) {
    let _done = CancelOnDrop(shared.stdout_done.clone());
    let mut reader = BufReader::new(stdout);
    loop {
        let frame = tokio::select! {
            biased;
            _ = shared.stop.cancelled() => return,
            frame = wire::read_frame(&mut reader) => frame,
        };
        let frame = match frame {
            Ok(Some(frame)) => frame,
            Ok(None) => {
                shared.close("Agent 输出管道已关闭");
                return;
            }
            Err(error) => {
                shared.close(&error);
                return;
            }
        };
        let message = match wire::parse(&frame) {
            Ok(message) => message,
            Err(error) => {
                shared.close(&error);
                return;
            }
        };
        if let Err(error) = dispatch_message(&shared, message) {
            shared.close(&error);
            return;
        }
        // 一次管道读取可能含大量短 token，给接收端消费事件的调度机会。
        tokio::task::yield_now().await;
    }
}

pub(super) fn dispatch_message(shared: &Shared, message: Incoming) -> Result<(), String> {
    match message {
        Incoming::Response { id, result } => {
            // 超时请求的迟到响应不应关闭仍可用的连接。
            if let RequestId::Number(id) = id {
                let sender = shared
                    .pending
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .remove(&id);
                if let Some(sender) = sender {
                    let _ = sender.send(result);
                }
            }
            Ok(())
        }
        Incoming::Notification { method, params } => {
            shared.publish(AcpEvent::Notification { method, params })
        }
        Incoming::Request { id, method, params } => {
            {
                let mut requests = shared
                    .agent_requests
                    .lock()
                    .unwrap_or_else(|error| error.into_inner());
                if requests.len() >= MAX_AGENT_REQUESTS {
                    return Err("Agent 反向请求过多".into());
                }
                if !requests.insert(id.clone()) {
                    return Err("Agent 重复使用了反向请求编号".into());
                }
            }
            shared.publish(AcpEvent::Request {
                id: id.value(),
                method,
                params,
            })
        }
    }
}

pub(super) async fn write_stdin(
    mut stdin: ChildStdin,
    mut receiver: mpsc::Receiver<Outbound>,
    shared: Arc<Shared>,
) {
    loop {
        let frame = tokio::select! {
            biased;
            _ = shared.stop.cancelled() => break,
            frame = receiver.recv() => match frame {
                Some(frame) => frame,
                None => break,
            },
        };
        if frame.request_id.is_some_and(|id| {
            !shared
                .pending
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .contains_key(&id)
        }) {
            continue;
        }
        let write = async {
            stdin.write_all(&frame.bytes).await?;
            stdin.flush().await
        };
        let result = tokio::select! {
            biased;
            _ = shared.stop.cancelled() => Err("Agent 连接已关闭".to_string()),
            result = time::timeout(WRITE_TIMEOUT, write) => match result {
                Ok(Ok(())) => Ok(()),
                Ok(Err(_)) => Err("Agent 输入管道写入失败".to_string()),
                Err(_) => Err("Agent 输入管道写入超时".to_string()),
            },
        };
        let failed = result.is_err();
        if let Some(sender) = frame.written {
            let _ = sender.send(result);
        }
        if failed {
            shared.close("Agent 输入管道不可用");
            break;
        }
    }
    receiver.close();
    while let Some(frame) = receiver.recv().await {
        if let Some(sender) = frame.written {
            let _ = sender.send(Err("Agent 连接已关闭".into()));
        }
    }
}

pub(super) async fn drain_stderr(mut stderr: ChildStderr, shared: Arc<Shared>) {
    let mut bytes = [0_u8; 4096];
    let mut line_bytes = 0_usize;
    let mut total_bytes = 0_usize;
    loop {
        let read = tokio::select! {
            biased;
            _ = shared.stop.cancelled() => return,
            read = stderr.read(&mut bytes) => read,
        };
        match read {
            Ok(0) => return,
            Ok(count) => {
                // stderr 可能包含登录票据、请求头或提示词，不持久化也不转发原文。
                total_bytes = total_bytes.saturating_add(count);
                for byte in &bytes[..count] {
                    line_bytes = if *byte == b'\n' {
                        0
                    } else {
                        line_bytes.saturating_add(1)
                    };
                    if line_bytes > MAX_STDERR_LINE_BYTES || total_bytes > MAX_STDERR_TOTAL_BYTES {
                        shared.close("Agent 错误输出超过限制");
                        return;
                    }
                }
            }
            Err(_) => {
                shared.close("Agent 错误输出管道不可用");
                return;
            }
        }
    }
}

pub(super) async fn supervise(mut child: Child, group: ProcessGroup, shared: Arc<Shared>) {
    let reason = tokio::select! {
        biased;
        _ = shared.stop.cancelled() => None,
        result = child.wait() => Some(match result {
            Ok(status) if status.success() => "Agent 进程已退出".to_string(),
            Ok(status) => match status.code() {
                Some(code) => format!("Agent 进程异常退出（退出码 {code}）"),
                None => "Agent 进程异常退出".to_string(),
            },
            Err(_) => "无法读取 Agent 进程状态".to_string(),
        }),
    };
    group.terminate();
    if let Some(reason) = reason {
        // 退出状态可能先于管道中最后一条响应到达，先给 reader 排空的机会。
        let _ = time::timeout(Duration::from_secs(1), shared.stdout_done.cancelled()).await;
        shared.close(&reason);
    }
    if child.try_wait().ok().flatten().is_none() {
        let _ = child.start_kill();
        let _ = time::timeout(SHUTDOWN_TIMEOUT, child.wait()).await;
    }
    shared.stopped.cancel();
}

struct CancelOnDrop(CancellationToken);

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}
