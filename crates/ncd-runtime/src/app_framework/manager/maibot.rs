//! 麦麦运行期接口：WebUI 会话、提示词、学到的内容、人物、表情包、长期记忆

use super::*;

fn maibot_api(adapter: &dyn AppFrameworkAdapter) -> Result<&dyn MaiBotRuntimeApi, AppFrameworkError> {
    adapter.maibot_runtime().ok_or_else(|| {
        AppFrameworkError::ConfigUnsupported(adapter.manifest().id.as_str().to_string())
    })
}

/// 提示词改在哪：跑着走 WebUI（上游改完会清它的缓存），停着改盘上文件
enum PromptPlace {
    Live(MaiBotSession),
    Disk(Arc<dyn Host>, AppInstance),
}

impl PromptPlace {
    fn target(&self) -> MaiBotPromptTarget<'_> {
        match self {
            Self::Live(s) => MaiBotPromptTarget::Live(s),
            Self::Disk(host, instance) => MaiBotPromptTarget::Disk { host: host.as_ref(), instance },
        }
    }
}

impl AppManager {
    /// 不报错：没在跑、隧道没通、token 不对都折成 gate，前端照着出提示
    pub async fn maibot_status(
        &self,
        id: &AppInstanceId,
    ) -> Result<MaiBotRuntimeStatus, AppFrameworkError> {
        let instance = self.store.require(id).await?;
        let adapter = self.registry.get(&instance.framework_id)?;
        let runtime = maibot_api(adapter.as_ref())?;
        if !matches!(instance.state, AppInstanceState::Running) {
            return Ok(MaiBotRuntimeStatus::not_running());
        }
        let port = match self.desktop_webui_loopback_port(&instance).await {
            Ok(p) => p,
            Err(e) => {
                return Ok(MaiBotRuntimeStatus::gate(MaiBotRuntimeGate::Unreachable, e.to_string()));
            }
        };
        let endpoint = self.webui_endpoint(&instance).await;
        let session = MaiBotSession {
            instance_id: instance.id.as_str().to_string(),
            port,
            token: endpoint.auth_key,
            utc_offset_secs: endpoint.utc_offset_secs,
        };
        let status = runtime.status(&session).await;
        // 详情页一直在轮询这个：token 在麦麦自己的 WebUI 里换过、或者口变了，
        // 下一轮就按盘上的重新读，别的接口跟着恢复（上游只在登录接口上记错次数，带 Cookie 的请求错了不封）
        if matches!(status.gate, MaiBotRuntimeGate::Auth | MaiBotRuntimeGate::Unreachable) {
            self.forget_webui_endpoint(id);
        }
        Ok(status)
    }

    /// 运行期调用都要实例在跑；口和 token 由这里备齐（按实例记着，见 `webui_endpoint`）
    async fn maibot_session(
        &self,
        id: &AppInstanceId,
    ) -> Result<(Arc<dyn AppFrameworkAdapter>, MaiBotSession), AppFrameworkError> {
        let instance = self.store.require(id).await?;
        let adapter = self.registry.get(&instance.framework_id)?;
        maibot_api(adapter.as_ref())?;
        if !matches!(instance.state, AppInstanceState::Running) {
            return Err(AppFrameworkError::NotRunning("启动麦麦后才能用".into()));
        }
        let port = self.desktop_webui_loopback_port(&instance).await?;
        let again = self.store.require(id).await?;
        if !matches!(again.state, AppInstanceState::Running) {
            return Err(AppFrameworkError::StateChanged("实例状态已变，请重试".into()));
        }
        let endpoint = self.webui_endpoint(&instance).await;
        let session = MaiBotSession {
            instance_id: instance.id.as_str().to_string(),
            port,
            token: endpoint.auth_key,
            utc_offset_secs: endpoint.utc_offset_secs,
        };
        Ok((adapter, session))
    }

    /// 走上游自己的重启：工作进程退出码 42，bot.py 外层重新拉起，桌面端记着的进程不变。
    /// 输出却换了一轮，日志和面板都从这里重新开始，跟在桌面端点启动一样
    pub async fn maibot_restart(&self, id: &AppInstanceId) -> Result<(), AppFrameworkError> {
        let (adapter, s) = self.maibot_session(id).await?;
        maibot_api(adapter.as_ref())?.restart(&s).await?;
        let instance = self.store.require(id).await?;
        let log_file = launch_log_file(adapter.as_ref(), &instance);
        let reset = match self.resolve_host(&instance.host_id).await {
            Ok(host) => self.runtime.reset_log(host, &instance, log_file).await,
            Err(e) => Err(e),
        };
        // 重启本身已经成了，换日志失败只是面板里还留着上一轮
        if let Err(e) = reset {
            tracing::warn!(instance = id.as_str(), error = %e, "reset app log after restart");
        }
        Ok(())
    }

