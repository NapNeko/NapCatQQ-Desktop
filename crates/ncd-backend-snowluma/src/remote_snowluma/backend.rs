//! 远端 SnowLuma BotBackend

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use async_trait::async_trait;
use ncd_domain::domain_event::DomainEvent;
use ncd_domain::kinds::BackendKind;
use ncd_domain::{BotConfig, BotFlavor, BotId, SnowLumaStartMode};
use ncd_host::{Host, HostPath};
use ncd_traits::events::{BroadcastEventBus, EventBus};
use ncd_traits::runtime_backend::{
    BotBackend, BotBackendError, BotRuntimeConfig, BotStartCtx, BotStatus, LogSnapshot, StopMode,
    TailOpts,
};
use serde_json::Value;
use tokio::sync::Mutex;

use super::layout::SnowLumaRemotePaths;
use super::orchestrator::{bot_cold_start, bot_stop};
use super::tunnel::RemoteSnowLumaTunnelRegistry;
use crate::snowluma::log_noise::prepare_snowluma_bot_history_lines;
use crate::snowluma::status_poller::{PollerDeps, SnowLumaStatusPoller};

use super::config::{render_native_snowluma_config_on_host, resolve_start_mode};
use super::daemon::RemoteSnowLumaDaemon;
use super::helpers::{read_remote_log_tail, read_remote_log_tail_lines};
use super::inject::{inject_via_tunnel, remote_qq_running_pid};

/// 远端 SL 指标探针：上传资产并返回应 export 进共享 node 的 env
#[async_trait]
pub trait RemoteSlMetricsInjector: Send + Sync {
    async fn prepare(
        &self,
        host: &dyn Host,
        home: &str,
        bot_id: &str,
        config: &BotConfig,
    ) -> Option<BTreeMap<String, String>>;
}

/// 远端 SnowLuma BotBackend(内联编排,非本机 SnowLumaDaemon)
pub struct RemoteSnowLumaBackend {
    backend_id: BotId,
    daemon: Arc<RemoteSnowLumaDaemon>,
    event_bus: Arc<BroadcastEventBus>,
    #[allow(dead_code)]
    tunnels: Arc<RemoteSnowLumaTunnelRegistry>,
    start_modes: Arc<Mutex<HashMap<BotId, SnowLumaStartMode>>>,
    pollers: Arc<Mutex<HashMap<BotId, SnowLumaStatusPoller>>>,
    /// Shared coordinator for flipping the common ~/Napcat/opt/QQ tree entry point.
    /// Passed from BotManager so that NC and SL cold starts on the same server_id
    /// serialize their package.json main changes.
    qq_entry_coordinator: Arc<ncd_deploy::remote_coordinator::RemoteQqEntryCoordinator>,
    metrics_injector: Option<Arc<dyn RemoteSlMetricsInjector>>,
}

impl RemoteSnowLumaBackend {
    /// 供 ncd-runtime facade 层统一构造远端后端实例。
    pub fn new(
        backend_id: impl Into<BotId>,
        daemon: Arc<RemoteSnowLumaDaemon>,
        event_bus: Arc<BroadcastEventBus>,
        tunnels: Arc<RemoteSnowLumaTunnelRegistry>,
        qq_entry_coordinator: Arc<ncd_deploy::remote_coordinator::RemoteQqEntryCoordinator>,
    ) -> Self {
        Self::new_with_metrics(
            backend_id,
            daemon,
            event_bus,
            tunnels,
            qq_entry_coordinator,
            None,
        )
    }

    pub fn new_with_metrics(
        backend_id: impl Into<BotId>,
        daemon: Arc<RemoteSnowLumaDaemon>,
        event_bus: Arc<BroadcastEventBus>,
        tunnels: Arc<RemoteSnowLumaTunnelRegistry>,
        qq_entry_coordinator: Arc<ncd_deploy::remote_coordinator::RemoteQqEntryCoordinator>,
        metrics_injector: Option<Arc<dyn RemoteSlMetricsInjector>>,
    ) -> Self {
        Self {
            backend_id: backend_id.into(),
            daemon,
            event_bus,
            tunnels,
            start_modes: Arc::new(Mutex::new(HashMap::new())),
            pollers: Arc::new(Mutex::new(HashMap::new())),
            qq_entry_coordinator,
            metrics_injector,
        }
    }

    /// 供 bootstrap reconcile 取日志 follow 路径,不暴露整个 daemon
    pub fn daemon_paths(&self) -> &SnowLumaRemotePaths {
        self.daemon.paths()
    }

