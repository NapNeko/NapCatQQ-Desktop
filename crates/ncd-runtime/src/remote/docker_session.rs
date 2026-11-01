//! 远端 Docker Bot 运行时会话:SSH 隧道,日志 follow,WebUI 事件,容器退出检测

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use ncd_backend_snowluma::{
    LinuxSinglePidProbe, PollerDeps, ReqwestSnowLumaWebUiClient, SnowLumaStatusPoller,
    SnowLumaWebUiClient,
};
use ncd_deploy::deployments::docker::resolve_bot_container_name_with_cli;
use ncd_deploy::docker::DockerCli;
use ncd_deploy::{Deployment, NativeRuntimeEventSink};
use ncd_deploy::{DockerDeployment, NapcatLogNoiseFilter, parse_napcat_webui_line};
use ncd_domain::docker::ContainerEngine;
use ncd_domain::docker::DockerDeploySpec;
use ncd_domain::{BackendType, BotConfig, DeploymentType, RuntimeScenario};
use ncd_host::remote::{TunnelHandle, TunnelSpec};
use ncd_host::{Host, HostError, StreamSource};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::warn;

use super::docker_webui_token::{TokenRecheck, token_to_publish};
use crate::events::{
    BroadcastEventBus, DomainEvent, DomainEventKind, EventBus, EventFilter, EventSubscription,
    NapCatWebuiAuthIssue,
};
use crate::native_deployment_adapter::EventBusSink;
use crate::native_deployment_adapter::docker_helpers::read_napcat_docker_webui_token;
use ncd_domain::ids::BotId;

/// SnowLuma Docker:本机隧道上的 WebUI / noVNC 端口
#[derive(Debug, Clone)]
pub struct SnowLumaDockerEndpoints {
    pub webui_local_port: u16,
    /// 远端主机上的实际映射口,供 ncd-watch 直接探活。
    pub webui_host_port: u16,
    pub novnc_local_port: u16,
    /// noVNC / VNC(compose VNC_PASSWD)
    pub vnc_password: String,
    /// SnowLuma WebUI 登录(SNOWLUMA_WEBUI_BOOTSTRAP_PASSWORD)
    pub webui_password: String,
}

/// 按容器实际的端口映射找宿主机口。导入的容器常是用户自己 compose 起的，不一定按
/// 桌面端的 qq 偏移规则映射；查不到时仍按规则推，和桌面端自己部署的容器对得上。
struct ContainerPorts<'h> {
    cli: Option<DockerCli<'h>>,
    container: &'h str,
}

impl<'h> ContainerPorts<'h> {
    async fn probe(host: &'h dyn Host, container: &'h str, engine: ContainerEngine) -> Self {
        let cli = DockerCli::for_engine(host, Some(engine));
        let ready = cli.ensure_daemon_ready().await.is_ok();
        Self {
            cli: ready.then_some(cli),
            container,
        }
    }

    async fn host_port(&self, spec: &DockerDeploySpec, container_port: u16) -> u16 {
        if let Some(cli) = &self.cli {
            match cli
                .published_host_port(self.container, container_port)
                .await
            {
                Ok(Some(port)) => return port,
                Ok(None) => {}
                Err(error) => warn!(
                    target: "ncd_runtime::docker_bot_session",
                    container = %self.container,
                    %error,
                    "读取容器端口映射失败，按默认规则推端口"
                ),
            }
        }
        spec.host_port_for_container(container_port)
            .unwrap_or(container_port)
    }

    /// 复用已探测过提权的 cli 读容器 webui.json
    async fn napcat_webui_token(&self, host: &dyn Host) -> Option<String> {
        read_napcat_docker_webui_token(host, self.cli.as_ref(), self.container).await
    }
}

/// 运行中跟着 NapCat 实际 token 走：日志里冒出新 token、或 poller 报 token 被拒时重读。
/// 规则见 docker_webui_token
struct NapcatTokenWatch {
    engine: ContainerEngine,
    container: String,
    local_port: u16,
    remote_webui: u16,
    published: Option<String>,
    latest_log: Arc<std::sync::Mutex<Option<String>>>,
    recheck: TokenRecheck,
}

