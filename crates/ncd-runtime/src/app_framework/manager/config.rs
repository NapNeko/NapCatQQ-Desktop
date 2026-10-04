//! 实例配置读写（类型化 + 原始文件），以及写盘之后的端口、对接、重启联动

use super::*;

impl AppManager {
    /// 读类型化配置（含合并版本号）。未安装的实例没有配置可读
    pub async fn read_config(
        &self,
        id: &AppInstanceId,
    ) -> Result<AppInstanceConfigEnvelope, AppFrameworkError> {
        let (instance, adapter, host) = self.config_context(id).await?;
        adapter.read_config(host.as_ref(), &instance).await
    }

    /// 写类型化配置：版本号比对 → 端口占用预检 → 适配器写盘 → 同步实例端口 / 重新对接 / 重启提示。
    /// `base_revision` 为 None 表示用户选择「覆盖」。
    pub async fn write_config(
        &self,
        id: &AppInstanceId,
        config: AppInstanceConfig,
        base_revision: Option<String>,
    ) -> Result<AppConfigWriteResult, AppFrameworkError> {
        self.write_config_profile(id, config, base_revision, None)
            .await
    }

    /// `conf_id` 只对跑着的 AstrBot 有意义（写到选中的 abconf）；停着始终只 patch `cmd_config.json`。
    pub async fn write_config_profile(
        &self,
        id: &AppInstanceId,
        config: AppInstanceConfig,
        base_revision: Option<String>,
        conf_id: Option<String>,
    ) -> Result<AppConfigWriteResult, AppFrameworkError> {
        let result = self
            .write_config_profile_inner(id, config, base_revision, conf_id)
            .await;
        // 写到一半失败也可能已经动了盘上的口或口令，不论成败都重新读
        self.forget_webui_endpoint(id);
        result
    }

    async fn write_config_profile_inner(
        &self,
        id: &AppInstanceId,
        config: AppInstanceConfig,
        base_revision: Option<String>,
        conf_id: Option<String>,
    ) -> Result<AppConfigWriteResult, AppFrameworkError> {
        let (instance, adapter, host) = self.config_context(id).await?;
        let before = adapter.read_config(host.as_ref(), &instance).await?;
        if let Some(base) = base_revision.as_deref()
            && base != before.revision
        {
            return Err(AppFrameworkError::ConfigConflict("config".to_string()));
        }

        let new_port = config.listen_port();
        if new_port != instance.port {
            self.ensure_port_free(&instance, new_port).await?;
        }

        let decided_running = matches!(instance.state, AppInstanceState::Running);
        let live = decided_running && adapter.supports_live_config();
        self.wait_config_write_slot(adapter.as_ref(), &instance)
            .await;
        let after = if live {
            let port = self.desktop_webui_loopback_port(&instance).await?;
            let again = self.store.require(id).await?;
            if matches!(again.state, AppInstanceState::Running) != decided_running {
                return Err(AppFrameworkError::StateChanged(
                    "实例状态已变，请重试".into(),
                ));
            }
            // 用户名密码类（AstrBot）要桌面端记着的密码；密钥类（MaiBot）的 token 在实例目录里，适配器自己读
            let (username, password, desktop_secret) =
                if adapter.manifest().webui_auth == AppWebUiAuthKind::UserPassword {
                    let desktop_secret = if adapter.desktop_session_env_keys().is_some() {
                        self.desktop_session_secret(&instance)
                    } else {
                        None
                    };
                    let password = self.remembered_secret(&instance, SECRET_WEBUI_PASSWORD);
                    if password.is_none() && desktop_secret.is_none() {
                        return Err(AppFrameworkError::DashboardAuth(
                            "没有可用的 WebUI 密码。到连接页写下密码后再保存".into(),
                        ));
                    }
                    (
                        self.webui_login_username(&instance).await,
                        password.unwrap_or_default(),
                        desktop_secret,
                    )
                } else {
                    (String::new(), String::new(), None)
                };
            let profile = conf_id
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or("default");
            adapter
                .write_live_config(
                    host.as_ref(),
                    &instance,
                    port,
                    &username,
                    &password,
                    desktop_secret.as_deref(),
                    &config,
                    profile,
                )
                .await?
        } else {
            let again = self.store.require(id).await?;
            if matches!(again.state, AppInstanceState::Running) && adapter.supports_live_config() {
                return Err(AppFrameworkError::StateChanged(
                    "实例状态已变，请重试".into(),
                ));
            }
            adapter
                .write_config(host.as_ref(), &instance, &config)
                .await?
        };
        let mut sync = self
            .sync_after_config_write(&instance, &before, &after)
            .await?;
        if live {
            sync.restart_required = false;
        }
        // 同一份文档里只在启动时读的字段（MaiBot 的端口 / 日志），走文件还是走接口写都得重启。
        // 按这次要写的配置判：走接口时回读的文件什么时候落盘由应用决定，不拿它当准
        if decided_running && before.config.restart_inputs_changed(&config) {
            sync.restart_required = true;
        }

        // 重新对接会再改 .env（HTTP_PORT / WS_SERVER_AUTH_KEY 对齐），回读一次让前端拿到最终版本号
        let fin = if sync.relinked {
            adapter.read_config(host.as_ref(), &instance).await?
        } else {
            after
        };
        Ok(AppConfigWriteResult {
            config: fin.config,
            revision: fin.revision,
            documents: fin.documents,
            restart_required: sync.restart_required,
            relinked: sync.relinked,
            port_changed: sync.port_changed,
        })
    }

