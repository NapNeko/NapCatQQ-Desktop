//! 应用实例的进程骨架（框架无关）：起 / 停 / 活着吗 / 日志推送
//!
//! - 本机：`Host::spawn` 拿子进程，stdout/stderr 边读边发 `app_instance_log_appended`
//!   并落到实例目录的日志文件；Desktop 退出时 [NativeAppRuntime::shutdown_local] 一并停掉
//! - 远端：SSH exec channel 关了长驻进程会跟着死（同 SnowLuma 远端栈），所以用
//!   `nohup setsid … &` + pid 文件投递；日志用 wc/tail 按偏移增量拉（同 bot_log_follow）
//! - 冷启动 reconcile：pid 文件 + 进程身份校验（本机比进程名，远端比 /proc/pid/cwd），
//!   防 pid 复用误杀
//!
//! 进程退出 / 被停都由这里更新实例表状态并发 `app_instance_changed`。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use ncd_domain::{AppInstance, AppInstanceId, AppInstanceState};
use ncd_host::{ExitStatus, Host, HostCommand, HostPath, HostProcess, Locality, PathStyle};
use ncd_traits::{AppFrameworkError, EventBus};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncSeekExt, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use super::instances::AppInstanceStore;
use crate::events::{BroadcastEventBus, DomainEvent};

/// pid 文件名由运行时统一决定，框架适配器不用管
pub const APP_PID_FILE: &str = ".ncd-app.pid";
const REMOTE_POLL: Duration = Duration::from_secs(2);
const LOCAL_TAIL_POLL: Duration = Duration::from_secs(1);
const MAX_CHUNK: u64 = 512 * 1024;

/// 一次启动所需：命令 + 日志文件（框架适配器给）
pub struct AppLaunchSpec {
    pub command: HostCommand,
    pub log_file: HostPath,
}

struct LocalManaged {
    pid: u32,
    process: Arc<Mutex<Box<dyn HostProcess>>>,
    tasks: Vec<JoinHandle<()>>,
}

type LocalTable = Arc<Mutex<HashMap<AppInstanceId, LocalManaged>>>;

pub struct NativeAppRuntime {
    event_bus: Arc<BroadcastEventBus>,
    store: Arc<AppInstanceStore>,
    /// 本会话 spawn 出来的本机子进程
    local: LocalTable,
    /// 远端 / 重连实例的日志 + 存活轮询任务
    followers: Mutex<HashMap<AppInstanceId, JoinHandle<()>>>,
}

impl NativeAppRuntime {
    pub fn new(event_bus: Arc<BroadcastEventBus>, store: Arc<AppInstanceStore>) -> Self {
        Self {
            event_bus,
            store,
            local: Arc::new(Mutex::new(HashMap::new())),
            followers: Mutex::new(HashMap::new()),
        }
    }

    pub fn pid_file(instance: &AppInstance) -> HostPath {
        HostPath::from_posix(&instance.install_dir).join(APP_PID_FILE)
    }

    /// 启动并接管；返回 pid。退出 / 被停的状态回写由这里的任务负责
    pub async fn start(
        &self,
        host: Arc<dyn Host>,
        instance: &AppInstance,
        spec: AppLaunchSpec,
    ) -> Result<u32, AppFrameworkError> {
        if self.local.lock().await.contains_key(&instance.id) {
            return Err(AppFrameworkError::Runtime("实例已在运行".to_string()));
        }
        self.stop_follow(&instance.id).await;
        match host.locality() {
            Locality::Local => self.start_local(host, instance, spec).await,
            Locality::Remote => self.start_remote(host, instance, spec).await,
        }
    }

    /// 停止：本会话接管的直接 kill；否则按 pid 文件停。总是清 pid 文件
    pub async fn stop(
        &self,
        host: Arc<dyn Host>,
        instance: &AppInstance,
    ) -> Result<(), AppFrameworkError> {
        self.stop_follow(&instance.id).await;
        let managed = self.local.lock().await.remove(&instance.id);
        if let Some(managed) = managed {
            for t in &managed.tasks {
                t.abort();
            }
            let mut proc = managed.process.lock().await;
            if let Err(e) = proc.kill().await {
                tracing::warn!(pid = managed.pid, error = %e, "kill app process failed");
            }
        } else if let Some(pid) = self.reconcile_pid(host.as_ref(), instance).await? {
            match host.locality() {
                Locality::Local => kill_local_pid(pid),
                Locality::Remote => remote_stop(host.as_ref(), instance, pid).await?,
            }
        }
        let _ = host.remove_file(&Self::pid_file(instance)).await;
        Ok(())
    }

