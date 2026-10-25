//! uv 安装的流式诊断、等待状态和命令收尾。

use std::collections::VecDeque;
use std::future::Future;
use std::time::Duration;

use ncd_component::{ActionCtx, ActionError, ProgressKind};
use ncd_host::{CommandOutput, Host, HostCommand, HostError};
use tokio::sync::{mpsc, oneshot};
use tokio::time::Instant;

#[derive(Clone, Copy)]
struct MonitorPolicy {
    heartbeat: Duration,
    quiet_warning: Duration,
    timeout: Duration,
    flush_interval: Duration,
}

struct OutputRelay {
    tx: mpsc::Sender<String>,
    overflowed: bool,
}

impl OutputRelay {
    fn send(&mut self, line: String) {
        // 在 tracing、任务快照和 IPC 之前过滤，隐藏 UI 文本不能减少这些开销。
        if !visible_line(&line) {
            return;
        }
        if let Err(mpsc::error::TrySendError::Full(_)) = self.tx.try_send(line) {
            if !self.overflowed {
                self.overflowed = true;
                tracing::warn!("安装输出过多，省略未能及时回传的行");
            }
        }
    }
}

pub(crate) async fn run_uv_logged(
    host: &dyn Host,
    ctx: &ActionCtx,
    step: u32,
    name: &str,
    cmd: HostCommand,
    timeout: Duration,
) -> Result<CommandOutput, ActionError> {
    if ctx.is_cancelled() {
        return Err(ActionError::Cancelled);
    }
    let cmd = prepare_command(cmd, ctx, timeout);
    let (tx, rx) = mpsc::channel(1024);
    let mut relay = OutputRelay {
        tx,
        overflowed: false,
    };
    let run = host.run_streaming(
        cmd,
        Box::new(move |_source, line| {
            relay.send(line);
        }),
    );
    monitor_output(
        ctx,
        step,
        name,
        run,
        rx,
        MonitorPolicy {
            heartbeat: Duration::from_secs(15),
            quiet_warning: Duration::from_secs(120),
            timeout,
            flush_interval: Duration::from_millis(250),
        },
    )
    .await
}

fn prepare_command(mut cmd: HostCommand, ctx: &ActionCtx, timeout: Duration) -> HostCommand {
    let binary = cmd.program.rsplit(['/', '\\']).next().unwrap_or_default();
    if binary.eq_ignore_ascii_case("uv") || binary.eq_ignore_ascii_case("uv.exe") {
        // 非终端下的进度条不可用，等待状态由监测器定时报告。
        cmd = cmd
            .env("UV_NO_PROGRESS", "1")
            .env("UV_HTTP_CONNECT_TIMEOUT", "10")
            .env("UV_HTTP_TIMEOUT", "30")
            .env("UV_HTTP_RETRIES", "3")
            .env("UV_LOCK_TIMEOUT", "120");
    }
    cmd.timeout(timeout)
        .cancel_token(ctx.cancel_token())
        .env("PYTHONUTF8", "1")
}

struct StepMonitor<'a> {
    name: &'a str,
    phase: &'static str,
    last_output: Instant,
    warned: bool,
    phase_dirty: bool,
    pending_lines: VecDeque<String>,
    omitted_lines: usize,
    error_header: Option<String>,
    warning_header: Option<String>,
}

impl<'a> StepMonitor<'a> {
    fn new(name: &'a str) -> Self {
        Self {
            name,
            phase: "等待安装输出",
            last_output: Instant::now(),
            warned: false,
            phase_dirty: false,
            pending_lines: VecDeque::new(),
            omitted_lines: 0,
            error_header: None,
            warning_header: None,
        }
    }

    fn line(&mut self, line: &str) {
        if !visible_line(line) {
            return;
        }
        self.last_output = Instant::now();
        self.warned = false;
        let mut line = redact_line(line.trim_end());
        if let Some(phase) = phase_of(&line) {
            if phase != self.phase {
                self.phase = phase;
                self.phase_dirty = true;
            }
        }
        if line.len() > 1024 {
            let mut end = 1024;
            while !line.is_char_boundary(end) {
                end -= 1;
            }
            line.truncate(end);
            line.push('…');
        }
        if self.error_header.is_none() && is_error_header(&line) {
            self.error_header = Some(line.clone());
        }
        if self.warning_header.is_none() && is_warning_header(&line) {
            self.warning_header = Some(line.clone());
        }
        if self.pending_lines.len() == 8 {
            self.pending_lines.pop_front();
            self.omitted_lines += 1;
        }
        self.pending_lines.push_back(line);
    }

