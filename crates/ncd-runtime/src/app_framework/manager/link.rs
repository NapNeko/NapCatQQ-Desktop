//! 协议 Bot 一键对接应用端：发现已有对接、预览、应用、解绑，以及往 Bot 配置里按连接名 upsert

use super::*;

impl AppManager {
    pub(super) async fn discover_existing_link(
        &self,
        instance: &AppInstance,
    ) -> Option<AppLinkRecord> {
        let bots = self.bot_manager.list_bot_configs_for_link().await.ok()?;
        if bots.is_empty() {
            return None;
        }
        let host = self.resolve_host(&instance.host_id).await.ok()?;
        let adapter = self.registry.get(&instance.framework_id).ok()?;
        let token = adapter
            .read_access_token(host.as_ref(), instance)
            .await
            .ok()
            .flatten();
        let outbound = adapter
            .read_outbound_ws_urls(host.as_ref(), instance)
            .await
            .unwrap_or_default();
        let claimed = self.claimed_link_names(&instance.id).await;
        super::existing_link::discover_existing_link(
            &instance.host_id,
            instance.port,
            token.as_deref(),
            &outbound,
            &claimed,
            &bots,
        )
    }

    async fn claimed_link_names(&self, except: &AppInstanceId) -> HashSet<(String, String)> {
        self.store
            .list()
            .await
            .into_iter()
            .filter(|i| &i.id != except)
            .filter_map(|i| {
                let link = i.link?;
                if link.connection_name.is_empty() {
                    return None;
                }
                Some((link.bot_id.as_str().to_string(), link.connection_name))
            })
            .collect()
    }

    /// 导入实例:已有反向 / 正向 WS 能唯一对上协议 Bot 时补上 link,不改双方配置
    pub(super) async fn adopt_existing_link(
        &self,
        id: &AppInstanceId,
    ) -> Result<AppInstance, AppFrameworkError> {
        let instance = self.store.require(id).await?;
        if instance.link.is_some() || !instance.origin.is_imported() {
            return Ok(instance);
        }
        let Some(found) = self.discover_existing_link(&instance).await else {
            return Ok(instance);
        };
        let updated = self
            .store
            .update(id, |i| {
                if i.link.is_none() {
                    i.link = Some(found);
                }
            })
            .await?;
        if updated.link.is_some() {
            self.publish(&updated, "linked");
        }
        Ok(updated)
    }

    /// 预览：不写任何东西，只算计划
    pub async fn preview_link(
        &self,
        instance_id: &AppInstanceId,
        bot_id: &BotId,
    ) -> Result<OneBotLinkPlan, AppFrameworkError> {
        let (instance, bot, adapter, host) = self.link_context(instance_id, bot_id).await?;
        self.plan_link_for(&instance, &bot, adapter.as_ref(), host.as_ref())
            .await
    }

    /// 应用：写应用端 → Bot 侧 upsert 连接 + 热推 → 记 link；Bot 侧失败回滚应用端
    pub async fn apply_link(
        &self,
        instance_id: &AppInstanceId,
        bot_id: &BotId,
    ) -> Result<AppInstance, AppFrameworkError> {
        let _config_guard = self.framework_config_gate.lock().await;
        self.apply_link_inner(instance_id, bot_id).await
    }