    /// 冷启动 / 刷新用：实例现在有活进程吗（pid 文件 + 进程身份校验）
    pub async fn reconcile_pid(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
    ) -> Result<Option<u32>, AppFrameworkError> {
        if let Some(m) = self.local.lock().await.get(&instance.id) {
            return Ok(Some(m.pid));
        }
        if let Some((pid, program)) = read_pid_file(host, &Self::pid_file(instance)).await? {
            let alive = match host.locality() {
                Locality::Local => local_pid_matches(pid, &program),
                Locality::Remote => remote_pid_matches(host, pid, &instance.install_dir).await,
            };
            if alive {
                return Ok(Some(pid));
            }
        }
        self.discover_and_claim(host, instance).await
    }

    pub async fn claim_pid(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        pid: u32,
        program: &str,
    ) -> Result<(), AppFrameworkError> {
        let body = render_pid_file(pid, program);
        host.write_file(&Self::pid_file(instance), body.as_bytes())
            .await
            .map_err(host_err)
    }

    pub async fn discover_and_claim(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
    ) -> Result<Option<u32>, AppFrameworkError> {
        let kind = super::supervisor::AppProcessKind::from_framework(instance.framework_id.as_str());
        let found = match host.locality() {
            Locality::Remote => {
                let listing =
                    super::supervisor::list_cwd_processes(host, &instance.install_dir).await?;
                super::supervisor::pick_app_pid(&listing, kind)
            }
            Locality::Local => discover_local_pid(&instance.install_dir, kind),
        };
        let Some((pid, program)) = found else {
            return Ok(None);
        };
        self.claim_pid(host, instance, pid, &program).await?;
        Ok(Some(pid))
    }

    /// 本会话已经在跟这条实例的日志 / 存活了（本机 spawn 或 attach 的 follower）
    pub async fn is_following(&self, id: &AppInstanceId) -> bool {
        if self.local.lock().await.contains_key(id) {
            return true;
        }
        self.followers.lock().await.contains_key(id)
    }

    /// 对已经在跑但不是本会话起的实例（冷启动 reconcile 命中）挂上日志 + 存活轮询
    pub async fn attach(
        &self,
        host: Arc<dyn Host>,
        instance: &AppInstance,
        log_file: HostPath,
    ) -> Result<(), AppFrameworkError> {
        self.stop_follow(&instance.id).await;
        let task = match host.locality() {
            Locality::Local => {
                let Some((pid, program)) =
                    read_pid_file(host.as_ref(), &Self::pid_file(instance)).await?
                else {
                    return Ok(());
                };
                self.spawn_local_tail(instance.id.clone(), log_file, pid, program, true)
            }
            Locality::Remote => self.spawn_remote_follow(host, instance.clone(), log_file, true),
        };
        self.followers.lock().await.insert(instance.id.clone(), task);
        Ok(())
    }

    /// Desktop 退出：本机实例随之停止（对齐协议 Bot 语义），远端脱管
    pub async fn shutdown_local(&self) {
        let mut local = self.local.lock().await;
        for (_, managed) in local.drain() {
            for t in &managed.tasks {
                t.abort();
            }
            let mut proc = managed.process.lock().await;
            let _ = proc.kill().await;
        }
        let mut followers = self.followers.lock().await;
        for (_, t) in followers.drain() {
            t.abort();
        }
    }

    async fn stop_follow(&self, id: &AppInstanceId) {
        if let Some(t) = self.followers.lock().await.remove(id) {
            t.abort();
        }
    }

    // ---- 本机 ----

