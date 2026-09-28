//! 对接要的跨机隧道：桌面端握着的 SSH 转发和应用机上常驻的 ssh，建立、复用、对账、拆除

use super::*;

pub(super) struct AppInstanceTunnel {
    app_port: u16,
    topology: AppLinkTopology,
    handle: TunnelHandle,
}

/// 键上登记着同一个口、同一种拓扑、`reach` 那一侧也分到了口的隧道
fn live_tunnel_port(
    map: &HashMap<String, AppInstanceTunnel>,
    key: &str,
    app_port: u16,
    topology: AppLinkTopology,
    reach: fn(&TunnelHandle) -> u16,
) -> Option<u16> {
    map.get(key)
        .filter(|t| t.app_port == app_port && t.topology == topology)
        .map(|t| reach(&t.handle))
        .filter(|port| *port != 0)
}

/// 正向对接时 Bot 在自己机上开的那个服务口
fn forward_listen_port(bot: &BotConfig, connection_name: &str) -> Option<u16> {
    bot.connect
        .websocket_servers
        .iter()
        .find(|s| s.base.name == connection_name && s.port > 0)
        .map(|s| s.port)
}

impl AppManager {
    pub(super) async fn ensure_link_tunnel(
        &self,
        instance: &AppInstance,
        bot: &BotConfig,
    ) -> Result<u16, AppFrameworkError> {
        match classify_app_link(&bot.bot.runtime_target, &instance.host_id) {
            Some(AppLinkTopology::SameHost) => Ok(instance.port),
            Some(AppLinkTopology::LocalBotRemoteApp) => {
                self.ensure_local_to_remote_tunnel(instance).await
            }
            Some(AppLinkTopology::RemoteBotLocalApp) => {
                self.ensure_remote_to_local_tunnel(instance, bot).await
            }
            Some(AppLinkTopology::RemoteBotRemoteApp) => Err(AppFrameworkError::Validation(
                "两台远端对接不走桌面隧道".into(),
            )),
            None => Err(unsupported_link_topology(
                &bot.bot.runtime_target,
                &instance.host_id,
            )),
        }
    }

    pub(super) async fn ensure_local_to_remote_tunnel(
        &self,
        instance: &AppInstance,
    ) -> Result<u16, AppFrameworkError> {
        self.ensure_local_to_remote_port_tunnel(
            instance,
            instance.port,
            instance.id.as_str().to_string(),
        )
        .await
    }

    pub(super) async fn ensure_local_to_remote_port_tunnel(
        &self,
        instance: &AppInstance,
        remote_port: u16,
        key: String,
    ) -> Result<u16, AppFrameworkError> {
        if server_id_of_host(&instance.host_id).is_none() {
            return Ok(remote_port);
        }
        if remote_port == 0 {
            return Err(AppFrameworkError::Validation("应用实例端口无效".into()));
        }
        self.ensure_desktop_tunnel(
            key,
            remote_port,
            AppLinkTopology::LocalBotRemoteApp,
            TunnelHandle::local_port,
            "SSH 隧道未分配本地端口",
            async {
                let host = self.resolve_host(&instance.host_id).await?;
                host.open_tunnel(TunnelSpec::local_to_remote(0, remote_port))
                    .await
                    .map_err(host_err)
            },
        )
        .await
    }

    async fn ensure_remote_to_local_tunnel(
        &self,
        instance: &AppInstance,
        bot: &BotConfig,
    ) -> Result<u16, AppFrameworkError> {
        if instance.port == 0 {
            return Err(AppFrameworkError::Validation("应用实例端口无效".into()));
        }
        self.ensure_desktop_tunnel(
            instance.id.as_str().to_string(),
            instance.port,
            AppLinkTopology::RemoteBotLocalApp,
            TunnelHandle::remote_listen_port,
            "SSH 隧道未分配远端端口",
            async {
                let host = self
                    .resolve_host(&host_id_of_runtime_target(&bot.bot.runtime_target))
                    .await?;
                host.open_tunnel(TunnelSpec::remote_to_local(0, instance.port))
                    .await
                    .map_err(host_err)
            },
        )
        .await
    }

