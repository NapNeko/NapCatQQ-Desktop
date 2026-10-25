//! 会话快照、事件和只在本轮有效的桌面交互。

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};
use std::time::Duration;

use async_trait::async_trait;
use ncd_domain::assistant::*;
use ncd_traits::assistant::{AssistantEventSink, AssistantInteraction, AssistantToolHost};
use tokio::sync::{Mutex as AsyncMutex, oneshot};
use tokio_util::sync::CancellationToken;

use super::acp::AcpProcess;
use super::storage::StoredSession;

pub(super) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

pub(super) fn now_ms() -> f64 {
    chrono::Utc::now().timestamp_millis() as f64
}

#[derive(Default)]
pub(super) struct EventHub {
    sinks: Mutex<Vec<Arc<dyn AssistantEventSink>>>,
}

impl EventHub {
    pub fn clear(&self) {
        lock(&self.sinks).clear();
    }

    pub fn subscribe(&self, sink: Arc<dyn AssistantEventSink>) {
        let mut sinks = lock(&self.sinks);
        if sinks.len() >= 4 {
            sinks.remove(0);
        }
        sinks.push(sink);
    }

    pub fn send(&self, event: &AssistantEvent) {
        lock(&self.sinks).retain(|sink| sink.send(event));
    }
}

pub(super) struct LiveSession {
    pub process: Arc<AcpProcess>,
    pub remote_id: String,
    pub epoch: u64,
}

enum PendingInteraction {
    Permission {
        epoch: u64,
        options: Vec<String>,
        response: oneshot::Sender<Option<String>>,
    },
    Question {
        epoch: u64,
        response: oneshot::Sender<AssistantQuestionAnswer>,
    },
}

pub(super) struct SessionData {
    pub stored: StoredSession,
    pub active_message: Option<String>,
    pub raw_messages: HashMap<String, String>,
}

pub(super) struct Session {
    pub data: Mutex<SessionData>,
    pub live: AsyncMutex<Option<LiveSession>>,
    pub builtin_live: AsyncMutex<Option<super::builtin_connection::BuiltinLive>>,
    pub turn: AsyncMutex<()>,
    pub connecting: AsyncMutex<()>,
    pub persisting: AsyncMutex<()>,
    pub deleted: AtomicBool,
    pub running: AtomicBool,
    pub stopping: AtomicBool,
    pub accepting_updates: AtomicBool,
    pub epoch: AtomicU64,
    pub turn_generation: AtomicU64,
    pub cancelled: Mutex<CancellationToken>,
    pub host: Arc<dyn AssistantToolHost>,
    hub: Arc<EventHub>,
    publishing: Mutex<()>,
    sequence: AtomicU32,
    pending: Mutex<HashMap<String, PendingInteraction>>,
    reverse: Mutex<HashMap<String, CancellationToken>>,
    model_secret: Mutex<Option<String>>,
}

impl Session {
    pub fn new(
        stored: StoredSession,
        host: Arc<dyn AssistantToolHost>,
        hub: Arc<EventHub>,
    ) -> Self {
        let sequence = stored.session.sequence;
        Self {
            data: Mutex::new(SessionData {
                stored,
                active_message: None,
                raw_messages: HashMap::new(),
            }),
            live: AsyncMutex::new(None),
            builtin_live: AsyncMutex::new(None),
            turn: AsyncMutex::new(()),
            connecting: AsyncMutex::new(()),
            persisting: AsyncMutex::new(()),
            deleted: AtomicBool::new(false),
            running: AtomicBool::new(false),
            stopping: AtomicBool::new(false),
            accepting_updates: AtomicBool::new(false),
            epoch: AtomicU64::new(0),
            turn_generation: AtomicU64::new(0),
            cancelled: Mutex::new(CancellationToken::new()),
            host,
            hub,
            publishing: Mutex::new(()),
            sequence: AtomicU32::new(sequence),
            pending: Mutex::new(HashMap::new()),
            reverse: Mutex::new(HashMap::new()),
            model_secret: Mutex::new(None),
        }
    }

