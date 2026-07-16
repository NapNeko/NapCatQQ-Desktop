//! 远端主机共享 SnowLuma daemon（图形栈 + node + 隧道）

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use ncd_domain::domain_event::DomainEvent;
use ncd_host::Host;
use ncd_traits::events::{BroadcastEventBus, EventBus};
use ncd_traits::runtime_backend::BotBackendError;
use tokio::sync::Mutex;

use super::layout::{
    DEFAULT_WEBUI_PORT, RemoteSnowLumaLayout, SnowLumaRemotePaths, probe_remote_snowluma_layout,
};
use super::orchestrator::{
    daemon_start, daemon_stop, remote_daemon_already_ready, wait_webui_tcp, write_status_daemon_json,
};
use super::stack::restart_node_with_env;
use super::tunnel::{RemoteSnowLumaTunnelEndpoints, RemoteSnowLumaTunnelRegistry};
use crate::snowluma::daemon::DaemonState;

use super::config::ensure_remote_daemon_prereqs;
use super::helpers::read_remote_file_trimmed;

/// 单台远端主机共享的 SL daemon(单例图形栈 + node);多 Bot 共用,按 qq_id 分别启停 QQ
pub struct RemoteSnowLumaDaemon {
    pub(crate) host: Arc<dyn Host>,
    layout: RemoteSnowLumaLayout,
    server_id: String,
    refcount: Mutex<u32>,
    /// 同一 SSH 主机上多 Bot 并发 start 时,整段启栈(Xvfb/x11vnc/node)单飞,避免抢 5900 等端口
    stack_bootstrap: Mutex<()>,
    tunnels: Arc<RemoteSnowLumaTunnelRegistry>,
    event_bus: Arc<BroadcastEventBus>,
    tunnel_eps: Mutex<Option<RemoteSnowLumaTunnelEndpoints>>,
    /// 最近一次应用到共享 node 的 metrics env（多 bot 时后启动者覆盖，与本机 daemon 一致）
    metrics_node_env: Mutex<Option<BTreeMap<String, String>>>,
}

impl RemoteSnowLumaDaemon {
    pub async fn new(
        server_id: String,
        host: Arc<dyn Host>,
        tunnels: Arc<RemoteSnowLumaTunnelRegistry>,
        event_bus: Arc<BroadcastEventBus>,
    ) -> Result<Self, BotBackendError> {
        let layout = probe_remote_snowluma_layout(host.as_ref()).await?;
        Ok(Self {
            host,
            layout,
            server_id,
            refcount: Mutex::new(0),
            stack_bootstrap: Mutex::new(()),
            tunnels,
            event_bus,
            tunnel_eps: Mutex::new(None),
            metrics_node_env: Mutex::new(None),
        })
    }

    /// 应用 metrics env 到共享 node：与当前已应用 env 不同则重启 node 并等 WebUI。
    /// 调用方应在 ensure_running 之后调用（栈已就绪）。
    pub async fn apply_metrics_node_env(
        &self,
        env: Option<BTreeMap<String, String>>,
    ) -> Result<(), BotBackendError> {
        let mut guard = self.metrics_node_env.lock().await;
        if *guard == env {
            return Ok(());
        }
        let host = self.host.as_ref();
        let layout = &self.layout;
        restart_node_with_env(host, layout, env.as_ref()).await?;
        wait_webui_tcp(host, DEFAULT_WEBUI_PORT, Duration::from_secs(60)).await?;
        *guard = env;
        Ok(())
    }

    pub fn server_id(&self) -> &str {
        &self.server_id
    }

    pub fn remote_home(&self) -> &str {
        &self.layout.home
    }

    pub fn paths(&self) -> &SnowLumaRemotePaths {
        &self.layout.paths
    }

    pub fn layout(&self) -> &RemoteSnowLumaLayout {
        &self.layout
    }

