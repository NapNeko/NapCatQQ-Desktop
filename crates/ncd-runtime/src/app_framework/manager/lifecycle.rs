//! 实例生命周期：新建、导入、安装、启停、删除，以及实例端口的分配

use super::*;

impl AppManager {
    pub async fn create_instance(
        &self,
        req: CreateAppInstanceRequest,
    ) -> Result<AppInstance, AppFrameworkError> {
        let adapter = self.registry.get(&req.framework_id)?;
        let manifest = adapter.manifest();
        if !manifest.terms.is_empty() && req.accept_terms != Some(true) {
            let titles: Vec<&str> = manifest.terms.iter().map(|t| t.title.as_str()).collect();
            return Err(AppFrameworkError::Validation(format!(
                "需要先阅读并同意{}",
                titles.join("、")
            )));
        }
        let placement = AppPlacement::native_for_host(&req.host_id);
        if !manifest.supported_placements.contains(&placement) {
            return Err(AppFrameworkError::PlacementUnsupported(format!(
                "{} 不支持 {}",
                manifest.display_name,
                placement.as_str()
            )));
        }
        let host = self.resolve_host(&req.host_id).await?;
        let port = self
            .allocate_instance_port(host.as_ref(), &req.host_id, req.port)
            .await?;
        // 账号密码类 WebUI：先把密码定下来，安装时种进配置，之后打开 WebUI 才有得显示
        let webui_account = if manifest.webui_auth == AppWebUiAuthKind::UserPassword {
            let password = resolve_webui_password(adapter.as_ref(), req.webui_password.clone())?;
            let username = req
                .webui_username
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(ToOwned::to_owned);
            Some((username, password))
        } else {
            None
        };

        let id = AppInstanceId::new(short_id());
        if let Some((username, password)) = &webui_account {
            if let Some(name) = username {
                self.remember_secret(&id, SECRET_WEBUI_USERNAME, name);
            }
            self.remember_secret(&id, SECRET_WEBUI_PASSWORD, password);
        }
        let install_dir = self
            .resolve_install_dir(
                host.as_ref(),
                &req.host_id,
                &req.framework_id,
                &id,
                req.install_dir.as_deref(),
            )
            .await?;
        let display_name = if req.display_name.trim().is_empty() {
            format!("{} {}", manifest.display_name, id.as_str())
        } else {
            req.display_name.trim().to_string()
        };

        let instance = AppInstance {
            id,
            framework_id: req.framework_id.clone(),
            display_name,
            placement,
            host_id: req.host_id.clone(),
            install_dir: install_dir.as_posix().to_string(),
            port,
            state: AppInstanceState::NotInstalled,
            link: None,
            installed_version: None,
            last_error: None,
            created_at_ms: now_ms(),
            install_renderer: req.install_renderer.unwrap_or(true),
            origin: ncd_domain::AppInstanceOrigin::Created,
            auto_start: req.auto_start,
        };
        let saved = self.store.upsert(instance).await?;
        self.publish(&saved, "created");
        Ok(saved)
    }

    pub async fn probe_project(
        &self,
        host_id: &str,
        framework_id: &AppFrameworkId,
        raw_path: &str,
    ) -> Result<AppProjectProbe, AppFrameworkError> {
        let adapter = self.registry.get(framework_id)?;
        let host = self.resolve_host(host_id).await?;
        let path = parse_user_install_dir(raw_path, host.os())?;
        if !host
            .exists(&path)
            .await
            .map_err(|e| AppFrameworkError::Host(e.to_string()))?
        {
            return Err(AppFrameworkError::Validation("目录不存在".into()));
        }
        let mut probe = adapter.probe_project(host.as_ref(), &path).await?;
        probe.path = path.as_posix().to_string();
        probe.supervisors =
            super::supervisor::list_supervisors(host.as_ref(), path.as_posix()).await?;
        if !probe.supervisors.is_empty() {
            probe.warnings.push(format!(
                "现在由 systemd 在跑（{}）。导入后改由这边开关，不要了可以还回去。",
                probe.supervisors.join("、")
            ));
        }
        let kind = super::supervisor::AppProcessKind::from_framework(framework_id.as_str());
        probe.running = match host.locality() {
            Locality::Remote => {
                let listing =
                    super::supervisor::list_cwd_processes(host.as_ref(), path.as_posix()).await?;
                super::supervisor::pick_app_pid(&listing, kind).is_some()
            }
            Locality::Local => {
                super::native_runtime::discover_local_pid(path.as_posix(), kind).is_some()
            }
        };
        let stub = probe_read_instance(
            framework_id,
            host_id,
            path.as_posix(),
            probe.port.unwrap_or(0),
        );
        if let Some(found) = self.discover_existing_link(&stub).await {
            probe.detected_bot_id = Some(found.bot_id);
        }
        Ok(probe)
    }

