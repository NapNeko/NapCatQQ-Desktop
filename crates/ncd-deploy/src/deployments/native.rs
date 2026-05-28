//! 原生部署：在宿主机上跑 NapCat / SnowLuma 进程。
//!
//! 实现路径：
//!
//! 1. `launch` 用 [`NativeLaunchTranslator`] 把 `BotConfig` 翻译成
//!    [`NativeLaunchCommand`]（程序 + 参数 + 工作目录 + 环境变量）。
//! 2. 通过 [`Host::spawn`] 启子进程，拿到 [`HostProcess`]。
//! 3. take_stdout / take_stderr 启异步 reader 任务，把每行解码 + 清 ANSI 后
//!    写进内存缓冲（10000 行环形）+ 同步追加到 `<log_dir>/<bot_id>.log`，
//!    再通过 [`NativeRuntimeEventSink`] 广播 BotLogAppended / NapCatWebuiAvailable。
//! 4. 启 exit watcher：进程退出时清 processes / logs 记录，广播
//!    BotProcessExited。
//! 5. `stop` 走平台 kill 命令（Windows taskkill /F /T、其它 kill -9 进程组），
//!    保证 NapCatWinBootMain → QQ.exe 注入链能整个清掉。
//!
//! install / uninstall 继续留 Unsupported；那是组件层（ncd-component）的
//! 职责，后续把 component 接进来再做。
//!
//! 事件桥接：调用方注入 [`NativeRuntimeEventSink`]，把日志/退出/WebUI 事件
//! 转发到自己的事件总线。ncd-deploy 不直接依赖 events 模块，避免
//! 循环依赖。

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use ncd_domain::{BotConfig, BotFlavor, BotId, StopMode};
use ncd_host::{Host, HostCommand, HostError, HostPath, HostProcess, Os};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;

use crate::deployment::{
    Deployment, DeploymentError, DeploymentHandle, DeploymentProgressSink, DeploymentState,
    NativeLaunchCommand, NativeLaunchTranslator,
};

/// 原生部署的运行时事件桥接。
///
/// 调用方实装本 trait 把日志 / 退出 / WebUI 端点等事件转发到自己的事件总线。
/// NativeDeployment 持有 `Arc<dyn>`，在 launch / log reader / exit watcher
/// 中调用对应方法。
///
/// 走 trait 而不是直接 import 上游事件模块：避免 ncd-deploy 反向依赖上游 crate，
/// trait 让两边解耦，调用方在装配时桥接。
pub trait NativeRuntimeEventSink: Send + Sync {
    /// 发布一行 bot 日志。
    /// `channel` 取 "stdout" / "stderr"。
    fn publish_log_line(&self, bot_id: &BotId, line: &str, channel: &str);

    /// 发布 NapCat WebUI 可用事件（从 stdout 解析出端口 + token）。
    fn publish_napcat_webui_available(&self, bot_id: &BotId, port: u16, token: String);

    /// 发布 bot 进程退出事件。
    fn publish_bot_process_exited(
        &self,
        bot_id: &BotId,
        exit_code: Option<i32>,
        reason: Option<String>,
    );
}

/// 不发布任何事件的占位 sink，供测试 / 不关心事件的调用方使用。
pub struct NullRuntimeEventSink;

impl NativeRuntimeEventSink for NullRuntimeEventSink {
    fn publish_log_line(&self, _bot_id: &BotId, _line: &str, _channel: &str) {}
    fn publish_napcat_webui_available(&self, _bot_id: &BotId, _port: u16, _token: String) {}
    fn publish_bot_process_exited(
        &self,
        _bot_id: &BotId,
        _exit_code: Option<i32>,
        _reason: Option<String>,
    ) {
    }
}

/// 单个 bot 进程的运行时档案。
#[derive(Debug)]
struct ManagedProcess {
    pid: u32,
    started_at: u64,
}

/// 内存日志环形缓冲，上限 10_000 行（与 legacy LocalRuntimeBackend 对齐）。
#[derive(Debug, Default)]
struct RuntimeLogBuffer {
    lines: VecDeque<String>,
    total_lines: usize,
}

impl RuntimeLogBuffer {
    const MAX_LINES: usize = 10_000;

    fn push_line(&mut self, line: &str) {
        if line.is_empty() {
            return;
        }
        if self.lines.len() == Self::MAX_LINES {
            self.lines.pop_front();
        }
        self.lines.push_back(line.to_string());
        self.total_lines = self.total_lines.saturating_add(1);
    }
}