    pub(super) async fn apply_link_inner(
        &self,
        instance_id: &AppInstanceId,
        bot_id: &BotId,
    ) -> Result<AppInstance, AppFrameworkError> {
        let (instance, mut bot, adapter, host) = self.link_context(instance_id, bot_id).await?;
        let topology = classify_app_link(&bot.bot.runtime_target, &instance.host_id)
            .ok_or_else(|| unsupported_link_topology(&bot.bot.runtime_target, &instance.host_id))?;
        // 先把计划算完再动旧隧道，免得算不出来还拆掉现有对接
        let mut plan = self
            .plan_link_for(&instance, &bot, adapter.as_ref(), host.as_ref())
            .await?;
        let mode = plan.connection.mode();
        if let Some(old) = instance.link.as_ref() {
            if old.resident_forward_port.is_some() {
                self.teardown_resident_best_effort(&instance, &old.bot_id)
                    .await;
            }
        }
        if !needs_desktop_ssh_tunnel(topology) {
            self.drop_instance_tunnel(instance_id).await;
        }

        let mut resident_forward_port = None;
        if let OneBotLinkEndpoint::WsClient(client) = &mut plan.connection {
            if topology == AppLinkTopology::RemoteBotRemoteApp {
                let fwd = self.ensure_resident_link(&instance, &bot, bot_id).await?;
                client.url = rewrite_ws_loopback_port(&client.url, fwd)
                    .map_err(AppFrameworkError::Validation)?;
                resident_forward_port = Some(fwd);
            } else if needs_desktop_ssh_tunnel(topology) {
                let loopback = self.ensure_link_tunnel(&instance, &bot).await?;
                client.url = rewrite_ws_loopback_port(&client.url, loopback)
                    .map_err(AppFrameworkError::Validation)?;
            }
        }
        // 正向：Bot 照旧在自己机上听 P，应用端要连的是自己机回环上的隧道口 Q
        let mut app_plan = plan.clone();
        if let OneBotLinkEndpoint::WsServer(server) = &plan.connection
            && topology != AppLinkTopology::SameHost
        {
            let q = if topology == AppLinkTopology::RemoteBotRemoteApp {
                let q = self
                    .ensure_resident_forward_link(&instance, &bot, bot_id, server.port)
                    .await?;
                resident_forward_port = Some(q);
                q
            } else {
                let preferred = adapter_link_port(adapter.as_ref(), host.as_ref(), &instance).await;
                self.ensure_forward_tunnel(&instance, &bot, topology, server.port, preferred)
                    .await?
            };
            if let OneBotLinkEndpoint::WsServer(app_side) = &mut app_plan.connection {
                app_side.host = "127.0.0.1".to_string();
                app_side.port = q;
            }
        }

        let applied = match self.prime_live_port(adapter.as_ref(), &instance).await {
            Ok(()) => {
                adapter
                    .apply_link(host.as_ref(), &instance, &app_plan)
                    .await
            }
            Err(e) => Err(e),
        };
        if let Err(e) = applied {
            if resident_forward_port.is_some() {
                self.teardown_resident_best_effort(&instance, bot_id).await;
            }
            if needs_desktop_ssh_tunnel(topology) {
                self.drop_instance_tunnel(instance_id).await;
            }
            return Err(e);
        }

        upsert_link_endpoint(&mut bot, plan.connection.clone());
        if let Err(e) = self.bot_manager.upsert_bot_config(bot).await {
            if instance.origin.is_imported() {
                if let Ok(Some(snap)) = self.adopt_store.load(&instance.id).await {
                    let root = HostPath::from_posix(&instance.install_dir);
                    if let Err(rb) = restore_adopted_files(
                        host.as_ref(),
                        &root,
                        &snap.files,
                        AdoptRestoreScope::Link,
                    )
                    .await
                    {
                        tracing::error!(
                            instance = instance_id.as_str(),
                            error = %rb,
                            "restore import snapshot after failed bot upsert"
                        );
                    }
                }
            }
            if let Err(rb) = adapter.rollback_link(host.as_ref(), &instance).await {
                tracing::error!(instance = instance_id.as_str(), error = %rb, "rollback app-side link");
            }
            if resident_forward_port.is_some() {
                self.teardown_resident_best_effort(&instance, bot_id).await;
            }
            if needs_desktop_ssh_tunnel(topology) {
                self.drop_instance_tunnel(instance_id).await;
            }
            return Err(AppFrameworkError::Integration(format!(
                "写入协议 Bot 连接失败，应用端配置已还原：{e}"
            )));
        }

        // 换 Bot、导入认领的旧连接名和 ncd-app:<id> 不同、或换了对接方向：把旧条目摘掉，避免双连
        if let Some(old) = instance.link.as_ref() {
            let replacing = old.bot_id != *bot_id
                || old.connection_name != plan.connection.name()
                || old.mode != mode;
            if replacing && !old.connection_name.is_empty() {
                if let Err(e) = self
                    .remove_link_connection_from_bot(&old.bot_id, &old.connection_name, old.mode)
                    .await
                {
                    tracing::warn!(instance = instance_id.as_str(), error = %e, "detach previous bot");
                }
            }
        }

        let updated = self
            .store
            .update(instance_id, |i| {
                i.link = Some(AppLinkRecord {
                    bot_id: bot_id.clone(),
                    mode,
                    connection_name: plan.connection.name().to_string(),
                    linked_at_ms: now_ms(),
                    resident_forward_port,
                });
                i.last_error = None;
            })
            .await?;
        // Karin 的对接会改 .env 里的口和密钥，WebUI 也读这两项
        self.forget_webui_endpoint(instance_id);
        self.publish(&updated, "linked");
        Ok(updated)
    }