    async fn start_local(
        &self,
        host: Arc<dyn Host>,
        instance: &AppInstance,
        spec: AppLaunchSpec,
    ) -> Result<u32, AppFrameworkError> {
        let program_name = program_file_name(&spec.command.program);
        let mut process = host.spawn(spec.command).await.map_err(host_err)?;
        let pid = process.id().native;
        host.write_file(
            &Self::pid_file(instance),
            render_pid_file(pid, &program_name).as_bytes(),
        )
        .await
        .map_err(host_err)?;

        let log_path = spec.log_file.render(PathStyle::Windows);
        let mut tasks = Vec::new();
        if let Some(out) = process.take_stdout() {
            tasks.push(self.spawn_pump(instance.id.clone(), out, log_path.clone()));
        }
        if let Some(err) = process.take_stderr() {
            tasks.push(self.spawn_pump(instance.id.clone(), err, log_path));
        }
        let process = Arc::new(Mutex::new(process));
        tasks.push(self.spawn_local_waiter(instance.id.clone(), Arc::clone(&process)));

        self.local.lock().await.insert(
            instance.id.clone(),
            LocalManaged {
                pid,
                process,
                tasks,
            },
        );
        Ok(pid)
    }

    fn spawn_pump(
        &self,
        id: AppInstanceId,
        reader: Box<dyn AsyncRead + Send + Unpin>,
        log_path: String,
    ) -> JoinHandle<()> {
        let bus = Arc::clone(&self.event_bus);
        tokio::spawn(async move {
            let mut file = tokio::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_path)
                .await
                .ok();
            let mut lines = BufReader::new(reader).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if let Some(f) = file.as_mut() {
                    let _ = f.write_all(line.as_bytes()).await;
                    let _ = f.write_all(b"\n").await;
                }
                publish_app_log(&bus, &id, &line);
            }
        })
    }

    fn spawn_local_waiter(
        &self,
        id: AppInstanceId,
        process: Arc<Mutex<Box<dyn HostProcess>>>,
    ) -> JoinHandle<()> {
        let store = Arc::clone(&self.store);
        let bus = Arc::clone(&self.event_bus);
        let local = Arc::clone(&self.local);
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(1)).await;
                let status = process.lock().await.try_wait().await;
                let exit = match status {
                    Ok(ExitStatus::Running) => continue,
                    Ok(other) => other,
                    Err(e) => {
                        tracing::warn!(error = %e, "try_wait failed; treating app as exited");
                        ExitStatus::Killed
                    }
                };
                // 谁把条目从表里摘掉，谁负责写状态：stop() 摘了就不重复
                if local.lock().await.remove(&id).is_some() {
                    let reason = match exit {
                        ExitStatus::Exited(0) => "进程已退出".to_string(),
                        ExitStatus::Exited(code) => format!("进程异常退出（code {code}）"),
                        _ => "进程被终止".to_string(),
                    };
                    mark_stopped(&store, &bus, &id, reason, !exit.success()).await;
                }
                return;
            }
        })
    }

    fn spawn_local_tail(
        &self,
        id: AppInstanceId,
        log_file: HostPath,
        pid: u32,
        program: String,
        backfill: bool,
    ) -> JoinHandle<()> {
        let store = Arc::clone(&self.store);
        let bus = Arc::clone(&self.event_bus);
        let path = log_file.render(PathStyle::Windows);
        tokio::spawn(async move {
            let mut offset = if backfill {
                0
            } else {
                tokio::fs::metadata(&path).await.map(|m| m.len()).unwrap_or(0)
            };
            let mut ticks: u32 = 0;
            loop {
                if ticks > 0 || !backfill {
                    tokio::time::sleep(LOCAL_TAIL_POLL).await;
                }
                if let Ok(meta) = tokio::fs::metadata(&path).await {
                    let size = meta.len();
                    if size < offset {
                        offset = 0;
                    }
                    if let Some(read_from) = log_follow_read_from(offset, size) {
                        if let Some(chunk) = local_read_from(&path, read_from, size).await {
                            for line in String::from_utf8_lossy(&chunk).lines() {
                                publish_app_log(&bus, &id, line);
                            }
                        }
                        offset = size;
                    }
                }
                ticks = ticks.wrapping_add(1);
                if ticks % 5 == 0 && !local_pid_matches(pid, &program) {
                    mark_stopped(&store, &bus, &id, "进程已退出".to_string(), false).await;
                    return;
                }
            }
        })
    }

    // ---- 远端 ----

    async fn start_remote(
        &self,
        host: Arc<dyn Host>,
        instance: &AppInstance,
        spec: AppLaunchSpec,
    ) -> Result<u32, AppFrameworkError> {
        let script = remote_start_script(
            &spec.command,
            &instance.install_dir,
            &spec.log_file,
            &Self::pid_file(instance),
        );
        let out = host
            .run_to_string(
                HostCommand::new("sh")
                    .arg("-c")
                    .arg(script)
                    .timeout(Duration::from_secs(30)),
            )
            .await
            .map_err(host_err)?;
        let pid = out.stdout.lines().find_map(|l| {
            l.strip_prefix("RUNNING ")
                .and_then(|p| p.trim().parse::<u32>().ok())
        });
        let Some(pid) = pid else {
            let detail = format!("{}\n{}", out.stdout.trim(), out.stderr.trim());
            return Err(AppFrameworkError::Runtime(format!(
                "远端进程启动后立即退出：{}",
                detail.trim()
            )));
        };
        let task = self.spawn_remote_follow(host, instance.clone(), spec.log_file, false);
        self.followers.lock().await.insert(instance.id.clone(), task);
        Ok(pid)
    }

    fn spawn_remote_follow(
        &self,
        host: Arc<dyn Host>,
        instance: AppInstance,
        log_file: HostPath,
        backfill: bool,
    ) -> JoinHandle<()> {
        let store = Arc::clone(&self.store);
        let bus = Arc::clone(&self.event_bus);
        let log_path = log_file.as_posix().to_string();
        let pid_file = Self::pid_file(&instance);
        let install_dir = instance.install_dir.clone();
        let id = instance.id.clone();
        tokio::spawn(async move {
            let mut last_size = if backfill {
                0
            } else {
                remote_file_size(host.as_ref(), &log_path)
                    .await
                    .unwrap_or(0)
            };
            let mut ticks: u32 = 0;
            loop {
                if ticks > 0 || !backfill {
                    tokio::time::sleep(REMOTE_POLL).await;
                }
                if let Some(size) = remote_file_size(host.as_ref(), &log_path).await {
                    if size < last_size {
                        last_size = 0;
                    }
                    if let Some(read_from) = log_follow_read_from(last_size, size) {
                        if let Some(chunk) =
                            remote_read_from(host.as_ref(), &log_path, read_from).await
                        {
                            for line in String::from_utf8_lossy(&chunk).lines() {
                                publish_app_log(&bus, &id, line);
                            }
                        }
                        last_size = size;
                    }
                }
                ticks = ticks.wrapping_add(1);
                if ticks % 3 == 0 {
                    let alive = match read_pid_file(host.as_ref(), &pid_file).await {
                        Ok(Some((pid, _))) => {
                            remote_pid_matches(host.as_ref(), pid, &install_dir).await
                        }
                        // 读不到 pid 文件（SSH 抖动）不当作退出，下轮再看
                        Err(_) => true,
                        Ok(None) => false,
                    };
                    if !alive {
                        mark_stopped(&store, &bus, &id, "远端进程已退出".to_string(), false)
                            .await;
                        return;
                    }
                }
            }
        })
    }
}