    pub async fn import_instance(
        &self,
        req: ImportAppInstanceRequest,
    ) -> Result<AppInstance, AppFrameworkError> {
        let adapter = self.registry.get(&req.framework_id)?;
        let manifest = adapter.manifest();
        let placement = AppPlacement::native_for_host(&req.host_id);
        if !manifest.supported_placements.contains(&placement) {
            return Err(AppFrameworkError::PlacementUnsupported(format!(
                "{} 不支持 {}",
                manifest.display_name,
                placement.as_str()
            )));
        }
        let probe = self
            .probe_project(&req.host_id, &req.framework_id, &req.path)
            .await?;
        let host = self.resolve_host(&req.host_id).await?;
        let id = AppInstanceId::new(short_id());
        let install_dir = self
            .bind_existing_dir(host.as_ref(), &req.host_id, &id, Some(probe.path.as_str()))
            .await?;
        let siblings = self.store.list().await;
        let taken: Vec<u16> = siblings
            .iter()
            .filter(|i| i.host_id == req.host_id)
            .map(|i| i.port)
            .collect();
        // 接管后对接用 instance.port；必须跟项目实际监听口一致，不能悄悄换成随机高位。
        // 探测不到口时不要填框架默认口：多监听口会把默认口误认成认领。
        let port = match adapter.suggested_import_port(&probe).filter(|p| *p > 0) {
            Some(p) => allocate_listen_port(Some(p), &taken, false)
                .map_err(AppFrameworkError::Validation)?,
            None => 0,
        };
        let display_name = if req.display_name.trim().is_empty() {
            probe.display_name.clone()
        } else {
            req.display_name.trim().to_string()
        };

        let instance = AppInstance {
            id,
            framework_id: req.framework_id.clone(),
            display_name,
            placement,
            host_id: req.host_id.clone(),
            install_dir: install_dir.as_posix().to_string(),
            port,
            state: AppInstanceState::NotInstalled,
            link: None,
            installed_version: probe.version.clone(),
            last_error: None,
            created_at_ms: now_ms(),
            install_renderer: false,
            origin: AppInstanceOrigin::Imported,
            auto_start: true,
        };
        let rels = adapter
            .adopt_watch_rels(host.as_ref(), &install_dir)
            .await?;
        let snapshot = adopt::capture_snapshot(
            host.as_ref(),
            &instance.id,
            &instance.host_id,
            &instance.install_dir,
            &rels,
            &probe.supervisors,
            instance.created_at_ms,
        )
        .await?;
        let saved = self.store.upsert(instance).await?;
        if let Err(e) = self.adopt_store.save(&snapshot).await {
            let _ = self.store.remove(&saved.id).await;
            return Err(e);
        }
        self.publish(&saved, "imported");

        if !probe.supervisors.is_empty() {
            if let Err(e) = super::supervisor::disable_now(host.as_ref(), &probe.supervisors).await
            {
                let _ = self.adopt_store.remove(&saved.id).await;
                let _ = self.store.remove(&saved.id).await;
                return Err(e);
            }
        }

        let refreshed = self.refresh_instance(&saved.id).await?;
        // systemd 没停干净就再拉一份，会和 Restart=always 对打
        let current = if probe.ready && refreshed.state != AppInstanceState::Running {
            match self.start_instance(&refreshed.id).await {
                Ok(started) => started,
                Err(_) => self.store.require(&refreshed.id).await?,
            }
        } else {
            refreshed
        };
        self.adopt_existing_link(&current.id).await
    }