    /// 桌面端握着的隧道：键上有能用的就复用，没有就现开一条登记上。开隧道要走 SSH，不能拿着表锁等；
    /// 开完再看一眼，并发的另一次已经登记了能用的就用它的，刚开的这条随手丢掉。
    /// `reach` 取调用方要的那一侧：`-L` 是本机听口，`-R` 是远端听口
    async fn ensure_desktop_tunnel(
        &self,
        key: String,
        app_port: u16,
        topology: AppLinkTopology,
        reach: fn(&TunnelHandle) -> u16,
        unassigned: &str,
        open: impl Future<Output = Result<TunnelHandle, AppFrameworkError>>,
    ) -> Result<u16, AppFrameworkError> {
        if let Some(port) = self
            .reuse_or_evict_tunnel(&key, app_port, topology, reach)
            .await
        {
            return Ok(port);
        }
        let handle = open.await?;
        let port = reach(&handle);
        if port == 0 {
            return Err(AppFrameworkError::Host(unassigned.to_string()));
        }
        let mut map = self.tunnels.lock().await;
        if let Some(existing) = live_tunnel_port(&map, &key, app_port, topology, reach) {
            return Ok(existing);
        }
        map.insert(
            key,
            AppInstanceTunnel {
                app_port,
                topology,
                handle,
            },
        );
        Ok(port)
    }

    /// 键上的隧道还能用就给出它的口；用不了先摘掉，调用方再去现开
    async fn reuse_or_evict_tunnel(
        &self,
        key: &str,
        app_port: u16,
        topology: AppLinkTopology,
        reach: fn(&TunnelHandle) -> u16,
    ) -> Option<u16> {
        let mut map = self.tunnels.lock().await;
        if let Some(port) = live_tunnel_port(&map, key, app_port, topology, reach) {
            return Some(port);
        }
        map.remove(key);
        None
    }

    pub(super) async fn drop_instance_tunnel(&self, id: &AppInstanceId) {
        let id_str = id.as_str().to_string();
        let webui = format!("{id_str}:webui");
        let mut map = self.tunnels.lock().await;
        map.remove(&id_str);
        map.remove(&webui);
        map.remove(&forward_tunnel_key(id));
    }

    /// 正向跨机、桌面端握着的隧道：应用端在自己机的回环上连 Q，隧道把 Q 接到 Bot 机回环上的 `bot_port`。
    /// 远端 Bot + 本机应用：桌面端 `-L`，Q 在本机；本机 Bot + 远端应用：桌面端对应用机开 `-R`，Q 在应用机。
    /// Q 先试 `preferred`（适配器配置里现有的口，重开隧道时不用改它的配置），占了再让系统分配
    pub(super) async fn ensure_forward_tunnel(
        &self,
        instance: &AppInstance,
        bot: &BotConfig,
        topology: AppLinkTopology,
        bot_port: u16,
        preferred: Option<u16>,
    ) -> Result<u16, AppFrameworkError> {
        if bot_port == 0 {
            return Err(AppFrameworkError::Validation(
                "协议 Bot 的 WS 服务还没分配端口".into(),
            ));
        }
        let key = forward_tunnel_key(&instance.id);
        let reach: fn(&TunnelHandle) -> u16 = match topology {
            AppLinkTopology::RemoteBotLocalApp => TunnelHandle::local_port,
            _ => TunnelHandle::remote_listen_port,
        };
        if let Some(port) = self
            .reuse_or_evict_tunnel(&key, bot_port, topology, reach)
            .await
        {
            return Ok(port);
        }
        let tunnel_host = match topology {
            AppLinkTopology::RemoteBotLocalApp => {
                self.resolve_host(&host_id_of_runtime_target(&bot.bot.runtime_target))
                    .await?
            }
            AppLinkTopology::LocalBotRemoteApp => self.resolve_host(&instance.host_id).await?,
            _ => {
                return Err(AppFrameworkError::Validation(
                    "这种拓扑不走桌面端隧道".into(),
                ));
            }
        };
        // Q 都是隧道的听口：`-L` 听在本机，`-R` 听在应用机
        let spec_for = |q: u16| match topology {
            AppLinkTopology::RemoteBotLocalApp => TunnelSpec::local_to_remote(q, bot_port),
            _ => TunnelSpec::remote_to_local(q, bot_port),
        };
        let first = match preferred.filter(|p| *p != 0) {
            Some(q) => tunnel_host.open_tunnel(spec_for(q)).await.ok(),
            None => None,
        };
        let handle = match first {
            Some(h) => h,
            None => tunnel_host
                .open_tunnel(spec_for(0))
                .await
                .map_err(host_err)?,
        };
        let q = reach(&handle);
        if q == 0 {
            return Err(AppFrameworkError::Host("SSH 隧道没分到端口".into()));
        }
        self.tunnels.lock().await.insert(
            key,
            AppInstanceTunnel {
                app_port: bot_port,
                topology,
                handle,
            },
        );
        Ok(q)
    }