impl NapcatTokenWatch {
    async fn tick(&mut self, host: &dyn Host, bus: &BroadcastEventBus, bot_id: &BotId) {
        let from_log = self
            .latest_log
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        if !self
            .recheck
            .should_check(self.published.as_deref(), from_log.as_deref())
        {
            return;
        }
        let cli = DockerCli::for_engine(host, Some(self.engine));
        let ready = cli.ensure_daemon_ready().await.is_ok();
        let from_file =
            read_napcat_docker_webui_token(host, ready.then_some(&cli), &self.container).await;
        self.recheck
            .checked(from_file.as_deref(), from_log.as_deref());
        // 被拒时只信文件：日志里那条可能正是被拒的旧值
        let from_log = if self.recheck.log_fallback_allowed() {
            from_log
        } else {
            None
        };
        let Some(token) = token_to_publish(
            self.published.as_deref(),
            from_file.as_deref(),
            from_log.as_deref(),
        ) else {
            return;
        };
        tracing::info!(
            target: "ncd_runtime::docker_bot_session",
            bot_id = %bot_id,
            from_file = from_file.is_some(),
            "NapCat Docker: WebUI token 已变化，重新发布"
        );
        bus.publish(DomainEvent::napcat_webui_available_remote(
            bot_id.clone(),
            self.local_port,
            self.remote_webui,
            token.clone(),
        ));
        self.published = Some(token);
    }
}

async fn wait_auth_issue(sub: Option<&mut EventSubscription>) -> Option<NapCatWebuiAuthIssue> {
    let Some(sub) = sub else {
        return std::future::pending().await;
    };
    loop {
        match sub.next().await {
            Some(DomainEvent::NapCatWebuiAuthChanged { issue, .. }) => return issue,
            Some(_) => continue,
            None => return std::future::pending().await,
        }
    }
}

struct SessionInner {
    tunnels: Vec<TunnelHandle>,
    log_task: Option<JoinHandle<()>>,
    watch_task: Option<JoinHandle<()>>,
    snowluma: Option<SnowLumaDockerEndpoints>,
    snowluma_poller: Option<SnowLumaStatusPoller>,
    snowluma_client: Option<Arc<dyn SnowLumaWebUiClient>>,
    qr_generation: u64,
    qr_login_detected: bool,
    qr_refresh_pending: bool,
    stop_expected: Arc<AtomicBool>,
}

pub struct DockerBotSession {
    server_id: String,
    backend: BackendType,
    inner: Mutex<SessionInner>,
}

impl DockerBotSession {
    pub async fn snowluma_endpoints_async(&self) -> Option<SnowLumaDockerEndpoints> {
        self.inner.lock().await.snowluma.clone()
    }

    pub async fn shutdown(&self) {
        let mut inner = self.inner.lock().await;
        inner.stop_expected.store(true, Ordering::SeqCst);
        inner.qr_generation = inner.qr_generation.wrapping_add(1);
        inner.qr_login_detected = false;
        inner.qr_refresh_pending = false;
        if let Some(p) = inner.snowluma_poller.take() {
            p.dispose();
        }
        if let Some(h) = inner.log_task.take() {
            h.abort();
        }
        if let Some(h) = inner.watch_task.take() {
            h.abort();
        }
        inner.tunnels.clear();
        inner.snowluma = None;
        inner.snowluma_client = None;
    }
}

pub struct DockerBotSessionRegistry {
    sessions: Mutex<HashMap<BotId, Arc<DockerBotSession>>>,
}

impl Default for DockerBotSessionRegistry {
    fn default() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
        }
    }
}

