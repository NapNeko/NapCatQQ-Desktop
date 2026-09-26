//! MaiBot 运行期能力对象：编排层只拿这个 trait，不认识 WebUI 客户端和上游 HTTP 的形状。

use async_trait::async_trait;
use ncd_traits::AppFrameworkError;

use super::resources::MaiBotResourceDone;
use super::resources::emoji::{
    self, MaiBotEmojiAction, MaiBotEmojiImage, MaiBotEmojiOverview, MaiBotEmojiPage, MaiBotEmojiQuery,
    MaiBotEmojiUpload, MaiBotEmojiUploadDone,
};
use super::resources::person::{self, MaiBotPersonAction, MaiBotPersonOverview, MaiBotPersonPage, MaiBotPersonQuery};
use super::resources::expression::{
    self, MaiBotExpressionAction, MaiBotExpressionOverview, MaiBotExpressionPage, MaiBotExpressionQuery,
};
use super::resources::jargon::{self, MaiBotJargonAction, MaiBotJargonOverview, MaiBotJargonPage, MaiBotJargonQuery};
use super::resources::prompts::{
    self, MaiBotPromptAction, MaiBotPromptCatalog, MaiBotPromptFile, MaiBotPromptTarget,
};
use super::runtime::{
    MaiBotChatSession, MaiBotMcpStatus, MaiBotMcpTest, MaiBotProviderCheck, MaiBotProviderModel,
    MaiBotRuntimeGate, MaiBotRuntimeStatus, MaiBotStatsSummary,
};
use super::schema::{MaiBotAPIProvider, MaiBotMCPServerItemConfig};
use super::webui_client::MaiBotWebUi;

/// 一次运行期调用要的连接参数。编排层负责确认实例在跑、算出回环口、读出盘上的 token
#[derive(Debug, Clone)]
pub struct MaiBotSession {
    pub instance_id: String,
    pub port: u16,
    pub token: String,
}

/// 提供商在哪：盘上已有且和表单里一样的按名字查（上游放行它自己配的内网地址，本机 Ollama 也能查），
/// 否则按表单里的草稿地址查（上游只放行公网）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaiBotProviderSource {
    Saved,
    Draft,
}

#[async_trait]
pub trait MaiBotRuntimeApi: Send + Sync {
    /// 不报错：连不上、token 不对都折成 gate，前端按 gate 出提示
    async fn status(&self, session: &MaiBotSession) -> MaiBotRuntimeStatus;

    async fn restart(&self, session: &MaiBotSession) -> Result<(), AppFrameworkError>;

    async fn stats_summary(
        &self,
        session: &MaiBotSession,
        hours: u32,
    ) -> Result<MaiBotStatsSummary, AppFrameworkError>;

    async fn chat_sessions(&self, session: &MaiBotSession) -> Result<Vec<MaiBotChatSession>, AppFrameworkError>;

    async fn provider_models(
        &self,
        session: &MaiBotSession,
        provider: &MaiBotAPIProvider,
        source: MaiBotProviderSource,
    ) -> Result<Vec<MaiBotProviderModel>, AppFrameworkError>;

    async fn test_provider(
        &self,
        session: &MaiBotSession,
        provider: &MaiBotAPIProvider,
        source: MaiBotProviderSource,
    ) -> Result<MaiBotProviderCheck, AppFrameworkError>;

    async fn mcp_status(&self, session: &MaiBotSession) -> Result<MaiBotMcpStatus, AppFrameworkError>;

    async fn test_mcp(
        &self,
        session: &MaiBotSession,
        server: &MaiBotMCPServerItemConfig,
    ) -> Result<MaiBotMcpTest, AppFrameworkError>;

