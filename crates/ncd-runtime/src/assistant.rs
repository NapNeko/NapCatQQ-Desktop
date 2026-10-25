//! 驱动本机 Agent 完成有目标范围的诊断会话。

mod acp;
mod builtin;
mod builtin_connection;
#[cfg(test)]
mod builtin_integration_tests;
mod connection;
mod credentials;
mod events;
mod login;
mod providers;
mod session;
mod settings;
mod storage;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use ncd_domain::assistant::*;
use ncd_traits::assistant::{AssistantEventSink, AssistantToolHost};
use tokio::sync::Mutex as AsyncMutex;
use tokio_util::sync::CancellationToken;

use session::{EventHub, Session, lock, now_ms};
use settings::normalized_settings;
use storage::{AssistantStorage, StoredSession};

pub struct AssistantManager {
    storage: AssistantStorage,
    host: Arc<dyn AssistantToolHost>,
    settings: Mutex<AssistantSettings>,
    sessions: Mutex<HashMap<String, Arc<Session>>>,
    notices: Mutex<Vec<String>>,
    hub: Arc<EventHub>,
    admission: AsyncMutex<()>,
    load_gate: AsyncMutex<()>,
    settings_gate: AsyncMutex<()>,
    loaded: AtomicBool,
    shutting_down: AtomicBool,
    shutdown_cancel: CancellationToken,
    credentials: credentials::ModelCredentials,
    updating_credentials: AtomicBool,
}

impl AssistantManager {
    pub fn new(data_root: PathBuf, host: Arc<dyn AssistantToolHost>) -> Self {
        Self {
            storage: AssistantStorage::new(&data_root),
            host,
            settings: Mutex::new(AssistantSettings::default()),
            sessions: Mutex::new(HashMap::new()),
            notices: Mutex::new(Vec::new()),
            hub: Arc::new(EventHub::default()),
            admission: AsyncMutex::new(()),
            load_gate: AsyncMutex::new(()),
            settings_gate: AsyncMutex::new(()),
            loaded: AtomicBool::new(false),
            shutting_down: AtomicBool::new(false),
            shutdown_cancel: CancellationToken::new(),
            credentials: credentials::ModelCredentials::default(),
            updating_credentials: AtomicBool::new(false),
        }
    }

    async fn ensure_loaded(&self) -> Result<(), String> {
        if self.loaded.load(Ordering::Acquire) {
            return Ok(());
        }
        let _load = self.load_gate.lock().await;
        if self.loaded.load(Ordering::Acquire) {
            return Ok(());
        }
        let storage = self.storage.clone();
        let (settings, sessions, mut notices) = tokio::task::spawn_blocking(move || {
            let settings = storage.settings();
            let (sessions, notices) = storage.load();
            (settings, sessions, notices)
        })
        .await
        .map_err(|_| "加载诊断助手失败")?;
        match settings {
            Ok(settings) => match providers::validate_settings(&settings) {
                Ok(()) => *lock(&self.settings) = normalized_settings(settings),
                Err(error) => notices.push(format!("Agent 设置无法使用，已保留原文件：{error}")),
            },
            Err(error) => notices.push(error),
        }
        let mut destination = lock(&self.sessions);
        for mut stored in sessions {
            session::trim_session(&mut stored.session);
            if stored.builtin_history.validate().is_err() {
                stored.builtin_history = builtin::History::default();
                notices.push(format!(
                    "会话 {} 的模型历史无效，显示记录已保留，继续时将重新建立模型历史",
                    stored.session.info.id
                ));
            }
            destination.insert(
                stored.session.info.id.clone(),
                Arc::new(Session::new(
                    stored,
                    Arc::clone(&self.host),
                    Arc::clone(&self.hub),
                )),
            );
        }
        *lock(&self.notices) = notices;
        self.loaded.store(true, Ordering::Release);
        Ok(())
    }

    pub async fn list_sessions(&self) -> Result<AssistantSessionList, String> {
        self.ensure_loaded().await?;
        let mut sessions = lock(&self.sessions)
            .values()
            .map(|session| session.snapshot().info)
            .collect::<Vec<_>>();
        sessions.sort_by(|a, b| b.updated_at_ms.total_cmp(&a.updated_at_ms));
        Ok(AssistantSessionList {
            sessions,
            notices: lock(&self.notices).clone(),
        })
    }

    fn session(&self, id: &str) -> Result<Arc<Session>, String> {
        storage::checked_id(id)?;
        lock(&self.sessions)
            .get(id)
            .cloned()
            .ok_or_else(|| "诊断会话不存在".into())
    }

    pub async fn get_session(&self, id: &str) -> Result<AssistantSession, String> {
        self.ensure_loaded().await?;
        Ok(self.session(id)?.snapshot())
    }