    pub fn snapshot(&self) -> AssistantSession {
        let _publishing = lock(&self.publishing);
        let data = lock(&self.data);
        let mut snapshot = data.stored.session.clone();
        snapshot.sequence = self.sequence.load(Ordering::Acquire);
        snapshot
    }
    pub fn stored(&self) -> StoredSession {
        let _publishing = lock(&self.publishing);
        let data = lock(&self.data);
        let mut stored = data.stored.clone();
        stored.session.sequence = self.sequence.load(Ordering::Acquire);
        stored
    }
    pub fn id(&self) -> String {
        lock(&self.data).stored.session.info.id.clone()
    }

    pub fn remember_model_secret(&self, key: &str) {
        *lock(&self.model_secret) = Some(key.to_owned());
    }

    pub fn clear_model_secret(&self) {
        *lock(&self.model_secret) = None;
    }

    pub fn redact(&self, text: &str) -> String {
        let secret = lock(&self.model_secret);
        let text = match secret.as_deref() {
            Some(secret) if !secret.is_empty() => text.replace(secret, "[REDACTED]"),
            _ => text.to_owned(),
        };
        self.host.redact(&text)
    }

    pub fn advance_epoch(&self) -> u64 {
        let _publishing = lock(&self.publishing);
        let _data = lock(&self.data);
        self.epoch.fetch_add(1, Ordering::AcqRel) + 1
    }

    pub fn interaction(self: &Arc<Self>, epoch: u64) -> Arc<dyn AssistantInteraction> {
        Arc::new(SessionInteraction {
            session: Arc::downgrade(self),
            epoch,
        })
    }

    pub fn emit(&self, update: AssistantUpdate) {
        let _publishing = lock(&self.publishing);
        self.emit_locked(update);
    }

    fn emit_locked(&self, update: AssistantUpdate) {
        self.hub.send(&AssistantEvent {
            v: 1,
            session_id: self.id(),
            sequence: self.sequence.fetch_add(1, Ordering::Relaxed) + 1,
            update,
        });
    }

    pub fn state(&self, state: AssistantSessionState, error: Option<String>) {
        self.set_state(None, state, error);
    }

    pub fn state_at(&self, epoch: u64, state: AssistantSessionState, error: Option<String>) {
        self.set_state(Some(epoch), state, error);
    }

    fn set_state(&self, epoch: Option<u64>, state: AssistantSessionState, error: Option<String>) {
        let _publishing = lock(&self.publishing);
        self.set_state_locked(epoch, state, error);
    }

    fn set_state_locked(
        &self,
        epoch: Option<u64>,
        mut state: AssistantSessionState,
        error: Option<String>,
    ) {
        if state == AssistantSessionState::Running && !lock(&self.pending).is_empty() {
            state = AssistantSessionState::WaitingForInput;
        }
        let info = {
            let mut data = lock(&self.data);
            if epoch.is_some_and(|epoch| self.epoch.load(Ordering::Acquire) != epoch) {
                return;
            }
            let info = &mut data.stored.session.info;
            info.state = state;
            info.error = error.map(|error| self.redact(&error));
            info.updated_at_ms = now_ms();
            info.clone()
        };
        self.emit_locked(AssistantUpdate::Session { info });
    }

    pub fn add_message(&self, role: AssistantMessageRole, text: String) {
        let _publishing = lock(&self.publishing);
        let message = AssistantMessage {
            id: uuid::Uuid::new_v4().to_string(),
            role,
            text: self.redact(&text),
            created_at_ms: now_ms(),
        };
        {
            let mut data = lock(&self.data);
            data.stored.session.messages.push(message.clone());
            trim_session(&mut data.stored.session);
        }
        self.emit_locked(AssistantUpdate::Message { message });
    }