    /// 安装 / 重装：按实例目录把应用端组件交给组件执行器（缺的依赖一起排），再盯任务结束；返回 task id
    pub async fn install_instance(
        self: &Arc<Self>,
        id: &AppInstanceId,
        task_id: Option<String>,
    ) -> Result<String, AppFrameworkError> {
        let components = Arc::clone(self.component_executor()?);
        let instance = self.store.require(id).await?;
        let component_id = self
            .registry
            .adapter(&instance.framework_id)
            .ok()
            .and_then(|adapter| ComponentId::parse(&adapter.manifest().component_id))
            .ok_or_else(|| {
                AppFrameworkError::Validation(format!(
                    "应用端框架未注册组件: {}",
                    instance.framework_id
                ))
            })?;
        let host = self.resolve_host(&instance.host_id).await?;
        let mut inputs = components
            .inputs_for(&instance.host_id, host.as_ref(), None)
            .await;
        inputs.app_component = Some(self.component_hint(&instance));
        let submitted = components
            .submit(ComponentActionRequest {
                component_id,
                host_id: instance.host_id.clone(),
                kind: StepKind::EnsureInstalled,
                task_id: task_id.filter(|s| !s.trim().is_empty()),
                host,
                inputs,
            })
            .await
            .map_err(AppFrameworkError::Validation)?;
        self.track_install(id, submitted.clone()).await?;
        Ok(submitted)
    }

    /// 安装任务已提交：置 Installing 并盯任务结束。
    ///
    /// 什么时候算装完只听任务的：事件为主，事件漏了按 task id 查队列兜底。不拿 detect 当信号，
    /// uv 先建 `.venv` 再装包，目录早早就「像装好了」，冷缓存时后面还要装好几分钟。
    /// 也不设截止时间：各安装步骤自己有超时，任务总会走到终态或被清出队列。
    pub(super) async fn track_install(
        self: &Arc<Self>,
        id: &AppInstanceId,
        task_id: String,
    ) -> Result<AppInstance, AppFrameworkError> {
        let tasks = self.task_queue()?.clone();
        let updated = self
            .store
            .update(id, |i| {
                i.state = AppInstanceState::Installing;
                i.last_error = None;
            })
            .await?;
        self.publish(&updated, "installing");

        // 重复点安装会命中同一个任务（去重），已经有人盯着就不再起一个
        {
            let mut watches = self
                .install_watches
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if watches.get(id) == Some(&task_id) {
                return Ok(updated);
            }
            watches.insert(id.clone(), task_id.clone());
        }

        let this = Arc::clone(self);
        let id = id.clone();
        let mut sub = self
            .event_bus
            .subscribe(EventFilter::kind(DomainEventKind::DeploymentTaskChanged));
        tokio::spawn(async move {
            let end = loop {
                tokio::select! {
                    event = sub.next() => {
                        let Some(event) = event else {
                            break None;
                        };
                        if let DomainEvent::DeploymentTaskChanged { task } = event
                            && task.task_id == task_id
                            && task.status.is_terminal()
                        {
                            break Some(InstallEnd::Finished { status: task.status, error: task.error });
                        }
                    }
                    _ = tokio::time::sleep(INSTALL_POLL_INTERVAL) => {
                        match tasks.status_of(&task_id).await {
                            Some((status, error)) if status.is_terminal() => {
                                break Some(InstallEnd::Finished { status, error });
                            }
                            Some(_) => {}
                            None => break Some(InstallEnd::Vanished),
                        }
                    }
                }
            };
            let settled = match end {
                Some(InstallEnd::Finished { status, error }) => {
                    let failure = (status != ncd_domain::DeploymentTaskStatus::Success)
                        .then(|| error.unwrap_or_else(|| "安装任务未成功结束".to_string()));
                    this.refresh_after_install(&id, failure).await.map(|_| ())
                }
                Some(InstallEnd::Vanished) => this.settle_unwatched_install(&id).await.map(|_| ()),
                // 事件总线关了 = 桌面端在退出，下次启动由对账收尾
                None => Ok(()),
            };
            if let Err(e) = settled {
                tracing::warn!(instance = id.as_str(), error = %e, "settle install");
            }
            // 收完尾再撤表：撤早了，并发的刷新会看到「安装中且没人盯」去按目录收尾
            let mut watches = this
                .install_watches
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if watches.get(&id) == Some(&task_id) {
                watches.remove(&id);
            }
        });
        Ok(updated)
    }

    /// 运行中的实例两次写配置之间留够应用要的间隔：先占好自己的时刻再睡，并发保存自然排队
    pub(super) async fn wait_config_write_slot(
        &self,
        adapter: &dyn AppFrameworkAdapter,
        instance: &AppInstance,
    ) {
        let Some(gap) = adapter.config_write_min_interval() else {
            return;
        };
        if instance.state != AppInstanceState::Running {
            return;
        }
        let wait = {
            let mut slots = self
                .config_write_slots
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let now = Instant::now();
            let at = slots.get(&instance.id).map_or(now, |next| (*next).max(now));
            slots.insert(instance.id.clone(), at + gap);
            at - now
        };
        if !wait.is_zero() {
            tokio::time::sleep(wait).await;
        }
    }