    pub async fn list_config_documents(
        &self,
        id: &AppInstanceId,
    ) -> Result<Vec<AppConfigDocument>, AppFrameworkError> {
        let instance = self.store.require(id).await?;
        let adapter = self.registry.get(&instance.framework_id)?;
        match self.resolve_host(&instance.host_id).await {
            Ok(host) => {
                adapter
                    .list_config_documents(host.as_ref(), &instance)
                    .await
            }
            Err(_) => Ok(adapter.config_documents(&instance)),
        }
    }

    pub async fn read_config_text(
        &self,
        id: &AppInstanceId,
        doc_id: &str,
    ) -> Result<AppConfigText, AppFrameworkError> {
        let (instance, adapter, host) = self.config_context(id).await?;
        adapter
            .read_config_text(host.as_ref(), &instance, doc_id)
            .await
    }

    /// 原始文本写入；写完对有类型化模型的框架做一次端口 / 对接同步（手改 HTTP_PORT 也不失联）
    pub async fn write_config_text(
        &self,
        id: &AppInstanceId,
        doc_id: &str,
        text: &str,
        base_revision: Option<String>,
    ) -> Result<AppConfigText, AppFrameworkError> {
        let (instance, adapter, host) = self.config_context(id).await?;
        let before = self
            .typed_snapshot(adapter.as_ref(), host.as_ref(), &instance)
            .await;
        self.wait_config_write_slot(adapter.as_ref(), &instance)
            .await;
        let written = adapter
            .write_config_text(
                host.as_ref(),
                &instance,
                doc_id,
                text,
                base_revision.as_deref(),
            )
            .await;
        self.forget_webui_endpoint(id);
        let written = written?;
        if let Some(before) = before
            && let Some(after) = self
                .typed_snapshot(adapter.as_ref(), host.as_ref(), &instance)
                .await
        {
            let new_port = after.config.listen_port();
            if new_port != instance.port
                && let Err(e) = self.ensure_port_free(&instance, new_port).await
            {
                tracing::warn!(instance = id.as_str(), error = %e, "raw edit picked a taken port");
            } else if let Err(e) = self
                .sync_after_config_write(&instance, &before, &after)
                .await
            {
                tracing::warn!(instance = id.as_str(), error = %e, "sync after raw config write");
            }
        }
        Ok(written)
    }

    async fn config_context(
        &self,
        id: &AppInstanceId,
    ) -> Result<(AppInstance, Arc<dyn AppFrameworkAdapter>, Arc<dyn Host>), AppFrameworkError> {
        let instance = self.store.require(id).await?;
        if !instance.state.is_installed() {
            return Err(AppFrameworkError::Validation(
                "应用实例尚未安装，还没有可编辑的配置".to_string(),
            ));
        }
        let adapter = self.registry.get(&instance.framework_id)?;
        let host = self.resolve_host(&instance.host_id).await?;
        Ok((instance, adapter, host))
    }

    /// 类型化读取；没有类型化模型（ConfigUnsupported）或读失败都视作「无」
    async fn typed_snapshot(
        &self,
        adapter: &dyn AppFrameworkAdapter,
        host: &dyn Host,
        instance: &AppInstance,
    ) -> Option<AppInstanceConfigEnvelope> {
        match adapter.read_config(host, instance).await {
            Ok(env) => Some(env),
            Err(AppFrameworkError::ConfigUnsupported(_)) => None,
            Err(e) => {
                tracing::debug!(instance = instance.id.as_str(), error = %e, "typed config snapshot");
                None
            }
        }
    }

    /// 写盘之后的联动：端口同步 → 已对接且对接输入变了就重新 apply_link → 非热加载文件变了提示重启
    async fn sync_after_config_write(
        &self,
        instance: &AppInstance,
        before: &AppInstanceConfigEnvelope,
        after: &AppInstanceConfigEnvelope,
    ) -> Result<ConfigSyncOutcome, AppFrameworkError> {
        let mut outcome = ConfigSyncOutcome::default();
        let new_port = after.config.listen_port();
        let running = instance.state == AppInstanceState::Running;

        if new_port != instance.port {
            let updated = self
                .store
                .update(&instance.id, |i| i.port = new_port)
                .await?;
            self.publish(&updated, "port_changed");
            outcome.port_changed = true;
            // 监听口是启动时绑定的，热加载救不了
            outcome.restart_required |= running;
        }

        if let Some(link) = instance.link.as_ref()
            && before.config.link_inputs_changed(&after.config)
        {
            self.apply_link(&instance.id, &link.bot_id)
                .await
                .map_err(|e| {
                    AppFrameworkError::Integration(format!(
                        "配置已保存，但重新对接协议 Bot 失败：{e}"
                    ))
                })?;
            outcome.relinked = true;
        }

        if running {
            let cold_changed = after.documents.iter().any(|d| {
                !d.hot_reload
                    && before
                        .documents
                        .iter()
                        .find(|b| b.doc_id == d.doc_id)
                        .is_none_or(|b| b.revision != d.revision)
            });
            outcome.restart_required |= cold_changed;
        }
        Ok(outcome)
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct ConfigSyncOutcome {
    port_changed: bool,
    relinked: bool,
    restart_required: bool,
}