    /// 冷启动后再开桌面:远端 QQ 仍在跑时恢复隧道注入与 status poller
    pub async fn attach_reconciled_running(
        &self,
        bot_id: BotId,
        pid: u32,
        config: &BotConfig,
    ) -> Result<(), BotBackendError> {
        self.daemon.ensure_running_for_reconcile().await?;
        let endpoints = self
            .daemon
            .tunnel_endpoints()
            .await
            .ok_or_else(|| BotBackendError::Io("SnowLuma 隧道未建立".into()))?;
        let http = inject_via_tunnel(&endpoints, pid).await?;
        self.event_bus
            .publish(DomainEvent::snowluma_bot_injected(bot_id.clone(), pid));
        self.event_bus
            .publish(DomainEvent::SnowLumaDockerEndpointsReady {
                bot_id: bot_id.clone(),
            });
        self.start_modes
            .lock()
            .await
            .insert(bot_id.clone(), resolve_start_mode(config));

        if self.pollers.lock().await.contains_key(&bot_id) {
            return Ok(());
        }
        let poller_deps = PollerDeps {
            event_bus: Arc::clone(&self.event_bus),
            http,
            proc_tree: Arc::new(crate::snowluma::linux_proc_probe::LinuxSinglePidProbe::new(
                pid,
            )),
            expected_uin: Some(config.bot.qq_id.to_string()),
        };
        let poller = SnowLumaStatusPoller::spawn(bot_id.clone(), pid, poller_deps);
        self.pollers.lock().await.insert(bot_id, poller);
        Ok(())
    }
}

#[async_trait]
impl BotBackend for RemoteSnowLumaBackend {
    fn id(&self) -> &BotId {
        &self.backend_id
    }

    fn kind(&self) -> BackendKind {
        BackendKind::RemoteSsh
    }

    fn flavor(&self) -> BotFlavor {
        BotFlavor::SnowLuma
    }

    async fn start(&self, ctx: &BotStartCtx) -> Result<BotStatus, BotBackendError> {
        let config = ctx
            .bot_config
            .as_ref()
            .ok_or_else(|| BotBackendError::ConfigNotFound(ctx.config.bot_id.clone()))?;
        let qq_id = config.bot.qq_id;
        let qq_id_str = qq_id.to_string();
        let bot_id = ctx.config.bot_id.clone();
        let start_mode = resolve_start_mode(config);

        self.daemon.ensure_running().await?;

        // 指标探针：上传到 ncd-watch/metrics，并重启共享 node 带上 NCD_* / NODE_OPTIONS
        // （失败不阻断启动；多 bot 同机时后启动者覆盖 env，与本机 SL daemon 一致）
        if let Some(inj) = &self.metrics_injector {
            let home = self.daemon.remote_home();
            if let Some(env) = inj
                .prepare(
                    self.daemon.host.as_ref(),
                    home,
                    bot_id.as_str(),
                    config,
                )
                .await
            {
                if let Err(e) = self.daemon.apply_metrics_node_env(Some(env)).await {
                    tracing::warn!(
                        target: "ncd_backend_snowluma::remote",
                        bot_id = %bot_id,
                        %e,
                        "remote SL metrics node env apply failed (start continues)"
                    );
                }
            }
        }

        let paths = self.daemon.paths();
        if let Err(e) =
            render_native_snowluma_config_on_host(self.daemon.host.as_ref(), &bot_id, config, paths)
                .await
        {
            self.daemon.release().await;
            return Err(e);
        }

        let host = self.daemon.host.as_ref();
        let layout = self.daemon.layout();

        let pid = match start_mode {
            SnowLumaStartMode::HotStart => {
                if let Some(pid) = remote_qq_running_pid(host, qq_id).await? {
                    pid
                } else {
                    self.daemon.release().await;
                    return Err(BotBackendError::InvalidConfig(format!(
                        "SnowLuma 热启动：远端未找到已登录 QQ {qq_id} 的进程（qq --no-sandbox -q {qq_id}）。\
                         请先在远端 Xvfb 上启动 QQ，或改为冷启动。"
                    )));
                }
            }
            SnowLumaStartMode::ColdStart => {
                // Ensure the shared remote QQ tree is in vanilla mode *before* we launch
                // a plain QQ for the SnowLuma daemon to inject into. This is serialized
                // per server_id via the coordinator so that a concurrent NC bot start on
                // the same host cannot race the package.json write.
                let install_base = HostPath::from_posix(format!("{}/Napcat", layout.home));
                if let Err(e) = self
                    .qq_entry_coordinator
                    .ensure_for_native(
                        self.daemon.host.as_ref(),
                        self.daemon.server_id(),
                        &install_base,
                    )
                    .await
                {
                    self.daemon.release().await;
                    return Err(BotBackendError::InvalidConfig(format!(
                        "SnowLuma 冷启动前确保纯净 QQ 入口失败: {e}"
                    )));
                }

                match bot_cold_start(host, layout, &qq_id_str, &qq_id_str).await {
                    Ok(pid) => pid,
                    Err(e) => {
                        self.daemon.release().await;
                        return Err(e);
                    }
                }
            }
        };

        let endpoints = self
            .daemon
            .tunnel_endpoints()
            .await
            .ok_or_else(|| BotBackendError::Io("SnowLuma 隧道未建立".into()))?;
        let http = match inject_via_tunnel(&endpoints, pid).await {
            Ok(c) => c,
            Err(e) => {
                if start_mode.is_cold() {
                    let _ = bot_stop(host, paths, &qq_id_str).await;
                    if let Ok(tail) =
                        read_remote_log_tail(host, &paths.log_bot_path(&qq_id_str), 40).await
                    {
                        if !tail.trim().is_empty() {
                            return Err(BotBackendError::Io(format!(
                                "{e}\n--- 启动日志末尾 (bot log) ---\n{tail}"
                            )));
                        }
                    }
                }
                self.daemon.release().await;
                return Err(e);
            }
        };

        self.event_bus
            .publish(DomainEvent::snowluma_bot_injected(bot_id.clone(), pid));
        self.start_modes
            .lock()
            .await
            .insert(bot_id.clone(), start_mode);

        let poller_deps = PollerDeps {
            event_bus: Arc::clone(&self.event_bus),
            http,
            proc_tree: Arc::new(crate::snowluma::linux_proc_probe::LinuxSinglePidProbe::new(
                pid,
            )),
            expected_uin: Some(qq_id_str.clone()),
        };
        {
            let mut guard = self.pollers.lock().await;
            if let Some(old) = guard.remove(&bot_id) {
                old.dispose();
            }
            let poller = SnowLumaStatusPoller::spawn(bot_id.clone(), pid, poller_deps);
            guard.insert(bot_id.clone(), poller);
        }

        self.event_bus
            .publish(DomainEvent::SnowLumaDockerEndpointsReady {
                bot_id: bot_id.clone(),
            });

        Ok(BotStatus::running(bot_id, pid, 0))
    }

