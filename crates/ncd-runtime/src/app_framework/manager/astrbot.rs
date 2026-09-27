//! AstrBot 运行期接口：Dashboard 登录态、人格、知识库、会话规则、配置文件、子代理

use super::*;

fn astrbot_api(
    adapter: &dyn AppFrameworkAdapter,
) -> Result<&dyn AstrBotRuntimeApi, AppFrameworkError> {
    adapter.astrbot_runtime().ok_or_else(|| {
        AppFrameworkError::ConfigUnsupported(adapter.manifest().id.as_str().to_string())
    })
}

impl AppManager {
    pub async fn astrbot_dashboard_status(
        &self,
        id: &AppInstanceId,
    ) -> Result<AstrBotDashboardStatus, AppFrameworkError> {
        let instance = self.store.require(id).await?;
        let adapter = self.registry.get(&instance.framework_id)?;
        let runtime = adapter.astrbot_runtime().ok_or_else(|| {
            AppFrameworkError::ConfigUnsupported(instance.framework_id.as_str().to_string())
        })?;
        if !matches!(instance.state, AppInstanceState::Running) {
            return Ok(AstrBotDashboardStatus::not_running());
        }
        let password = self.remembered_secret(&instance, SECRET_WEBUI_PASSWORD);
        let port = match self.desktop_webui_loopback_port(&instance).await {
            Ok(p) => p,
            Err(e) => {
                return Ok(AstrBotDashboardStatus::unreachable(
                    password.is_some(),
                    e.to_string(),
                ));
            }
        };
        let again = self.store.require(id).await?;
        if !matches!(again.state, AppInstanceState::Running) {
            return Err(AppFrameworkError::StateChanged(
                "实例状态已变，请重试".into(),
            ));
        }
        let session = AstrBotSession {
            instance_id: instance.id.as_str().to_string(),
            port,
            username: self.webui_login_username(&instance).await,
            password,
        };
        Ok(runtime.dashboard_status(&session).await)
    }

    /// 运行期资源都要实例在跑；口和密码由这里备齐，具体调用交给适配器的能力对象。
    /// 返回 Arc 而不是 `&dyn AstrBotRuntimeApi`：借用挂在 Arc 上，调用方自己 `astrbot_api`。
    async fn astrbot_session(
        &self,
        id: &AppInstanceId,
    ) -> Result<(Arc<dyn AppFrameworkAdapter>, AstrBotSession), AppFrameworkError> {
        let instance = self.store.require(id).await?;
        let adapter = self.registry.get(&instance.framework_id)?;
        if adapter.astrbot_runtime().is_none() {
            return Err(AppFrameworkError::ConfigUnsupported(
                instance.framework_id.as_str().to_string(),
            ));
        }
        if !matches!(instance.state, AppInstanceState::Running) {
            return Err(AppFrameworkError::NotRunning(
                "启动实例后才能改人格、知识库和会话规则".into(),
            ));
        }
        let port = self.desktop_webui_loopback_port(&instance).await?;
        let again = self.store.require(id).await?;
        if !matches!(again.state, AppInstanceState::Running) {
            return Err(AppFrameworkError::StateChanged(
                "实例状态已变，请重试".into(),
            ));
        }
        let session = AstrBotSession {
            instance_id: instance.id.as_str().to_string(),
            port,
            username: self.webui_login_username(&instance).await,
            password: self.remembered_secret(&instance, SECRET_WEBUI_PASSWORD),
        };
        Ok((adapter, session))
    }

    pub async fn astrbot_list_personas(
        &self,
        id: &AppInstanceId,
    ) -> Result<Vec<AstrBotPersona>, AppFrameworkError> {
        let (adapter, s) = self.astrbot_session(id).await?;
        astrbot_api(adapter.as_ref())?.list_personas(&s).await
    }

    pub async fn astrbot_upsert_persona(
        &self,
        id: &AppInstanceId,
        persona: AstrBotPersona,
        creating: bool,
    ) -> Result<Vec<AstrBotPersona>, AppFrameworkError> {
        let (adapter, s) = self.astrbot_session(id).await?;
        astrbot_api(adapter.as_ref())?
            .upsert_persona(&s, &persona, creating)
            .await
    }