    pub fn subscribe(&self, sink: Arc<dyn AssistantEventSink>) {
        self.hub.subscribe(sink);
    }

    pub fn page_loading(&self) {
        self.hub.clear();
    }

    pub async fn create_session(
        self: &Arc<Self>,
        provider: AssistantProvider,
        bot_id: String,
    ) -> Result<AssistantSession, String> {
        self.ensure_loaded().await?;
        let _admission = self.admission.lock().await;
        if self.shutting_down.load(Ordering::Acquire) {
            return Err("诊断助手正在退出".into());
        }
        if lock(&self.sessions).len() >= storage::SESSION_LIMIT {
            return Err("最多保留50个诊断会话，请先删除不再需要的会话".into());
        }
        if bot_id.is_empty() || bot_id.len() > 128 {
            return Err("请选择要诊断的 Bot".into());
        }
        let context = tokio::select! {
            biased;
            _ = self.shutdown_cancel.cancelled() => return Err("诊断助手正在退出".into()),
            result = self.host.context(&bot_id) => result?,
        };
        if self.shutting_down.load(Ordering::Acquire) {
            return Err("诊断助手正在退出".into());
        }
        let id = uuid::Uuid::new_v4().to_string();
        let now = now_ms();
        let session = Arc::new(Session::new(
            StoredSession {
                v: 1,
                remote_session_id: None,
                builtin_history: builtin::History::default(),
                session: AssistantSession {
                    sequence: 0,
                    info: AssistantSessionInfo {
                        id: id.clone(),
                        provider,
                        bot_id,
                        bot_name: self.host.redact(&context.bot_name),
                        title: "新的诊断".into(),
                        state: AssistantSessionState::Starting,
                        capabilities: AssistantCapabilities::default(),
                        created_at_ms: now,
                        updated_at_ms: now,
                        error: None,
                    },
                    messages: Vec::new(),
                    tools: Vec::new(),
                    permissions: Vec::new(),
                    questions: Vec::new(),
                },
            },
            Arc::clone(&self.host),
            Arc::clone(&self.hub),
        ));
        let _turn = session.turn.lock().await;
        let generation = session.turn_generation.load(Ordering::Acquire);
        lock(&self.sessions).insert(id, Arc::clone(&session));
        self.persist(&session).await?;
        if let Some(notice) = providers::configuration_notice(provider, &self.settings().await?) {
            session.add_message(AssistantMessageRole::System, notice.to_owned());
        }
        if let Err(error) = self.connect(&session, generation).await {
            if session.turn_generation.load(Ordering::Acquire) == generation {
                session.state(AssistantSessionState::Failed, Some(error));
            }
        }
        self.persist(&session).await?;
        Ok(session.snapshot())
    }

    pub async fn send_message(self: &Arc<Self>, id: &str, text: String) -> Result<(), String> {
        self.ensure_loaded().await?;
        if self.shutting_down.load(Ordering::Acquire) {
            return Err("诊断助手正在退出".into());
        }
        if text.trim().is_empty() || text.len() > 16 * 1024 {
            return Err("请输入问题，最多16 KiB".into());
        }
        let session = self.session(id)?;
        let generation = {
            let data = lock(&session.data);
            if data.stored.session.info.provider == AssistantProvider::Builtin
                && self.updating_credentials.load(Ordering::Acquire)
            {
                return Err("模型凭据正在更新，请稍后再试".into());
            }
            if session.stopping.load(Ordering::Acquire) {
                return Err("此会话正在停止，请稍后再试".into());
            }
            if data.stored.session.info.state == AssistantSessionState::Closed {
                return Err("此会话已关闭，请新建诊断".into());
            }
            if session
                .running
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
            {
                return Err("Agent 正在处理上一条消息，可以先停止".into());
            }
            session.turn_generation.fetch_add(1, Ordering::AcqRel) + 1
        };
        let manager = Arc::clone(self);
        tokio::spawn(async move {
            let _turn = session.turn.lock().await;
            {
                let data = lock(&session.data);
                if session.turn_generation.load(Ordering::Acquire) != generation
                    || session.stopping.load(Ordering::Acquire)
                    || data.stored.session.info.state == AssistantSessionState::Closed
                {
                    // 过期排队任务不能清除后来一轮持有的 running 标志。
                    if session.turn_generation.load(Ordering::Acquire) == generation {
                        session.running.store(false, Ordering::Release);
                    }
                    return;
                }
            }
            let result = manager.run_turn(&session, text, generation).await;
            session.running.store(false, Ordering::Release);
            session.clear_interactions();
            lock(&session.data).raw_messages.clear();
            if let Err(error) = result {
                if session.turn_generation.load(Ordering::Acquire) == generation
                    && !session.stopping.load(Ordering::Acquire)
                    && session.snapshot().info.state != AssistantSessionState::Closed
                {
                    manager.disconnect(&session).await;
                    if session.turn_generation.load(Ordering::Acquire) == generation
                        && !session.stopping.load(Ordering::Acquire)
                    {
                        session.state(AssistantSessionState::Failed, Some(error));
                    }
                }
            } else if matches!(
                session.snapshot().info.state,
                AssistantSessionState::Running | AssistantSessionState::WaitingForInput
            ) && session.turn_generation.load(Ordering::Acquire) == generation
                && !session.stopping.load(Ordering::Acquire)
            {
                session.state(AssistantSessionState::Ready, None);
            }
            if let Err(error) = manager.persist(&session).await {
                session.state(AssistantSessionState::Failed, Some(error));
            }
        });
        Ok(())
    }