    pub async fn ensure_running(&self) -> Result<(), BotBackendError> {
        let _stack_guard = self.stack_bootstrap.lock().await;

        self.event_bus
            .publish(DomainEvent::snowluma_daemon_state_changed(
                DaemonState::Starting,
                0,
                None,
                Some(self.server_id.clone()),
            ));

        ensure_remote_daemon_prereqs(self.host.as_ref(), &self.layout.home, &self.layout.paths)
            .await?;

        let stack_up = remote_daemon_already_ready(self.host.as_ref(), &self.layout.paths).await?;
        if !stack_up {
            daemon_start(self.host.as_ref(), &self.layout).await?;
            wait_webui_tcp(
                self.host.as_ref(),
                DEFAULT_WEBUI_PORT,
                Duration::from_secs(90),
            )
            .await?;
        } else {
            wait_webui_tcp(
                self.host.as_ref(),
                DEFAULT_WEBUI_PORT,
                Duration::from_secs(30),
            )
            .await?;
        }
        write_status_daemon_json(self.host.as_ref(), &self.layout.paths, true, true).await?;

        let webui_plain =
            read_remote_file_trimmed(self.host.as_ref(), &self.layout.paths.webui_secret).await?;
        let vnc_plain =
            read_remote_file_trimmed(self.host.as_ref(), &self.layout.paths.vnc_secret).await?;
        if webui_plain.is_empty() {
            return Err(BotBackendError::InvalidConfig(
                "远端 webui.secret 为空，无法建立 SnowLuma WebUI 隧道".into(),
            ));
        }

        let eps = self
            .tunnels
            .acquire(&self.server_id, self.host.as_ref(), webui_plain, vnc_plain)
            .await
            .map_err(|e| BotBackendError::Io(format!("SnowLuma SSH 隧道: {e}")))?;
        *self.tunnel_eps.lock().await = Some(eps);

        let mut guard = self.refcount.lock().await;
        *guard = guard.saturating_add(1);
        let rc = *guard;
        drop(guard);

        self.event_bus
            .publish(DomainEvent::snowluma_daemon_state_changed(
                DaemonState::Ready,
                rc,
                None,
                Some(self.server_id.clone()),
            ));
        Ok(())
    }

    pub async fn release(&self) {
        let mut guard = self.refcount.lock().await;
        if *guard == 0 {
            return;
        }
        *guard -= 1;
        let rc = *guard;
        let stop_daemon = rc == 0;
        drop(guard);

        self.tunnels.release(&self.server_id).await;

        if stop_daemon {
            let _ = daemon_stop(self.host.as_ref(), &self.layout.paths).await;
            *self.tunnel_eps.lock().await = None;
            self.event_bus
                .publish(DomainEvent::snowluma_daemon_state_changed(
                    DaemonState::Stopped,
                    0,
                    None,
                    Some(self.server_id.clone()),
                ));
        }
    }

    pub async fn tunnel_endpoints(&self) -> Option<RemoteSnowLumaTunnelEndpoints> {
        self.tunnel_eps.lock().await.clone()
    }

    /// 桌面退出:只拆掉本机 SSH 隧道,不 stop 远端 daemon / QQ
    pub async fn detach_local_sessions(&self) {
        *self.tunnel_eps.lock().await = None;
        self.tunnels.release(&self.server_id).await;
    }

    /// 冷启动 reconcile:远端进程已在跑时只补隧道与状态文件,不重复 daemon start shell
    pub async fn ensure_running_for_reconcile(&self) -> Result<(), BotBackendError> {
        ensure_remote_daemon_prereqs(self.host.as_ref(), &self.layout.home, &self.layout.paths)
            .await?;

        if !super::orchestrator::remote_daemon_already_ready(
            self.host.as_ref(),
            &self.layout.paths,
        )
        .await?
        {
            return Err(BotBackendError::InvalidConfig(
                "bootstrap reconcile: 远端 SnowLuma daemon 未就绪，请手动启动 Bot".into(),
            ));
        }

        write_status_daemon_json(self.host.as_ref(), &self.layout.paths, true, true).await?;

        let webui_plain =
            read_remote_file_trimmed(self.host.as_ref(), &self.layout.paths.webui_secret).await?;
        let vnc_plain =
            read_remote_file_trimmed(self.host.as_ref(), &self.layout.paths.vnc_secret).await?;
        if webui_plain.is_empty() {
            return Err(BotBackendError::InvalidConfig(
                "远端 webui.secret 为空，无法建立 SnowLuma WebUI 隧道".into(),
            ));
        }

        let eps = self
            .tunnels
            .acquire(&self.server_id, self.host.as_ref(), webui_plain, vnc_plain)
            .await
            .map_err(|e| BotBackendError::Io(format!("SnowLuma SSH 隧道: {e}")))?;
        *self.tunnel_eps.lock().await = Some(eps);

        let mut guard = self.refcount.lock().await;
        if *guard == 0 {
            *guard = 1;
        }
        drop(guard);

        self.event_bus
            .publish(DomainEvent::snowluma_daemon_state_changed(
                DaemonState::Ready,
                *self.refcount.lock().await,
                None,
                Some(self.server_id.clone()),
            ));
        Ok(())
    }
}