    pub(super) async fn ensure_resident_link(
        &self,
        instance: &AppInstance,
        bot: &BotConfig,
        bot_id: &BotId,
    ) -> Result<u16, AppFrameworkError> {
        let bot_host_id = host_id_of_runtime_target(&bot.bot.runtime_target);
        let bot_host = self.resolve_host(&bot_host_id).await?;
        let app_host = self.resolve_host(&instance.host_id).await?;
        let taken = self.taken_resident_ports(&instance.id, &bot_host_id).await;
        let reuse = instance
            .link
            .as_ref()
            .filter(|l| &l.bot_id == bot_id)
            .and_then(|l| l.resident_forward_port);
        resident_link::ensure_resident_link(
            app_host.as_ref(),
            bot_host.as_ref(),
            ResidentLinkSpec {
                instance,
                reuse_port: reuse,
                taken_ports: &taken,
                forward: ResidentForward::ExposeApp {
                    app_port: instance.port,
                },
            },
        )
        .await
    }

    /// 正向的两台远端：应用机常驻 `ssh -L`，把 Bot 机回环上的 `bot_port` 挂到应用机回环上，返回应用机上的口
    pub(super) async fn ensure_resident_forward_link(
        &self,
        instance: &AppInstance,
        bot: &BotConfig,
        bot_id: &BotId,
        bot_port: u16,
    ) -> Result<u16, AppFrameworkError> {
        let bot_host = self
            .resolve_host(&host_id_of_runtime_target(&bot.bot.runtime_target))
            .await?;
        let app_host = self.resolve_host(&instance.host_id).await?;
        let taken = self
            .taken_forward_listen_ports(&instance.id, &instance.host_id)
            .await;
        let reuse = instance
            .link
            .as_ref()
            .filter(|l| &l.bot_id == bot_id && l.mode == OneBotLinkMode::ForwardWs)
            .and_then(|l| l.resident_forward_port);
        resident_link::ensure_resident_link(
            app_host.as_ref(),
            bot_host.as_ref(),
            ResidentLinkSpec {
                instance,
                reuse_port: reuse,
                taken_ports: &taken,
                forward: ResidentForward::ReachBot { bot_port },
            },
        )
        .await
    }

    /// 应用机上已经用掉的口：那台机上的实例口，加别的正向常驻隧道在那台机上的听口
    async fn taken_forward_listen_ports(
        &self,
        current: &AppInstanceId,
        app_host_id: &str,
    ) -> Vec<u16> {
        let mut taken = Vec::new();
        for inst in self.store.list().await {
            if inst.host_id != app_host_id {
                continue;
            }
            taken.push(inst.port);
            if inst.id == *current {
                continue;
            }
            if let Some(port) = inst
                .link
                .as_ref()
                .filter(|l| l.mode == OneBotLinkMode::ForwardWs)
                .and_then(|l| l.resident_forward_port)
            {
                taken.push(port);
            }
        }
        taken
    }

    async fn taken_resident_ports(&self, current: &AppInstanceId, bot_host_id: &str) -> Vec<u16> {
        let mut taken = Vec::new();
        for inst in self.store.list().await {
            if inst.host_id == bot_host_id {
                taken.push(inst.port);
            }
            // 正向的常驻口开在应用机上，不占 Bot 机的口
            let Some(link) = inst
                .link
                .as_ref()
                .filter(|l| l.mode == OneBotLinkMode::ReverseWs)
            else {
                continue;
            };
            let Some(port) = link.resident_forward_port else {
                continue;
            };
            if inst.id == *current {
                continue;
            }
            let Ok(Some(other)) = self.bot_manager.bot_config(&link.bot_id).await else {
                continue;
            };
            if host_id_of_runtime_target(&other.bot.runtime_target) == bot_host_id {
                taken.push(port);
            }
        }
        taken
    }

    pub(super) async fn teardown_resident_best_effort(
        &self,
        instance: &AppInstance,
        bot_id: &BotId,
    ) {
        let app_host = match self.resolve_host(&instance.host_id).await {
            Ok(h) => h,
            Err(e) => {
                tracing::warn!(
                    instance = instance.id.as_str(),
                    error = %e,
                    "resident link teardown skipped (app host)"
                );
                return;
            }
        };
        let bot_host = match self.bot_manager.bot_config(bot_id).await {
            Ok(Some(bot)) => self
                .resolve_host(&host_id_of_runtime_target(&bot.bot.runtime_target))
                .await
                .ok(),
            _ => None,
        };
        if let Err(e) = resident_link::teardown_resident_link(
            app_host.as_ref(),
            bot_host.as_ref().map(|h| h.as_ref()),
            &instance.id,
        )
        .await
        {
            tracing::warn!(
                instance = instance.id.as_str(),
                error = %e,
                "resident link teardown failed"
            );
        }
    }