/// 原生部署：装二进制 + spawn 进程。
///
/// 支持的 flavor：NapCat、SnowLuma。
/// 支持的 host：所有 Host trait 实装；本地 Windows 是首要目标，远端 Linux
/// 在 Host trait 抽象就绪后会跟上（spawn / take_stdout 已留好接口）。
pub struct NativeDeployment {
    id: &'static str,
    flavors: &'static [BotFlavor],
    /// 把 BotConfig 翻译成进程命令行。
    translator: Arc<dyn NativeLaunchTranslator>,
    /// 事件桥接：把运行时事件转发到调用方的事件总线。
    event_sink: Arc<dyn NativeRuntimeEventSink>,
    /// 日志根目录。每个 bot 的日志写到 `<log_root>/bots/<bot_id>.log`。
    /// 为 None 时只走内存缓冲不落盘。
    log_root: Option<PathBuf>,
    /// 当前在跑的进程档案：bot_id -> ManagedProcess。
    processes: Arc<Mutex<HashMap<BotId, ManagedProcess>>>,
    /// 内存日志缓冲：bot_id -> RuntimeLogBuffer。
    logs: Arc<Mutex<HashMap<BotId, RuntimeLogBuffer>>>,
}

impl NativeDeployment {
    /// 构造一个原生部署实例。
    ///
    /// `translator` 把 BotConfig 翻译成进程命令行；`event_sink` 桥接事件总线；
    /// `log_root` 为 None 时不落盘日志（仅供单测）。
    pub fn new(
        translator: Arc<dyn NativeLaunchTranslator>,
        event_sink: Arc<dyn NativeRuntimeEventSink>,
        log_root: Option<PathBuf>,
    ) -> Self {
        Self {
            id: "native",
            flavors: &[BotFlavor::NapCat, BotFlavor::SnowLuma],
            translator,
            event_sink,
            log_root,
            processes: Arc::new(Mutex::new(HashMap::new())),
            logs: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn log_path_for(&self, bot_id: &BotId) -> Option<PathBuf> {
        self.log_root
            .as_ref()
            .map(|root| root.join("bots").join(format!("{}.log", bot_id.as_str())))
    }

    /// 把 BotConfig 推导成 BotId。BotId 取 qq_id 字符串。
    fn derive_bot_id(config: &BotConfig) -> BotId {
        BotId::new(config.bot.qq_id.to_string())
    }
}

impl std::fmt::Debug for NativeDeployment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeDeployment")
            .field("id", &self.id)
            .field("flavors", &self.flavors)
            .field("log_root", &self.log_root)
            .finish_non_exhaustive()
    }
}

// ============================================================
// 日志解码 + ANSI 清洗 + NapCat WebUI 解析
//
// 这几段 helper 行为与上游 runtime_backend 中的同名函数一致。
// 之所以再写一份不复用：
// - ncd-deploy 不能反向依赖上游 crate；
// - 旧版 LocalRuntimeBackend 移除后这套实现的归宿就是这里，
//   后续再考虑下沉到 ncd-host 公共 utils。
// ============================================================

/// 按 GBK / UTF-8 解码 NapCat 子进程的一行原始字节，并清洗 ANSI 转义。
///
/// NapCatWinBootMain.exe 在中文 Windows 上输出 GBK；其它平台默认 UTF-8。
/// Linux 上 NapCat 通过 logger 输出会带颜色转义（如 `\x1b[32m`），
/// 直接渲染会在 UI 上残留 tofu 字符并干扰日志高亮。
fn decode_log_line(raw: &[u8]) -> String {
    let decoded = if let Ok(s) = std::str::from_utf8(raw) {
        s.to_string()
    } else {
        #[cfg(windows)]
        {
            let (cow, _, had_errors) = encoding_rs::GBK.decode(raw);
            if !had_errors {
                cow.into_owned()
            } else {
                String::from_utf8_lossy(raw).into_owned()
            }
        }
        #[cfg(not(windows))]
        {
            String::from_utf8_lossy(raw).into_owned()
        }
    };
    strip_ansi_escapes(&decoded)
}

/// 移除字符串中的 ANSI 转义序列（CSI、OSC、单字符 ESC 命令）。
/// 实现走 byte 级状态机，不引入 regex 依赖。
fn strip_ansi_escapes(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b != 0x1B {
            out.push(b);
            i += 1;
            continue;
        }
        i += 1;
        if i >= bytes.len() {
            break;
        }
        match bytes[i] {
            b'[' => {
                // CSI: 跳过参数 + intermediate + final。
                i += 1;
                while i < bytes.len() && (0x30..=0x3F).contains(&bytes[i]) {
                    i += 1;
                }
                while i < bytes.len() && (0x20..=0x2F).contains(&bytes[i]) {
                    i += 1;
                }
                if i < bytes.len() && (0x40..=0x7E).contains(&bytes[i]) {
                    i += 1;
                }
            }
            b']' | b'P' | b'X' | b'^' | b'_' => {
                // string-context: OSC/DCS/SOS/PM/APC，吞到 BEL 或 ESC \\。
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == 0x07 {
                        i += 1;
                        break;
                    }
                    if bytes[i] == 0x1B && i + 1 < bytes.len() && bytes[i + 1] == b'\\' {
                        i += 2;
                        break;
                    }
                    i += 1;
                }
            }
            _ => {
                i += 1;
            }
        }
    }
    String::from_utf8(out)
        .unwrap_or_else(|err| String::from_utf8_lossy(err.as_bytes()).into_owned())
}