    async fn flush(&mut self, ctx: &ActionCtx, step: u32) {
        if self.phase_dirty {
            self.phase_dirty = false;
            self.progress(ctx, step, format!("{} · {}", self.name, self.phase))
                .await;
        }
        if !self.pending_lines.is_empty() {
            let mut message = self.pending_lines.drain(..).collect::<Vec<_>>().join("\n");
            for header in [self.warning_header.take(), self.error_header.take()]
                .into_iter()
                .flatten()
            {
                if !message.lines().any(|line| line == header) {
                    message = format!("{header}\n{message}");
                }
            }
            if self.omitted_lines > 0 {
                message = format!("省略 {} 行中间输出\n{message}", self.omitted_lines);
                self.omitted_lines = 0;
            }
            ctx.info(message).await;
        }
    }

    async fn tick(&mut self, ctx: &ActionCtx, step: u32, policy: MonitorPolicy) {
        let quiet = self.last_output.elapsed();
        let message = if quiet < policy.heartbeat {
            format!("{} · {}", self.name, self.phase)
        } else {
            format!(
                "{} · {} · {} 秒无新输出",
                self.name,
                self.phase,
                quiet.as_secs()
            )
        };
        self.progress(ctx, step, message).await;
        if quiet >= policy.quiet_warning && !self.warned {
            self.warned = true;
            ctx.warn("安装命令长时间没有新输出，可能仍在下载或编译。请检查安装主机的网络；可在设置 → 网络调整 Python 包源后重试。")
                .await;
        }
    }

    async fn progress(&self, ctx: &ActionCtx, step: u32, message: String) {
        ctx.emit(ProgressKind::StepProgress {
            step,
            percent: 0,
            message,
            speed_bps: None,
            downloaded_bytes: None,
            total_bytes: None,
            download_stage: None,
            docker_layers: None,
        })
        .await;
    }
}

fn trim_ansi_prefix(mut line: &str) -> &str {
    loop {
        line = line.trim_start();
        let Some(escape) = line.strip_prefix("\x1b[") else {
            return line;
        };
        let Some(end) = escape.find('m') else {
            return line;
        };
        line = &escape[end + 1..];
    }
}

