//! AstrBot 运行期能力对象：编排层只拿这个 trait，不认识 DashboardClient 和 AstrBot 的 HTTP 形状。

use async_trait::async_trait;
use ncd_traits::AppFrameworkError;

use super::dashboard_client::DashboardClient;
use super::runtime::{
    self, AstrBotAbconfInfo, AstrBotDashboardStatus, AstrBotKbCreate, AstrBotKnowledgeBase,
    AstrBotPersona, AstrBotSessionRule,
};

/// 一次运行期调用要的连接参数。编排层负责确认实例在跑、算出回环口、取出记住的密码。
#[derive(Debug, Clone)]
pub struct AstrBotSession {
    pub instance_id: String,
    pub port: u16,
    pub username: String,
    /// None = 桌面端没记住密码；除了状态探测以外都会被拒
    pub password: Option<String>,
}

impl AstrBotSession {
    fn password(&self) -> Result<&str, AppFrameworkError> {
        self.password
            .as_deref()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                AppFrameworkError::DashboardAuth(
                    "没有可用的 WebUI 密码。到连接页写下密码后再试".into(),
                )
            })
    }
}

/// 变更类方法统一回读全量，省得前端自己拼乐观更新。
#[async_trait]
pub trait AstrBotRuntimeApi: Send + Sync {
    async fn dashboard_status(&self, session: &AstrBotSession) -> AstrBotDashboardStatus;

    async fn list_personas(
        &self,
        session: &AstrBotSession,
    ) -> Result<Vec<AstrBotPersona>, AppFrameworkError>;

    async fn upsert_persona(
        &self,
        session: &AstrBotSession,
        persona: &AstrBotPersona,
        creating: bool,
    ) -> Result<Vec<AstrBotPersona>, AppFrameworkError>;

    async fn delete_persona(
        &self,
        session: &AstrBotSession,
        persona_id: &str,
    ) -> Result<Vec<AstrBotPersona>, AppFrameworkError>;

    async fn list_kbs(
        &self,
        session: &AstrBotSession,
    ) -> Result<Vec<AstrBotKnowledgeBase>, AppFrameworkError>;

    async fn create_kb(
        &self,
        session: &AstrBotSession,
        req: &AstrBotKbCreate,
    ) -> Result<Vec<AstrBotKnowledgeBase>, AppFrameworkError>;

    async fn delete_kb(
        &self,
        session: &AstrBotSession,
        kb_id: &str,
    ) -> Result<Vec<AstrBotKnowledgeBase>, AppFrameworkError>;

    async fn list_session_rules(
        &self,
        session: &AstrBotSession,
    ) -> Result<Vec<AstrBotSessionRule>, AppFrameworkError>;

    async fn update_session_rule(
        &self,
        session: &AstrBotSession,
        rule: &AstrBotSessionRule,
    ) -> Result<Vec<AstrBotSessionRule>, AppFrameworkError>;

    async fn delete_session_rule(
        &self,
        session: &AstrBotSession,
        umo: &str,
        rule_key: &str,
    ) -> Result<Vec<AstrBotSessionRule>, AppFrameworkError>;

    async fn list_abconfs(
        &self,
        session: &AstrBotSession,
    ) -> Result<Vec<AstrBotAbconfInfo>, AppFrameworkError>;

    async fn create_abconf(
        &self,
        session: &AstrBotSession,
        name: &str,
    ) -> Result<Vec<AstrBotAbconfInfo>, AppFrameworkError>;

    async fn delete_abconf(
        &self,
        session: &AstrBotSession,
        abconf_id: &str,
    ) -> Result<Vec<AstrBotAbconfInfo>, AppFrameworkError>;

    async fn list_source_models(
        &self,
        session: &AstrBotSession,
        source_id: &str,
    ) -> Result<Vec<String>, AppFrameworkError>;

    async fn list_subagent_tools(
        &self,
        session: &AstrBotSession,
    ) -> Result<Vec<String>, AppFrameworkError>;
}

/// 直连本机 Dashboard 的实现；`AstrBotAdapter` 用它。
pub struct DashboardRuntime;

impl DashboardRuntime {
    async fn client(session: &AstrBotSession) -> Result<DashboardClient, AppFrameworkError> {
        runtime::login_client(
            &session.instance_id,
            session.port,
            &session.username,
            session.password()?,
        )
        .await
    }
}