async fn mark_stopped(
    store: &AppInstanceStore,
    bus: &BroadcastEventBus,
    id: &AppInstanceId,
    reason: String,
    is_error: bool,
) {
    let updated = store
        .update(id, |i| {
            if i.state == AppInstanceState::Running {
                i.state = AppInstanceState::Stopped;
            }
            i.last_error = is_error.then(|| reason.clone());
        })
        .await;
    match updated {
        Ok(instance) => bus.publish(DomainEvent::app_instance_changed(instance, reason)),
        Err(e) => tracing::warn!(instance = id.as_str(), error = %e, "mark app stopped failed"),
    }
}

fn host_err(e: ncd_host::HostError) -> AppFrameworkError {
    AppFrameworkError::Host(e.to_string())
}

// ---- pid 文件 ----

/// 第一行 pid，第二行进程文件名（本机身份校验用）
fn render_pid_file(pid: u32, program: &str) -> String {
    format!("{pid}\n{program}\n")
}

fn parse_pid_file(text: &str) -> Option<(u32, String)> {
    let mut lines = text.lines();
    let pid = lines.next()?.trim().parse::<u32>().ok()?;
    let program = lines.next().unwrap_or("").trim().to_string();
    Some((pid, program))
}

async fn read_pid_file(
    host: &dyn Host,
    pid_file: &HostPath,
) -> Result<Option<(u32, String)>, AppFrameworkError> {
    if !host.exists(pid_file).await.map_err(host_err)? {
        return Ok(None);
    }
    let bytes = host.read_file(pid_file).await.map_err(host_err)?;
    Ok(parse_pid_file(&String::from_utf8_lossy(&bytes)))
}

