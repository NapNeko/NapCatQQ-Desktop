//! 远端 Docker Bot 运行时会话：SSH 隧道、日志 follow、WebUI 事件、容器退出检测。

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use ncd_deploy::DockerDeployment;
use ncd_deploy::docker::DockerCli;
use ncd_domain::{BackendType, BotConfig, DeploymentType};
use crate::kinds::RuntimeTarget;
use ncd_deploy::{Deployment, NativeRuntimeEventSink};
use ncd_host::remote::{TunnelHandle, TunnelSpec};
use ncd_host::{Host, HostError, StreamSource};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::warn;

use crate::events::{BroadcastEventBus, DomainEvent, EventBus};
use crate::ids::BotId;
use crate::native_deployment_adapter::EventBusSink;

/// SnowLuma Docker：本机隧道上的 WebUI / noVNC 端口。
#[derive(Debug, Clone)]
pub struct SnowLumaDockerEndpoints {
    pub webui_local_port: u16,
    pub novnc_local_port: u16,
    /// noVNC / VNC（compose `VNC_PASSWD`）
    pub vnc_password: String,
    /// SnowLuma WebUI 登录（`SNOWLUMA_WEBUI_BOOTSTRAP_PASSWORD`）
    pub webui_password: String,
}

struct SessionInner {
    tunnels: Vec<TunnelHandle>,
    log_task: Option<JoinHandle<()>>,
    watch_task: Option<JoinHandle<()>>,
    snowluma: Option<SnowLumaDockerEndpoints>,
    stop_expected: Arc<AtomicBool>,
}

pub struct DockerBotSession {
    inner: Mutex<SessionInner>,
}

impl DockerBotSession {
    pub async fn snowluma_endpoints_async(&self) -> Option<SnowLumaDockerEndpoints> {
        self.inner.lock().await.snowluma.clone()
    }

    pub async fn shutdown(&self) {
        let mut inner = self.inner.lock().await;
        inner.stop_expected.store(true, Ordering::SeqCst);
        if let Some(h) = inner.log_task.take() {
            h.abort();
        }
        if let Some(h) = inner.watch_task.take() {
            h.abort();
        }
        inner.tunnels.clear();
        inner.snowluma = None;
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

    pub async fn shutdown_bot(&self, bot_id: &BotId) {
        if let Some(s) = self.sessions.lock().await.remove(bot_id) {
            s.shutdown().await;
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
        if config.bot.runtime_target.is_local() {
            return;
        }

        self.shutdown_bot(&bot_id).await;

        let spec = DockerDeployment::build_spec(&config);
        let container = spec.container_name.clone();
        let stop_expected = Arc::new(AtomicBool::new(false));
        let sink = Arc::new(EventBusSink::new(bus.clone()));

        let mut tunnels = Vec::new();
        let mut snowluma_eps = None;

        match config.bot.backend_type {
            BackendType::NapCat => {
                let remote_webui = spec.host_port_for_container(6099).unwrap_or(6099);
                if let Ok(handle) = open_loopback_tunnel(host.as_ref(), remote_webui).await {
                    let local_port = handle.local_port();
                    tunnels.push(handle);
                    if let Some(token) = napcat_webui_token.filter(|t| !t.trim().is_empty()) {
                        bus.publish(DomainEvent::napcat_webui_available(
                            bot_id.clone(),
                            local_port,
                            token,
                        ));
                    }
                } else {
                    warn!(
                        target: "ncd_runtime::docker_bot_session",
                        bot_id = %bot_id,
                        "NapCat Docker: 建立 WebUI 隧道失败"
                    );
                }
            }
            BackendType::SnowLuma => {
                let remote_webui = spec.host_port_for_container(5099).unwrap_or(5099);
                let remote_novnc = spec.host_port_for_container(6081).unwrap_or(6081);
                let vnc = snowluma_vnc_passwd.unwrap_or_default();
                let webui = snowluma_webui_bootstrap.unwrap_or_default();
                match (
                    open_loopback_tunnel(host.as_ref(), remote_webui).await,
                    open_loopback_tunnel(host.as_ref(), remote_novnc).await,
                ) {
                    (Ok(w), Ok(n)) => {
                        snowluma_eps = Some(SnowLumaDockerEndpoints {
                            webui_local_port: w.local_port(),
                            novnc_local_port: n.local_port(),
                            vnc_password: vnc.clone(),
                            webui_password: webui,
                        });
                        tunnels.push(w);
                        tunnels.push(n);
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
        let log_task = tokio::spawn(async move {
            let cli = DockerCli::new(host_log.as_ref());
            let _ = cli
                .logs_follow(&container, move |source, line| {
                    let ch = match source {
                        StreamSource::Stdout => "stdout",
                        StreamSource::Stderr => "stderr",
                    };
                    sink_log.publish_log_line(&bot_log, &line, ch);
                })
                .await;
        });

        let host_watch = Arc::clone(&host);
        let bot_watch = bot_id.clone();
        let bus_watch = bus.clone();
        let stop_flag = Arc::clone(&stop_expected);
        let watch_task = tokio::spawn(async move {
            let deployment = ncd_deploy::DockerDeployment::new();
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                if stop_flag.load(Ordering::SeqCst) {
                    break;
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

        let session = Arc::new(DockerBotSession {
            inner: Mutex::new(SessionInner {
                tunnels,
                log_task: Some(log_task),
                watch_task: Some(watch_task),
                snowluma: snowluma_eps,
                stop_expected,
            }),
        });
        self.sessions.lock().await.insert(bot_id, session);
    }
}

async fn open_loopback_tunnel(host: &dyn Host, remote_port: u16) -> Result<TunnelHandle, HostError> {
    let spec = TunnelSpec {
        local_host: "127.0.0.1".to_string(),
        local_port: 0,
        remote_host: "127.0.0.1".to_string(),
        remote_port,
    };
    host.open_tunnel(spec).await
}

pub fn is_remote_docker_config(config: &BotConfig) -> bool {
    config.bot.deployment_type == DeploymentType::Docker
        && matches!(config.bot.runtime_target, RuntimeTarget::Server(_))
}

/// 远端 SSH +「直接运行」+ NapCat（非 Docker、非本机 SnowLuma daemon）。
pub fn is_remote_native_napcat_config(config: &BotConfig) -> bool {
    config.bot.deployment_type == DeploymentType::Native
        && config.bot.backend_type == BackendType::NapCat
        && matches!(config.bot.runtime_target, RuntimeTarget::Server(_))
}