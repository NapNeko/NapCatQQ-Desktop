//! 远端 SnowLuma BotBackend

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
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
use tokio::sync::Mutex;

use super::layout::SnowLumaRemotePaths;
use super::orchestrator::{bot_cold_start, bot_stop, remember_remote_bot_pid};
use super::tunnel::RemoteSnowLumaTunnelRegistry;
use crate::snowluma::log_noise::prepare_snowluma_bot_history_lines;
use crate::snowluma::status_poller::{PollerDeps, SnowLumaStatusPoller};

use super::config::{render_native_snowluma_config_on_host, resolve_start_mode};
use super::daemon::RemoteSnowLumaDaemon;
use super::helpers::{read_remote_log_tail, read_remote_log_tail_lines};
use super::inject::{inject_via_tunnel, remote_qq_running_pid_with_hint};

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
    /// Shared coordinator for flipping the selected QQ tree entry point.
    /// Passed from BotManager so that NC and SL cold starts on the same server_id
    /// serialize their package.json main changes.
    qq_entry_coordinator: Arc<ncd_deploy::remote_coordinator::RemoteQqEntryCoordinator>,
    metrics_injector: Option<Arc<dyn RemoteSlMetricsInjector>>,
    /// 本机 `state/snowluma`，用来读全局 WebUI 固定密码覆盖
    snowluma_data_root: Option<PathBuf>,
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
            snowluma_data_root: None,
        }
    }

    pub fn with_snowluma_data_root(mut self, path: impl Into<PathBuf>) -> Self {
        self.snowluma_data_root = Some(path.into());
        self
    }

    fn webui_password_override(&self) -> Option<String> {
        let root = self.snowluma_data_root.as_ref()?;
        let cfg = crate::snowluma::session::load_snowluma_app_config(root);
        let trimmed = cfg.webui_password_override.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    }

    async fn dispose_poller(&self, bot_id: &BotId) {
        if let Some(poller) = self.pollers.lock().await.remove(bot_id) {
            poller.dispose();
        }
    }

    /// 供 bootstrap reconcile 取日志 follow 路径,不暴露整个 daemon
    pub fn daemon_paths(&self) -> &SnowLumaRemotePaths {
        self.daemon.paths()
    }

    pub fn qq_bin(&self) -> &str {
        &self.daemon.layout().qq_bin
    }

    /// 冷启动后再开桌面 / 导入：记下 QQ pid；WebUI 注入失败不阻断运行态。
    pub async fn attach_reconciled_running(
        &self,
        bot_id: BotId,
        pid: u32,
        config: &BotConfig,
    ) -> Result<(), BotBackendError> {
        let qq_id = config.bot.qq_id.to_string();
        let host = self.daemon.current_host().await;
        if let Err(err) =
            remember_remote_bot_pid(host.as_ref(), self.daemon.paths(), &qq_id, pid).await
        {
            tracing::warn!(
                target: "ncd_runtime::remote_snowluma",
                bot_id = %bot_id,
                %err,
                "reconcile: 未能写下 pid_bot（仍按进程认运行）"
            );
        }

        self.start_modes
            .lock()
            .await
            .insert(bot_id.clone(), resolve_start_mode(config));

        if let Err(err) = self.daemon.ensure_running_for_reconcile().await {
            tracing::warn!(
                target: "ncd_runtime::remote_snowluma",
                bot_id = %bot_id,
                %err,
                "reconcile: SnowLuma 隧道未建立，跳过 WebUI 注入"
            );
            return Ok(());
        }
        let Some(endpoints) = self.daemon.tunnel_endpoints().await else {
            tracing::info!(
                target: "ncd_runtime::remote_snowluma",
                bot_id = %bot_id,
                "reconcile: 无 WebUI 隧道（node 未在或端口未解析），仅同步运行态"
            );
            return Ok(());
        };
        let http = match inject_via_tunnel(&endpoints, pid).await {
            Ok(c) => c,
            Err(err) => {
                tracing::warn!(
                    target: "ncd_runtime::remote_snowluma",
                    bot_id = %bot_id,
                    pid,
                    %err,
                    "reconcile: WebUI 注入失败（密码未知或端口刚换），Bot 仍标为运行中"
                );
                return Ok(());
            }
        };
        self.event_bus
            .publish(DomainEvent::snowluma_bot_injected(bot_id.clone(), pid));
        self.event_bus
            .publish(DomainEvent::SnowLumaDockerEndpointsReady {
                bot_id: bot_id.clone(),
            });

        if self.pollers.lock().await.contains_key(&bot_id) {
            return Ok(());
        }
        let poller_deps = PollerDeps {
            event_bus: Arc::clone(&self.event_bus),
            http,
            proc_tree: Arc::new(crate::snowluma::linux_proc_probe::LinuxSinglePidProbe::new(
                pid,
            )),
            expected_uin: Some(qq_id),
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

        // 指标资产先上传，再随 node 第一次拉起带上 env，避免先听口再 SIGTERM 重启
        // 导致隧道打到已死的旧 WebUI 口、界面卡在启动中。
        let host = self.daemon.current_host().await;
        let metrics_env = if let Some(inj) = &self.metrics_injector {
            let home = self.daemon.remote_home();
            inj.prepare(host.as_ref(), home, bot_id.as_str(), config)
                .await
        } else {
            None
        };

        // 接管会换密并重启共享 node。旧 poller 仍持上一轮明文，401 会计入
        // 上游按 IP 的 5 次锁；必须先停掉再写盘。
        self.dispose_poller(&bot_id).await;

        // 接管开关：用户显式勾选后每次启动覆盖远端 WebUI 凭据并落 secret。
        // 有全局固定密码就用固定的，否则生成新密码。SnowLuma 只在进程启动时
        // 加载 webui.json，所以覆盖后必须重启已在跑的 node，否则登录 401。
        // 未勾选绝不碰现有配置。
        if config.bot.webui_password_takeover {
            let override_pwd = self.webui_password_override();
            let takeover_pwd = super::config::take_over_remote_webui_credentials(
                host.as_ref(),
                self.daemon.paths(),
                override_pwd.as_deref(),
            )
            .await?;
            tracing::info!(
                target: "ncd_backend_snowluma::remote",
                bot_id = %bot_id,
                used_override = override_pwd.is_some(),
                "已按接管设置重写远端 SnowLuma WebUI 凭据"
            );
            self.daemon
                .ensure_running_with_metrics(true, metrics_env, Some(takeover_pwd))
                .await?;
        } else {
            self.daemon
                .ensure_running_with_metrics(false, metrics_env, None)
                .await?;
        }

        let paths = self.daemon.paths();
        if let Err(e) =
            render_native_snowluma_config_on_host(host.as_ref(), &bot_id, config, paths).await
        {
            self.daemon.release().await;
            return Err(e);
        }

        let layout = self.daemon.layout();

        let pid = match start_mode {
            SnowLumaStartMode::HotStart => {
                if let Some(pid) = remote_qq_running_pid_with_hint(
                    host.as_ref(),
                    qq_id,
                    Some(&paths.pid_bot_path(&qq_id_str)),
                    Some(&layout.qq_bin),
                    false,
                )
                .await?
                {
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
                let install_base = HostPath::from_posix(layout.qq_install_base.clone());
                if let Err(e) = self
                    .qq_entry_coordinator
                    .ensure_for_native(host.as_ref(), self.daemon.server_id(), &install_base)
                    .await
                {
                    self.daemon.release().await;
                    return Err(BotBackendError::InvalidConfig(format!(
                        "SnowLuma 冷启动前确保纯净 QQ 入口失败: {e}"
                    )));
                }

                match bot_cold_start(host.as_ref(), layout, &qq_id_str, &qq_id_str).await {
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
                    let _ = bot_stop(host.as_ref(), paths, &qq_id_str).await;
                    if let Ok(tail) =
                        read_remote_log_tail(host.as_ref(), &paths.log_bot_path(&qq_id_str), 40)
                            .await
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
            self.dispose_poller(&bot_id).await;
            let poller = SnowLumaStatusPoller::spawn(bot_id.clone(), pid, poller_deps);
            self.pollers.lock().await.insert(bot_id.clone(), poller);
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

        // 先停 poller：否则停 QQ / 释放 daemon 期间它还会拿旧 token 打登录。
        self.dispose_poller(&bot_id).await;
        if start_mode.is_cold() {
            let host = self.daemon.current_host().await;
            let _ = bot_stop(host.as_ref(), paths, qq_id_str).await;
        }
        self.daemon.release().await;
        Ok(())
    }

    async fn status(&self, bot_id: BotId) -> Result<BotStatus, BotBackendError> {
        let qq_id = bot_id.as_str();
        let paths = self.daemon.paths();
        if let Ok(qq_id_u) = qq_id.parse::<u64>() {
            let host = self.daemon.current_host().await;
            let pid_file = paths.pid_bot_path(qq_id);
            if let Some(pid) = remote_qq_running_pid_with_hint(
                host.as_ref(),
                qq_id_u,
                Some(pid_file.as_str()),
                Some(self.daemon.layout().qq_bin.as_str()),
                false,
            )
            .await?
            {
                return Ok(BotStatus::running(bot_id, pid, 0));
            }
        }
        Ok(BotStatus::stopped(bot_id))
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
        let host = self.daemon.current_host().await;
        // 历史快照与实时跟随共用同一解析：外来安装的真实日志在 framework 自带
        // logs 目录，布局路径可能不存在（空快照会让页面在无增量时永远空白）
        let resolved = super::probe::resolve_remote_snowluma_log_targets(
            host.as_ref(),
            paths,
            &paths.log_bot_path(qq_id),
        )
        .await;
        let same_source = resolved.bot == resolved.daemon;
        let daemon_raw: Vec<String> =
            read_remote_log_tail_lines(host.as_ref(), &resolved.daemon, raw_n)
                .await
                .unwrap_or_default();
        let bot_raw: Vec<String> = if same_source {
            // 两源同文件时只走 daemon 侧（带会话裁剪与 UIN 收窄），避免整段重复
            Vec::new()
        } else {
            read_remote_log_tail_lines(host.as_ref(), &resolved.bot, raw_n)
                .await
                .unwrap_or_default()
        };
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