#[async_trait]
impl AstrBotRuntimeApi for DashboardRuntime {
    async fn dashboard_status(&self, session: &AstrBotSession) -> AstrBotDashboardStatus {
        runtime::probe_status(
            &session.instance_id,
            session.port,
            &session.username,
            session.password.as_deref(),
        )
        .await
    }

    async fn list_personas(
        &self,
        session: &AstrBotSession,
    ) -> Result<Vec<AstrBotPersona>, AppFrameworkError> {
        runtime::list_personas(&Self::client(session).await?).await
    }

    async fn upsert_persona(
        &self,
        session: &AstrBotSession,
        persona: &AstrBotPersona,
        creating: bool,
    ) -> Result<Vec<AstrBotPersona>, AppFrameworkError> {
        let client = Self::client(session).await?;
        runtime::upsert_persona(&client, persona, creating).await?;
        runtime::list_personas(&client).await
    }

    async fn delete_persona(
        &self,
        session: &AstrBotSession,
        persona_id: &str,
    ) -> Result<Vec<AstrBotPersona>, AppFrameworkError> {
        let client = Self::client(session).await?;
        runtime::delete_persona(&client, persona_id).await?;
        runtime::list_personas(&client).await
    }

    async fn list_kbs(
        &self,
        session: &AstrBotSession,
    ) -> Result<Vec<AstrBotKnowledgeBase>, AppFrameworkError> {
        runtime::list_kbs(&Self::client(session).await?).await
    }

    async fn create_kb(
        &self,
        session: &AstrBotSession,
        req: &AstrBotKbCreate,
    ) -> Result<Vec<AstrBotKnowledgeBase>, AppFrameworkError> {
        let client = Self::client(session).await?;
        runtime::create_kb(&client, req).await?;
        runtime::list_kbs(&client).await
    }

    async fn delete_kb(
        &self,
        session: &AstrBotSession,
        kb_id: &str,
    ) -> Result<Vec<AstrBotKnowledgeBase>, AppFrameworkError> {
        let client = Self::client(session).await?;
        runtime::delete_kb(&client, kb_id).await?;
        runtime::list_kbs(&client).await
    }

    async fn list_session_rules(
        &self,
        session: &AstrBotSession,
    ) -> Result<Vec<AstrBotSessionRule>, AppFrameworkError> {
        runtime::list_session_rules(&Self::client(session).await?).await
    }

    async fn update_session_rule(
        &self,
        session: &AstrBotSession,
        rule: &AstrBotSessionRule,
    ) -> Result<Vec<AstrBotSessionRule>, AppFrameworkError> {
        let client = Self::client(session).await?;
        runtime::update_session_rule(&client, rule).await?;
        runtime::list_session_rules(&client).await
    }

    async fn delete_session_rule(
        &self,
        session: &AstrBotSession,
        umo: &str,
        rule_key: &str,
    ) -> Result<Vec<AstrBotSessionRule>, AppFrameworkError> {
        let client = Self::client(session).await?;
        runtime::delete_session_rule(&client, umo, rule_key).await?;
        runtime::list_session_rules(&client).await
    }

    async fn list_abconfs(
        &self,
        session: &AstrBotSession,
    ) -> Result<Vec<AstrBotAbconfInfo>, AppFrameworkError> {
        runtime::list_abconfs(&Self::client(session).await?).await
    }

    async fn create_abconf(
        &self,
        session: &AstrBotSession,
        name: &str,
    ) -> Result<Vec<AstrBotAbconfInfo>, AppFrameworkError> {
        let client = Self::client(session).await?;
        runtime::create_abconf(&client, name).await?;
        runtime::list_abconfs(&client).await
    }

    async fn delete_abconf(
        &self,
        session: &AstrBotSession,
        abconf_id: &str,
    ) -> Result<Vec<AstrBotAbconfInfo>, AppFrameworkError> {
        let client = Self::client(session).await?;
        runtime::delete_abconf(&client, abconf_id).await?;
        runtime::list_abconfs(&client).await
    }

    async fn list_source_models(
        &self,
        session: &AstrBotSession,
        source_id: &str,
    ) -> Result<Vec<String>, AppFrameworkError> {
        runtime::list_source_models(&Self::client(session).await?, source_id).await
    }

    async fn list_subagent_tools(
        &self,
        session: &AstrBotSession,
    ) -> Result<Vec<String>, AppFrameworkError> {
        runtime::list_available_tools(&Self::client(session).await?).await
    }
}