    /// 提示词停着也能改：target 由编排层按实例状态给
    async fn prompt_catalog(&self, target: MaiBotPromptTarget<'_>) -> Result<MaiBotPromptCatalog, AppFrameworkError>;

    async fn prompt_file(
        &self,
        target: MaiBotPromptTarget<'_>,
        language: &str,
        name: &str,
    ) -> Result<MaiBotPromptFile, AppFrameworkError>;

    async fn prompt_version(
        &self,
        target: MaiBotPromptTarget<'_>,
        language: &str,
        name: &str,
        version_id: &str,
    ) -> Result<String, AppFrameworkError>;

    async fn prompt_action(
        &self,
        target: MaiBotPromptTarget<'_>,
        action: &MaiBotPromptAction,
    ) -> Result<MaiBotPromptFile, AppFrameworkError>;

    async fn expressions(
        &self,
        session: &MaiBotSession,
        query: &MaiBotExpressionQuery,
    ) -> Result<MaiBotExpressionPage, AppFrameworkError>;

    async fn expression_overview(&self, session: &MaiBotSession) -> Result<MaiBotExpressionOverview, AppFrameworkError>;

    async fn expression_action(
        &self,
        session: &MaiBotSession,
        action: &MaiBotExpressionAction,
    ) -> Result<MaiBotResourceDone, AppFrameworkError>;

    async fn jargons(&self, session: &MaiBotSession, query: &MaiBotJargonQuery) -> Result<MaiBotJargonPage, AppFrameworkError>;

    async fn jargon_overview(&self, session: &MaiBotSession) -> Result<MaiBotJargonOverview, AppFrameworkError>;

    async fn jargon_action(
        &self,
        session: &MaiBotSession,
        action: &MaiBotJargonAction,
    ) -> Result<MaiBotResourceDone, AppFrameworkError>;

    async fn persons(&self, session: &MaiBotSession, query: &MaiBotPersonQuery) -> Result<MaiBotPersonPage, AppFrameworkError>;

    async fn person_overview(&self, session: &MaiBotSession) -> Result<MaiBotPersonOverview, AppFrameworkError>;

    async fn person_action(
        &self,
        session: &MaiBotSession,
        action: &MaiBotPersonAction,
    ) -> Result<MaiBotResourceDone, AppFrameworkError>;

    async fn emojis(&self, session: &MaiBotSession, query: &MaiBotEmojiQuery) -> Result<MaiBotEmojiPage, AppFrameworkError>;

    async fn emoji_overview(&self, session: &MaiBotSession) -> Result<MaiBotEmojiOverview, AppFrameworkError>;

    async fn emoji_action(
        &self,
        session: &MaiBotSession,
        action: &MaiBotEmojiAction,
    ) -> Result<MaiBotResourceDone, AppFrameworkError>;

    /// 缩略图，`original` 时是原图（动图保留）
    async fn emoji_image(
        &self,
        session: &MaiBotSession,
        id: i64,
        original: bool,
    ) -> Result<MaiBotEmojiImage, AppFrameworkError>;

    /// 读本机的图传上去；读不了、上游不收的逐张记在结果里，不整批失败
    async fn emoji_upload(
        &self,
        session: &MaiBotSession,
        upload: &MaiBotEmojiUpload,
    ) -> Result<MaiBotEmojiUploadDone, AppFrameworkError>;
}

/// 直连本机回环 WebUI 的实现；`MaiBotAdapter` 用它
pub struct WebUiRuntime;

fn client(s: &MaiBotSession) -> Result<MaiBotWebUi, AppFrameworkError> {
    MaiBotWebUi::connect(s.port, &s.token)
}

#[async_trait]
impl MaiBotRuntimeApi for WebUiRuntime {
    async fn status(&self, session: &MaiBotSession) -> MaiBotRuntimeStatus {
        let result = match client(session) {
            Ok(c) => c.status().await,
            Err(e) => Err(e),
        };
        match result {
            Ok(s) => MaiBotRuntimeStatus {
                gate: MaiBotRuntimeGate::Ok,
                message: String::new(),
                version: s.version.filter(|v| !v.is_empty()),
                uptime_secs: s.uptime,
            },
            Err(AppFrameworkError::DashboardAuth(m)) => MaiBotRuntimeStatus::gate(MaiBotRuntimeGate::Auth, m),
            Err(AppFrameworkError::DashboardUnreachable(_)) => MaiBotRuntimeStatus::gate(
                MaiBotRuntimeGate::Unreachable,
                "麦麦的 WebUI 还没应答，刚启动的话等它起来",
            ),
            Err(e) => MaiBotRuntimeStatus::gate(MaiBotRuntimeGate::Unreachable, e.to_string()),
        }
    }

    async fn restart(&self, session: &MaiBotSession) -> Result<(), AppFrameworkError> {
        client(session)?.restart().await
    }

    async fn stats_summary(
        &self,
        session: &MaiBotSession,
        hours: u32,
    ) -> Result<MaiBotStatsSummary, AppFrameworkError> {
        client(session)?.stats_summary(hours).await
    }

    async fn chat_sessions(&self, session: &MaiBotSession) -> Result<Vec<MaiBotChatSession>, AppFrameworkError> {
        Ok(client(session)?.chat_sessions().await?.into_sessions())
    }

    async fn provider_models(
        &self,
        session: &MaiBotSession,
        provider: &MaiBotAPIProvider,
        source: MaiBotProviderSource,
    ) -> Result<Vec<MaiBotProviderModel>, AppFrameworkError> {
        let c = client(session)?;
        let list = match source {
            MaiBotProviderSource::Saved => {
                c.provider_models_by_name(&provider.name, provider.client_type == "gemini").await?
            }
            MaiBotProviderSource::Draft => c.provider_models_by_url(provider).await?,
        };
        Ok(list.into_models())
    }

    async fn test_provider(
        &self,
        session: &MaiBotSession,
        provider: &MaiBotAPIProvider,
        source: MaiBotProviderSource,
    ) -> Result<MaiBotProviderCheck, AppFrameworkError> {
        let c = client(session)?;
        let check = match source {
            MaiBotProviderSource::Saved => c.test_provider_by_name(&provider.name).await?,
            MaiBotProviderSource::Draft => c.test_provider_by_url(provider).await?,
        };
        Ok(check.into())
    }