    pub(super) fn install_watched(&self, id: &AppInstanceId) -> bool {
        self.install_watches
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains_key(id)
    }

    /// 「安装中」却没人盯（上次装到一半桌面端退了，或任务结束后被清出队列）：只能按目录现状收尾
    async fn settle_unwatched_install(
        &self,
        id: &AppInstanceId,
    ) -> Result<AppInstance, AppFrameworkError> {
        let instance = self.store.require(id).await?;
        let host = self.resolve_host(&instance.host_id).await?;
        let detected = self.detect(host.as_ref(), &instance).await?;
        let updated = self
            .store
            .update(id, |i| {
                if i.state != AppInstanceState::Installing {
                    return;
                }
                if let DetectOutcome::Installed(v) = &detected {
                    i.state = AppInstanceState::Installed;
                    i.installed_version = Some(v.version.clone());
                } else {
                    i.state = AppInstanceState::NotInstalled;
                    i.last_error = Some("上次安装没有完成，重新安装即可".to_string());
                }
            })
            .await?;
        if updated.state != instance.state {
            let reason = if updated.state == AppInstanceState::Installed {
                "installed"
            } else {
                "install_failed"
            };
            self.publish(&updated, reason);
        }
        Ok(updated)
    }

    async fn refresh_after_install(
        &self,
        id: &AppInstanceId,
        failure: Option<String>,
    ) -> Result<AppInstance, AppFrameworkError> {
        // 任务成功先改状态，避免 UI 卡在「安装中」等 detect 读刚写完的 uv.lock / .venv
        if failure.is_none() {
            let instant = self
                .store
                .update(id, |i| {
                    if matches!(
                        i.state,
                        AppInstanceState::Installing | AppInstanceState::NotInstalled
                    ) {
                        i.state = AppInstanceState::Installed;
                    }
                    i.last_error = None;
                })
                .await?;
            self.publish(&instant, "installed");
        }

        let instance = self.store.require(id).await?;
        let host = match self.resolve_host(&instance.host_id).await {
            Ok(host) => host,
            Err(e) if failure.is_some() => {
                let updated = self
                    .store
                    .update(id, |i| {
                        i.state = AppInstanceState::NotInstalled;
                        i.last_error = Some(failure.clone().unwrap_or_else(|| e.to_string()));
                    })
                    .await?;
                self.publish(&updated, "install_failed");
                return Ok(updated);
            }
            Err(_) => return Ok(instance),
        };
        let detected = self.detect(host.as_ref(), &instance).await;
        let updated = self
            .store
            .update(id, |i| match &detected {
                Ok(DetectOutcome::Installed(v)) => {
                    i.state = AppInstanceState::Installed;
                    i.installed_version = Some(v.version.clone());
                    i.last_error = failure.clone();
                }
                Ok(DetectOutcome::Unusable(u)) if failure.is_some() => {
                    i.state = AppInstanceState::NotInstalled;
                    i.last_error = Some(failure.clone().unwrap_or_else(|| u.reason.clone()));
                }
                Ok(DetectOutcome::NotInstalled) if failure.is_some() => {
                    i.state = AppInstanceState::NotInstalled;
                    i.last_error = failure.clone();
                }
                Err(e) if failure.is_some() => {
                    i.state = AppInstanceState::NotInstalled;
                    i.last_error = Some(failure.clone().unwrap_or_else(|| e.to_string()));
                }
                Ok(DetectOutcome::Unusable(u)) => {
                    i.last_error = Some(u.reason.clone());
                }
                _ => {}
            })
            .await?;
        if updated != instance {
            self.publish(
                &updated,
                if failure.is_some() {
                    "install_failed"
                } else {
                    "installed"
                },
            );
        }
        Ok(updated)
    }