impl DockerBotSessionRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn stop_expected(&self, bot_id: &BotId) {
        if let Some(s) = self.sessions.lock().await.get(bot_id) {
            s.inner
                .lock()
                .await
                .stop_expected
                .store(true, Ordering::SeqCst);
        }
    }

    pub async fn snowluma_endpoints(&self, bot_id: &BotId) -> Option<SnowLumaDockerEndpoints> {
        let guard = self.sessions.lock().await;
        let s = guard.get(bot_id)?;
        s.snowluma_endpoints_async().await
    }

    pub async fn snowluma_client(
        &self,
        bot_id: &BotId,
    ) -> Option<(Arc<dyn SnowLumaWebUiClient>, u64, bool)> {
        let session = self.sessions.lock().await.get(bot_id).cloned()?;
        let inner = session.inner.lock().await;
        Some((
            inner.snowluma_client.clone()?,
            inner.qr_generation,
            inner.qr_login_detected,
        ))
    }

    pub async fn mark_qr_login_detected(
        &self,
        bot_id: &BotId,
        generation: u64,
        client: &Arc<dyn SnowLumaWebUiClient>,
    ) {
        if let Some(session) = self.sessions.lock().await.get(bot_id).cloned() {
            let mut inner = session.inner.lock().await;
            if inner.qr_generation == generation
                && inner
                    .snowluma_client
                    .as_ref()
                    .is_some_and(|current| Arc::ptr_eq(current, client))
            {
                inner.qr_login_detected = true;
            }
        }
    }

    pub async fn cancel_qr_login(&self, bot_id: &BotId) {
        if let Some(session) = self.sessions.lock().await.get(bot_id).cloned() {
            let mut inner = session.inner.lock().await;
            inner.qr_generation = inner.qr_generation.wrapping_add(1);
            inner.qr_login_detected = false;
            inner.qr_refresh_pending = false;
        }
    }

    pub async fn request_qr_refresh(&self, bot_id: &BotId) {
        if let Some(session) = self.sessions.lock().await.get(bot_id).cloned() {
            session.inner.lock().await.qr_refresh_pending = true;
        }
    }

    pub async fn take_qr_refresh(
        &self,
        bot_id: &BotId,
        generation: u64,
        client: &Arc<dyn SnowLumaWebUiClient>,
    ) -> Option<bool> {
        let session = self.sessions.lock().await.get(bot_id).cloned()?;
        let mut inner = session.inner.lock().await;
        if inner.qr_generation != generation
            || !inner
                .snowluma_client
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current, client))
        {
            return None;
        }
        Some(std::mem::take(&mut inner.qr_refresh_pending))
    }

    pub async fn shutdown_bot(&self, bot_id: &BotId) {
        if let Some(s) = self.sessions.lock().await.remove(bot_id) {
            s.shutdown().await;
        }
    }

    pub async fn detach_snowluma_server(&self, server_id: &str) {
        let sessions: Vec<_> = self
            .sessions
            .lock()
            .await
            .values()
            .filter(|session| {
                session.server_id == server_id && session.backend == BackendType::SnowLuma
            })
            .cloned()
            .collect();
        for session in sessions {
            // 只释放桌面端持有的资源；容器由原启动进程继续运行。
            session.shutdown().await;
        }
    }

    pub async fn shutdown_all(&self) {
        let mut guard = self.sessions.lock().await;
        for (_, s) in guard.drain() {
            s.shutdown().await;
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn start_session(
        &self,
        bot_id: BotId,
        config: BotConfig,
        host: Arc<dyn Host>,
        bus: Arc<BroadcastEventBus>,
        napcat_webui_token: Option<String>,
        snowluma_vnc_passwd: Option<String>,
        snowluma_webui_bootstrap: Option<String>,
    ) {
        if config.bot.deployment_type != DeploymentType::Docker {
            return;
        }
        let Ok(scenario) = RuntimeScenario::from_config(&config) else {
            return;
        };
        if !scenario.is_docker() {
            return;
        }
        let server_id = scenario.server_id().unwrap_or("local").to_string();

        self.shutdown_bot(&bot_id).await;

        let spec = DockerDeployment::build_spec(&config);
        let engine = ncd_deploy::docker::engine::engine_for_bot(host.as_ref(), &config);
        let cli = DockerCli::for_engine(host.as_ref(), Some(engine));
        let container = if cli.ensure_daemon_ready().await.is_ok() {
            resolve_bot_container_name_with_cli(&cli, &bot_id)
                .await
                .ok()
                .flatten()
        } else {
            None
        }
        .unwrap_or_else(|| spec.container_name.clone());
        let stop_expected = Arc::new(AtomicBool::new(false));
        let sink = Arc::new(EventBusSink::new(bus.clone()));

        let mut tunnels = Vec::new();
        let mut snowluma_eps = None;
        let mut napcat_token_watch = None;
        let latest_log_token: Arc<std::sync::Mutex<Option<String>>> =
            Arc::new(std::sync::Mutex::new(None));
        // 先订阅再发 webui_available，poller 第一次就被拒也收得到
        let mut auth_sub = (config.bot.backend_type == BackendType::NapCat).then(|| {
            bus.subscribe(EventFilter {
                bot_id: Some(bot_id.clone()),
                kind: Some(DomainEventKind::NapCatWebuiAuthChanged),
            })
        });
        let ports = ContainerPorts::probe(host.as_ref(), &container, engine).await;

        match config.bot.backend_type {
            BackendType::NapCat => {
                let remote_webui = ports.host_port(&spec, 6099).await;
                // 容器 webui.json 才是 NapCat 认的；SecretStore 那份只在读不到时兜底
                let from_container = ports.napcat_webui_token(host.as_ref()).await;
                let from_store = napcat_webui_token.filter(|t| !t.trim().is_empty());
                if from_container.is_some() && from_store.is_some() && from_container != from_store
                {
                    tracing::info!(
                        target: "ncd_runtime::docker_bot_session",
                        bot_id = %bot_id,
                        "NapCat Docker: 容器 webui.json 的 token 与本机记录不同，以容器为准"
                    );
                }
                let initial = from_container.or(from_store);
                if let Ok((local_port, handle)) =
                    connect_loopback_endpoint(host.as_ref(), remote_webui).await
                {
                    tunnels.extend(handle);
                    if let Some(token) = initial.clone() {
                        bus.publish(DomainEvent::napcat_webui_available_remote(
                            bot_id.clone(),
                            local_port,
                            remote_webui,
                            token,
                        ));
                    }
                    napcat_token_watch = Some(NapcatTokenWatch {
                        engine,
                        container: container.clone(),
                        local_port,
                        remote_webui,
                        published: initial,
                        latest_log: Arc::clone(&latest_log_token),
                        recheck: TokenRecheck::default(),
                    });
                } else {
                    warn!(
                        target: "ncd_runtime::docker_bot_session",
                        bot_id = %bot_id,
                        "NapCat Docker: 建立 WebUI 隧道失败"
                    );
                }
            }
            BackendType::SnowLuma => {
                let remote_webui = ports.host_port(&spec, 5099).await;
                let remote_novnc = ports.host_port(&spec, 6081).await;
                let vnc = snowluma_vnc_passwd.unwrap_or_default();
                let webui = snowluma_webui_bootstrap.unwrap_or_default();
                match (
                    connect_loopback_endpoint(host.as_ref(), remote_webui).await,
                    connect_loopback_endpoint(host.as_ref(), remote_novnc).await,
                ) {
                    (Ok((webui_port, w)), Ok((novnc_port, n))) => {
                        snowluma_eps = Some(SnowLumaDockerEndpoints {
                            webui_local_port: webui_port,
                            webui_host_port: remote_webui,
                            novnc_local_port: novnc_port,
                            vnc_password: vnc.clone(),
                            webui_password: webui,
                        });
                        tunnels.extend(w);
                        tunnels.extend(n);
                        bus.publish(DomainEvent::SnowLumaDockerEndpointsReady {
                            bot_id: bot_id.clone(),
                        });
                    }
                    _ => {
                        warn!(
                            target: "ncd_runtime::docker_bot_session",
                            bot_id = %bot_id,
                            "SnowLuma Docker: 建立 WebUI/noVNC 隧道失败"
                        );
                    }
                }
            }
        }

        let host_log = Arc::clone(&host);
        let bot_log = bot_id.clone();
        let sink_log = Arc::clone(&sink);
        let is_napcat = config.bot.backend_type == BackendType::NapCat;
        let log_token = Arc::clone(&latest_log_token);
        let log_task = tokio::spawn(async move {
            let cli = DockerCli::for_engine(host_log.as_ref(), Some(engine));
            // logs_follow callback 串行调用；poison 时 recover 后继续滤，避免脏行放行
            let noise = std::sync::Mutex::new(NapcatLogNoiseFilter::new());
            let _ = cli
                .logs_follow(&container, move |source, line| {
                    let ch = match source {
                        StreamSource::Stdout => "stdout",
                        StreamSource::Stderr => "stderr",
                    };
                    if is_napcat {
                        // 原始行先抓 token，噪声过滤可能改写这一行
                        if let Some((_, token)) = parse_napcat_webui_line(&line) {
                            *log_token
                                .lock()
                                .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(token);
                        }
                        let mut guard = noise
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner());
                        let Some(cleaned) = guard.process_line(&line) else {
                            return;
                        };
                        drop(guard);
                        sink_log.publish_log_line(&bot_log, &cleaned, ch);
                    } else {
                        sink_log.publish_log_line(&bot_log, &line, ch);
                    }
                })
                .await;
        });

        let host_watch = Arc::clone(&host);
        let bot_watch = bot_id.clone();
        let bus_watch = bus.clone();
        let stop_flag = Arc::clone(&stop_expected);
        let watch_task = tokio::spawn(async move {
            let deployment = ncd_deploy::DockerDeployment::new().with_engine(engine);
            let mut token_watch = napcat_token_watch;
            loop {
                tokio::select! {
                    _ = tokio::time::sleep(std::time::Duration::from_secs(3)) => {}
                    issue = wait_auth_issue(auth_sub.as_mut()) => {
                        if let Some(watch) = token_watch.as_mut() {
                            watch.recheck.on_auth_issue(issue);
                        }
                    }
                }
                if stop_flag.load(Ordering::SeqCst) {
                    break;
                }
                if let Some(watch) = token_watch.as_mut() {
                    watch
                        .tick(host_watch.as_ref(), &bus_watch, &bot_watch)
                        .await;
                }
                let state = deployment.observe(host_watch.as_ref(), &bot_watch).await;
                let stopped = matches!(
                    state,
                    Ok(ncd_deploy::DeploymentState::Stopped)
                        | Ok(ncd_deploy::DeploymentState::Failed { .. })
                ) || state.is_err();
                if stopped {
                    if !stop_flag.load(Ordering::SeqCst) {
                        bus_watch.publish(DomainEvent::bot_process_exited(
                            bot_watch.clone(),
                            None,
                            Some("docker container stopped".into()),
                        ));
                    }
                    break;
                }
            }
        });

        // SL Docker 隧道就绪时派生 poller init 参数(容器 WebUI 慢启动,异步 spawn 不阻塞)
        let snowluma_poller_init = snowluma_eps.as_ref().map(|eps| {
            (
                eps.webui_local_port,
                eps.webui_password.clone(),
                config.bot.qq_id,
            )
        });

        let session = Arc::new(DockerBotSession {
            server_id,
            backend: config.bot.backend_type,
            inner: Mutex::new(SessionInner {
                tunnels,
                log_task: Some(log_task),
                watch_task: Some(watch_task),
                snowluma: snowluma_eps,
                snowluma_poller: None,
                snowluma_client: None,
                qr_generation: 0,
                qr_login_detected: false,
                qr_refresh_pending: false,
                stop_expected,
            }),
        });

        // 异步初始化 SL 登录态 poller:wait_ready 要等容器内 WebUI 起来,不能阻塞 start_session
        if let Some((port, pwd, qq_id)) = snowluma_poller_init {
            let session_for_init = Arc::clone(&session);
            let bot_id_init = bot_id.clone();
            let bus_init = bus.clone();
            tokio::spawn(async move {
                let bot_id_log = bot_id_init.clone();
                match build_and_spawn_snowluma_poller(bot_id_init, port, pwd, qq_id, bus_init).await
                {
                    Ok((poller, client)) => {
                        let mut inner = session_for_init.inner.lock().await;
                        if inner.stop_expected.load(Ordering::SeqCst) {
                            // init 期间 session 已 shutdown(容器挂/用户停),直接 dispose 防泄漏
                            poller.dispose();
                        } else {
                            inner.snowluma_poller = Some(poller);
                            inner.snowluma_client = Some(client);
                        }
                    }
                    Err(e) => warn!(
                        target: "ncd_runtime::docker_bot_session",
                        bot_id = %bot_id_log,
                        "SnowLuma Docker poller init failed: {e}"
                    ),
                }
            });
        }

        self.sessions.lock().await.insert(bot_id, session);
    }
}

