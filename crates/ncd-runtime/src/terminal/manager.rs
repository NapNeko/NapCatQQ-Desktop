//! 终端会话：会话表、回放、攒批、流控、重开、全关
//!
//! 会话活在后端，和网页无关：切页面、收起面板、轻量模式销毁网页都不断；前端回来重新 attach，
//! 先收一遍回放再接实时输出。关标签（close）或退出桌面端（close_all）才真正结束。
//!
//! 流控：前端每写完一批回一次确认。没确认的超过高水位就先不从终端读，本机那边顶回子进程
//! （`cat` 大文件时子进程自己停下来等），界面不会被刷屏拖死。前端十秒没动静当它没了，
//! 输出照常收进回放，下次接上时补给它。

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use bytes::Bytes;
use ncd_domain::{
    LocalShellOption, TerminalEvent, TerminalHostOs, TerminalOpenRequest, TerminalSessionId,
    TerminalSessionInfo, TerminalStatus, TerminalTarget,
};
use ncd_host::{Host, HostError, PtyControl, PtyExit, PtyRequest, PtySession, PtySize};
use tokio::sync::{Notify, mpsc, oneshot};

use super::TerminalError;
use super::plan::{TerminalLaunchPlan, TerminalPlanner};
use super::replay::{REPLAY_CAPACITY, ReplayBuffer};
use super::stats::StatsSample;

/// 前端那头：一条收输出字节，一条收状态事件
pub trait TerminalSink: Send + Sync {
    /// 一批输出；返回 false 说明前端已经没了（网页销毁、通道失效）
    fn output(&self, bytes: &[u8]) -> bool;
    fn event(&self, event: &TerminalEvent) -> bool;
}

/// 同时开着的会话上限，防止忘关的标签越积越多
pub const MAX_SESSIONS: usize = 16;
/// 两批输出至少隔这么久，高频小块合成一批，少走几次 IPC
const FLUSH_INTERVAL: Duration = Duration::from_millis(4);
const MAX_BATCH: usize = 64 * 1024;
/// 没确认的超过这么多就停读；回落到低水位再读
const HIGH_WATER: u64 = 2 * 1024 * 1024;
const LOW_WATER: u64 = 512 * 1024;
const STALL_DETACH_AFTER: Duration = Duration::from_secs(10);

pub(super) struct Session {
    state: Mutex<SessionState>,
    acked: Notify,
}

struct SessionState {
    info: TerminalSessionInfo,
    request: TerminalOpenRequest,
    /// 每次（重）开加一；旧终端的收尾不去碰新的
    epoch: u64,
    control: Option<PtyControl>,
    host: Option<Arc<dyn Host>>,
    replay: ReplayBuffer,
    sink: Option<Arc<dyn TerminalSink>>,
    unacked: u64,
    closed: bool,
    restarting: bool,
}

impl Session {
    fn lock(&self) -> MutexGuard<'_, SessionState> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// 记进回放再发给前端；前端没了就把它摘掉
    fn deliver(&self, epoch: u64, bytes: &[u8]) {
        let sink = {
            let mut st = self.lock();
            if st.epoch != epoch || st.closed {
                return;
            }
            st.replay.push(bytes);
            let sink = st.sink.clone();
            if sink.is_some() {
                st.unacked += bytes.len() as u64;
            }
            sink
        };
        if let Some(sink) = sink {
            if !sink.output(bytes) {
                self.drop_sink(&sink);
            }
        }
    }

    fn emit(&self, event: &TerminalEvent) {
        let sink = self.lock().sink.clone();
        if let Some(sink) = sink {
            if !sink.event(event) {
                self.drop_sink(&sink);
            }
        }
    }

    fn drop_sink(&self, failed: &Arc<dyn TerminalSink>) {
        let mut st = self.lock();
        if st.sink.as_ref().is_some_and(|s| Arc::ptr_eq(s, failed)) {
            st.sink = None;
            st.unacked = 0;
        }
    }

    /// 前端落后太多时在这等确认
    async fn wait_for_client(&self) {
        loop {
            {
                let st = self.lock();
                if st.sink.is_none() || st.closed || st.unacked < HIGH_WATER {
                    return;
                }
            }
            if tokio::time::timeout(STALL_DETACH_AFTER, self.acked.notified())
                .await
                .is_err()
            {
                let mut st = self.lock();
                st.sink = None;
                st.unacked = 0;
                return;
            }
        }
    }