    pub fn delta(&self, epoch: u64, remote_id: Option<&str>, text: &str) {
        let _publishing = lock(&self.publishing);
        let message = {
            let mut data = lock(&self.data);
            if self.epoch.load(Ordering::Acquire) != epoch {
                return;
            }
            let id = remote_id
                .map(|id| format!("agent-{id}"))
                .or_else(|| data.active_message.clone())
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
            data.active_message = Some(id.clone());
            let raw = data.raw_messages.entry(id.clone()).or_default();
            let remaining = 32 * 1024usize - raw.len().min(32 * 1024);
            let mut end = text.len().min(remaining);
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            raw.push_str(&text[..end]);
            let clean = self.redact(raw);
            let messages = &mut data.stored.session.messages;
            let message =
                if let Some(message) = messages.iter_mut().find(|message| message.id == id) {
                    message.text = clean;
                    message.clone()
                } else {
                    let message = AssistantMessage {
                        id,
                        role: AssistantMessageRole::Assistant,
                        text: clean,
                        created_at_ms: now_ms(),
                    };
                    messages.push(message.clone());
                    message
                };
            trim_session(&mut data.stored.session);
            let kept = data
                .stored
                .session
                .messages
                .iter()
                .map(|message| message.id.clone())
                .collect::<std::collections::HashSet<_>>();
            data.raw_messages.retain(|id, _| kept.contains(id));
            message
        };
        self.emit_locked(AssistantUpdate::Message { message });
    }

    pub fn tool(&self, epoch: u64, mut tool: AssistantToolCall) {
        let _publishing = lock(&self.publishing);
        tool.id = tool.id.chars().take(256).collect();
        tool.title = self.redact(&tool.title);
        tool.details = self.redact(&tool.details).chars().take(1024).collect();
        {
            let mut data = lock(&self.data);
            if self.epoch.load(Ordering::Acquire) != epoch {
                return;
            }
            data.active_message = None;
            let tools = &mut data.stored.session.tools;
            if let Some(existing) = tools.iter_mut().find(|existing| existing.id == tool.id) {
                if tool.title.is_empty() {
                    tool.title.clone_from(&existing.title);
                }
                if tool.details.is_empty() {
                    tool.details.clone_from(&existing.details);
                }
                tool.created_at_ms = existing.created_at_ms;
                *existing = tool.clone();
            } else {
                tools.push(tool.clone());
            }
            trim_session(&mut data.stored.session);
        }
        self.emit_locked(AssistantUpdate::Tool { tool });
    }

    pub fn revoke(&self) {
        lock(&self.cancelled).cancel();
        for (_, cancel) in lock(&self.reverse).drain() {
            cancel.cancel();
        }
        self.clear_interactions();
    }

    pub fn clear_interactions(&self) {
        let _publishing = lock(&self.publishing);
        lock(&self.pending).clear();
        let ids = {
            let mut data = lock(&self.data);
            let ids = data
                .stored
                .session
                .permissions
                .iter()
                .map(|request| request.id.clone())
                .chain(
                    data.stored
                        .session
                        .questions
                        .iter()
                        .map(|question| question.id.clone()),
                )
                .collect::<Vec<_>>();
            data.stored.session.permissions.clear();
            data.stored.session.questions.clear();
            ids
        };
        for id in ids {
            self.emit_locked(AssistantUpdate::InteractionResolved { id });
        }
    }

    pub fn register_reverse(
        &self,
        epoch: u64,
        id: &serde_json::Value,
    ) -> Option<(String, CancellationToken)> {
        let mut reverse = lock(&self.reverse);
        if reverse.len() >= 8 || self.epoch.load(Ordering::Acquire) != epoch {
            return None;
        }
        let key = format!("{epoch}:{id}");
        if reverse.contains_key(&key) {
            return None;
        }
        let cancel = CancellationToken::new();
        reverse.insert(key.clone(), cancel.clone());
        Some((key, cancel))
    }

    pub fn cancel_reverse(&self, epoch: u64, id: &serde_json::Value) {
        if let Some(cancel) = lock(&self.reverse).remove(&format!("{epoch}:{id}")) {
            cancel.cancel();
        }
    }

    pub fn finish_reverse(&self, key: &str) {
        lock(&self.reverse).remove(key);
    }