fn program_file_name(program: &str) -> String {
    program
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(program)
        .to_ascii_lowercase()
}

// ---- 本机进程 ----

fn local_pid_matches(pid: u32, program: &str) -> bool {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
    let mut sys = System::new();
    let spid = Pid::from_u32(pid);
    sys.refresh_processes_specifics(ProcessesToUpdate::Some(&[spid]), ProcessRefreshKind::new());
    let Some(p) = sys.process(spid) else {
        return false;
    };
    if program.is_empty() {
        return true;
    }
    p.name().to_string_lossy().to_ascii_lowercase() == program
}

pub(crate) fn discover_local_pid(
    install_dir: &str,
    kind: super::supervisor::AppProcessKind,
) -> Option<(u32, String)> {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        ProcessRefreshKind::new().with_cwd(UpdateKind::Always).with_cmd(UpdateKind::Always),
    );
    let want = HostPath::from_posix(install_dir);
    let mut lines = String::new();
    for (pid, proc) in sys.processes() {
        let Some(cwd) = proc.cwd() else {
            continue;
        };
        let got = HostPath::from_windows(&cwd.to_string_lossy());
        if got.as_posix() != want.as_posix() {
            continue;
        }
        let cmd = proc
            .cmd()
            .iter()
            .map(|s| s.to_string_lossy())
            .collect::<Vec<_>>()
            .join(" ");
        lines.push_str(&format!("{} {cmd}\n", pid.as_u32()));
    }
    super::supervisor::pick_app_pid(&lines, kind)
}

fn kill_local_pid(pid: u32) {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
    let mut sys = System::new();
    let spid = Pid::from_u32(pid);
    sys.refresh_processes_specifics(ProcessesToUpdate::Some(&[spid]), ProcessRefreshKind::new());
    if let Some(p) = sys.process(spid) {
        p.kill();
    }
}

async fn local_read_from(path: &str, from: u64, to: u64) -> Option<Vec<u8>> {
    let mut f = tokio::fs::File::open(path).await.ok()?;
    f.seek(std::io::SeekFrom::Start(from)).await.ok()?;
    let mut buf = vec![0u8; (to - from) as usize];
    let n = f.read(&mut buf).await.ok()?;
    buf.truncate(n);
    Some(buf)
}

// ---- 远端进程 ----

fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\"'\"'"))
}

/// `cd dir && export … && nohup setsid prog args >> log 2>&1 </dev/null & echo $! > pid`，
/// 起完 sleep 1 再 kill -0 校验，失败带日志尾巴回来
fn remote_start_script(
    cmd: &HostCommand,
    install_dir: &str,
    log_file: &HostPath,
    pid_file: &HostPath,
) -> String {
    let dir = shell_quote(install_dir);
    let log = shell_quote(log_file.as_posix());
    let pid = shell_quote(pid_file.as_posix());
    let exports: String = cmd
        .environment
        .iter()
        .map(|(k, v)| format!("export {k}={}\n", shell_quote(v)))
        .collect();
    let invoke = std::iter::once(cmd.program.as_str())
        .chain(cmd.args.iter().map(String::as_str))
        .map(shell_quote)
        .collect::<Vec<_>>()
        .join(" ");
    let program_name = program_file_name(&cmd.program);
    format!(
        "cd {dir} || exit 97\n\
         {exports}\
         nohup setsid {invoke} >> {log} 2>&1 </dev/null &\n\
         pid=$!\n\
         printf '%s\\n%s\\n' \"$pid\" {prog} > {pid}\n\
         sleep 1\n\
         if kill -0 \"$pid\" 2>/dev/null; then echo \"RUNNING $pid\"; else echo EXITED; tail -n 40 {log} 2>/dev/null; exit 98; fi\n",
        prog = shell_quote(&program_name),
    )
}

