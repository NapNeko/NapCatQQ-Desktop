//! Bot 配置持久化、派生文件渲染和删除。

use super::auto_restart;
use super::helpers::{
    docker_container_config_changed, is_shared_napcat_json, set_value_at_dot_path,
};
use super::{BotManager, BotManagerError, MAX_BOTS};
use crate::backend_config_renderer::output_paths_for_backend;
use crate::bot_actor::{BotActorHandle, BotActorSnapshot, BotActorState};
use crate::events::{DomainEvent, EventBus};
use ncd_domain::bot_config::{BackendType, BotConfig};
use ncd_domain::ids::BotId;
use ncd_domain::kinds::StopMode;
use ncd_traits::{BotConfigRepo, ConfigStore, JsonTransaction};
use std::sync::Arc;
use tracing::{info, warn};

impl<R: BotConfigRepo + 'static, S: ConfigStore + 'static> BotManager<R, S> {
    /// 启动前检测派生配置文件 drift
    ///
    /// 返回 None:派生文件不存在或跟 BotConfig 完全一致,可以直接启动
    /// 返回 Some(drift):有差异,前端应弹 ConfigDriftDialog 让用户抉择
    pub async fn detect_config_drift(
        &self,
        bot_id: &BotId,
    ) -> Result<Option<crate::config_drift::ConfigDrift>, BotManagerError> {
        let config = self.get_required_bot_config(bot_id).await?;
        let drift = crate::config_drift::detect_drift(bot_id, &config, self.renderer.as_ref())
            .await
            .map_err(|e| BotManagerError::Render(e.to_string()))?;
        if drift.is_clean() {
            Ok(None)
        } else {
            Ok(Some(drift))
        }
    }

    // ─── 配置管理 ─────────────────────────────────────────────────────────

    /// 新增或更新 Bot 配置
    /// 策略:先持久化 bot.json(source of truth),再写派生文件
    /// - 如果 bot.json 写入失败,派生文件不会被写入,状态完全未变
    /// - 如果派生文件写入失败,bot.json 已是最新,派生文件可在下次启动时重新生成
    ///   不会造成不可恢复的不一致
    /// - 新增时:检查 4 开上限,持久化,写派生文件,创建 Actor
    /// - 更新时:持久化,写派生文件,热推送(通过 restart 通知 Actor 重新加载)
    /// - 如果 backend_type 发生切换(NapCat <-> SnowLuma),必须用旧 backend
    ///   停掉运行中的进程,再用新 backend 启动,避免老进程留尸
    pub async fn upsert_bot_config(
        &self,
        config: BotConfig,
    ) -> Result<BotActorSnapshot, BotManagerError> {
        self.upsert_bot_config_with_overrides(config, &std::collections::HashMap::new())
            .await
    }