    /// 刷新单个实例：安装探测 + 进程对账（冷启动 / 页面手动刷新）
    pub async fn refresh_instance(
        &self,
        id: &AppInstanceId,
    ) -> Result<AppInstance, AppFrameworkError> {
        let instance = self.store.require(id).await?;
        // 手动刷新 / 冷启动对账：用户可能在盘上改过口令或端口
        self.forget_webui_endpoint(id);
        if instance.state == AppInstanceState::Installing {
            // 任务还在跑就以任务为准，目录里的半成品不算数
            if self.install_watched(id) {
                return Ok(instance);
            }
            return self.settle_unwatched_install(id).await;
        }
        let host = self.resolve_host(&instance.host_id).await?;
        let detected = self.detect(host.as_ref(), &instance).await?;
        let running = match &detected {
            DetectOutcome::Installed(_) => {
                self.runtime.reconcile_pid(host.as_ref(), &instance).await?
            }
            _ => None,
        };
        if running.is_some() && !self.runtime.is_following(&instance.id).await {
            let log = self.resolve_log_file(host.as_ref(), &instance).await;
            self.runtime
                .attach(Arc::clone(&host), &instance, log)
                .await?;
        }
        let updated = self
            .store
            .update(id, |i| match &detected {
                DetectOutcome::Installed(v) => {
                    i.installed_version = Some(v.version.clone());
                    i.state = if running.is_some() {
                        AppInstanceState::Running
                    } else if i.state == AppInstanceState::Running {
                        AppInstanceState::Stopped
                    } else if i.state == AppInstanceState::NotInstalled {
                        AppInstanceState::Installed
                    } else {
                        i.state
                    };
                }
                DetectOutcome::Unusable(u) => {
                    i.state = AppInstanceState::NotInstalled;
                    i.last_error = Some(u.reason.clone());
                }
                DetectOutcome::NotInstalled => {
                    i.state = AppInstanceState::NotInstalled;
                    i.installed_version = None;
                }
            })
            .await?;
        if updated.state != instance.state
            || updated.installed_version != instance.installed_version
        {
            self.publish(&updated, "refreshed");
        }
        if updated.origin.is_imported() && updated.link.is_none() {
            return self.adopt_existing_link(&updated.id).await;
        }
        Ok(updated)
    }

    /// 开页拉历史:broadcast 无 backlog,启动对账推过的行会丢
    pub async fn tail_log(
        &self,
        id: &AppInstanceId,
        lines: usize,
    ) -> Result<LogSnapshot, AppFrameworkError> {
        let instance = self.store.require(id).await?;
        let host = self.resolve_host(&instance.host_id).await?;
        let path = self.resolve_log_file(host.as_ref(), &instance).await;
        let mut collected = super::log_tail::tail_file(host.as_ref(), &path, lines).await;
        if collected.is_empty() {
            let units = self
                .adopt_store
                .load(&instance.id)
                .await
                .ok()
                .flatten()
                .map(|s| s.supervisors)
                .unwrap_or_default();
            let pid = self
                .runtime
                .reconcile_pid(host.as_ref(), &instance)
                .await
                .ok()
                .flatten();
            collected = super::log_tail::tail_journal(host.as_ref(), &units, pid, lines).await;
        }
        Ok(LogSnapshot {
            total_lines: collected.len(),
            lines: collected,
        })
    }

    async fn resolve_log_file(&self, host: &dyn Host, instance: &AppInstance) -> HostPath {
        let primary = match self.registry.get(&instance.framework_id) {
            Ok(adapter) => launch_log_file(adapter.as_ref(), instance),
            Err(_) => HostPath::from_posix(&instance.install_dir).join(".ncd-app.log"),
        };
        // 桌面端起过的实例主日志一定在，新一轮刚开头可能还是空的：空也认它，
        // 不然刚启动那一下会回落到别的 *.log，把上一轮的东西又翻出来
        if host.exists(&primary).await.unwrap_or(false) {
            return primary;
        }
        if let Some(found) = super::log_tail::newest_project_log(host, &instance.install_dir).await
        {
            if super::log_tail::file_size(host, &found).await.unwrap_or(0) > 0 {
                return found;
            }
        }
        primary
    }

    /// 冷启动：逐实例对账；连不上的远端主机跳过（脱管语义）
    pub async fn reconcile_all(&self) {
        for instance in self.store.list().await {
            self.reconcile_one(&instance.id).await;
        }
    }

    /// 对账完成后自动启动符合条件的实例（开机/桌面端启动时调用）
    /// 条件：全局开关开启 && 实例 auto_start=true && 已安装 && 未运行
    pub async fn auto_start_instances(&self, global_enabled: bool) {
        if !global_enabled {
            return;
        }
        let instances = self.store.list().await;
        for instance in instances {
            if instance.auto_start
                && instance.state.is_installed()
                && instance.state != AppInstanceState::Running
            {
                if let Err(e) = self.start_instance(&instance.id).await {
                    tracing::warn!(
                        instance = instance.id.as_str(),
                        display_name = %instance.display_name,
                        error = %e,
                        "auto start instance failed"
                    );
                } else {
                    tracing::info!(
                        instance = instance.id.as_str(),
                        display_name = %instance.display_name,
                        "auto started instance"
                    );
                }
            }
        }
    }