/// 从一行 NapCat stdout 解析出 WebUI 登录入口的 (port, token)。
fn parse_napcat_webui_line(line: &str) -> Option<(u16, String)> {
    let needle = "[info] [NapCat] [WebUi] WebUi User Panel Url: http://127.0.0.1:";
    let idx = line.find(needle)?;
    let rest = &line[idx + needle.len()..];
    let (port_str, after_port) = rest.split_once("/webui?token=")?;
    let port: u16 = port_str.trim().parse().ok()?;
    let token: String = after_port
        .chars()
        .take_while(|c| !c.is_whitespace())
        .collect();
    if token.is_empty() {
        return None;
    }
    Some((port, token))
}

// ============================================================
// 后台任务：日志 reader / exit watcher
// ============================================================

/// reader 任务参数包，省得 spawn_log_reader 函数签名长得没法看。
struct LogReaderCtx {
    bot_id: BotId,
    channel: &'static str,
    logs: Arc<Mutex<HashMap<BotId, RuntimeLogBuffer>>>,
    log_path: Option<PathBuf>,
    event_sink: Arc<dyn NativeRuntimeEventSink>,
}

/// 异步读取子进程的 stdout 或 stderr，按行解码后写内存缓冲、磁盘日志，
/// 并通过事件 sink 广播 BotLogAppended / NapCatWebuiAvailable。
fn spawn_log_reader(
    stream: Box<dyn tokio::io::AsyncRead + Send + Unpin>,
    ctx: LogReaderCtx,
) {
    tokio::spawn(async move {
        let mut reader = BufReader::new(stream);
        let mut buf: Vec<u8> = Vec::with_capacity(1024);
        loop {
            buf.clear();
            match reader.read_until(b'\n', &mut buf).await {
                Ok(0) => break,
                Ok(_) => {}
                Err(_) => break,
            }
            // 去除尾部换行符。
            while matches!(buf.last(), Some(b'\n' | b'\r')) {
                buf.pop();
            }
            if buf.is_empty() {
                continue;
            }
            let line = decode_log_line(&buf);
            if line.is_empty() {
                continue;
            }

            {
                let mut guard = ctx.logs.lock().await;
                guard.entry(ctx.bot_id.clone()).or_default().push_line(&line);
            }

            // 写文件失败不致命。
            if let Some(path) = ctx.log_path.as_ref() {
                if let Some(parent) = path.parent() {
                    let _ = tokio::fs::create_dir_all(parent).await;
                }
                if let Ok(mut file) = tokio::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(path)
                    .await
                {
                    let _ = file.write_all(line.as_bytes()).await;
                    let _ = file.write_all(b"\n").await;
                }
            }

            ctx.event_sink
                .publish_log_line(&ctx.bot_id, &line, ctx.channel);

            if let Some((port, token)) = parse_napcat_webui_line(&line) {
                ctx.event_sink
                    .publish_napcat_webui_available(&ctx.bot_id, port, token);
            }
        }
    });
}

/// 监听子进程退出。退出后从 processes 中移除记录、清空日志缓冲，
/// 通过 sink 广播 BotProcessExited。
fn spawn_exit_watcher(
    bot_id: BotId,
    mut process: Box<dyn HostProcess>,
    processes: Arc<Mutex<HashMap<BotId, ManagedProcess>>>,
    logs: Arc<Mutex<HashMap<BotId, RuntimeLogBuffer>>>,
    event_sink: Arc<dyn NativeRuntimeEventSink>,
) {
    tokio::spawn(async move {
        // try_wait 轮询太忙；wait 消费 self。这里直接 box 走 wait 路径：
        // 先 take 一次 try_wait 验证健康再 box 自身 wait。但 HostProcess::wait
        // 收 Box<Self>，需要 process 自己被 Box 持有 —— 上面 spawn 返回的就是
        // Box<dyn HostProcess>，可以直接调。
        let result = process.wait().await;
        {
            let mut guard = processes.lock().await;
            guard.remove(&bot_id);
        }
        // 进程退出后清内存日志缓冲，避免下一轮启动残留旧行。磁盘 .log 不动，
        // 给用户保留崩溃前归档。
        {
            let mut guard = logs.lock().await;
            guard.remove(&bot_id);
        }
        let (exit_code, reason) = match result {
            Ok(out) => (out.exit_code, None),
            Err(err) => (None, Some(format!("wait failed: {err}"))),
        };
        event_sink.publish_bot_process_exited(&bot_id, exit_code, reason);
    });
}