    /// 带 drift overrides 的 upsert前端保存时如果检测到 drift 并确认了决议,
    /// 把 overrides 带进来;无 drift 时传空 map
    pub async fn upsert_bot_config_with_overrides(
        &self,
        mut config: BotConfig,
        overrides: &std::collections::HashMap<String, Vec<(String, serde_json::Value)>>,
    ) -> Result<BotActorSnapshot, BotManagerError> {
        ncd_deploy::docker::engine::pin_bot_engine(&mut config);
        let bot_id = BotId::new(config.bot.qq_id.to_string());
        let _migration_guard = self.guard_protocol_mutation(&bot_id)?;
        let is_new = {
            let actors = self.actors.read().await;
            !actors.contains_key(&bot_id)
        };
        info!(
            target: "ncd_runtime::bot_manager",
            bot_id = %bot_id,
            qq_id = config.bot.qq_id,
            is_new,
            deployment = ?config.bot.deployment_type,
            "保存 Bot 配置"
        );

        if is_new {
            let current_count = {
                let actors = self.actors.read().await;
                actors.len()
            };
            if current_count >= MAX_BOTS {
                return Err(BotManagerError::BotLimitReached);
            }
        }

        config.validate()?;
        // cron 解析依赖在 runtime，domain validate 不覆盖；坏表达式在这里挡住不落盘
        auto_restart::validate_schedule(&config.bot.auto_restart_schedule)?;

        // 0. 读旧 config 拿原 backend_type,用于检测 backend 切换走特殊路径
        //    新建 bot 没有旧 config,这步返回 None
        let config_guard = self.config_write_lock.lock().await;
        let previous_config: Option<BotConfig> = self.repo.get(config.bot.qq_id).await?;
        if let Some(previous) = &previous_config
            && previous.bot.deployment_type == ncd_domain::DeploymentType::Docker
            && config.bot.deployment_type == ncd_domain::DeploymentType::Docker
            && previous.bot.runtime_target == config.bot.runtime_target
        {
            let mut pinned = previous.clone();
            ncd_deploy::docker::engine::pin_bot_engine(&mut pinned);
            if pinned.docker.as_ref().and_then(|s| s.engine)
                != config.docker.as_ref().and_then(|s| s.engine)
            {
                return Err(BotManagerError::RemoteRead(
                    "已有容器 Bot 的引擎不能直接更换，请新建实例或先迁移容器数据。".into(),
                ));
            }
        }
        let recreate_docker = previous_config
            .as_ref()
            .is_some_and(|previous| docker_container_config_changed(previous, &config));
        if recreate_docker
            && self.get_snapshot(&bot_id).await.is_ok_and(|snapshot| {
                matches!(
                    snapshot.state,
                    BotActorState::Running | BotActorState::Starting
                )
            })
        {
            // 导入的同名容器可能有自定义挂载，不能按 Desktop 模板替换。
            // 在持久化与停止之前确认归属，拒绝时原配置和容器都保持原样。
            let resolver = self.host_resolver.as_ref().ok_or_else(|| {
                BotManagerError::RemoteRead("无法检查 Docker 容器归属：远端主机解析器未就绪".into())
            })?;
            let host = resolver
                .resolve(&config.bot.runtime_target)
                .await
                .map_err(|error| {
                    BotManagerError::RemoteRead(format!("无法检查 Docker 容器归属：{error}"))
                })?;
            ncd_deploy::ensure_managed_bot_container(host.as_ref(), &config)
                .await
                .map_err(|error| BotManagerError::RemoteRead(error.to_string()))?;
        }

        // 1. 先持久化 bot.json(source of truth)
        self.repo.upsert(config.clone()).await?;
        drop(config_guard);
        if let Some(previous) = &previous_config {
            if previous.bot.runtime_target != config.bot.runtime_target
                || previous.bot.backend_type != config.bot.backend_type
                || previous.bot.deployment_type != config.bot.deployment_type
            {
                self.remove_snowluma_metrics_registration(previous).await;
            }
        }
        self.refresh_snowluma_metrics_registration(&config).await;

        // 2. 渲染派生配置文件(走 render_backend_config:读 existing + merge unknown + apply overrides)
        self.render_backend_config(&bot_id, &config, overrides)
            .await?;

        if !is_new
            && !recreate_docker
            && previous_config.as_ref().is_some_and(|previous| {
                previous.bot.backend_type == config.bot.backend_type
                    && previous.bot.runtime_target == config.bot.runtime_target
            })
            && self
                .get_snapshot(&bot_id)
                .await
                .is_ok_and(|snapshot| snapshot.state.is_active())
        {
            let sync_result = async {
                self.backend_for_config(&config)
                    .await?
                    .sync_bot_config(&config)
                    .await?;
                Ok::<(), BotManagerError>(())
            }
            .await;
            if let Err(error) = sync_result {
                if config.bot.deployment_type == ncd_domain::DeploymentType::Docker {
                    let snapshot = self.get_actor(&bot_id).await?.snapshot();
                    self.publish_state_change(&snapshot, "config_saved_docker_apply_failed");
                    return Err(BotManagerError::RemoteRead(format!(
                        "DOCKER_CONFIG_APPLY_FAILED: 配置已保存，但同步 Docker 协议配置失败：{error}。请检查容器状态后重试保存"
                    )));
                }
                return Err(error);
            }
        }

        // 2b. 指标节点映射热更新：连接配置可热改，探针按 mtime 重读 nodes.json
        // 本机写 data_root；远端若已缓存 host，同步写 ~/ncd-watch/metrics/<bot>/nodes.json
        if let Some(data_root) = self.store.config_dir().parent() {
            let prefs = crate::metrics::BotRuntimeMetricsPrefs::from_data_root(data_root);
            if prefs.enabled {
                if let Err(err) =
                    crate::metrics::write_nodes_map(data_root, bot_id.as_str(), &config)
                {
                    warn!(
                        target: "ncd_runtime::bot_manager",
                        bot_id = %bot_id,
                        %err,
                        "metrics nodes map refresh failed"
                    );
                }
                self.refresh_remote_metrics_nodes_map(&bot_id, &config)
                    .await;
            }
        }

        // 清理不再需要的旧 backend 派生文件(例如 NapCat→SL 时删除 onebot11/napcat 文件)
        // napcat.json 是本机安装目录共享文件，WebUI/启动 Bypass 也读它，不随单个 Bot 切 backend 删除
        let target_backend = config.bot.backend_type;
        let current_paths: Vec<_> = self
            .renderer
            .render(&bot_id, &config)?
            .writes
            .into_iter()
            .map(|write| write.path)
            .collect();
        let all_paths = {
            let mut paths = self.renderer.output_paths(&bot_id);
            paths.sort();
            paths.dedup();
            paths
        };
        let delete_paths: Vec<_> = all_paths
            .into_iter()
            .filter(|path| !current_paths.contains(path))
            .filter(|path| !is_shared_napcat_json(path))
            .collect();
        if !delete_paths.is_empty() {
            let mut txn = ncd_traits::JsonTransaction::new();
            for path in delete_paths {
                txn = txn.delete(path);
            }
            let store = Arc::clone(&self.store);
            tokio::task::spawn_blocking(move || store.apply_transaction(txn))
                .await
                .map_err(|e| BotManagerError::Render(e.to_string()))?
                .map_err(|e| BotManagerError::Render(e.to_string()))?;
        }

        // 3. 创建或热推送 Actor
        if is_new {
            let handle = BotActorHandle::spawn(bot_id.clone());
            let snapshot = handle.snapshot();
            {
                let mut actors = self.actors.write().await;
                actors.insert(bot_id, handle);
            }
            self.publish_state_change(&snapshot, "bot_created");
            Ok(snapshot)
        } else {
            let handle = self.get_actor(&bot_id).await?;
            let current = handle.snapshot();
            if current.state == BotActorState::Running || current.state == BotActorState::Starting {
                if recreate_docker {
                    // 复用完整重启流程，释放旧日志流、poller 与 SSH 隧道，再以新映射重建。
                    // 期望配置已经持久化；应用失败必须明确告知，用户重试启动即可恢复。
                    return match self.restart_bot(&bot_id).await {
                        Ok(snapshot) => {
                            let reason = if snapshot.state == BotActorState::Running {
                                "config_docker_recreated"
                            } else {
                                "config_saved_pending_reload"
                            };
                            self.publish_state_change(&snapshot, reason);
                            Ok(snapshot)
                        }
                        Err(error) => {
                            let snapshot = handle.snapshot();
                            self.publish_state_change(
                                &snapshot,
                                "config_saved_docker_apply_failed",
                            );
                            Err(BotManagerError::RemoteRead(format!(
                                "DOCKER_CONFIG_APPLY_FAILED: 配置已保存，但应用 Docker 容器配置失败：{error}。请检查容器状态后重试启动"
                            )))
                        }
                    };
                }
                if let Some(prev_config) = previous_config
                    .as_ref()
                    .filter(|prev| prev.bot.backend_type != target_backend)
                {
                    // backend 切换:必须停旧 + 起新,没法热推送(NapCat <-> SL 协议完全不同)
                    let snapshot = self
                        .restart_bot_with_backend_switch(&bot_id, prev_config.clone())
                        .await?;
                    self.publish_state_change(&snapshot, "config_hot_reload");
                    Ok(snapshot)
                } else {
                    // 同 backend 运行中:派生文件已写盘(step 2),尝试通过 WebUI 热推送
                    // NapCat: POST /api/OB11Config/SetConfig (需 port + token,端点表已含远端隧道口)
                    // SnowLuma: POST /api/config/:uin (本机 daemon 共享 client;远端走 SSH 隧道 client)
                    // 热推送失败不阻塞保存流程,只给前端一个 warning;下次重启生效
                    let hot_config = crate::native_deployment_adapter::docker_helpers::config_with_docker_listener_hosts(&config);
                    if target_backend == BackendType::SnowLuma {
                        let uin = config.bot.qq_id.to_string();
                        let payload = self
                            .renderer
                            .render(&bot_id, &hot_config)
                            .ok()
                            .and_then(|txn| txn.writes.into_iter().next())
                            .map(|w| w.payload);
                        let reason = match payload {
                            Some(payload) => {
                                self.push_snowluma_hot_reload(&config, &uin, &payload).await
                            }
                            // 渲染失败也算 pending_reload:配置已落盘,等下次重启生效
                            None => "config_saved_pending_reload",
                        };
                        self.event_bus
                            .publish(DomainEvent::bot_state_changed(current.clone(), reason));
                    } else {
                        // NapCat 热推送:endpoint 表里有 (port, token) 才能继续
                        // (bot 已经把 WebUI 端点报到 stdout 上)没有就只写盘
                        // 等下次启动生效配置 payload 直接复用 renderer 写入
                        // onebot11_{bot}.json 的内容——NapCat WebUI
                        // /api/OB11Config/SetConfig 期望的 schema 与该文件一致
                        let endpoint = self.napcat_endpoints.snapshot(&bot_id).await;
                        let onebot_payload = self
                            .renderer
                            .render(&bot_id, &hot_config)
                            .ok()
                            .and_then(|txn| {
                                txn.writes
                                    .into_iter()
                                    .find(|w| {
                                        w.path
                                            .file_name()
                                            .and_then(|n| n.to_str())
                                            .is_some_and(|n| n.starts_with("onebot11_"))
                                    })
                                    .map(|w| w.payload)
                            });
                        match (endpoint, onebot_payload) {
                            (Some(ep), Some(payload)) => {
                                let reason = self.push_napcat_hot_reload(ep, payload).await;
                                self.event_bus.publish(DomainEvent::bot_state_changed(
                                    current.clone(),
                                    reason,
                                ));
                            }
                            // 端点没拿到(bot 还没 ready)或者渲染异常 → 配置已落盘,
                            // 等下次重启再生效
                            _ => {
                                self.event_bus.publish(DomainEvent::bot_state_changed(
                                    current.clone(),
                                    "config_saved_pending_reload",
                                ));
                            }
                        }
                    }
                    Ok(current)
                }
            } else {
                self.event_bus.publish(DomainEvent::bot_state_changed(
                    current.clone(),
                    "config_updated",
                ));
                Ok(current)
            }
        }
    }