    /// 主机连上后补跑启动时因 SSH 未就绪跳过的远端对账 / 日志挂接
    pub async fn reconcile_for_server(&self, server_id: &str) {
        let host_id = format!("{REMOTE_HOST_ID_PREFIX}{server_id}");
        for instance in self.store.list().await {
            if instance.host_id != host_id {
                continue;
            }
            self.reconcile_one(&instance.id).await;
        }
    }

    async fn reconcile_one(&self, id: &AppInstanceId) {
        if let Err(e) = self.refresh_instance(id).await {
            tracing::info!(instance = id.as_str(), error = %e, "app reconcile skipped");
        }
        let current = match self.store.require(id).await {
            Ok(i) => i,
            Err(_) => return,
        };
        if current
            .link
            .as_ref()
            .and_then(|l| l.resident_forward_port)
            .is_some()
        {
            if let Err(e) = self.reconcile_resident_link(&current).await {
                tracing::info!(
                    instance = id.as_str(),
                    error = %e,
                    "app resident link reconcile skipped"
                );
            }
        } else if let Err(e) = self.reconcile_link_tunnel(&current).await {
            tracing::info!(
                instance = id.as_str(),
                error = %e,
                "app link tunnel reconcile skipped"
            );
        }
    }

    pub async fn run_host_connection_recovered_listener(self: Arc<Self>) {
        let mut subscription = self.event_bus.subscribe(EventFilter::all());
        let mut last_reconciled: HashMap<String, Instant> = HashMap::new();
        while let Some(event) = subscription.next().await {
            let DomainEvent::HostConnectionRecovered { server_id, .. } = event else {
                continue;
            };
            let now = Instant::now();
            if let Some(prev) = last_reconciled.get(&server_id) {
                if now.duration_since(*prev) < Duration::from_secs(30) {
                    continue;
                }
            }
            last_reconciled.insert(server_id.clone(), now);
            self.reconcile_for_server(&server_id).await;
        }
    }

    pub async fn start_instance(
        &self,
        id: &AppInstanceId,
    ) -> Result<AppInstance, AppFrameworkError> {
        let _config_guard = self.framework_config_gate.lock().await;
        let instance = self.store.require(id).await?;
        if self
            .data_root
            .join(format!("config/framework-configs/{}.json", id.as_str()))
            .exists()
        {
            return Err(AppFrameworkError::Validation(
                "该实例有待恢复的框架配置，请先在设置 → 数据中重试恢复框架配置".into(),
            ));
        }
        if !instance.state.is_installed() {
            return Err(AppFrameworkError::Validation("实例尚未安装".to_string()));
        }
        if instance.state == AppInstanceState::Running {
            return Ok(instance);
        }
        // 停着的时候配置可能被手改过，这一轮起来按盘上的重新认
        self.forget_webui_endpoint(id);
        let adapter = self.registry.get(&instance.framework_id)?;
        let host = self.resolve_host(&instance.host_id).await?;
        let spec = self.component_spec(host.as_ref(), &instance);
        let log_file = launch_log_file(adapter.as_ref(), &instance);
        let launch_args = self.desktop_session_launch_args(&instance, adapter.as_ref());
        // 起不来的原因（缺 venv、条款没同意）也要落到 last_error：开机自启没人盯着看报错条
        let started = match adapter
            .launch_command(host.as_ref(), &spec, &launch_args)
            .await
        {
            Ok(command) => {
                let launch = AppLaunchSpec { command, log_file };
                self.runtime
                    .start(Arc::clone(&host), &instance, launch)
                    .await
            }
            Err(e) => Err(e),
        };

        match started {
            Ok(_pid) => {
                let updated = self
                    .store
                    .update(id, |i| {
                        i.state = AppInstanceState::Running;
                        i.last_error = None;
                    })
                    .await?;
                self.publish(&updated, "started");
                Ok(updated)
            }
            Err(e) => {
                let updated = self
                    .store
                    .update(id, |i| {
                        i.state = AppInstanceState::Stopped;
                        i.last_error = Some(e.to_string());
                    })
                    .await?;
                self.publish(&updated, "start_failed");
                Err(e)
            }
        }
    }