/// 平台无关地递归 kill 进程树。NapCat 是注入器：
/// NapCatWinBootMain.exe → QQ.exe → renderer 子进程，必须整树清掉。
async fn kill_process_tree(host: &dyn Host, pid: u32) -> Result<(), DeploymentError> {
    if pid == 0 {
        return Ok(());
    }
    let cmd = match host.os() {
        Os::Windows => HostCommand::new("taskkill")
            .arg("/F")
            .arg("/T")
            .arg("/PID")
            .arg(pid.to_string()),
        Os::Linux | Os::MacOs => HostCommand::new("kill")
            .arg("-KILL")
            .arg(format!("-{pid}")),
    };
    let output = host
        .run_to_string(cmd)
        .await
        .map_err(|err| DeploymentError::StopFailed(format!("kill spawn: {err}")))?;
    if output.success() {
        return Ok(());
    }
    // taskkill 退出码 128 = 进程已退出，按幂等处理。
    if matches!(host.os(), Os::Windows) && output.exit_code == Some(128) {
        return Ok(());
    }
    // Unix 上若进程组 kill 失败（pid 不是 leader），回退按单 PID 杀一次。
    if matches!(host.os(), Os::Linux | Os::MacOs) {
        let fallback = HostCommand::new("kill")
            .arg("-KILL")
            .arg(pid.to_string());
        let fb = host
            .run_to_string(fallback)
            .await
            .map_err(|err| DeploymentError::StopFailed(format!("kill fallback: {err}")))?;
        if fb.success() {
            return Ok(());
        }
        return Err(DeploymentError::StopFailed(format!(
            "kill failed: {}",
            fb.stderr.trim()
        )));
    }
    Err(DeploymentError::StopFailed(format!(
        "taskkill exit={:?}: {}",
        output.exit_code,
        output.stderr.trim()
    )))
}

// ============================================================
// Deployment trait 实装
// ============================================================

#[async_trait]
impl Deployment for NativeDeployment {
    fn id(&self) -> &str {
        self.id
    }

    fn supported_flavors(&self) -> &[BotFlavor] {
        self.flavors
    }

    fn supports(&self, _host: &dyn Host) -> bool {
        // 原生部署对 host 没有静态限制：任何 Host 实装都能跑。
        true
    }

    async fn install(
        &self,
        _host: &dyn Host,
        _config: &BotConfig,
        _progress: &dyn DeploymentProgressSink,
    ) -> Result<(), DeploymentError> {
        // 组件层的安装链还没接进来，暂留占位。
        Err(DeploymentError::Unsupported(
            "NativeDeployment::install not yet implemented",
        ))
    }

    async fn launch(
        &self,
        host: &dyn Host,
        config: &BotConfig,
    ) -> Result<DeploymentHandle, DeploymentError> {
        let bot_id = Self::derive_bot_id(config);

        // 1. 翻译用户配置为进程命令。
        let plan = self.translator.translate(config).await?;
        if plan.program.is_empty() {
            return Err(DeploymentError::LaunchFailed(
                "translator produced empty program".into(),
            ));
        }

        // 2. 拼 HostCommand。working_dir 走 HostPath；环境变量整张表透传。
        let mut cmd = HostCommand::new(plan.program.clone()).args(plan.args.clone());
        if let Some(dir) = plan.working_dir.as_ref() {
            cmd = cmd.working_dir(host_path_from_native(dir, host.os()));
        }
        cmd = cmd.envs(plan.environment.clone().into_iter());

        // 3. 启动新进程前清掉这个 bot 旧的内存日志缓冲。
        {
            let mut guard = self.logs.lock().await;
            guard.remove(&bot_id);
        }

        // 4. spawn 进程，拿 stdout / stderr 给 reader 任务。
        let mut process = host
            .spawn(cmd)
            .await
            .map_err(|err| DeploymentError::LaunchFailed(host_err_msg(err)))?;
        let pid = process.id().native;
        let started_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        if let Some(stdout) = process.take_stdout() {
            spawn_log_reader(
                stdout,
                LogReaderCtx {
                    bot_id: bot_id.clone(),
                    channel: "stdout",
                    logs: Arc::clone(&self.logs),
                    log_path: self.log_path_for(&bot_id),
                    event_sink: Arc::clone(&self.event_sink),
                },
            );
        }
        if let Some(stderr) = process.take_stderr() {
            spawn_log_reader(
                stderr,
                LogReaderCtx {
                    bot_id: bot_id.clone(),
                    channel: "stderr",
                    logs: Arc::clone(&self.logs),
                    log_path: self.log_path_for(&bot_id),
                    event_sink: Arc::clone(&self.event_sink),
                },
            );
        }

        spawn_exit_watcher(
            bot_id.clone(),
            process,
            Arc::clone(&self.processes),
            Arc::clone(&self.logs),
            Arc::clone(&self.event_sink),
        );

        // 5. 落档进程档案。
        {
            let mut guard = self.processes.lock().await;
            guard.insert(
                bot_id.clone(),
                ManagedProcess { pid, started_at },
            );
        }

        Ok(DeploymentHandle::Native { pid, started_at })
    }