    fn finish(&self, epoch: u64, exit: PtyExit) {
        let (line, status, sink) = {
            let mut st = self.lock();
            if st.epoch != epoch || st.closed {
                return;
            }
            st.control = None;
            let status = match exit {
                PtyExit::Exited(code) => TerminalStatus::Exited { code },
                PtyExit::Disconnected(reason) => TerminalStatus::Disconnected { reason },
            };
            st.info.status = status.clone();
            let line = exit_line(&status);
            st.replay.push(&line);
            (line, status, st.sink.clone())
        };
        if let Some(sink) = sink {
            sink.output(&line);
            sink.event(&TerminalEvent::Status { status });
        }
    }
}

fn dim_line(text: &str) -> Vec<u8> {
    format!("\x1b[2m{text}\x1b[0m\r\n").into_bytes()
}

fn banner_bytes(lines: &[String]) -> Vec<u8> {
    lines.iter().flat_map(|l| dim_line(l)).collect()
}

fn exit_line(status: &TerminalStatus) -> Vec<u8> {
    let text = match status {
        TerminalStatus::Exited { code: Some(code) } => {
            format!("[已退出，退出码 {code}，按回车重新打开]")
        }
        TerminalStatus::Exited { code: None } => "[已结束，按回车重新打开]".to_string(),
        TerminalStatus::Disconnected { reason } => format!("[连接断了：{reason}，按回车重连]"),
        TerminalStatus::Failed { message } => format!("[没开起来：{message}，按回车再试]"),
        TerminalStatus::Starting | TerminalStatus::Running => String::new(),
    };
    let mut out = b"\r\n".to_vec();
    out.extend(dim_line(&text));
    out
}

fn info_from_plan(
    id: &TerminalSessionId,
    target: &TerminalTarget,
    plan: &TerminalLaunchPlan,
    created_at_ms: i64,
) -> TerminalSessionInfo {
    TerminalSessionInfo {
        id: id.clone(),
        target: target.clone(),
        title: plan.title.clone(),
        host_id: plan.host_id.clone(),
        host_label: plan.host_label.clone(),
        host_os: plan.host_os,
        cwd: plan.cwd_display.clone(),
        shell: plan.shell,
        status: TerminalStatus::Running,
        created_at_ms,
        features: plan.features,
        snippets: plan.snippets.clone(),
    }
}

fn host_error(err: HostError) -> TerminalError {
    match err {
        HostError::Unsupported { .. } => {
            TerminalError::Unsupported("这台主机不支持内嵌终端".into())
        }
        HostError::InvalidArgument { reason } => TerminalError::Host(reason),
        HostError::RemoteDisconnected { reason } | HostError::RemoteConnection { reason } => {
            TerminalError::Host(format!("连接断了：{reason}"))
        }
        other => TerminalError::Host(other.to_string()),
    }
}

/// 读终端输出、攒批、按流控发给前端，结束时报状态
async fn pump(
    session: Arc<Session>,
    epoch: u64,
    mut output: mpsc::Receiver<Bytes>,
    exit: oneshot::Receiver<PtyExit>,
) {
    let mut batch: Vec<u8> = Vec::with_capacity(MAX_BATCH);
    let mut last_flush = Instant::now() - FLUSH_INTERVAL;
    loop {
        session.wait_for_client().await;
        let Some(first) = output.recv().await else {
            break;
        };
        batch.extend_from_slice(&first);
        loop {
            while batch.len() < MAX_BATCH {
                match output.try_recv() {
                    Ok(more) => batch.extend_from_slice(&more),
                    Err(_) => break,
                }
            }
            let since = last_flush.elapsed();
            if batch.len() >= MAX_BATCH || since >= FLUSH_INTERVAL {
                break;
            }
            match tokio::time::timeout(FLUSH_INTERVAL - since, output.recv()).await {
                Ok(Some(more)) => batch.extend_from_slice(&more),
                Ok(None) | Err(_) => break,
            }
        }
        session.deliver(epoch, &batch);
        batch.clear();
        last_flush = Instant::now();
    }
    let exit = exit
        .await
        .unwrap_or_else(|_| PtyExit::Disconnected("终端后台意外结束".into()));
    session.finish(epoch, exit);
}

fn spawn_pump(session: Arc<Session>, epoch: u64, pty: PtySession) {
    let PtySession { output, exit, .. } = pty;
    tokio::spawn(pump(session, epoch, output, exit));
}

pub struct TerminalManager {
    planner: Arc<dyn TerminalPlanner>,
    sessions: Mutex<HashMap<String, Arc<Session>>>,
    next_id: AtomicU64,
    /// 服务器状态条上一次的读数（按主机），CPU 和网速要跟上一次比
    pub(super) stats: tokio::sync::Mutex<HashMap<String, StatsSample>>,
}

impl TerminalManager {
    pub fn new(planner: Arc<dyn TerminalPlanner>) -> Self {
        Self {
            planner,
            sessions: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(0),
            stats: tokio::sync::Mutex::new(HashMap::new()),
        }
    }