    pub async fn maibot_stats(
        &self,
        id: &AppInstanceId,
        hours: u32,
    ) -> Result<MaiBotStatsSummary, AppFrameworkError> {
        let (adapter, s) = self.maibot_session(id).await?;
        maibot_api(adapter.as_ref())?
            .stats_summary(&s, hours.clamp(1, 24 * 90))
            .await
    }

    pub async fn maibot_chat_sessions(
        &self,
        id: &AppInstanceId,
    ) -> Result<Vec<MaiBotChatSession>, AppFrameworkError> {
        let (adapter, s) = self.maibot_session(id).await?;
        maibot_api(adapter.as_ref())?.chat_sessions(&s).await
    }

    pub async fn maibot_provider_models(
        &self,
        id: &AppInstanceId,
        provider: MaiBotAPIProvider,
    ) -> Result<Vec<MaiBotProviderModel>, AppFrameworkError> {
        let (adapter, s) = self.maibot_session(id).await?;
        let source = self.maibot_provider_source(id, &provider).await;
        maibot_api(adapter.as_ref())?
            .provider_models(&s, &provider, source)
            .await
    }

    pub async fn maibot_test_provider(
        &self,
        id: &AppInstanceId,
        provider: MaiBotAPIProvider,
    ) -> Result<MaiBotProviderCheck, AppFrameworkError> {
        let (adapter, s) = self.maibot_session(id).await?;
        let source = self.maibot_provider_source(id, &provider).await;
        maibot_api(adapter.as_ref())?
            .test_provider(&s, &provider, source)
            .await
    }

    /// 盘上有一模一样的提供商就按名字查：上游用它自己读到的配置，放行回环 / 内网地址。
    /// 表单里改过没保存的只能按地址查，上游那条只放行公网
    async fn maibot_provider_source(
        &self,
        id: &AppInstanceId,
        provider: &MaiBotAPIProvider,
    ) -> MaiBotProviderSource {
        match self.read_config(id).await.map(|env| env.config) {
            Ok(AppInstanceConfig::MaiBot(saved)) if saved.models.api_providers.contains(provider) => {
                MaiBotProviderSource::Saved
            }
            _ => MaiBotProviderSource::Draft,
        }
    }

    pub async fn maibot_mcp_status(
        &self,
        id: &AppInstanceId,
    ) -> Result<MaiBotMcpStatus, AppFrameworkError> {
        let (adapter, s) = self.maibot_session(id).await?;
        maibot_api(adapter.as_ref())?.mcp_status(&s).await
    }

    pub async fn maibot_test_mcp(
        &self,
        id: &AppInstanceId,
        server: MaiBotMCPServerItemConfig,
    ) -> Result<MaiBotMcpTest, AppFrameworkError> {
        let (adapter, s) = self.maibot_session(id).await?;
        maibot_api(adapter.as_ref())?.test_mcp(&s, &server).await
    }

    async fn maibot_prompt_place(
        &self,
        id: &AppInstanceId,
    ) -> Result<(Arc<dyn AppFrameworkAdapter>, PromptPlace), AppFrameworkError> {
        let instance = self.store.require(id).await?;
        let adapter = self.registry.get(&instance.framework_id)?;
        maibot_api(adapter.as_ref())?;
        match instance.state {
            AppInstanceState::Running => {
                let (adapter, s) = self.maibot_session(id).await?;
                Ok((adapter, PromptPlace::Live(s)))
            }
            AppInstanceState::Installed | AppInstanceState::Stopped => {
                let host = self.resolve_host(&instance.host_id).await?;
                Ok((adapter, PromptPlace::Disk(host, instance)))
            }
            AppInstanceState::NotInstalled | AppInstanceState::Installing => {
                Err(AppFrameworkError::NotRunning("麦麦装好后才能改提示词".into()))
            }
        }
    }