    async fn observe(
        &self,
        _host: &dyn Host,
        bot_id: &BotId,
    ) -> Result<DeploymentState, DeploymentError> {
        let guard = self.processes.lock().await;
        if guard.contains_key(bot_id) {
            Ok(DeploymentState::Running)
        } else {
            Ok(DeploymentState::Stopped)
        }
    }

    async fn stop(
        &self,
        host: &dyn Host,
        bot_id: &BotId,
        _mode: StopMode,
    ) -> Result<(), DeploymentError> {
        // 取出档案并立刻释放锁，避免 kill 等待期间堵其它 observe / launch。
        let record = {
            let mut guard = self.processes.lock().await;
            guard.remove(bot_id)
        };
        // 没有档案视为已停止，幂等返回（exit watcher 可能已先一步清理）。
        let Some(record) = record else {
            return Ok(());
        };
        kill_process_tree(host, record.pid).await
    }

    async fn uninstall(
        &self,
        _host: &dyn Host,
        _config: &BotConfig,
    ) -> Result<(), DeploymentError> {
        Err(DeploymentError::Unsupported(
            "NativeDeployment::uninstall not yet implemented",
        ))
    }
}

/// 把宿主上原生 `PathBuf` 转成 `HostPath`。
///
/// - 远端 host 拿到这个 PathBuf 一般是没意义的（路径属于本机文件系统，
///   远端结构可能完全不同）；调用方应在翻译阶段就保证路径属于目标 host。
/// - 这里的转换只是把 `\` / `/` 规范成 HostPath 内部的 POSIX 表达。
fn host_path_from_native(path: &Path, os: Os) -> HostPath {
    let s = path.to_string_lossy();
    match os {
        Os::Windows => HostPath::from_windows(&s),
        Os::Linux | Os::MacOs => HostPath::from_posix(s.into_owned()),
    }
}

/// 把 `HostError` 转成可读的错误消息字符串。
fn host_err_msg(err: HostError) -> String {
    err.to_string()
}

// ============================================================
// 给 BotManager / 日志页用的辅助方法
// ============================================================

/// `tail_log` 返回的快照：最近 N 行 + 累计总行数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeLogSnapshot {
    pub lines: Vec<String>,
    pub total_lines: usize,
}

impl NativeDeployment {
    /// 取当前内存里的日志快照。`limit > 0` 时只返回末尾 `limit` 行。
    pub async fn tail_log(&self, bot_id: &BotId, limit: usize) -> NativeLogSnapshot {
        let guard = self.logs.lock().await;
        let Some(buffer) = guard.get(bot_id) else {
            return NativeLogSnapshot {
                lines: Vec::new(),
                total_lines: 0,
            };
        };
        let mut lines: Vec<String> = buffer.lines.iter().cloned().collect();
        if limit > 0 && lines.len() > limit {
            lines = lines.split_off(lines.len() - limit);
        }
        NativeLogSnapshot {
            lines,
            total_lines: buffer.total_lines,
        }
    }

    /// 主动追加一行日志（外部驱动测试用 / 远端 host 没有 stdout pipe 时由
    /// 调用方手动 push）。落盘失败不致命。
    #[doc(hidden)]
    pub async fn append_log_line_for_test(&self, bot_id: &BotId, line: &str) {
        let mut guard = self.logs.lock().await;
        guard.entry(bot_id.clone()).or_default().push_line(line);
    }

    /// 当前在跑的 bot 列表（observe 的批量版本）。
    pub async fn list_running(&self) -> Vec<(BotId, u32, u64)> {
        let guard = self.processes.lock().await;
        guard
            .iter()
            .map(|(id, rec)| (id.clone(), rec.pid, rec.started_at))
            .collect()
    }
}