    fn table(&self) -> MutexGuard<'_, HashMap<String, Arc<Session>>> {
        self.sessions.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn get(&self, id: &str) -> Result<Arc<Session>, TerminalError> {
        self.table().get(id).cloned().ok_or(TerminalError::NotFound)
    }

    pub fn local_shells(&self) -> Vec<LocalShellOption> {
        self.planner.local_shells()
    }

    pub fn list(&self) -> Vec<TerminalSessionInfo> {
        let sessions: Vec<Arc<Session>> = self.table().values().cloned().collect();
        let mut out: Vec<TerminalSessionInfo> =
            sessions.iter().map(|s| s.lock().info.clone()).collect();
        out.sort_by_key(|info| info.created_at_ms);
        out
    }

    async fn launch(
        &self,
        request: &TerminalOpenRequest,
    ) -> Result<(TerminalLaunchPlan, PtySession), TerminalError> {
        let plan = self.planner.plan(request).await?;
        let pty_request = PtyRequest {
            program: plan.program.clone(),
            cwd: plan.cwd.clone(),
            env: plan.env.clone(),
            size: PtySize::new(request.cols, request.rows),
        };
        let pty = plan.host.open_pty(pty_request).await.map_err(host_error)?;
        Ok((plan, pty))
    }

    /// 开一个新会话：规划、开终端都成了才返回；失败直接报错，不留空标签
    pub async fn open(
        &self,
        request: TerminalOpenRequest,
    ) -> Result<TerminalSessionInfo, TerminalError> {
        if self.table().len() >= MAX_SESSIONS {
            return Err(TerminalError::TooMany(MAX_SESSIONS));
        }
        let (plan, pty) = self.launch(&request).await?;
        let id = TerminalSessionId::new(format!(
            "t{}",
            self.next_id.fetch_add(1, Ordering::SeqCst) + 1
        ));
        let info = info_from_plan(
            &id,
            &request.target,
            &plan,
            chrono::Utc::now().timestamp_millis(),
        );
        let mut replay = ReplayBuffer::new(REPLAY_CAPACITY);
        replay.push(&banner_bytes(&plan.banner));
        let session = Arc::new(Session {
            state: Mutex::new(SessionState {
                info: info.clone(),
                request,
                epoch: 1,
                control: Some(pty.control.clone()),
                host: Some(Arc::clone(&plan.host)),
                replay,
                sink: None,
                unacked: 0,
                closed: false,
                restarting: false,
            }),
            acked: Notify::new(),
        });
        self.table()
            .insert(id.as_str().to_string(), Arc::clone(&session));
        spawn_pump(session, 1, pty);
        Ok(info)
    }

    /// 前端接上：先回放，之后的输出都走这个 sink；旧的 sink（上一个网页）直接顶掉
    pub fn attach(
        &self,
        id: &str,
        sink: Arc<dyn TerminalSink>,
    ) -> Result<TerminalSessionInfo, TerminalError> {
        let session = self.get(id)?;
        let info = {
            let mut st = session.lock();
            let snapshot = st.replay.snapshot();
            if !snapshot.is_empty() && !sink.output(&snapshot) {
                return Err(TerminalError::Invalid("前端的输出通道用不了".into()));
            }
            st.sink = Some(sink);
            st.unacked = snapshot.len() as u64;
            st.info.clone()
        };
        session.acked.notify_one();
        Ok(info)
    }

    /// 前端清屏时一起清掉回放，下次接上不会把清掉的内容又放出来
    pub fn clear_history(&self, id: &str) -> Result<(), TerminalError> {
        self.get(id)?.lock().replay.clear();
        Ok(())
    }

    /// 前端写完了这么多字节
    pub fn ack(&self, id: &str, bytes: u64) {
        let Ok(session) = self.get(id) else {
            return;
        };
        let wake = {
            let mut st = session.lock();
            st.unacked = st.unacked.saturating_sub(bytes);
            st.unacked < LOW_WATER
        };
        if wake {
            session.acked.notify_one();
        }
    }

    pub fn write(&self, id: &str, data: &str) -> Result<(), TerminalError> {
        let control = self.get(id)?.lock().control.clone();
        control
            .ok_or(TerminalError::NotRunning)?
            .write(Bytes::copy_from_slice(data.as_bytes()))
            .map_err(|_| TerminalError::NotRunning)
    }

    pub fn resize(&self, id: &str, cols: u16, rows: u16) -> Result<(), TerminalError> {
        let session = self.get(id)?;
        let mut st = session.lock();
        st.request.cols = cols;
        st.request.rows = rows;
        if let Some(control) = &st.control {
            let _ = control.resize(PtySize::new(cols, rows));
        }
        Ok(())
    }