    /// 解绑：按名从 Bot 对应那张连接表删并热推；应用端怎么收尾交给适配器（多数不动监听口）
    pub async fn unlink(
        &self,
        instance_id: &AppInstanceId,
    ) -> Result<AppInstance, AppFrameworkError> {
        let _config_guard = self.framework_config_gate.lock().await;
        self.unlink_inner(instance_id).await
    }

    pub(super) async fn unlink_inner(
        &self,
        instance_id: &AppInstanceId,
    ) -> Result<AppInstance, AppFrameworkError> {
        let instance = self.store.require(instance_id).await?;
        let Some(link) = instance.link.clone() else {
            return Ok(instance);
        };
        self.remove_link_connection_from_bot(&link.bot_id, &link.connection_name, link.mode)
            .await?;
        if let Ok(adapter) = self.registry.get(&instance.framework_id)
            && let Ok(host) = self.resolve_host(&instance.host_id).await
            && let Err(e) = match self.prime_live_port(adapter.as_ref(), &instance).await {
                Ok(()) => adapter.unlink(host.as_ref(), &instance).await,
                Err(e) => Err(e),
            }
        {
            tracing::warn!(instance = instance_id.as_str(), error = %e, "app-side unlink");
        }
        if link.resident_forward_port.is_some() {
            self.teardown_resident_best_effort(&instance, &link.bot_id)
                .await;
        }
        self.drop_instance_tunnel(instance_id).await;
        self.forget_webui_endpoint(instance_id);
        let updated = self
            .store
            .update(instance_id, |i| {
                i.link = None;
            })
            .await?;
        self.publish(&updated, "unlinked");
        Ok(updated)
    }

    async fn link_context(
        &self,
        instance_id: &AppInstanceId,
        bot_id: &BotId,
    ) -> Result<
        (
            AppInstance,
            BotConfig,
            Arc<dyn AppFrameworkAdapter>,
            Arc<dyn Host>,
        ),
        AppFrameworkError,
    > {
        let instance = self.store.require(instance_id).await?;
        if !instance.state.is_installed() {
            return Err(AppFrameworkError::Validation(
                "应用实例尚未安装，先完成安装再对接".to_string(),
            ));
        }
        let adapter = self.registry.get(&instance.framework_id)?;
        if adapter.manifest().link_modes.is_empty() {
            return Err(AppFrameworkError::LinkModeUnsupported(
                "该框架不支持对接协议 Bot".to_string(),
            ));
        }
        let bot = self
            .bot_manager
            .bot_config(bot_id)
            .await
            .map_err(AppFrameworkError::Integration)?
            .ok_or_else(|| {
                AppFrameworkError::Validation(format!("协议 Bot {} 不存在", bot_id.as_str()))
            })?;
        // 容器走 bridge 网络加固定端口映射：Bot 在容器里开的服务、连的 127.0.0.1 都是容器自己的，
        // 宿主机上的应用端和隧道口它碰不到，哪个方向都连不上
        if bot.bot.deployment_type == DeploymentType::Docker {
            return Err(AppFrameworkError::LinkModeUnsupported(
                "Docker 部署的协议 Bot 还不能对接应用端：Bot 在容器里，容器里的 127.0.0.1 不是宿主机，两边连不上".into(),
            ));
        }
        if classify_app_link(&bot.bot.runtime_target, &instance.host_id).is_none() {
            return Err(unsupported_link_topology(
                &bot.bot.runtime_target,
                &instance.host_id,
            ));
        }
        let host = self.resolve_host(&instance.host_id).await?;
        Ok((instance, bot, adapter, host))
    }