    pub async fn maibot_prompt_catalog(
        &self,
        id: &AppInstanceId,
    ) -> Result<MaiBotPromptCatalog, AppFrameworkError> {
        let (adapter, place) = self.maibot_prompt_place(id).await?;
        maibot_api(adapter.as_ref())?.prompt_catalog(place.target()).await
    }

    pub async fn maibot_prompt_file(
        &self,
        id: &AppInstanceId,
        language: &str,
        name: &str,
    ) -> Result<MaiBotPromptFile, AppFrameworkError> {
        let (adapter, place) = self.maibot_prompt_place(id).await?;
        maibot_api(adapter.as_ref())?
            .prompt_file(place.target(), language, name)
            .await
    }

    pub async fn maibot_prompt_version(
        &self,
        id: &AppInstanceId,
        language: &str,
        name: &str,
        version_id: &str,
    ) -> Result<String, AppFrameworkError> {
        let (adapter, place) = self.maibot_prompt_place(id).await?;
        maibot_api(adapter.as_ref())?
            .prompt_version(place.target(), language, name, version_id)
            .await
    }

    pub async fn maibot_prompt_action(
        &self,
        id: &AppInstanceId,
        action: MaiBotPromptAction,
    ) -> Result<MaiBotPromptFile, AppFrameworkError> {
        let (adapter, place) = self.maibot_prompt_place(id).await?;
        maibot_api(adapter.as_ref())?
            .prompt_action(place.target(), &action)
            .await
    }

    pub async fn maibot_expressions(
        &self,
        id: &AppInstanceId,
        query: MaiBotExpressionQuery,
    ) -> Result<MaiBotExpressionPage, AppFrameworkError> {
        let (adapter, s) = self.maibot_session(id).await?;
        maibot_api(adapter.as_ref())?.expressions(&s, &query).await
    }

    pub async fn maibot_expression_overview(
        &self,
        id: &AppInstanceId,
    ) -> Result<MaiBotExpressionOverview, AppFrameworkError> {
        let (adapter, s) = self.maibot_session(id).await?;
        maibot_api(adapter.as_ref())?.expression_overview(&s).await
    }

    pub async fn maibot_expression_action(
        &self,
        id: &AppInstanceId,
        action: MaiBotExpressionAction,
    ) -> Result<MaiBotResourceDone, AppFrameworkError> {
        let (adapter, s) = self.maibot_session(id).await?;
        maibot_api(adapter.as_ref())?.expression_action(&s, &action).await
    }

    pub async fn maibot_jargons(
        &self,
        id: &AppInstanceId,
        query: MaiBotJargonQuery,
    ) -> Result<MaiBotJargonPage, AppFrameworkError> {
        let (adapter, s) = self.maibot_session(id).await?;
        maibot_api(adapter.as_ref())?.jargons(&s, &query).await
    }

    pub async fn maibot_jargon_overview(
        &self,
        id: &AppInstanceId,
    ) -> Result<MaiBotJargonOverview, AppFrameworkError> {
        let (adapter, s) = self.maibot_session(id).await?;
        maibot_api(adapter.as_ref())?.jargon_overview(&s).await
    }

    pub async fn maibot_jargon_action(
        &self,
        id: &AppInstanceId,
        action: MaiBotJargonAction,
    ) -> Result<MaiBotResourceDone, AppFrameworkError> {
        let (adapter, s) = self.maibot_session(id).await?;
        maibot_api(adapter.as_ref())?.jargon_action(&s, &action).await
    }

    pub async fn maibot_behaviors(
        &self,
        id: &AppInstanceId,
        query: MaiBotBehaviorQuery,
    ) -> Result<MaiBotBehaviorPage, AppFrameworkError> {
        let (adapter, s) = self.maibot_session(id).await?;
        maibot_api(adapter.as_ref())?.behaviors(&s, &query).await
    }

    pub async fn maibot_behavior_overview(
        &self,
        id: &AppInstanceId,
    ) -> Result<MaiBotBehaviorOverview, AppFrameworkError> {
        let (adapter, s) = self.maibot_session(id).await?;
        maibot_api(adapter.as_ref())?.behavior_overview(&s).await
    }

    pub async fn maibot_behavior(
        &self,
        id: &AppInstanceId,
        behavior_id: i64,
    ) -> Result<MaiBotBehaviorDetail, AppFrameworkError> {
        let (adapter, s) = self.maibot_session(id).await?;
        maibot_api(adapter.as_ref())?.behavior(&s, behavior_id).await
    }