    /// 同一个标签重新开（退出、断线、失败之后按回车）
    pub async fn restart(&self, id: &str) -> Result<TerminalSessionInfo, TerminalError> {
        let session = self.get(id)?;
        let (request, epoch) = {
            let mut st = session.lock();
            if st.info.status.is_live() || st.restarting {
                return Ok(st.info.clone());
            }
            st.restarting = true;
            st.epoch += 1;
            st.info.status = TerminalStatus::Starting;
            (st.request.clone(), st.epoch)
        };
        session.emit(&TerminalEvent::Status {
            status: TerminalStatus::Starting,
        });
        let result = self.launch(&request).await;

        let mut st = session.lock();
        st.restarting = false;
        if st.closed || st.epoch != epoch {
            if let Ok((_, pty)) = result {
                pty.control.close();
            }
            return Err(TerminalError::NotFound);
        }
        match result {
            Ok((plan, pty)) => {
                let info =
                    info_from_plan(&st.info.id, &request.target, &plan, st.info.created_at_ms);
                st.info = info.clone();
                st.control = Some(pty.control.clone());
                st.host = Some(Arc::clone(&plan.host));
                let mut bytes = b"\r\n".to_vec();
                bytes.extend(dim_line("──────── 重新打开 ────────"));
                bytes.extend(banner_bytes(&plan.banner));
                st.replay.push(&bytes);
                let sink = st.sink.clone();
                drop(st);
                if let Some(sink) = sink {
                    sink.output(&bytes);
                    sink.event(&TerminalEvent::Info { info: info.clone() });
                }
                spawn_pump(Arc::clone(&session), epoch, pty);
                Ok(info)
            }
            Err(err) => {
                let status = TerminalStatus::Failed {
                    message: err.to_string(),
                };
                st.info.status = status.clone();
                let line = exit_line(&status);
                st.replay.push(&line);
                let sink = st.sink.clone();
                drop(st);
                if let Some(sink) = sink {
                    sink.output(&line);
                    sink.event(&TerminalEvent::Status { status });
                }
                Err(err)
            }
        }
    }

    /// 关标签：结束程序、扔掉会话
    pub fn close(&self, id: &str) {
        let removed = self.table().remove(id);
        let Some(session) = removed else {
            return;
        };
        {
            let mut st = session.lock();
            st.closed = true;
            st.sink = None;
            if let Some(control) = st.control.take() {
                control.close();
            }
        }
        session.acked.notify_one();
    }

    /// 网页要被销毁了（进轻量模式）：摘掉所有前端，输出照常收进回放，网页重建后重新接上
    pub fn detach_all(&self) {
        let sessions: Vec<Arc<Session>> = self.table().values().cloned().collect();
        for session in sessions {
            {
                let mut st = session.lock();
                st.sink = None;
                st.unacked = 0;
            }
            session.acked.notify_one();
        }
    }

    /// 桌面端退出前调用
    pub fn close_all(&self) {
        let ids: Vec<String> = self.table().keys().cloned().collect();
        for id in ids {
            self.close(&id);
        }
    }

    /// 出现 sudo 密码提示时，用户点了「填入密码」：把这台主机存的密码直接写进终端，不经过前端
    pub fn fill_sudo(&self, id: &str) -> Result<(), TerminalError> {
        let session = self.get(id)?;
        let (host_id, control) = {
            let st = session.lock();
            (st.info.host_id.clone(), st.control.clone())
        };
        let control = control.ok_or(TerminalError::NotRunning)?;
        let password = self.planner.sudo_password(&host_id).ok_or_else(|| {
            TerminalError::Invalid("这台主机没存提权密码，先到远端页的主机设置里存一个".into())
        })?;
        let mut bytes = password.into_bytes();
        bytes.push(b'\r');
        control
            .write(Bytes::from(bytes))
            .map_err(|_| TerminalError::NotRunning)
    }

    /// 文件栏、状态条用：会话所在的主机
    pub(super) fn session_host(
        &self,
        id: &str,
    ) -> Result<(Arc<dyn Host>, TerminalHostOs, String), TerminalError> {
        let session = self.get(id)?;
        let st = session.lock();
        let host = st.host.clone().ok_or(TerminalError::NotRunning)?;
        Ok((host, st.info.host_os, st.info.host_id.clone()))
    }

    /// 在系统终端里打开（本机目标）
    pub async fn plan_for_external(
        &self,
        request: &TerminalOpenRequest,
    ) -> Result<TerminalLaunchPlan, TerminalError> {
        self.planner.plan(request).await
    }
}