    fn interaction_done(&self, id: &str, epoch: u64) {
        let _publishing = lock(&self.publishing);
        lock(&self.pending).remove(id);
        {
            let mut data = lock(&self.data);
            data.stored
                .session
                .permissions
                .retain(|request| request.id != id);
            data.stored
                .session
                .questions
                .retain(|question| question.id != id);
        }
        self.emit_locked(AssistantUpdate::InteractionResolved { id: id.to_owned() });
        if self.epoch.load(Ordering::Acquire) == epoch
            && self.running.load(Ordering::Acquire)
            && !lock(&self.cancelled).is_cancelled()
            && lock(&self.pending).is_empty()
        {
            self.set_state_locked(Some(epoch), AssistantSessionState::Running, None);
        }
    }

    pub fn answer_permission(&self, id: &str, option_id: Option<String>) -> Result<(), String> {
        let mut pending = lock(&self.pending);
        let Some(PendingInteraction::Permission { epoch, options, .. }) = pending.get(id) else {
            return Err("审批已经结束或已取消".into());
        };
        if *epoch != self.epoch.load(Ordering::Acquire) || lock(&self.cancelled).is_cancelled() {
            return Err("审批已经撤销".into());
        }
        if option_id
            .as_ref()
            .is_some_and(|option| !options.contains(option))
        {
            return Err("审批选项无效".into());
        }
        if let Some(PendingInteraction::Permission { response, .. }) = pending.remove(id) {
            response
                .send(option_id)
                .map_err(|_| "审批已经结束".to_owned())?;
        }
        Ok(())
    }

    pub fn answer_question(&self, id: &str, answer: AssistantQuestionAnswer) -> Result<(), String> {
        if !matches!(lock(&self.pending).get(id), Some(PendingInteraction::Question { epoch, .. })
            if *epoch == self.epoch.load(Ordering::Acquire) && !lock(&self.cancelled).is_cancelled())
        {
            return Err("问题已经结束或已取消".into());
        }
        if answer.values.len() > 24 || answer.values.values().any(|value| value.len() > 4096) {
            return Err("回答内容过长".into());
        }
        if let Some(PendingInteraction::Question { response, .. }) = lock(&self.pending).remove(id)
        {
            response
                .send(answer)
                .map_err(|_| "问题已经结束".to_owned())?;
        }
        Ok(())
    }

    pub async fn ask_question(
        &self,
        epoch: u64,
        mut question: AssistantQuestion,
    ) -> AssistantQuestionAnswer {
        let cancelled_answer = AssistantQuestionAnswer {
            cancelled: true,
            ..Default::default()
        };
        if !self.running.load(Ordering::Acquire) {
            return cancelled_answer;
        }
        question.id = uuid::Uuid::new_v4().to_string();
        question.title = self.redact(&question.title);
        for field in &mut question.fields {
            field.title = self.redact(&field.title);
            field.description = field.description.as_deref().map(|text| self.redact(text));
            field.options = field.options.iter().map(|text| self.redact(text)).collect();
        }
        let id = question.id.clone();
        let cancel = lock(&self.cancelled).clone();
        let (sender, receiver) = oneshot::channel();
        {
            let _publishing = lock(&self.publishing);
            let mut data = lock(&self.data);
            let mut pending = lock(&self.pending);
            if pending.len() >= 8
                || cancel.is_cancelled()
                || self.epoch.load(Ordering::Acquire) != epoch
            {
                return cancelled_answer;
            }
            pending.insert(
                id.clone(),
                PendingInteraction::Question {
                    epoch,
                    response: sender,
                },
            );
            data.stored.session.questions.push(question.clone());
            drop(pending);
            drop(data);
            self.set_state_locked(Some(epoch), AssistantSessionState::WaitingForInput, None);
            self.emit_locked(AssistantUpdate::Question { question });
        }
        let pending_guard = InteractionGuard {
            session: self,
            id: id.clone(),
            epoch,
        };
        let answer = tokio::select! {
            result = receiver => result.unwrap_or(cancelled_answer),
            _ = cancel.cancelled() => AssistantQuestionAnswer { cancelled: true, ..Default::default() },
            _ = tokio::time::sleep(Duration::from_secs(300)) => AssistantQuestionAnswer { cancelled: true, ..Default::default() },
        };
        drop(pending_guard);
        if self.epoch.load(Ordering::Acquire) != epoch || cancel.is_cancelled() {
            AssistantQuestionAnswer {
                cancelled: true,
                ..Default::default()
            }
        } else {
            answer
        }
    }
}