    pub async fn maibot_chat_ticket(&self, id: &AppInstanceId) -> Result<MaiBotChatTicket, AppFrameworkError> {
        let (adapter, s) = self.maibot_session(id).await?;
        maibot_api(adapter.as_ref())?.chat_ticket(&s).await
    }

    pub async fn maibot_chat_clear(&self, id: &AppInstanceId) -> Result<MaiBotResourceDone, AppFrameworkError> {
        let (adapter, s) = self.maibot_session(id).await?;
        maibot_api(adapter.as_ref())?.chat_clear(&s).await
    }

    pub async fn maibot_persons(
        &self,
        id: &AppInstanceId,
        query: MaiBotPersonQuery,
    ) -> Result<MaiBotPersonPage, AppFrameworkError> {
        let (adapter, s) = self.maibot_session(id).await?;
        maibot_api(adapter.as_ref())?.persons(&s, &query).await
    }

    pub async fn maibot_person_overview(
        &self,
        id: &AppInstanceId,
    ) -> Result<MaiBotPersonOverview, AppFrameworkError> {
        let (adapter, s) = self.maibot_session(id).await?;
        maibot_api(adapter.as_ref())?.person_overview(&s).await
    }

    pub async fn maibot_person_action(
        &self,
        id: &AppInstanceId,
        action: MaiBotPersonAction,
    ) -> Result<MaiBotResourceDone, AppFrameworkError> {
        let (adapter, s) = self.maibot_session(id).await?;
        maibot_api(adapter.as_ref())?.person_action(&s, &action).await
    }

    pub async fn maibot_emojis(
        &self,
        id: &AppInstanceId,
        query: MaiBotEmojiQuery,
    ) -> Result<MaiBotEmojiPage, AppFrameworkError> {
        let (adapter, s) = self.maibot_session(id).await?;
        maibot_api(adapter.as_ref())?.emojis(&s, &query).await
    }

    pub async fn maibot_emoji_overview(
        &self,
        id: &AppInstanceId,
    ) -> Result<MaiBotEmojiOverview, AppFrameworkError> {
        let (adapter, s) = self.maibot_session(id).await?;
        maibot_api(adapter.as_ref())?.emoji_overview(&s).await
    }

    pub async fn maibot_emoji_action(
        &self,
        id: &AppInstanceId,
        action: MaiBotEmojiAction,
    ) -> Result<MaiBotResourceDone, AppFrameworkError> {
        let (adapter, s) = self.maibot_session(id).await?;
        maibot_api(adapter.as_ref())?.emoji_action(&s, &action).await
    }

    pub async fn maibot_emoji_image(
        &self,
        id: &AppInstanceId,
        emoji_id: i64,
        original: bool,
    ) -> Result<MaiBotEmojiImage, AppFrameworkError> {
        let (adapter, s) = self.maibot_session(id).await?;
        maibot_api(adapter.as_ref())?.emoji_image(&s, emoji_id, original).await
    }

    pub async fn maibot_emoji_upload(
        &self,
        id: &AppInstanceId,
        upload: MaiBotEmojiUpload,
    ) -> Result<MaiBotEmojiUploadDone, AppFrameworkError> {
        let (adapter, s) = self.maibot_session(id).await?;
        maibot_api(adapter.as_ref())?.emoji_upload(&s, &upload).await
    }

    /// 上传前在本机看一眼要传的图，不碰实例：拖进来时麦麦停着也能先挑
    pub async fn maibot_local_images(&self, paths: Vec<String>) -> Vec<MaiBotLocalImage> {
        ncd_appframework::inspect_local_images(paths).await
    }

    /// 导入长期记忆前在本机看一眼要导的文件，不碰实例
    pub async fn maibot_local_texts(&self, paths: Vec<String>) -> Vec<MaiBotLocalTextFile> {
        ncd_appframework::inspect_local_texts(paths).await
    }

    pub async fn maibot_memory_status(&self, id: &AppInstanceId) -> Result<MaiBotMemoryStatus, AppFrameworkError> {
        let (adapter, s) = self.maibot_session(id).await?;
        maibot_api(adapter.as_ref())?.memory_status(&s).await
    }

    pub async fn maibot_memory_import_setup(
        &self,
        id: &AppInstanceId,
    ) -> Result<MaiBotMemoryImportSetup, AppFrameworkError> {
        let (adapter, s) = self.maibot_session(id).await?;
        maibot_api(adapter.as_ref())?.memory_import_setup(&s).await
    }