    /// 还没同意、或更新后改过的上游条款；框架没有条款、实例还没装好时都为空
    pub async fn pending_terms(
        &self,
        id: &AppInstanceId,
    ) -> Result<Vec<AppPendingTerms>, AppFrameworkError> {
        let instance = self.store.require(id).await?;
        let adapter = self.registry.get(&instance.framework_id)?;
        if adapter.manifest().terms.is_empty() || !instance.state.is_installed() {
            return Ok(Vec::new());
        }
        let host = self.resolve_host(&instance.host_id).await?;
        adapter.pending_terms(host.as_ref(), &instance).await
    }

    /// 用户在同意框里点了同意
    pub async fn accept_terms(&self, id: &AppInstanceId) -> Result<(), AppFrameworkError> {
        let instance = self.store.require(id).await?;
        let adapter = self.registry.get(&instance.framework_id)?;
        let host = self.resolve_host(&instance.host_id).await?;
        adapter.accept_terms(host.as_ref(), &instance).await
    }

    pub async fn stop_instance(
        &self,
        id: &AppInstanceId,
    ) -> Result<AppInstance, AppFrameworkError> {
        let _config_guard = self.framework_config_gate.lock().await;
        let instance = self.store.require(id).await?;
        self.forget_webui_endpoint(id);
        let host = self.resolve_host(&instance.host_id).await?;
        if instance.state == AppInstanceState::Running {
            self.stop_gracefully(host.as_ref(), &instance).await;
        }
        self.runtime.stop(host, &instance).await?;
        let updated = self
            .store
            .update(id, |i| {
                if i.state == AppInstanceState::Running {
                    i.state = AppInstanceState::Stopped;
                }
                i.last_error = None;
            })
            .await?;
        self.publish(&updated, "stopped");
        Ok(updated)
    }

    /// 框架有自己的退出入口时先走它，等它收完再由 runtime 兜底收树。失败只记日志：
    /// 请求没送到、等超时都还有收树这一步
    async fn stop_gracefully(&self, host: &dyn Host, instance: &AppInstance) {
        let Ok(adapter) = self.registry.get(&instance.framework_id) else {
            return;
        };
        match adapter.request_graceful_stop(host, instance).await {
            Ok(true) => {
                let timeout = adapter.graceful_stop_timeout();
                if !self.runtime.wait_exited(host, instance, timeout).await {
                    tracing::info!(
                        instance = instance.id.as_str(),
                        "app did not exit in time after graceful stop; killing tree"
                    );
                }
            }
            Ok(false) => {}
            Err(e) => {
                tracing::warn!(instance = instance.id.as_str(), error = %e, "graceful stop request")
            }
        }
    }

    /// 桌面端退出：本机在跑的实例先各自请退（并发，整体受各框架超时约束），再统一收树；远端脱管
    pub async fn shutdown_local(&self) {
        let running: Vec<AppInstance> = self
            .store
            .list()
            .await
            .into_iter()
            .filter(|i| i.host_id == LOCAL_HOST_ID && i.state == AppInstanceState::Running)
            .collect();
        if !running.is_empty() {
            if let Ok(host) = self.resolve_host(LOCAL_HOST_ID).await {
                let host = host.as_ref();
                futures_util::future::join_all(
                    running.iter().map(|i| self.stop_gracefully(host, i)),
                )
                .await;
            }
        }
        self.runtime.shutdown_local().await;
    }

    /// 修改实例的开机自启设置
    pub async fn set_instance_auto_start(
        &self,
        id: &AppInstanceId,
        auto_start: bool,
    ) -> Result<AppInstance, AppFrameworkError> {
        let updated = self.store.update(id, |i| i.auto_start = auto_start).await?;
        self.publish(&updated, "auto_start_changed");
        Ok(updated)
    }