    /// token 优先级：应用端已有 → Bot 上同名连接已有 → 新生成
    async fn pick_access_token(
        &self,
        host: &dyn Host,
        instance: &AppInstance,
        bot: &BotConfig,
        adapter: &dyn AppFrameworkAdapter,
    ) -> Result<String, AppFrameworkError> {
        if let Some(t) = adapter.read_access_token(host, instance).await? {
            return Ok(t);
        }
        // ncd-app:<id> 只可能是我们写的，两张表一起看，换过对接方向也能沿用旧 token
        let name = app_link_connection_name(&instance.id);
        let clients = bot
            .connect
            .websocket_clients
            .iter()
            .filter(|c| c.base.name == name)
            .map(|c| &c.base);
        let servers = bot
            .connect
            .websocket_servers
            .iter()
            .filter(|s| s.base.name == name)
            .map(|s| &s.base);
        if let Some(existing) = clients
            .chain(servers)
            .map(|b| b.token.trim().to_string())
            .find(|t| !t.is_empty())
        {
            return Ok(existing);
        }
        Ok(generate_token())
    }

    /// 算计划并补上只有编排层知道的部分：正向的 Bot 听口在 Bot 主机上分配（`plan_link` 填 0）。
    /// 跨机时这里给的还是 Bot 侧的口，应用端连的隧道口到 `apply_link` 开隧道时才知道（预览不开隧道）
    pub(super) async fn plan_link_for(
        &self,
        instance: &AppInstance,
        bot: &BotConfig,
        adapter: &dyn AppFrameworkAdapter,
        host: &dyn Host,
    ) -> Result<OneBotLinkPlan, AppFrameworkError> {
        let token = self.pick_access_token(host, instance, bot, adapter).await?;
        let mut plan = adapter.integration().plan_link(instance, bot, &token)?;
        if plan.mode != plan.connection.mode() {
            return Err(AppFrameworkError::Integration(format!(
                "对接计划自相矛盾：mode={} 但连接是 {}",
                plan.mode.as_str(),
                plan.connection.mode().as_str()
            )));
        }
        if let OneBotLinkEndpoint::WsServer(server) = &mut plan.connection {
            server.port = self.pick_forward_port(instance, bot).await?;
        }
        Ok(plan)
    }

    /// 正向对接的 Bot 听口（开在 Bot 那台机上）：重新对接沿用已有 `ncd-app:<id>` 的口；否则避开
    /// Bot 主机上所有 Bot 的服务口和应用实例口，按实例 + Bot 算出稳定的口（预览与写入一致）；
    /// 本机再探一次能否 bind，远端避开服务器上正在监听的口
    async fn pick_forward_port(
        &self,
        instance: &AppInstance,
        bot: &BotConfig,
    ) -> Result<u16, AppFrameworkError> {
        let name = app_link_connection_name(&instance.id);
        if let Some(existing) = bot
            .connect
            .websocket_servers
            .iter()
            .find(|s| s.base.name == name && s.port > 0)
        {
            return Ok(existing.port);
        }
        let bot_host_id = host_id_of_runtime_target(&bot.bot.runtime_target);
        let mut taken: Vec<u16> = self
            .store
            .list()
            .await
            .into_iter()
            .filter(|i| i.host_id == bot_host_id)
            .map(|i| i.port)
            .collect();
        let bots = self
            .bot_manager
            .list_bot_configs_for_link()
            .await
            .map_err(AppFrameworkError::Integration)?;
        for b in bots
            .iter()
            .chain(std::iter::once(bot))
            .filter(|b| runtime_target_matches_host(&b.bot.runtime_target, &bot_host_id))
        {
            taken.extend(bot_listen_ports(b));
        }
        let local = bot_host_id == LOCAL_HOST_ID;
        if !local {
            let host = self.resolve_host(&bot_host_id).await?;
            taken.extend(remote_listening_ports(host.as_ref()).await);
        }
        let seed = format!("{}:{}", instance.id.as_str(), bot.bot.qq_id);
        allocate_stable_port(&seed, &taken, local).map_err(AppFrameworkError::Validation)
    }