    /// 切换框架时先按旧配置确认停止，再按已保存的新配置启动。
    async fn restart_bot_with_backend_switch(
        &self,
        bot_id: &BotId,
        previous_config: BotConfig,
    ) -> Result<BotActorSnapshot, BotManagerError> {
        let handle = self.get_actor(bot_id).await?;
        let before = handle.snapshot();
        let stopping = handle.request_restart().await?;
        self.publish_state_change(&stopping, "restart_requested");
        self.finish_requested_stop(bot_id, &handle, before, stopping, Some(previous_config))
            .await
    }

    /// 删除 Bot 配置及其 Actor如果 Bot 正在运行,先停止
    pub async fn delete_bot_config(&self, bot_id: &BotId) -> Result<(), BotManagerError> {
        self.delete_bot_internal(bot_id).await
    }

    pub async fn pin_container_engines(&self) -> Result<(), BotManagerError> {
        let _config_guard = self.config_write_lock.lock().await;
        for mut config in self.repo.list().await? {
            if ncd_deploy::docker::engine::pin_bot_engine(&mut config) {
                self.repo.upsert(config).await?;
            }
        }
        Ok(())
    }
    /// 渲染派生配置文件
    ///
    /// 调用 renderer.render_with_existing 而不是 render,让 NapCat / SnowLuma
    /// renderer 把磁盘上派生文件里用户加的扩展字段(如 imageDownloadProxy,
    /// autoTimeSync)合并进新输出,避免每次启动覆盖时丢掉用户的手改
    ///
    /// overrides 来自前端 ConfigDriftDialog 的 AcceptExternal 决议:先按
    /// 默认 BotConfig 渲染输出,再用 overrides 把对应 JSON path 的值换成外部值
    /// 没有决议时传空 map
    pub(super) async fn render_backend_config(
        &self,
        bot_id: &BotId,
        config: &BotConfig,
        overrides: &std::collections::HashMap<String, Vec<(String, serde_json::Value)>>,
    ) -> Result<(), BotManagerError> {
        config.validate_network_urls()?;
        // 1. 把现有派生文件读进来(不存在的跳过)
        let mut existing: std::collections::HashMap<std::path::PathBuf, serde_json::Value> =
            std::collections::HashMap::new();
        for path in self.renderer.output_paths(bot_id) {
            match tokio::fs::read(&path).await {
                Ok(bytes) => match serde_json::from_slice::<serde_json::Value>(&bytes) {
                    Ok(value) => {
                        existing.insert(path, value);
                    }
                    Err(_) => {
                        // 派生文件被人改坏了,无法 parse;当作"不存在"处理,下面
                        // render_with_existing 会写一份干净的覆盖
                    }
                },
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => {
                    return Err(BotManagerError::Render(format!(
                        "read derived file {} failed: {e}",
                        path.display()
                    )));
                }
            }
        }

        // 2. 渲染(合并 unknown 顶层字段)
        let mut txn = self
            .renderer
            .render_with_existing(bot_id, config, &existing)?;
        if txn.is_empty() {
            return Ok(());
        }

        // 3. 应用 overrides:按文件名找 write 项,按 dot-path 替换值
        if !overrides.is_empty() {
            for write in txn.writes.iter_mut() {
                let Some(file_name) = write
                    .path
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                else {
                    continue;
                };
                if let Some(file_overrides) = overrides.get(&file_name) {
                    for (path, value) in file_overrides {
                        // mode 由完整渲染契约决定，旧 drift 决议不能恢复全局继承。
                        if config.bot.backend_type == BackendType::SnowLuma
                            && path.split('.').next() == Some("mode")
                        {
                            continue;
                        }
                        set_value_at_dot_path(&mut write.payload, path, value.clone()).map_err(
                            |e| {
                                BotManagerError::Render(format!(
                                    "应用 drift 决议到 {file_name} 的 {path} 失败: {e}"
                                ))
                            },
                        )?;
                    }
                }
            }
        }

        let store = Arc::clone(&self.store);
        tokio::task::spawn_blocking(move || store.apply_transaction(txn))
            .await
            .map_err(|e| BotManagerError::Render(e.to_string()))?
            .map_err(|e| BotManagerError::Render(e.to_string()))?;
        Ok(())
    }