    pub async fn maibot_memory_import(
        &self,
        id: &AppInstanceId,
        req: MaiBotMemoryImport,
    ) -> Result<MaiBotMemoryTask, AppFrameworkError> {
        let (adapter, s) = self.maibot_session(id).await?;
        maibot_api(adapter.as_ref())?.memory_import(&s, &req).await
    }

    pub async fn maibot_memory_tasks(&self, id: &AppInstanceId) -> Result<Vec<MaiBotMemoryTask>, AppFrameworkError> {
        let (adapter, s) = self.maibot_session(id).await?;
        maibot_api(adapter.as_ref())?.memory_tasks(&s).await
    }

    pub async fn maibot_memory_task(
        &self,
        id: &AppInstanceId,
        task_id: &str,
    ) -> Result<MaiBotMemoryTaskDetail, AppFrameworkError> {
        let (adapter, s) = self.maibot_session(id).await?;
        maibot_api(adapter.as_ref())?.memory_task(&s, task_id).await
    }

    pub async fn maibot_memory_task_action(
        &self,
        id: &AppInstanceId,
        action: MaiBotMemoryTaskAction,
    ) -> Result<MaiBotMemoryTask, AppFrameworkError> {
        let (adapter, s) = self.maibot_session(id).await?;
        maibot_api(adapter.as_ref())?.memory_task_action(&s, &action).await
    }

    pub async fn maibot_memory_records(
        &self,
        id: &AppInstanceId,
        query: MaiBotMemoryQuery,
    ) -> Result<MaiBotMemoryRecordPage, AppFrameworkError> {
        let (adapter, s) = self.maibot_session(id).await?;
        maibot_api(adapter.as_ref())?.memory_records(&s, &query).await
    }

    pub async fn maibot_memory_record(
        &self,
        id: &AppInstanceId,
        kind: MaiBotMemoryRecordKind,
        record_id: &str,
    ) -> Result<MaiBotMemoryRecordDetail, AppFrameworkError> {
        let (adapter, s) = self.maibot_session(id).await?;
        maibot_api(adapter.as_ref())?.memory_record(&s, kind, record_id).await
    }

    pub async fn maibot_memory_sources(&self, id: &AppInstanceId) -> Result<Vec<MaiBotMemorySource>, AppFrameworkError> {
        let (adapter, s) = self.maibot_session(id).await?;
        maibot_api(adapter.as_ref())?.memory_sources(&s).await
    }

    pub async fn maibot_memory_delete(
        &self,
        id: &AppInstanceId,
        action: MaiBotMemoryDeleteAction,
    ) -> Result<MaiBotMemoryDeleteResult, AppFrameworkError> {
        let (adapter, s) = self.maibot_session(id).await?;
        maibot_api(adapter.as_ref())?.memory_delete(&s, &action).await
    }

    pub async fn maibot_memory_delete_ops(
        &self,
        id: &AppInstanceId,
    ) -> Result<Vec<MaiBotMemoryDeleteOp>, AppFrameworkError> {
        let (adapter, s) = self.maibot_session(id).await?;
        maibot_api(adapter.as_ref())?.memory_delete_ops(&s).await
    }

    pub async fn maibot_memory_graph(
        &self,
        id: &AppInstanceId,
        max_nodes: u32,
    ) -> Result<MaiBotMemoryGraph, AppFrameworkError> {
        let (adapter, s) = self.maibot_session(id).await?;
        maibot_api(adapter.as_ref())?.memory_graph(&s, max_nodes).await
    }

    pub async fn maibot_memory_graph_node(
        &self,
        id: &AppInstanceId,
        node_id: &str,
    ) -> Result<MaiBotMemoryNodeDetail, AppFrameworkError> {
        let (adapter, s) = self.maibot_session(id).await?;
        maibot_api(adapter.as_ref())?.memory_graph_node(&s, node_id).await
    }

    pub async fn maibot_memory_graph_search(
        &self,
        id: &AppInstanceId,
        query: &str,
    ) -> Result<Vec<MaiBotMemoryGraphHit>, AppFrameworkError> {
        let (adapter, s) = self.maibot_session(id).await?;
        maibot_api(adapter.as_ref())?.memory_graph_search(&s, query).await
    }
}