    /// 只摘对接方向对应的那张表：导入认领的连接名是用户自己起的，
    /// 另一张表里可能正好有同名的用户连接
    async fn remove_link_connection_from_bot(
        &self,
        bot_id: &BotId,
        connection_name: &str,
        mode: OneBotLinkMode,
    ) -> Result<(), AppFrameworkError> {
        let Some(mut bot) = self
            .bot_manager
            .bot_config(bot_id)
            .await
            .map_err(AppFrameworkError::Integration)?
        else {
            // Bot 已删：没有连接可摘
            return Ok(());
        };
        if !remove_link_connection(&mut bot, connection_name, mode) {
            return Ok(());
        }
        self.bot_manager
            .upsert_bot_config(bot)
            .await
            .map_err(AppFrameworkError::Integration)
    }
}

/// 按连接名 upsert（同名替换，保证重复对接是替换不是追加）
pub fn upsert_ws_client(bot: &mut BotConfig, connection: ncd_domain::WebsocketClientConfig) {
    match bot
        .connect
        .websocket_clients
        .iter_mut()
        .find(|c| c.base.name == connection.base.name)
    {
        Some(slot) => *slot = connection,
        None => bot.connect.websocket_clients.push(connection),
    }
}

pub fn upsert_ws_server(bot: &mut BotConfig, server: ncd_domain::WebsocketServerConfig) {
    match bot
        .connect
        .websocket_servers
        .iter_mut()
        .find(|s| s.base.name == server.base.name)
    {
        Some(slot) => *slot = server,
        None => bot.connect.websocket_servers.push(server),
    }
}

/// 反向进 websocket_clients，正向进 websocket_servers
pub fn upsert_link_endpoint(bot: &mut BotConfig, endpoint: OneBotLinkEndpoint) {
    match endpoint {
        OneBotLinkEndpoint::WsClient(c) => upsert_ws_client(bot, c),
        OneBotLinkEndpoint::WsServer(s) => upsert_ws_server(bot, s),
    }
}

/// 按对接方向从对应的表摘掉同名连接；返回有没有摘到
pub(super) fn remove_link_connection(
    bot: &mut BotConfig,
    name: &str,
    mode: OneBotLinkMode,
) -> bool {
    match mode {
        OneBotLinkMode::ReverseWs => {
            let before = bot.connect.websocket_clients.len();
            bot.connect
                .websocket_clients
                .retain(|c| c.base.name != name);
            bot.connect.websocket_clients.len() != before
        }
        OneBotLinkMode::ForwardWs => {
            let before = bot.connect.websocket_servers.len();
            bot.connect
                .websocket_servers
                .retain(|s| s.base.name != name);
            bot.connect.websocket_servers.len() != before
        }
    }
}

/// 这个 Bot 自己会监听的口（分配正向听口时避开）
pub(super) fn bot_listen_ports(bot: &BotConfig) -> impl Iterator<Item = u16> + '_ {
    let c = &bot.connect;
    c.http_servers
        .iter()
        .map(|s| s.port)
        .chain(c.http_sse_servers.iter().map(|s| s.port))
        .chain(c.websocket_servers.iter().map(|s| s.port))
        .filter(|p| *p > 0)
}

/// Bot 上有没有应用端对接连接（UI 徽章 / 迁移提示用）
pub fn app_link_connections(bot: &BotConfig) -> Vec<String> {
    let clients = bot.connect.websocket_clients.iter().map(|c| &c.base.name);
    let servers = bot.connect.websocket_servers.iter().map(|s| &s.base.name);
    clients
        .chain(servers)
        .filter(|name| is_app_link_connection_name(name))
        .cloned()
        .collect()
}