    /// 删除实例：停进程 → 解绑 Bot 侧连接 → 导入项先还原快照 → （可选）删目录 → 删记录
    pub async fn delete_instance(
        &self,
        id: &AppInstanceId,
        remove_files: bool,
    ) -> Result<(), AppFrameworkError> {
        let _config_guard = self.framework_config_gate.lock().await;
        let instance = self.store.require(id).await?;
        self.forget_webui_endpoint(id);
        let host = self.resolve_host(&instance.host_id).await;
        if let Ok(host) = &host {
            if let Err(e) = self.runtime.stop(Arc::clone(host), &instance).await {
                tracing::warn!(instance = id.as_str(), error = %e, "stop before delete");
            }
        }
        if instance.link.is_some() {
            if let Err(e) = self.unlink_inner(id).await {
                tracing::warn!(instance = id.as_str(), error = %e, "unlink before delete");
            }
        }
        self.drop_instance_tunnel(id).await;
        if instance.origin.is_imported() && !remove_files {
            let host = host?;
            self.release_imported(&instance, host.as_ref()).await?;
        } else if remove_files {
            let host = host?;
            let dir = HostPath::from_posix(&instance.install_dir);
            if host.exists(&dir).await.map_err(host_err)? {
                host.remove_dir_all(&dir).await.map_err(host_err)?;
            }
            let _ = self.adopt_store.remove(id).await;
        } else {
            let _ = self.adopt_store.remove(id).await;
        }
        if let Some(removed) = self.store.remove(id).await? {
            self.forget_secrets(id);
            self.publish(&removed, "deleted");
        }
        Ok(())
    }

    /// 还原导入快照 + 清桌面端落盘 + 把 systemd 还给系统。失败则保留实例记录以便重试。
    async fn release_imported(
        &self,
        instance: &AppInstance,
        host: &dyn Host,
    ) -> Result<(), AppFrameworkError> {
        let root = HostPath::from_posix(&instance.install_dir);
        let snap = self.adopt_store.load(&instance.id).await?;
        if let Some(snap) = &snap {
            restore_adopted_files(host, &root, &snap.files, AdoptRestoreScope::All).await?;
            if let Err(e) = remove_ncd_debris(host, &root, &snap.files).await {
                tracing::warn!(instance = instance.id.as_str(), error = %e, "clean ncd debris");
            }
            if !snap.supervisors.is_empty() {
                super::supervisor::enable_now(host, &snap.supervisors).await?;
            }
        } else {
            if let Err(e) = remove_ncd_debris(host, &root, &[]).await {
                tracing::warn!(instance = instance.id.as_str(), error = %e, "clean ncd debris");
            }
            let units = super::supervisor::list_supervisors(host, root.as_posix()).await?;
            if !units.is_empty() {
                super::supervisor::enable_now(host, &units).await?;
            }
        }
        self.adopt_store.remove(&instance.id).await
    }

    pub(super) async fn ensure_port_free(
        &self,
        instance: &AppInstance,
        port: u16,
    ) -> Result<(), AppFrameworkError> {
        if port == 0 {
            return Err(AppFrameworkError::Validation("端口不能为 0".to_string()));
        }
        let taken = self
            .store
            .list()
            .await
            .into_iter()
            .any(|i| i.id != instance.id && i.host_id == instance.host_id && i.port == port);
        if taken {
            return Err(AppFrameworkError::Validation(format!(
                "该主机上已有应用实例占用端口 {port}"
            )));
        }
        if instance.host_id == LOCAL_HOST_ID && !local_port_free(port) {
            return Err(AppFrameworkError::Validation(format!(
                "本机端口 {port} 已被其它程序占用"
            )));
        }
        if instance.host_id != LOCAL_HOST_ID {
            let host = self.resolve_host(&instance.host_id).await?;
            if remote_listening_ports(host.as_ref()).await.contains(&port) {
                return Err(AppFrameworkError::Validation(format!(
                    "远端主机上端口 {port} 已被其它程序占用"
                )));
            }
        }
        Ok(())
    }

    /// 新实例的口：避开同主机登记过的实例口；本机探一次 bind，远端避开服务器上正在监听的口
    async fn allocate_instance_port(
        &self,
        host: &dyn Host,
        host_id: &str,
        requested: Option<u16>,
    ) -> Result<u16, AppFrameworkError> {
        let mut taken: Vec<u16> = self
            .store
            .list()
            .await
            .iter()
            .filter(|i| i.host_id == host_id)
            .map(|i| i.port)
            .collect();
        let local = host.locality() == Locality::Local;
        if !local {
            let busy = remote_listening_ports(host).await;
            if let Some(port) = requested.filter(|p| busy.contains(p)) {
                return Err(AppFrameworkError::Validation(format!(
                    "远端主机上端口 {port} 已被其它程序占用"
                )));
            }
            taken.extend(busy);
        }
        allocate_listen_port(requested, &taken, local).map_err(AppFrameworkError::Validation)
    }
}