    pub async fn astrbot_delete_persona(
        &self,
        id: &AppInstanceId,
        persona_id: &str,
    ) -> Result<Vec<AstrBotPersona>, AppFrameworkError> {
        let (adapter, s) = self.astrbot_session(id).await?;
        astrbot_api(adapter.as_ref())?
            .delete_persona(&s, persona_id)
            .await
    }

    pub async fn astrbot_list_kbs(
        &self,
        id: &AppInstanceId,
    ) -> Result<Vec<AstrBotKnowledgeBase>, AppFrameworkError> {
        let (adapter, s) = self.astrbot_session(id).await?;
        astrbot_api(adapter.as_ref())?.list_kbs(&s).await
    }

    pub async fn astrbot_create_kb(
        &self,
        id: &AppInstanceId,
        req: AstrBotKbCreate,
    ) -> Result<Vec<AstrBotKnowledgeBase>, AppFrameworkError> {
        let (adapter, s) = self.astrbot_session(id).await?;
        astrbot_api(adapter.as_ref())?.create_kb(&s, &req).await
    }

    pub async fn astrbot_delete_kb(
        &self,
        id: &AppInstanceId,
        kb_id: &str,
    ) -> Result<Vec<AstrBotKnowledgeBase>, AppFrameworkError> {
        let (adapter, s) = self.astrbot_session(id).await?;
        astrbot_api(adapter.as_ref())?.delete_kb(&s, kb_id).await
    }

    pub async fn astrbot_list_session_rules(
        &self,
        id: &AppInstanceId,
    ) -> Result<Vec<AstrBotSessionRule>, AppFrameworkError> {
        let (adapter, s) = self.astrbot_session(id).await?;
        astrbot_api(adapter.as_ref())?
            .list_session_rules(&s)
            .await
    }

    pub async fn astrbot_update_session_rule(
        &self,
        id: &AppInstanceId,
        rule: AstrBotSessionRule,
    ) -> Result<Vec<AstrBotSessionRule>, AppFrameworkError> {
        let (adapter, s) = self.astrbot_session(id).await?;
        astrbot_api(adapter.as_ref())?
            .update_session_rule(&s, &rule)
            .await
    }

    pub async fn astrbot_delete_session_rule(
        &self,
        id: &AppInstanceId,
        umo: &str,
        rule_key: &str,
    ) -> Result<Vec<AstrBotSessionRule>, AppFrameworkError> {
        let (adapter, s) = self.astrbot_session(id).await?;
        astrbot_api(adapter.as_ref())?
            .delete_session_rule(&s, umo, rule_key)
            .await
    }

    pub async fn astrbot_list_abconfs(
        &self,
        id: &AppInstanceId,
    ) -> Result<Vec<AstrBotAbconfInfo>, AppFrameworkError> {
        let (adapter, s) = self.astrbot_session(id).await?;
        astrbot_api(adapter.as_ref())?.list_abconfs(&s).await
    }

    pub async fn astrbot_create_abconf(
        &self,
        id: &AppInstanceId,
        name: &str,
    ) -> Result<Vec<AstrBotAbconfInfo>, AppFrameworkError> {
        let (adapter, s) = self.astrbot_session(id).await?;
        astrbot_api(adapter.as_ref())?
            .create_abconf(&s, name)
            .await
    }

    pub async fn astrbot_delete_abconf(
        &self,
        id: &AppInstanceId,
        abconf_id: &str,
    ) -> Result<Vec<AstrBotAbconfInfo>, AppFrameworkError> {
        let (adapter, s) = self.astrbot_session(id).await?;
        astrbot_api(adapter.as_ref())?
            .delete_abconf(&s, abconf_id)
            .await
    }

    pub async fn astrbot_list_source_models(
        &self,
        id: &AppInstanceId,
        source_id: &str,
    ) -> Result<Vec<String>, AppFrameworkError> {
        let (adapter, s) = self.astrbot_session(id).await?;
        astrbot_api(adapter.as_ref())?
            .list_source_models(&s, source_id)
            .await
    }

    pub async fn astrbot_list_subagent_tools(
        &self,
        id: &AppInstanceId,
    ) -> Result<Vec<String>, AppFrameworkError> {
        let (adapter, s) = self.astrbot_session(id).await?;
        astrbot_api(adapter.as_ref())?
            .list_subagent_tools(&s)
            .await
    }
}