async fn remote_pid_matches(host: &dyn Host, pid: u32, install_dir: &str) -> bool {
    let dir = shell_quote(install_dir);
    let script = format!(
        "if kill -0 {pid} 2>/dev/null; then \
           cwd=$(readlink /proc/{pid}/cwd 2>/dev/null); \
           if [ -z \"$cwd\" ] || [ \"$cwd\" = {dir} ]; then echo ALIVE; else echo OTHER; fi; \
         else echo DEAD; fi"
    );
    match host
        .run_to_string(HostCommand::new("sh").arg("-c").arg(script))
        .await
    {
        Ok(out) => out.stdout.trim() == "ALIVE",
        Err(_) => false,
    }
}

async fn remote_stop(
    host: &dyn Host,
    instance: &AppInstance,
    pid: u32,
) -> Result<(), AppFrameworkError> {
    let script = format!(
        "kill {pid} 2>/dev/null; \
         for i in 1 2 3 4 5 6 7 8 9 10; do kill -0 {pid} 2>/dev/null || exit 0; sleep 0.5; done; \
         kill -9 {pid} 2>/dev/null; exit 0"
    );
    host.run_to_string(
        HostCommand::new("sh")
            .arg("-c")
            .arg(script)
            .timeout(Duration::from_secs(20)),
    )
    .await
    .map_err(host_err)?;
    let _ = instance;
    Ok(())
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

/// attach 时 last_size=0,只吐尾巴,避免整文件灌事件总线
fn log_follow_read_from(last_size: u64, size: u64) -> Option<u64> {
    if size <= last_size {
        return None;
    }
    if last_size == 0 {
        Some(size.saturating_sub(MAX_CHUNK))
    } else {
        Some(last_size)
    }
}

fn publish_app_log(bus: &BroadcastEventBus, id: &AppInstanceId, line: &str) {
    let cleaned = ncd_deploy::strip_ansi_escapes(line);
    if cleaned.trim().is_empty() {
        return;
    }
    bus.publish(DomainEvent::app_instance_log(id.clone(), cleaned));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pid_file_round_trip() {
        let text = render_pid_file(4242, "node.exe");
        assert_eq!(parse_pid_file(&text), Some((4242, "node.exe".to_string())));
        assert_eq!(parse_pid_file("77\n"), Some((77, String::new())));
        assert_eq!(parse_pid_file("garbage"), None);
    }

    #[test]
    fn program_file_name_strips_dirs_and_lowercases() {
        assert_eq!(program_file_name("C:\\Tools\\Node\\NODE.EXE"), "node.exe");
        assert_eq!(program_file_name("/usr/bin/node"), "node");
        assert_eq!(program_file_name("node"), "node");
    }

    #[test]
    fn remote_script_quotes_and_detaches() {
        let cmd = HostCommand::new("/home/u/node/bin/node")
            .arg("node_modules/node-karin/dist/start/app.mjs")
            .env("NODE_ENV", "production")
            .env("PATH", "/a b:/usr/bin");
        let script = remote_start_script(
            &cmd,
            "/home/u/ncd/apps/karin/k1",
            &HostPath::from_posix("/home/u/ncd/apps/karin/k1/.ncd-karin.log"),
            &HostPath::from_posix("/home/u/ncd/apps/karin/k1/.ncd-app.pid"),
        );
        assert!(script.starts_with("cd '/home/u/ncd/apps/karin/k1' || exit 97\n"));
        assert!(script.contains("export NODE_ENV='production'\n"));
        assert!(script.contains("export PATH='/a b:/usr/bin'\n"));
        assert!(script.contains(
            "nohup setsid '/home/u/node/bin/node' 'node_modules/node-karin/dist/start/app.mjs' >> '/home/u/ncd/apps/karin/k1/.ncd-karin.log' 2>&1 </dev/null &"
        ));
        assert!(script.contains("> '/home/u/ncd/apps/karin/k1/.ncd-app.pid'"));
        assert!(script.contains("'node'"));
        assert!(script.contains("RUNNING $pid"));
    }

    #[test]
    fn shell_quote_escapes_single_quotes() {
        assert_eq!(shell_quote("it's"), "'it'\"'\"'s'");
    }

    #[test]
    fn attach_backfill_reads_tail_not_whole_file() {
        assert_eq!(log_follow_read_from(0, 100), Some(0));
        assert_eq!(log_follow_read_from(0, MAX_CHUNK + 80), Some(80));
        assert_eq!(log_follow_read_from(40, 80), Some(40));
        assert_eq!(log_follow_read_from(80, 80), None);
        assert_eq!(log_follow_read_from(90, 80), None);
    }
}