fn is_diagnostic_line(line: &str) -> bool {
    let line = trim_ansi_prefix(line);
    let (first, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
    let token = if first.bytes().next().is_some_and(|c| c.is_ascii_digit()) && first.contains(':') {
        trim_ansi_prefix(rest)
            .split_whitespace()
            .next()
            .unwrap_or_default()
    } else {
        first
    };
    ["DEBUG", "TRACE", "[DEBUG]", "[TRACE]"]
        .iter()
        .any(|level| {
            token
                .strip_prefix(*level)
                .is_some_and(|tail| tail.is_empty() || tail.starts_with("\x1b["))
        })
}

fn is_package_listing(line: &str) -> bool {
    let line = trim_ansi_prefix(line);
    let Some(package) = line
        .strip_prefix("+ ")
        .or_else(|| line.strip_prefix("- "))
        .or_else(|| line.strip_prefix("~ "))
    else {
        return false;
    };
    let Some((name, _)) = package
        .split_once("==")
        .or_else(|| package.split_once(" @ "))
    else {
        return false;
    };
    !name.is_empty()
        && name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
}

fn visible_line(line: &str) -> bool {
    !line.trim().is_empty() && !is_diagnostic_line(line) && !is_package_listing(line)
}

fn is_error_header(line: &str) -> bool {
    let lower = trim_ansi_prefix(line).to_ascii_lowercase();
    lower.starts_with("error:") || lower.starts_with("fatal:") || lower.starts_with("error ")
}

fn is_warning_header(line: &str) -> bool {
    let lower = trim_ansi_prefix(line).to_ascii_lowercase();
    ["warning:", "warn:", "warning ", "warn "]
        .iter()
        .any(|prefix| lower.starts_with(*prefix))
}

fn clean_output(text: &str) -> String {
    text.lines()
        .filter(|line| visible_line(line))
        .map(redact_line)
        .collect::<Vec<_>>()
        .join("\n")
}

async fn monitor_output(
    ctx: &ActionCtx,
    step: u32,
    name: &str,
    run: impl Future<Output = Result<CommandOutput, HostError>>,
    rx: mpsc::Receiver<String>,
    policy: MonitorPolicy,
) -> Result<CommandOutput, ActionError> {
    let mut monitor = StepMonitor::new(name);
    let (finished, completion) = oneshot::channel();
    let result = {
        let reporter = report_output(&mut monitor, ctx, step, rx, policy, completion);
        tokio::pin!(reporter);
        // 上报进度可能被慢消费者阻塞；Host 的取消和 deadline 必须继续推进。
        let result = tokio::select! {
            result = run => result,
            _ = &mut reporter => return Err(ActionError::install_step(name, "安装日志通道意外结束")),
        };
        let _ = finished.send(());
        if !matches!(result, Err(HostError::Cancelled)) && !ctx.is_cancelled() {
            let cancel = ctx.cancel_token();
            tokio::select! {
                _ = &mut reporter => {},
                _ = cancel.cancelled() => {},
                _ = tokio::time::sleep(Duration::from_secs(2)) => {
                    tracing::warn!("安装进度消费者未响应，停止等待尾日志回传");
                },
            }
        }
        result
    };
    match result {
        Ok(mut output) => {
            output.stdout = clean_output(&output.stdout);
            output.stderr = clean_output(&output.stderr);
            Ok(output)
        }
        Err(HostError::Cancelled) => Err(ActionError::Cancelled),
        Err(HostError::Timeout { .. }) => Err(ActionError::install_step(
            name,
            format!(
                "超过 {} 秒，最后阶段：{}。请检查安装主机网络、Python 包源或其他安装任务后重试。",
                policy.timeout.as_secs(),
                monitor.phase
            ),
        )),
        Err(error) => Err(ActionError::install_step(
            name,
            redact_line(&error.to_string()),
        )),
    }
}

async fn report_output(
    monitor: &mut StepMonitor<'_>,
    ctx: &ActionCtx,
    step: u32,
    mut rx: mpsc::Receiver<String>,
    policy: MonitorPolicy,
    mut completion: oneshot::Receiver<()>,
) {
    let mut tick = tokio::time::interval_at(Instant::now() + policy.heartbeat, policy.heartbeat);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut flush = tokio::time::interval_at(
        Instant::now() + policy.flush_interval,
        policy.flush_interval,
    );
    flush.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            _ = &mut completion => {
                rx.close();
                while let Some(line) = rx.recv().await {
                    monitor.line(&line);
                }
                monitor.flush(ctx, step).await;
                return;
            },
            Some(line) = rx.recv() => monitor.line(&line),
            _ = flush.tick() => monitor.flush(ctx, step).await,
            _ = tick.tick() => monitor.tick(ctx, step, policy).await,
        }
    }
}

fn phase_of(line: &str) -> Option<&'static str> {
    let lower = line.to_ascii_lowercase();
    if lower.contains("waiting") && lower.contains("lock") {
        Some("等待安装锁")
    } else if lower.contains("building") || lower.contains("built ") {
        Some("编译依赖")
    } else if lower.contains("installing") || lower.contains("installed ") {
        Some("安装依赖")
    } else if lower.contains("no cache entry")
        || lower.contains("sending") && lower.contains("request")
    {
        Some("读取包索引")
    } else if lower.contains("downloading") || lower.contains("fetching") {
        Some("下载依赖")
    } else if lower.contains("solving")
        || lower.contains("resolving")
        || lower.contains("selecting")
        || lower.contains("searching for")
        || lower.contains("metadata")
        || lower.contains("sending request")
        || lower.contains("resolved ")
    {
        Some("解析依赖")
    } else {
        None
    }
}

fn redact_line(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = ["https://", "http://", "socks5://", "socks5h://"]
        .iter()
        .filter_map(|scheme| rest.find(scheme))
        .min()
    {
        result.push_str(&rest[..start]);
        rest = &rest[start..];
        let end = rest
            .find(|c: char| c.is_whitespace() || matches!(c, '\'' | '"' | '<' | '>' | ')'))
            .unwrap_or(rest.len());
        let candidate = &rest[..end];
        if let Ok(mut url) = reqwest::Url::parse(candidate) {
            let _ = url.set_username("");
            let _ = url.set_password(None);
            url.set_query(None);
            url.set_fragment(None);
            result.push_str(url.as_str());
        } else {
            result.push_str("[已隐藏的下载地址]");
        }
        rest = &rest[end..];
    }
    result.push_str(rest);
    result
}

#[cfg(test)]
mod tests;