async fn connect_loopback_endpoint(
    host: &dyn Host,
    host_port: u16,
) -> Result<(u16, Option<TunnelHandle>), HostError> {
    if host.locality() == ncd_host::Locality::Local {
        return Ok((host_port, None));
    }
    let tunnel = host
        .open_tunnel(TunnelSpec::local_to_remote(0, host_port))
        .await?;
    Ok((tunnel.local_port(), Some(tunnel)))
}

/// 构造 SnowLuma WebUI client(隧道本地端口 + bootstrap 密码)→ wait_ready → login →
/// spawn 登录态 poller。Docker 场景无 daemon 单例也无需 load_process(容器内
/// SNOWLUMA_HOOK_AUTOLOAD 自动注入);initial_qq_pid 传 0,完全靠 expected_uin +
/// probe-login 兜底锁定 UIN(见 status_poller 的策略 B + probe 补偿)
async fn build_and_spawn_snowluma_poller(
    bot_id: BotId,
    webui_port: u16,
    webui_password: String,
    qq_id: u64,
    event_bus: Arc<BroadcastEventBus>,
) -> Result<(SnowLumaStatusPoller, Arc<dyn SnowLumaWebUiClient>), String> {
    let client =
        ReqwestSnowLumaWebUiClient::new(webui_port, webui_password).map_err(|e| e.to_string())?;
    client
        .wait_ready(Duration::from_secs(90), Box::new(|| false))
        .await
        .map_err(|e| e.to_string())?;
    client.login().await.map_err(|e| e.to_string())?;
    let client: Arc<dyn SnowLumaWebUiClient> = Arc::new(client);
    let deps = PollerDeps {
        event_bus,
        http: Arc::clone(&client),
        proc_tree: Arc::new(LinuxSinglePidProbe::new(0)),
        expected_uin: Some(qq_id.to_string()),
    };
    Ok((SnowLumaStatusPoller::spawn(bot_id, 0, deps), client))
}

pub use ncd_domain::bot_config::{is_remote_docker_config, is_remote_native_napcat_config};

#[cfg(test)]
#[path = "docker_session_tests.rs"]
mod tests;