    /// 内部删除流程:停止旧 runtime → 持久化删除 → shutdown → 移除内存 Actor
    /// 停止运行中 Bot 必须发生在 repo.delete 前;否则旧 config/backend identity
    /// 丢失后只能按新默认路由停进程,远端或 Docker 场景会留下真实 runtime
    pub(super) async fn delete_bot_internal(&self, bot_id: &BotId) -> Result<(), BotManagerError> {
        let _migration_guard = self.guard_protocol_mutation(bot_id)?;
        info!(target: "ncd_runtime::bot_manager", bot_id = %bot_id, "删除 Bot 配置与 Actor");
        let qq_id: u64 = bot_id
            .as_str()
            .parse()
            .map_err(|_| BotManagerError::BotNotFound(bot_id.clone()))?;
        let previous_config = self.repo.get(qq_id).await?;

        let maybe_handle = {
            let actors = self.actors.read().await;
            actors.get(bot_id).cloned()
        };

        if let Some(handle) = &maybe_handle {
            let current = handle.snapshot();
            if current.state.is_active() {
                let config = previous_config
                    .as_ref()
                    .ok_or_else(|| BotManagerError::BotNotFound(bot_id.clone()))?;
                let backend = self.backend_for_lifecycle(config).await?;
                let stopping = handle.request_stop().await?;
                self.publish_state_change(&stopping, "delete_stop_requested");
                if let Err(err) = backend.stop(bot_id.clone(), StopMode::Force).await {
                    let message = err.to_string();
                    self.event_bus.publish(DomainEvent::bot_error(
                        bot_id.clone(),
                        message,
                        Some("删除前停止 Bot 失败，配置与 Actor 已保留，可重试删除。".to_string()),
                    ));
                    return Err(err.into());
                }
                match handle.confirm_stopped().await {
                    Ok(stopped) => self.publish_state_change(&stopped, "delete_stop_completed"),
                    Err(crate::bot_actor::BotActorError::InvalidTransition { .. }) => {}
                    Err(err) => return Err(err.into()),
                }
            }
        }

        {
            let _config_guard = self.config_write_lock.lock().await;
            self.repo.delete(qq_id).await?;
        }
        if let Some(config) = &previous_config {
            self.remove_snowluma_metrics_registration(config).await;
        }

        let mut paths = self.renderer.output_paths(bot_id);
        for backend in [BackendType::NapCat, BackendType::SnowLuma] {
            paths.extend(output_paths_for_backend(
                backend,
                self.store.config_dir(),
                bot_id,
            ));
        }
        paths.sort();
        paths.dedup();
        let mut txn = JsonTransaction::new();
        for path in paths
            .into_iter()
            .filter(|path| !is_shared_napcat_json(path))
        {
            txn = txn.delete(path);
        }
        if !txn.is_empty() {
            let store = Arc::clone(&self.store);
            tokio::task::spawn_blocking(move || store.apply_transaction(txn))
                .await
                .map_err(|e| BotManagerError::Render(e.to_string()))?
                .map_err(|e| BotManagerError::Render(e.to_string()))?;
        }

        if let Some(handle) = maybe_handle {
            let _ = handle.shutdown().await;
        }

        {
            let mut actors = self.actors.write().await;
            actors.remove(bot_id);
        }

        self.dispose_poller(bot_id).await;
        self.remote_runtime_sessions().shutdown_bot(bot_id).await;

        self.event_bus.publish(DomainEvent::BotStateChanged {
            snapshot: BotActorSnapshot::new(bot_id.clone()),
            reason: Some("bot_deleted".to_string()),
        });

        Ok(())
    }
}