impl Session {
    pub async fn request_permission_at(
        &self,
        epoch: u64,
        mut request: AssistantPermissionRequest,
    ) -> Result<Option<String>, String> {
        if !self.running.load(Ordering::Acquire) {
            return Ok(None);
        }
        request.id = uuid::Uuid::new_v4().to_string();
        request.title = self.redact(&request.title);
        request.details = self.redact(&request.details);
        for option in &mut request.options {
            option.label = self.redact(&option.label);
        }
        let id = request.id.clone();
        let cancel = lock(&self.cancelled).clone();
        let (sender, receiver) = oneshot::channel();
        {
            let _publishing = lock(&self.publishing);
            let mut data = lock(&self.data);
            let mut pending = lock(&self.pending);
            if pending.len() >= 8
                || cancel.is_cancelled()
                || self.epoch.load(Ordering::Acquire) != epoch
            {
                return Ok(None);
            }
            pending.insert(
                id.clone(),
                PendingInteraction::Permission {
                    epoch,
                    options: request
                        .options
                        .iter()
                        .map(|option| option.id.clone())
                        .collect(),
                    response: sender,
                },
            );
            data.stored.session.permissions.push(request.clone());
            drop(pending);
            drop(data);
            self.set_state_locked(Some(epoch), AssistantSessionState::WaitingForInput, None);
            self.emit_locked(AssistantUpdate::Permission { request });
        }
        let pending_guard = InteractionGuard {
            session: self,
            id: id.clone(),
            epoch,
        };
        let answer = tokio::select! {
            result = receiver => result.unwrap_or(None),
            _ = cancel.cancelled() => None,
            _ = tokio::time::sleep(Duration::from_secs(180)) => None,
        };
        drop(pending_guard);
        if self.epoch.load(Ordering::Acquire) != epoch || cancel.is_cancelled() {
            Ok(None)
        } else {
            Ok(answer)
        }
    }
}

struct SessionInteraction {
    session: Weak<Session>,
    epoch: u64,
}

struct InteractionGuard<'a> {
    session: &'a Session,
    id: String,
    epoch: u64,
}

impl Drop for InteractionGuard<'_> {
    fn drop(&mut self) {
        self.session.interaction_done(&self.id, self.epoch);
    }
}

#[async_trait]
impl AssistantInteraction for SessionInteraction {
    async fn request_permission(
        &self,
        request: AssistantPermissionRequest,
    ) -> Result<Option<String>, String> {
        let Some(session) = self.session.upgrade() else {
            return Ok(None);
        };
        session.request_permission_at(self.epoch, request).await
    }
}

pub(super) fn trim_session(session: &mut AssistantSession) {
    fn trim(text: &mut String, max: usize) {
        let mut end = text.len().min(max);
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
    }
    for message in &mut session.messages {
        trim(&mut message.text, 32 * 1024);
        trim(&mut message.id, 512);
    }
    for tool in &mut session.tools {
        trim(&mut tool.details, 4096);
        trim(&mut tool.title, 1024);
        trim(&mut tool.id, 256);
        trim(&mut tool.status, 32);
    }
    trim(&mut session.info.title, 256);
    trim(&mut session.info.bot_name, 512);
    if let Some(error) = &mut session.info.error {
        trim(error, 4096);
    }
    if session.messages.len() > 128 {
        session.messages.drain(..session.messages.len() - 128);
    }
    if session.tools.len() > 64 {
        session.tools.drain(..session.tools.len() - 64);
    }
    while session
        .messages
        .iter()
        .map(|message| message.text.len())
        .sum::<usize>()
        > 384 * 1024
        && session.messages.len() > 1
    {
        session.messages.remove(0);
    }
}