    pub async fn cancel_session(&self, id: &str) -> Result<(), String> {
        self.ensure_loaded().await?;
        let session = self.session(id)?;
        self.stop_session(&session, false, false).await
    }

    pub async fn close_session(&self, id: &str) -> Result<(), String> {
        self.ensure_loaded().await?;
        let session = self.session(id)?;
        self.stop_session(&session, true, false).await
    }

    pub async fn delete_session(&self, id: &str) -> Result<(), String> {
        self.ensure_loaded().await?;
        let session = self.session(id)?;
        self.close_session(id).await?;
        self.delete_archive(&session).await?;
        lock(&self.sessions).remove(id);
        session.emit(AssistantUpdate::Removed);
        Ok(())
    }

    async fn delete_archive(&self, session: &Session) -> Result<(), String> {
        // 已入场的原子写必须完成后再删，之后排队的持久化看到 deleted 会跳过。
        let _persist = session.persisting.lock().await;
        session.deleted.store(true, Ordering::Release);
        let storage = self.storage.clone();
        let saved_id = session.id();
        let result = tokio::task::spawn_blocking(move || storage.delete(&saved_id))
            .await
            .map_err(|_| "删除会话失败".to_owned())
            .and_then(|result| result);
        if result.is_err() {
            session.deleted.store(false, Ordering::Release);
        }
        result
    }

    pub async fn respond_permission(
        &self,
        session_id: &str,
        request_id: &str,
        option_id: Option<String>,
    ) -> Result<(), String> {
        self.session(session_id)?
            .answer_permission(request_id, option_id)
    }

    pub async fn respond_question(
        &self,
        session_id: &str,
        request_id: &str,
        answer: AssistantQuestionAnswer,
    ) -> Result<(), String> {
        self.session(session_id)?
            .answer_question(request_id, answer)
    }

    pub async fn shutdown(&self) {
        self.shutting_down.store(true, Ordering::Release);
        self.shutdown_cancel.cancel();
        self.suspend_all().await;
    }

    pub async fn suspend_all(&self) {
        let _admission = self.admission.lock().await;
        let sessions = lock(&self.sessions).values().cloned().collect::<Vec<_>>();
        for session in sessions {
            let _ = self.stop_session(&session, false, true).await;
        }
    }

    async fn stop_session(
        &self,
        session: &Arc<Session>,
        close: bool,
        wait: bool,
    ) -> Result<(), String> {
        loop {
            let admitted = {
                let _data = lock(&session.data);
                if session
                    .stopping
                    .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok()
                {
                    session.turn_generation.fetch_add(1, Ordering::AcqRel);
                    session.accepting_updates.store(false, Ordering::Release);
                    true
                } else {
                    false
                }
            };
            if admitted {
                break;
            }
            if !wait {
                return Err("此会话正在停止，请稍后再试".into());
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        let _admission = StopAdmission(session);
        session.revoke();
        self.disconnect(session).await;
        let _turn = session.turn.lock().await;
        // 创建连接也可能在首次断开时尚未装入 live；拿到轮次锁后再清理一次。
        self.disconnect(session).await;
        session.running.store(false, Ordering::Release);
        let closed = close || session.snapshot().info.state == AssistantSessionState::Closed;
        session.state(
            if closed {
                AssistantSessionState::Closed
            } else {
                AssistantSessionState::Disconnected
            },
            None,
        );
        let result = self.persist(session).await;
        session.clear_model_secret();
        result
    }

    async fn persist(&self, session: &Session) -> Result<(), String> {
        storage::persist_session(&self.storage, session).await
    }
}

struct StopAdmission<'a>(&'a Session);

impl Drop for StopAdmission<'_> {
    fn drop(&mut self) {
        let _data = lock(&self.0.data);
        self.0.stopping.store(false, Ordering::Release);
    }
}