    async fn stop(&self, bot_id: BotId, _mode: StopMode) -> Result<(), BotBackendError> {
        let qq_id_str = bot_id.as_str();
        let paths = self.daemon.paths();
        let start_mode = self
            .start_modes
            .lock()
            .await
            .remove(&bot_id)
            .unwrap_or(SnowLumaStartMode::ColdStart);

        if start_mode.is_cold() {
            let _ = bot_stop(self.daemon.host.as_ref(), paths, qq_id_str).await;
        }
        if let Some(poller) = self.pollers.lock().await.remove(&bot_id) {
            poller.dispose();
        }
        self.daemon.release().await;
        Ok(())
    }

    async fn status(&self, bot_id: BotId) -> Result<BotStatus, BotBackendError> {
        let qq_id = bot_id.as_str();
        let paths = self.daemon.paths();
        let start_mode = self
            .start_modes
            .lock()
            .await
            .get(&bot_id)
            .copied()
            .unwrap_or(SnowLumaStartMode::ColdStart);

        if start_mode.is_hot() {
            if let Ok(qq_id_u) = qq_id.parse::<u64>() {
                if let Some(pid) = remote_qq_running_pid(self.daemon.host.as_ref(), qq_id_u).await?
                {
                    return Ok(BotStatus::running(bot_id, pid, 0));
                }
            }
            return Ok(BotStatus::stopped(bot_id));
        }

        let status_path = paths.status_bot_path(qq_id);
        match self
            .daemon
            .host
            .read_file(&HostPath::from_posix(&status_path))
            .await
        {
            Ok(bytes) => {
                let status: Value = serde_json::from_slice(&bytes)
                    .map_err(|e| BotBackendError::Json(e.to_string()))?;
                let running = status.get("running").and_then(|v| v.as_bool()) == Some(true);
                if running {
                    let pid = status
                        .get("pid")
                        .and_then(|v| v.as_u64())
                        .map(|p| p as u32)
                        .unwrap_or(0);
                    return Ok(BotStatus::running(bot_id, pid, 0));
                }
                Ok(BotStatus::stopped(bot_id))
            }
            Err(_) => Ok(BotStatus::stopped(bot_id)),
        }
    }

    async fn read_config(&self, bot_id: BotId) -> Result<BotRuntimeConfig, BotBackendError> {
        Err(BotBackendError::ConfigNotFound(bot_id))
    }

    async fn write_config(
        &self,
        _bot_id: BotId,
        _cfg: &BotRuntimeConfig,
    ) -> Result<(), BotBackendError> {
        Ok(())
    }

    async fn tail_log(
        &self,
        bot_id: BotId,
        opts: TailOpts,
    ) -> Result<LogSnapshot, BotBackendError> {
        let qq_id = bot_id.as_str();
        let paths = self.daemon.paths();
        let want = if opts.lines > 0 { opts.lines } else { 1000 };
        // saturating_mul 后下界 800 ≤ 上界 20_000，clamp 不会 panic
        let raw_n = want.saturating_mul(5).clamp(800, 20_000);
        let host = self.daemon.host.as_ref();
        let bot_path = paths.log_bot_path(qq_id);
        let daemon_raw: Vec<String> = read_remote_log_tail_lines(host, &paths.log_daemon, raw_n)
            .await
            .unwrap_or_default();
        let bot_raw: Vec<String> = read_remote_log_tail_lines(host, &bot_path, raw_n)
            .await
            .unwrap_or_default();
        let mut lines = prepare_snowluma_bot_history_lines(bot_raw, daemon_raw, qq_id);
        let total = lines.len();
        if lines.len() > want {
            lines = lines.split_off(lines.len() - want);
        }
        Ok(LogSnapshot {
            lines,
            total_lines: total,
        })
    }
}
