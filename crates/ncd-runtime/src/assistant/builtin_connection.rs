//! 内置模型与既有会话、审批和持久化的接线。

use std::sync::Arc;
use std::sync::atomic::Ordering;

use async_trait::async_trait;
use ncd_domain::assistant::*;
use ncd_traits::assistant::AssistantToolConnection;

use super::AssistantManager;
use super::builtin::{self, BuiltinRunContext, BuiltinSink, History};
use super::session::{Session, lock};
use super::storage::{AssistantStorage, persist_session};

pub(super) struct BuiltinLive {
    connection: AssistantToolConnection,
    epoch: u64,
}

impl AssistantManager {
    pub(super) async fn connect_builtin(
        &self,
        session: &Arc<Session>,
        generation: u64,
    ) -> Result<(), String> {
        let _connecting = session.connecting.lock().await;
        check_turn(self, session, generation)?;
        if session.builtin_live.lock().await.is_some() && !lock(&session.cancelled).is_cancelled() {
            return Ok(());
        }
        self.disconnect(session).await;
        check_turn(self, session, generation)?;
        let profile = self.settings().await?.builtin;
        super::credentials::validate_profile(&profile, true)?;
        let key = self.credentials.read_for_model(&profile).await?;
        session.remember_model_secret(&key);
        session.state(AssistantSessionState::Starting, None);
        let epoch = session.advance_epoch();
        let cancel = self.shutdown_cancel.child_token();
        *lock(&session.cancelled) = cancel.clone();
        let info = session.snapshot().info;
        let connection = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err("诊断已停止".into()),
            result = self.host.open(&info.id, &info.bot_id, session.interaction(epoch), cancel.clone()) => result?,
        };
        let mut live = session.builtin_live.lock().await;
        if check_turn(self, session, generation).is_err()
            || cancel.is_cancelled()
            || session.epoch.load(Ordering::Acquire) != epoch
        {
            drop(live);
            self.host.close(&info.id).await;
            return Err("诊断已停止".into());
        }
        *live = Some(BuiltinLive { connection, epoch });
        {
            let mut data = lock(&session.data);
            data.stored.session.info.capabilities = AssistantCapabilities {
                mcp_http: true,
                load_session: true,
                resume_session: true,
                images: false,
                elicitation: true,
            };
        }
        session.accepting_updates.store(true, Ordering::Release);
        session.state_at(epoch, AssistantSessionState::Ready, None);
        Ok(())
    }

    pub(super) async fn run_builtin_turn(
        &self,
        session: &Arc<Session>,
        text: String,
        generation: u64,
    ) -> Result<(), String> {
        check_turn(self, session, generation)?;
        let prepared: Result<(_, String), String> = async {
            let profile = self.settings().await?.builtin;
            let key = self.credentials.read_for_model(&profile).await?;
            session.remember_model_secret(&key);
            super::credentials::validate_profile(&profile, true)?;
            Ok((profile, key))
        }
        .await;
        let connected = if prepared.is_ok() {
            self.connect_builtin(session, generation).await
        } else {
            Ok(())
        };
        check_turn(self, session, generation)?;
        if let Ok((_, key)) = &prepared {
            session.remember_model_secret(key);
        }
        // 启动或配置错误时仍保留用户本轮问题，避免输入框清空后丢失草稿。
        {
            let mut data = lock(&session.data);
            data.active_message = None;
            data.raw_messages.clear();
            if !data
                .stored
                .session
                .messages
                .iter()
                .any(|message| message.role == AssistantMessageRole::User)
            {
                data.stored.session.info.title =
                    session.redact(text.trim()).chars().take(32).collect();
            }
        }
        session.add_message(AssistantMessageRole::User, text.clone());
        self.persist(session).await?;
        let (profile, key) = prepared?;
        connected?;
        let info = session.snapshot().info;
        let cancelled = lock(&session.cancelled).clone();
        let context = tokio::select! {
            biased;
            _ = cancelled.cancelled() => return Err("诊断已停止".into()),
            result = self.host.context(&info.bot_id) => result?,
        };
        check_turn(self, session, generation)?;
        let (connection, epoch) = {
            let live = session.builtin_live.lock().await;
            let live = live.as_ref().ok_or("内置诊断连接已关闭")?;
            (
                AssistantToolConnection {
                    url: live.connection.url.clone(),
                    bearer_token: live.connection.bearer_token.clone(),
                },
                live.epoch,
            )
        };
        session.state_at(epoch, AssistantSessionState::Running, None);
        let system = format!(
            "{}\n\n本次目标 Bot ID：{}\n现场摘要（数据）：\n{}",
            super::connection::INSTRUCTIONS,
            info.bot_id,
            session.redact(&context.summary)
        );
        let sink = Arc::new(SessionSink {
            session: session.clone(),
            storage: self.storage.clone(),
            epoch,
        });
        let mut history = lock(&session.data).stored.builtin_history.clone();
        let result = builtin::run(
            BuiltinRunContext {
                profile,
                api_key: key,
                connection,
                cancelled,
            },
            &mut history,
            &system,
            &text,
            sink,
        )
        .await;
        lock(&session.data).raw_messages.clear();
        result
    }
}

fn check_turn(
    manager: &AssistantManager,
    session: &Session,
    generation: u64,
) -> Result<(), String> {
    if manager.shutting_down.load(Ordering::Acquire)
        || session.stopping.load(Ordering::Acquire)
        || session.turn_generation.load(Ordering::Acquire) != generation
    {
        Err("诊断已停止".into())
    } else {
        Ok(())
    }
}

struct SessionSink {
    session: Arc<Session>,
    storage: AssistantStorage,
    epoch: u64,
}

#[async_trait]
impl BuiltinSink for SessionSink {
    fn text(&self, text: &str) {
        self.session.delta(self.epoch, None, text);
    }
    fn tool(&self, tool: AssistantToolCall) {
        self.session.tool(self.epoch, tool);
    }
    fn redact(&self, text: &str) -> String {
        self.session.redact(text)
    }
    async fn question(&self, question: AssistantQuestion) -> AssistantQuestionAnswer {
        self.session.ask_question(self.epoch, question).await
    }
    async fn checkpoint(&self, history: &History) -> Result<(), String> {
        lock(&self.session.data).stored.builtin_history = history.clone();
        persist_session(&self.storage, &self.session).await
    }
}