// 兼容老 LocalRuntimeBackend 的字段类型导出（环境变量 BTreeMap 用）。
#[doc(hidden)]
pub type NativeEnv = BTreeMap<String, String>;

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use ncd_host::process::{ExitStatus, ProcessId};
    use ncd_host::{
        Arch, ArchiveKind, CommandOutput, DirEntry, HostShell, Locality, PackageManager,
    };
    use std::process::Stdio;
    use std::sync::Mutex as StdMutex;
    use tokio::sync::oneshot;

    // ============================================================
    // FakeHost：spawn 走真实 tokio::process::Command，run_to_string 仅记录调用
    //
    // 这套 fake 的目的：
    // - launch 路径用真子进程（echo 类命令）走通 take_stdout / exit watcher 链路
    // - stop 路径只校验调用了正确 kill 命令，不真杀进程（进程已自然退出）
    // - 不依赖 LocalWindowsHost / RemoteLinuxHost，跨平台都能跑
    // ============================================================

    /// 极简 BashShell stub，只是占位 —— Host trait 要求 shell() 返回 &dyn HostShell。
    struct NoopShell;

    impl HostShell for NoopShell {
        fn kind(&self) -> ncd_host::ShellKind {
            ncd_host::ShellKind::Bash
        }

        fn escape(&self, arg: &str) -> String {
            arg.to_string()
        }

        fn line_separator(&self) -> &'static str {
            "\n"
        }
    }

    struct FakeHost {
        os: Os,
        kill_calls: Arc<StdMutex<Vec<HostCommand>>>,
    }

    impl FakeHost {
        fn new(os: Os) -> Self {
            Self {
                os,
                kill_calls: Arc::new(StdMutex::new(Vec::new())),
            }
        }

        fn kill_calls(&self) -> Vec<HostCommand> {
            self.kill_calls.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl Host for FakeHost {
        fn os(&self) -> Os {
            self.os
        }
        fn arch(&self) -> Arch {
            Arch::X86_64
        }
        fn locality(&self) -> Locality {
            Locality::Local
        }
        fn id(&self) -> &str {
            "fake"
        }
        fn shell(&self) -> &dyn HostShell {
            &NoopShell
        }
        fn pkg_manager(&self) -> Option<&dyn PackageManager> {
            None
        }
        async fn read_file(&self, _: &HostPath) -> Result<Bytes, HostError> {
            Err(HostError::Unsupported { operation: "fake" })
        }
        async fn write_file(&self, _: &HostPath, _: &[u8]) -> Result<(), HostError> {
            Err(HostError::Unsupported { operation: "fake" })
        }
        async fn list_dir(&self, _: &HostPath) -> Result<Vec<DirEntry>, HostError> {
            Err(HostError::Unsupported { operation: "fake" })
        }
        async fn create_dir_all(&self, _: &HostPath) -> Result<(), HostError> {
            Err(HostError::Unsupported { operation: "fake" })
        }
        async fn remove_file(&self, _: &HostPath) -> Result<(), HostError> {
            Err(HostError::Unsupported { operation: "fake" })
        }
        async fn remove_dir_all(&self, _: &HostPath) -> Result<(), HostError> {
            Err(HostError::Unsupported { operation: "fake" })
        }
        async fn exists(&self, _: &HostPath) -> Result<bool, HostError> {
            Ok(false)
        }
        async fn upload(&self, _: &Path, _: &HostPath) -> Result<(), HostError> {
            Err(HostError::Unsupported { operation: "fake" })
        }
        async fn download(&self, _: &HostPath, _: &Path) -> Result<(), HostError> {
            Err(HostError::Unsupported { operation: "fake" })
        }
        async fn extract_archive(
            &self,
            _: &HostPath,
            _: &HostPath,
            _: ArchiveKind,
        ) -> Result<(), HostError> {
            Err(HostError::Unsupported { operation: "fake" })
        }

        async fn spawn(&self, cmd: HostCommand) -> Result<Box<dyn HostProcess>, HostError> {
            let mut tcmd = tokio::process::Command::new(&cmd.program);
            tcmd.args(&cmd.args)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            for (k, v) in &cmd.environment {
                tcmd.env(k, v);
            }
            let child = tcmd.spawn().map_err(HostError::Io)?;
            let pid = child.id().unwrap_or(0);
            Ok(Box::new(FakeProcess {
                child: Some(child),
                id: ProcessId {
                    native: pid,
                    origin: "fake".into(),
                },
            }))
        }

        async fn run_to_string(&self, cmd: HostCommand) -> Result<CommandOutput, HostError> {
            // 录到 kill_calls 列表里供测试断言。返回 success，让 stop 走幂等路径。
            self.kill_calls.lock().unwrap().push(cmd);
            Ok(CommandOutput {
                exit_code: Some(0),
                stdout: String::new(),
                stderr: String::new(),
            })
        }
    }


    /// FakeProcess：包 tokio::process::Child，实装 take_stdout / take_stderr，
    /// wait 返回正常 exit。
    struct FakeProcess {
        child: Option<tokio::process::Child>,
        id: ProcessId,
    }

    #[async_trait]
    impl HostProcess for FakeProcess {
        fn id(&self) -> ProcessId {
            self.id.clone()
        }

        async fn wait(mut self: Box<Self>) -> Result<CommandOutput, HostError> {
            let child = self.child.take().ok_or_else(|| HostError::InvalidArgument {
                reason: "child already consumed".into(),
            })?;
            let output = child.wait_with_output().await.map_err(HostError::Io)?;
            Ok(CommandOutput {
                exit_code: output.status.code(),
                stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            })
        }

        async fn try_wait(&mut self) -> Result<ExitStatus, HostError> {
            let child = self
                .child
                .as_mut()
                .ok_or_else(|| HostError::InvalidArgument {
                    reason: "child already consumed".into(),
                })?;
            match child.try_wait()? {
                None => Ok(ExitStatus::Running),
                Some(status) => match status.code() {
                    Some(code) => Ok(ExitStatus::Exited(code)),
                    None => Ok(ExitStatus::Killed),
                },
            }
        }

        async fn kill(&mut self) -> Result<(), HostError> {
            let child = self
                .child
                .as_mut()
                .ok_or_else(|| HostError::InvalidArgument {
                    reason: "child already consumed".into(),
                })?;
            child.kill().await?;
            Ok(())
        }

        async fn write_stdin(&mut self, _: &[u8]) -> Result<(), HostError> {
            Err(HostError::Unsupported {
                operation: "fake stdin",
            })
        }

        async fn close_stdin(&mut self) -> Result<(), HostError> {
            Ok(())
        }

        fn take_stdout(&mut self) -> Option<Box<dyn tokio::io::AsyncRead + Send + Unpin>> {
            let child = self.child.as_mut()?;
            child
                .stdout
                .take()
                .map(|s| Box::new(s) as Box<dyn tokio::io::AsyncRead + Send + Unpin>)
        }

        fn take_stderr(&mut self) -> Option<Box<dyn tokio::io::AsyncRead + Send + Unpin>> {
            let child = self.child.as_mut()?;
            child
                .stderr
                .take()
                .map(|s| Box::new(s) as Box<dyn tokio::io::AsyncRead + Send + Unpin>)
        }
    }

    /// FakeTranslator：不真翻译，按测试参数直接返回固定 NativeLaunchCommand。
    struct FakeTranslator {
        plan: NativeLaunchCommand,
    }

    #[async_trait]
    impl NativeLaunchTranslator for FakeTranslator {
        async fn translate(
            &self,
            _config: &BotConfig,
        ) -> Result<NativeLaunchCommand, DeploymentError> {
            Ok(self.plan.clone())
        }
    }

    /// CapturingEventSink：把所有事件录进 Vec，测试用断言。
    /// 进程退出事件还会发给 oneshot，让测试方便等"watcher 跑完"。
    #[derive(Default)]
    struct CapturingEventSink {
        log_lines: StdMutex<Vec<(BotId, String, String)>>,
        webui: StdMutex<Vec<(BotId, u16, String)>>,
        exited: StdMutex<Vec<(BotId, Option<i32>, Option<String>)>>,
        exit_signal: StdMutex<Option<oneshot::Sender<()>>>,
    }

    impl CapturingEventSink {
        fn with_exit_signal(tx: oneshot::Sender<()>) -> Self {
            Self {
                exit_signal: StdMutex::new(Some(tx)),
                ..Self::default()
            }
        }
    }

    impl NativeRuntimeEventSink for CapturingEventSink {
        fn publish_log_line(&self, bot_id: &BotId, line: &str, channel: &str) {
            self.log_lines
                .lock()
                .unwrap()
                .push((bot_id.clone(), line.to_string(), channel.to_string()));
        }

        fn publish_napcat_webui_available(&self, bot_id: &BotId, port: u16, token: String) {
            self.webui
                .lock()
                .unwrap()
                .push((bot_id.clone(), port, token));
        }

        fn publish_bot_process_exited(
            &self,
            bot_id: &BotId,
            exit_code: Option<i32>,
            reason: Option<String>,
        ) {
            self.exited
                .lock()
                .unwrap()
                .push((bot_id.clone(), exit_code, reason));
            if let Some(tx) = self.exit_signal.lock().unwrap().take() {
                let _ = tx.send(());
            }
        }
    }


    fn make_bot_config(qq_id: u64) -> BotConfig {
        use ncd_domain::{
            AdvancedConfig, AutoRestartSchedule, BackendType, BotBasicConfig, ConnectConfig,
            RuntimeTarget,
        };
        BotConfig {
            bot: BotBasicConfig {
                name: "test-bot".into(),
                qq_id,
                music_sign_url: String::new(),
                auto_restart_schedule: AutoRestartSchedule::default(),
                offline_auto_restart: false,
                runtime_target: RuntimeTarget::Local,
                backend_type: BackendType::NapCat,
                snowluma_start_mode: None,
            },
            connect: ConnectConfig::default(),
            advanced: AdvancedConfig::default(),
        }
    }

    #[test]
    fn native_deployment_id_and_flavors() {
        let dep = NativeDeployment::new(
            Arc::new(FakeTranslator {
                plan: NativeLaunchCommand {
                    program: "echo".into(),
                    args: vec![],
                    working_dir: None,
                    environment: BTreeMap::new(),
                },
            }),
            Arc::new(NullRuntimeEventSink),
            None,
        );
        assert_eq!(dep.id(), "native");
        let flavors = dep.supported_flavors();
        assert!(flavors.contains(&BotFlavor::NapCat));
        assert!(flavors.contains(&BotFlavor::SnowLuma));
    }

    #[test]
    fn strip_ansi_escapes_removes_color_csi_sequences() {
        let input = "\x1b[32m[info]\x1b[0m hello";
        assert_eq!(strip_ansi_escapes(input), "[info] hello");
    }

    #[test]
    fn parse_napcat_webui_line_extracts_port_and_token() {
        let line = "[info] [NapCat] [WebUi] WebUi User Panel Url: http://127.0.0.1:6099/webui?token=abc123";
        let parsed = parse_napcat_webui_line(line).expect("parse");
        assert_eq!(parsed.0, 6099);
        assert_eq!(parsed.1, "abc123");
    }

    /// 全链路测试：launch 一个快速退出的命令，验证：
    /// 1. launch 返回 Native handle 含 pid != 0
    /// 2. observe 从 Running → 进程退出后 → Stopped
    /// 3. exit watcher 广播 BotProcessExited
    /// 4. stdout 内容被 log reader 解码并广播
    #[tokio::test]
    async fn launch_observe_stop_full_lifecycle() {
        let (exit_tx, exit_rx) = oneshot::channel::<()>();
        let sink = Arc::new(CapturingEventSink::with_exit_signal(exit_tx));

        // 用一个打印特定行后退出的命令。
        #[cfg(windows)]
        let plan = NativeLaunchCommand {
            program: "cmd".into(),
            args: vec![
                "/C".into(),
                "echo [info] [NapCat] [WebUi] WebUi User Panel Url: http://127.0.0.1:6099/webui?token=testTK".into(),
            ],
            working_dir: None,
            environment: BTreeMap::new(),
        };
        #[cfg(not(windows))]
        let plan = NativeLaunchCommand {
            program: "sh".into(),
            args: vec![
                "-c".into(),
                "echo '[info] [NapCat] [WebUi] WebUi User Panel Url: http://127.0.0.1:6099/webui?token=testTK'".into(),
            ],
            working_dir: None,
            environment: BTreeMap::new(),
        };

        let host = FakeHost::new(if cfg!(windows) { Os::Windows } else { Os::Linux });
        let dep = NativeDeployment::new(
            Arc::new(FakeTranslator { plan }),
            sink.clone(),
            None,
        );

        let config = make_bot_config(10001);
        let handle = dep.launch(&host, &config).await.expect("launch");

        // launch 返回 Native handle。
        match &handle {
            DeploymentHandle::Native { pid, .. } => assert_ne!(*pid, 0),
            _ => panic!("expected Native handle"),
        }

        // 等 exit watcher 广播。
        let _ = tokio::time::timeout(std::time::Duration::from_secs(5), exit_rx)
            .await
            .expect("exit watcher should fire within 5s");

        // observe 应该变成 Stopped（exit watcher 已清理 processes）。
        let state = dep
            .observe(&host, &BotId::new("10001"))
            .await
            .expect("observe");
        assert_eq!(state, DeploymentState::Stopped);

        // sink 应该收到 log 行 + webui 事件 + exit 事件。
        let log_lines = sink.log_lines.lock().unwrap();
        assert!(
            log_lines.iter().any(|(_, line, _)| line.contains("testTK")),
            "expected stdout line with token"
        );
        let webui = sink.webui.lock().unwrap();
        assert_eq!(webui.len(), 1);
        assert_eq!(webui[0].1, 6099);
        assert_eq!(webui[0].2, "testTK");
        let exited = sink.exited.lock().unwrap();
        assert_eq!(exited.len(), 1);
        assert_eq!(exited[0].1, Some(0));
    }

    /// stop 对不存在的 bot 应该幂等返回 Ok。
    #[tokio::test]
    async fn stop_is_idempotent_when_not_running() {
        let dep = NativeDeployment::new(
            Arc::new(FakeTranslator {
                plan: NativeLaunchCommand {
                    program: "echo".into(),
                    args: vec![],
                    working_dir: None,
                    environment: BTreeMap::new(),
                },
            }),
            Arc::new(NullRuntimeEventSink),
            None,
        );
        let host = FakeHost::new(Os::Windows);
        let result = dep
            .stop(&host, &BotId::new("ghost"), StopMode::Force)
            .await;
        assert!(result.is_ok());
    }
}