    async fn mcp_status(&self, session: &MaiBotSession) -> Result<MaiBotMcpStatus, AppFrameworkError> {
        Ok(client(session)?.mcp_status().await?.into())
    }

    async fn test_mcp(
        &self,
        session: &MaiBotSession,
        server: &MaiBotMCPServerItemConfig,
    ) -> Result<MaiBotMcpTest, AppFrameworkError> {
        let body = serde_json::to_value(server)
            .map_err(|e| AppFrameworkError::Integration(format!("MCP 服务配置转不成 JSON：{e}")))?;
        Ok(client(session)?.mcp_test(&body).await?.into())
    }

    async fn prompt_catalog(&self, target: MaiBotPromptTarget<'_>) -> Result<MaiBotPromptCatalog, AppFrameworkError> {
        prompts::catalog(target).await
    }

    async fn prompt_file(
        &self,
        target: MaiBotPromptTarget<'_>,
        language: &str,
        name: &str,
    ) -> Result<MaiBotPromptFile, AppFrameworkError> {
        prompts::file(target, language, name).await
    }

    async fn prompt_version(
        &self,
        target: MaiBotPromptTarget<'_>,
        language: &str,
        name: &str,
        version_id: &str,
    ) -> Result<String, AppFrameworkError> {
        prompts::version(target, language, name, version_id).await
    }

    async fn prompt_action(
        &self,
        target: MaiBotPromptTarget<'_>,
        action: &MaiBotPromptAction,
    ) -> Result<MaiBotPromptFile, AppFrameworkError> {
        prompts::act(target, action).await
    }

    async fn expressions(
        &self,
        session: &MaiBotSession,
        query: &MaiBotExpressionQuery,
    ) -> Result<MaiBotExpressionPage, AppFrameworkError> {
        expression::list(&client(session)?, query).await
    }

    async fn expression_overview(&self, session: &MaiBotSession) -> Result<MaiBotExpressionOverview, AppFrameworkError> {
        expression::overview(&client(session)?).await
    }

    async fn expression_action(
        &self,
        session: &MaiBotSession,
        action: &MaiBotExpressionAction,
    ) -> Result<MaiBotResourceDone, AppFrameworkError> {
        expression::act(&client(session)?, action).await
    }

    async fn jargons(&self, session: &MaiBotSession, query: &MaiBotJargonQuery) -> Result<MaiBotJargonPage, AppFrameworkError> {
        jargon::list(&client(session)?, query).await
    }

    async fn jargon_overview(&self, session: &MaiBotSession) -> Result<MaiBotJargonOverview, AppFrameworkError> {
        jargon::overview(&client(session)?).await
    }

    async fn jargon_action(
        &self,
        session: &MaiBotSession,
        action: &MaiBotJargonAction,
    ) -> Result<MaiBotResourceDone, AppFrameworkError> {
        jargon::act(&client(session)?, action).await
    }

    async fn persons(&self, session: &MaiBotSession, query: &MaiBotPersonQuery) -> Result<MaiBotPersonPage, AppFrameworkError> {
        person::list(&client(session)?, query).await
    }

    async fn person_overview(&self, session: &MaiBotSession) -> Result<MaiBotPersonOverview, AppFrameworkError> {
        person::overview(&client(session)?).await
    }

    async fn person_action(
        &self,
        session: &MaiBotSession,
        action: &MaiBotPersonAction,
    ) -> Result<MaiBotResourceDone, AppFrameworkError> {
        person::act(&client(session)?, action).await
    }

    async fn emojis(&self, session: &MaiBotSession, query: &MaiBotEmojiQuery) -> Result<MaiBotEmojiPage, AppFrameworkError> {
        emoji::list(&client(session)?, query).await
    }

    async fn emoji_overview(&self, session: &MaiBotSession) -> Result<MaiBotEmojiOverview, AppFrameworkError> {
        emoji::overview(&client(session)?).await
    }

    async fn emoji_action(
        &self,
        session: &MaiBotSession,
        action: &MaiBotEmojiAction,
    ) -> Result<MaiBotResourceDone, AppFrameworkError> {
        emoji::act(&client(session)?, action).await
    }

    async fn emoji_image(
        &self,
        session: &MaiBotSession,
        id: i64,
        original: bool,
    ) -> Result<MaiBotEmojiImage, AppFrameworkError> {
        emoji::image(&client(session)?, id, original).await
    }

    async fn emoji_upload(
        &self,
        session: &MaiBotSession,
        upload: &MaiBotEmojiUpload,
    ) -> Result<MaiBotEmojiUploadDone, AppFrameworkError> {
        emoji::upload(&client(session)?, upload).await
    }
}