impl<S: ConfigStore + 'static> BotManager<crate::LocalBotConfigRepo<S>, S> {
    pub async fn replace_bot_configs_with<T>(
        &self,
        bots: Option<Vec<BotConfig>>,
        write: impl std::future::Future<Output = Result<T, String>>,
    ) -> Result<T, String> {
        let _migration_guard = if bots.is_some() {
            Some(
                self.guard_all_protocol_mutation()
                    .map_err(|e| e.to_string())?,
            )
        } else {
            None
        };
        let previous = if bots.is_some() {
            self.repo.list().await.unwrap_or_default()
        } else {
            Vec::new()
        };
        let result = self.repo.replace_with(bots, write).await?;
        if !previous.is_empty() {
            let manager = self.clone();
            tokio::spawn(async move {
                let Ok(current) = manager.repo.list().await else {
                    return;
                };
                for config in previous {
                    if !current.iter().any(|next| {
                        next.bot.qq_id == config.bot.qq_id
                            && next.bot.backend_type == config.bot.backend_type
                            && next.bot.deployment_type == config.bot.deployment_type
                            && next.bot.runtime_target == config.bot.runtime_target
                    }) {
                        manager.remove_snowluma_metrics_registration(&config).await;
                    }
                }
            });
        }
        Ok(result)
    }
}