    pub(super) async fn reconcile_resident_link(
        &self,
        instance: &AppInstance,
    ) -> Result<(), AppFrameworkError> {
        let Some(link) = instance.link.as_ref() else {
            return Ok(());
        };
        let Some(fwd) = link.resident_forward_port else {
            return Ok(());
        };
        let Some(bot) = self
            .bot_manager
            .bot_config(&link.bot_id)
            .await
            .map_err(AppFrameworkError::Integration)?
        else {
            return Ok(());
        };
        let forward = match link.mode {
            OneBotLinkMode::ReverseWs => ResidentForward::ExposeApp {
                app_port: instance.port,
            },
            OneBotLinkMode::ForwardWs => match forward_listen_port(&bot, &link.connection_name) {
                Some(bot_port) => ResidentForward::ReachBot { bot_port },
                // Bot 侧那条服务被手动删了：隧道重拉了也没东西可连
                None => return Ok(()),
            },
        };
        let app_host = self.resolve_host(&instance.host_id).await?;
        let bot_host = self
            .resolve_host(&host_id_of_runtime_target(&bot.bot.runtime_target))
            .await?;
        resident_link::reconcile_resident_link(
            app_host.as_ref(),
            bot_host.as_ref(),
            instance,
            forward,
            fwd,
        )
        .await
    }

    /// 已对接的跨机实例：重开 Desktop 隧道；分配口变了就再热推 Bot URL。
    pub(super) async fn reconcile_link_tunnel(
        &self,
        instance: &AppInstance,
    ) -> Result<(), AppFrameworkError> {
        let Some(link) = instance.link.as_ref() else {
            return Ok(());
        };
        let Some(mut bot) = self
            .bot_manager
            .bot_config(&link.bot_id)
            .await
            .map_err(AppFrameworkError::Integration)?
        else {
            return Ok(());
        };
        let Some(topology) = classify_app_link(&bot.bot.runtime_target, &instance.host_id) else {
            return Ok(());
        };
        if !needs_desktop_ssh_tunnel(topology) {
            return Ok(());
        }
        if link.mode == OneBotLinkMode::ForwardWs {
            return self
                .reconcile_forward_tunnel(instance, &bot, topology)
                .await;
        }
        let local_port = self.ensure_link_tunnel(instance, &bot).await?;
        let Some(conn) = bot
            .connect
            .websocket_clients
            .iter_mut()
            .find(|c| c.base.name == link.connection_name)
        else {
            return Ok(());
        };
        let next = rewrite_ws_loopback_port(&conn.url, local_port)
            .map_err(AppFrameworkError::Validation)?;
        if next == conn.url {
            return Ok(());
        }
        conn.url = next;
        self.bot_manager
            .upsert_bot_config(bot)
            .await
            .map_err(AppFrameworkError::Integration)?;
        Ok(())
    }

    /// 正向跨机的桌面端隧道跟着桌面端退出就没了：重开时先用应用端配置里的口，
    /// 分到的口变了才把应用端改过去（麦麦的适配器插件盯着自己的配置热加载），Bot 侧的口不动
    async fn reconcile_forward_tunnel(
        &self,
        instance: &AppInstance,
        bot: &BotConfig,
        topology: AppLinkTopology,
    ) -> Result<(), AppFrameworkError> {
        let Some(link) = instance.link.as_ref() else {
            return Ok(());
        };
        let Some(bot_port) = forward_listen_port(bot, &link.connection_name) else {
            return Ok(());
        };
        let adapter = self.registry.get(&instance.framework_id)?;
        let host = self.resolve_host(&instance.host_id).await?;
        let current = adapter_link_port(adapter.as_ref(), host.as_ref(), instance).await;
        let q = self
            .ensure_forward_tunnel(instance, bot, topology, bot_port, current)
            .await?;
        if current == Some(q) {
            return Ok(());
        }
        let mut plan = self
            .plan_link_for(instance, bot, adapter.as_ref(), host.as_ref())
            .await?;
        if let OneBotLinkEndpoint::WsServer(server) = &mut plan.connection {
            server.host = "127.0.0.1".to_string();
            server.port = q;
        }
        adapter.apply_link(host.as_ref(), instance, &plan).await
    }
}
