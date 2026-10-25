use std::collections::BTreeMap;
use std::sync::atomic::AtomicUsize;
use std::time::Duration;

use async_trait::async_trait;
use ncd_traits::assistant::{AssistantContext, AssistantInteraction, AssistantToolConnection};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

use super::*;

#[derive(Default)]
struct MockHost {
    contexts: AtomicUsize,
    opens: AtomicUsize,
    closes: AtomicUsize,
}

#[async_trait]
impl AssistantToolHost for MockHost {
    async fn context(&self, _bot_id: &str) -> Result<AssistantContext, String> {
        self.contexts.fetch_add(1, Ordering::AcqRel);
        Ok(AssistantContext {
            bot_name: "测试 Bot".into(),
            summary: "本地测试摘要".into(),
        })
    }

    async fn open(
        &self,
        _session_id: &str,
        _bot_id: &str,
        _interaction: Arc<dyn AssistantInteraction>,
        _cancelled: CancellationToken,
    ) -> Result<AssistantToolConnection, String> {
        self.opens.fetch_add(1, Ordering::AcqRel);
        Err("此测试不允许启动 Agent 或 MCP".into())
    }

    async fn close(&self, _session_id: &str) {
        self.closes.fetch_add(1, Ordering::AcqRel);
    }

    fn redact(&self, text: &str) -> String {
        text.replace("TEST_SECRET", "[redacted]")
    }
}

fn stored_session(state: AssistantSessionState) -> StoredSession {
    StoredSession {
        v: 1,
        remote_session_id: Some("native-session".into()),
        builtin_history: builtin::History::default(),
        session: AssistantSession {
            sequence: 0,
            info: AssistantSessionInfo {
                id: uuid::Uuid::new_v4().to_string(),
                provider: AssistantProvider::Custom,
                bot_id: "test-bot".into(),
                bot_name: "测试 Bot".into(),
                title: "诊断测试".into(),
                state,
                capabilities: AssistantCapabilities::default(),
                created_at_ms: 1.0,
                updated_at_ms: 1.0,
                error: None,
            },
            messages: Vec::new(),
            tools: Vec::new(),
            permissions: Vec::new(),
            questions: Vec::new(),
        },
    }
}

fn session_fixture() -> Arc<Session> {
    Arc::new(Session::new(
        stored_session(AssistantSessionState::Ready),
        Arc::new(MockHost::default()),
        Arc::new(EventHub::default()),
    ))
}

fn manager_fixture() -> (
    Arc<AssistantManager>,
    Arc<MockHost>,
    Arc<Session>,
    tempfile::TempDir,
) {
    let root = tempfile::tempdir().unwrap();
    let host = Arc::new(MockHost::default());
    let manager = Arc::new(AssistantManager::new(root.path().to_owned(), host.clone()));
    manager.loaded.store(true, Ordering::Release);
    *lock(&manager.settings) = AssistantSettings {
        default_provider: Some(AssistantProvider::Custom),
        providers: vec![AssistantProviderConfig {
            provider: AssistantProvider::Custom,
            enabled: false,
            ..Default::default()
        }],
        ..Default::default()
    };
    let session = Arc::new(Session::new(
        stored_session(AssistantSessionState::Disconnected),
        host.clone(),
        manager.hub.clone(),
    ));
    lock(&manager.sessions).insert(session.id(), session.clone());
    (manager, host, session, root)
}

fn begin_round(session: &Session) -> u64 {
    session.revoke();
    *lock(&session.cancelled) = CancellationToken::new();
    session.running.store(true, Ordering::Release);
    session.advance_epoch()
}

fn permission_request() -> AssistantPermissionRequest {
    AssistantPermissionRequest {
        id: String::new(),
        source: AssistantPermissionSource::NcdOperation,
        title: "操作 TEST_SECRET".into(),
        details: "目标 TEST_SECRET".into(),
        created_at_ms: 1.0,
        options: vec![AssistantPermissionOption {
            id: "once".into(),
            label: "仅此一次".into(),
            kind: "allow_once".into(),
        }],
    }
}

fn question_request() -> AssistantQuestion {
    AssistantQuestion {
        id: String::new(),
        title: "需要补充的信息".into(),
        created_at_ms: 1.0,
        fields: vec![AssistantQuestionField {
            name: "detail".into(),
            title: "现象".into(),
            description: None,
            kind: "string".into(),
            required: true,
            options: Vec::new(),
        }],
    }
}

async fn interaction_ids(
    session: &Session,
    permissions: usize,
    questions: usize,
) -> (Vec<String>, Vec<String>) {
    for _ in 0..64 {
        let snapshot = session.snapshot();
        if snapshot.permissions.len() == permissions && snapshot.questions.len() == questions {
            return (
                snapshot
                    .permissions
                    .iter()
                    .map(|item| item.id.clone())
                    .collect(),
                snapshot
                    .questions
                    .iter()
                    .map(|item| item.id.clone())
                    .collect(),
            );
        }
        tokio::task::yield_now().await;
    }
    panic!("交互没有进入预期状态");
}

#[tokio::test]
async fn immediate_send_then_cancel_never_opens_a_provider_or_tool_connection() {
    let (manager, host, session, _root) = manager_fixture();
    manager
        .send_message(&session.id(), "检查一下为什么没响应".into())
        .await
        .unwrap();
    manager.cancel_session(&session.id()).await.unwrap();
    for _ in 0..8 {
        tokio::task::yield_now().await;
    }
    assert_eq!(host.contexts.load(Ordering::Acquire), 0);
    assert_eq!(host.opens.load(Ordering::Acquire), 0);
    assert_eq!(
        session.snapshot().info.state,
        AssistantSessionState::Disconnected
    );
    assert!(session.snapshot().messages.is_empty());
}

#[tokio::test]
async fn immediate_send_then_close_cannot_restart_or_reopen_the_session() {
    let (manager, host, session, _root) = manager_fixture();
    manager
        .send_message(&session.id(), "排查启动问题".into())
        .await
        .unwrap();
    manager.close_session(&session.id()).await.unwrap();
    for _ in 0..8 {
        tokio::task::yield_now().await;
    }
    assert_eq!(host.opens.load(Ordering::Acquire), 0);
    assert_eq!(host.contexts.load(Ordering::Acquire), 0);
    assert_eq!(session.snapshot().info.state, AssistantSessionState::Closed);
    assert!(
        manager
            .send_message(&session.id(), "不能再次发送".into())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn an_old_host_interaction_cannot_admit_permission_into_a_new_epoch() {
    let session = session_fixture();
    let first = begin_round(&session);
    let old = session.interaction(first);
    begin_round(&session);
    assert!(
        old.request_permission(permission_request())
            .await
            .unwrap()
            .is_none()
    );
    assert!(session.snapshot().permissions.is_empty());
}

#[tokio::test]
async fn approved_old_permission_is_revoked_before_its_callback_can_resume() {
    let session = session_fixture();
    let first = begin_round(&session);
    let caller = session.clone();
    let old = tokio::spawn(async move {
        caller
            .request_permission_at(first, permission_request())
            .await
    });
    let (ids, _) = interaction_ids(&session, 1, 0).await;
    session
        .answer_permission(&ids[0], Some("once".into()))
        .unwrap();
    let second = begin_round(&session);
    let caller = session.clone();
    let current = tokio::spawn(async move {
        caller
            .request_permission_at(second, permission_request())
            .await
    });
    let (current_ids, _) = interaction_ids(&session, 1, 0).await;
    assert!(old.await.unwrap().unwrap().is_none());
    assert_eq!(
        session.snapshot().info.state,
        AssistantSessionState::WaitingForInput
    );
    assert!(
        session
            .answer_permission(&ids[0], Some("once".into()))
            .is_err()
    );
    session
        .answer_permission(&current_ids[0], Some("once".into()))
        .unwrap();
    assert_eq!(current.await.unwrap().unwrap().as_deref(), Some("once"));
}

#[tokio::test]
async fn cancellation_clears_permissions_and_questions_and_rejects_late_answers() {
    let session = session_fixture();
    let epoch = begin_round(&session);
    let caller = session.clone();
    let permission = tokio::spawn(async move {
        caller
            .request_permission_at(epoch, permission_request())
            .await
    });
    let caller = session.clone();
    let question =
        tokio::spawn(async move { caller.ask_question(epoch, question_request()).await });
    let (permissions, questions) = interaction_ids(&session, 1, 1).await;
    session.revoke();
    assert!(permission.await.unwrap().unwrap().is_none());
    assert!(question.await.unwrap().cancelled);
    interaction_ids(&session, 0, 0).await;
    assert!(
        session
            .answer_permission(&permissions[0], Some("once".into()))
            .is_err()
    );
    assert!(
        session
            .answer_question(&questions[0], AssistantQuestionAnswer::default())
            .is_err()
    );
}

#[tokio::test]
async fn aborting_an_interaction_future_removes_the_desktop_request() {
    let session = session_fixture();
    let epoch = begin_round(&session);
    let caller = session.clone();
    let task = tokio::spawn(async move {
        caller
            .request_permission_at(epoch, permission_request())
            .await
    });
    let (ids, _) = interaction_ids(&session, 1, 0).await;
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert!(session.snapshot().permissions.is_empty());
    assert!(
        session
            .answer_permission(&ids[0], Some("once".into()))
            .is_err()
    );
    let caller = session.clone();
    let task = tokio::spawn(async move { caller.ask_question(epoch, question_request()).await });
    let (_, ids) = interaction_ids(&session, 0, 1).await;
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert!(session.snapshot().questions.is_empty());
    assert!(
        session
            .answer_question(&ids[0], AssistantQuestionAnswer::default())
            .is_err()
    );
}

#[tokio::test(start_paused = true)]
async fn permission_and_question_timeouts_remove_their_visible_pending_state() {
    let session = session_fixture();
    let epoch = begin_round(&session);
    let caller = session.clone();
    let permission = tokio::spawn(async move {
        caller
            .request_permission_at(epoch, permission_request())
            .await
    });
    let caller = session.clone();
    let question =
        tokio::spawn(async move { caller.ask_question(epoch, question_request()).await });
    let (permissions, questions) = interaction_ids(&session, 1, 1).await;
    tokio::time::advance(Duration::from_secs(181)).await;
    assert!(permission.await.unwrap().unwrap().is_none());
    assert!(session.snapshot().permissions.is_empty());
    assert_eq!(
        session.snapshot().info.state,
        AssistantSessionState::WaitingForInput
    );
    tokio::time::advance(Duration::from_secs(120)).await;
    assert!(question.await.unwrap().cancelled);
    interaction_ids(&session, 0, 0).await;
    assert!(
        session
            .answer_permission(&permissions[0], Some("once".into()))
            .is_err()
    );
    assert!(
        session
            .answer_question(&questions[0], AssistantQuestionAnswer::default())
            .is_err()
    );
}

#[tokio::test]
async fn invalid_permission_options_and_oversized_answers_leave_the_request_pending() {
    let session = session_fixture();
    let epoch = begin_round(&session);
    let caller = session.clone();
    let task = tokio::spawn(async move {
        caller
            .request_permission_at(epoch, permission_request())
            .await
    });
    let (ids, _) = interaction_ids(&session, 1, 0).await;
    let snapshot = session.snapshot();
    assert!(!snapshot.permissions[0].title.contains("TEST_SECRET"));
    assert!(!snapshot.permissions[0].details.contains("TEST_SECRET"));
    assert!(
        session
            .answer_permission(&ids[0], Some("unknown-option".into()))
            .is_err()
    );
    session
        .answer_permission(&ids[0], Some("once".into()))
        .unwrap();
    assert_eq!(task.await.unwrap().unwrap().as_deref(), Some("once"));
    let caller = session.clone();
    let task = tokio::spawn(async move { caller.ask_question(epoch, question_request()).await });
    let (_, ids) = interaction_ids(&session, 0, 1).await;
    assert!(
        session
            .answer_question(
                &ids[0],
                AssistantQuestionAnswer {
                    cancelled: false,
                    values: BTreeMap::from([("detail".into(), "x".repeat(4097))]),
                }
            )
            .is_err()
    );
    assert_eq!(session.snapshot().questions.len(), 1);
    session
        .answer_question(
            &ids[0],
            AssistantQuestionAnswer {
                cancelled: true,
                ..Default::default()
            },
        )
        .unwrap();
    assert!(task.await.unwrap().cancelled);
}

#[test]
fn streamed_secrets_are_redacted_after_reassembly_and_respect_utf8_boundaries() {
    let session = session_fixture();
    let epoch = begin_round(&session);
    session.delta(epoch, Some("split-secret"), "TEST_");
    session.delta(epoch, Some("split-secret"), "SECRET");
    assert_eq!(session.snapshot().messages[0].text, "[redacted]");
    session.delta(epoch, Some("long-unicode"), &"猫".repeat(20_000));
    let data = lock(&session.data);
    let raw = &data.raw_messages["agent-long-unicode"];
    assert!(raw.len() <= 32 * 1024);
    assert_eq!(raw.len() % "猫".len(), 0);
}

#[test]
fn message_history_raw_buffers_and_tool_details_remain_bounded() {
    let session = session_fixture();
    let epoch = begin_round(&session);
    for index in 0..180 {
        session.delta(epoch, Some(&format!("short-{index}")), "small");
    }
    assert!(session.snapshot().messages.len() <= 128);
    for index in 0..80 {
        session.delta(epoch, Some(&format!("large-{index}")), &"字".repeat(5000));
        session.tool(
            epoch,
            AssistantToolCall {
                id: format!("tool-{index}"),
                title: "诊断 TEST_SECRET".into(),
                status: "completed".into(),
                details: "字".repeat(5000),
                created_at_ms: 1.0,
            },
        );
    }
    let data = lock(&session.data);
    let messages = &data.stored.session.messages;
    assert!(
        messages
            .iter()
            .map(|message| message.text.len())
            .sum::<usize>()
            <= 384 * 1024
    );
    assert!(data.raw_messages.values().map(String::len).sum::<usize>() <= 384 * 1024);
    assert!(
        data.raw_messages
            .keys()
            .all(|id| messages.iter().any(|message| &message.id == id))
    );
    assert!(data.stored.session.tools.len() <= 64);
    assert!(data.stored.session.tools.iter().all(|tool| tool.details.chars().count() <= 1024 && !tool.title.contains("TEST_SECRET")));
}

#[test]
fn previous_epoch_streams_cannot_change_current_messages_or_tools() {
    let session = session_fixture();
    let old_epoch = begin_round(&session);
    begin_round(&session);
    session.delta(old_epoch, Some("stale"), "不能进入新轮次");
    session.tool(
        old_epoch,
        AssistantToolCall {
            id: "stale-tool".into(),
            title: "旧工具".into(),
            status: "completed".into(),
            details: String::new(),
            created_at_ms: 1.0,
        },
    );
    assert!(session.snapshot().messages.is_empty());
    assert!(session.snapshot().tools.is_empty());
}

fn archive_path(root: &std::path::Path, id: &str) -> PathBuf {
    root.join("state/assistant/sessions")
        .join(format!("{id}.json"))
}

#[test]
fn reverse_cancellation_uses_standard_request_id_and_preserves_legacy_fallback() {
    let session = session_fixture();
    let epoch = begin_round(&session);
    let (_, standard) = session.register_reverse(epoch, &json!(17)).unwrap();
    let (_, legacy) = session.register_reverse(epoch, &json!("legacy")).unwrap();
    let (_, null_id) = session.register_reverse(epoch, &Value::Null).unwrap();

    events::cancel_request(&session, epoch, &json!({"requestId":17, "id":"legacy"}));
    assert!(standard.is_cancelled());
    assert!(!legacy.is_cancelled());
    events::cancel_request(&session, epoch, &json!({"id":"legacy"}));
    assert!(legacy.is_cancelled());
    events::cancel_request(&session, epoch, &json!({}));
    assert!(!null_id.is_cancelled());
    events::cancel_request(&session, epoch, &json!({"requestId":null}));
    assert!(null_id.is_cancelled());
}

#[test]
fn history_context_keeps_newest_whole_messages_in_order_within_the_encoded_budget() {
    let messages = (0..15)
        .map(|index| AssistantMessage {
            id: index.to_string(),
            role: AssistantMessageRole::Assistant,
            text: format!("{index}:{}", "x".repeat(16 * 1024)),
            created_at_ms: 1.0,
        })
        .collect::<Vec<_>>();
    let history = connection::history_context(&messages).unwrap();
    assert!(history.len() <= 64 * 1024);
    let selected: Vec<Value> = serde_json::from_str(&history).unwrap();
    assert_eq!(selected.len(), 3);
    for (selected, original) in selected.iter().zip(&messages[12..]) {
        assert_eq!(selected["text"], original.text);
        assert_eq!(selected["role"], "assistant");
        assert!(selected.get("id").is_none());
    }
    let short = (0..15)
        .map(|index| AssistantMessage {
            id: index.to_string(),
            role: AssistantMessageRole::User,
            text: index.to_string(),
            created_at_ms: 1.0,
        })
        .collect::<Vec<_>>();
    let selected: Vec<Value> =
        serde_json::from_str(&connection::history_context(&short).unwrap()).unwrap();
    assert_eq!(selected.len(), 12);
    assert_eq!(selected.first().unwrap()["text"], "3");
    assert_eq!(selected.last().unwrap()["text"], "14");
}

#[test]
fn history_budget_prevents_quotes_and_backslashes_from_overflowing_the_outer_acp_frame() {
    let mut messages = (0..12)
        .map(|index| AssistantMessage {
            id: index.to_string(),
            role: AssistantMessageRole::Assistant,
            text: "\"\\".repeat(16 * 1024),
            created_at_ms: 1.0,
        })
        .collect::<Vec<_>>();
    messages.last_mut().unwrap().text = "最近的完整回答：\"引用\" 和 \\ 路径".into();
    let unbounded = serde_json::to_string(
        &messages
            .iter()
            .map(|message| json!({"role":message.role, "text":message.text}))
            .collect::<Vec<_>>(),
    )
    .unwrap();
    let frame = |history: &str| {
        serde_json::to_vec(&json!({
            "jsonrpc":"2.0", "id":1, "method":"session/prompt",
            "params":{"sessionId":"test", "prompt":[{"type":"text", "text":history}]},
        }))
        .unwrap()
    };
    assert!(frame(&unbounded).len() > 1024 * 1024);

    let bounded = connection::history_context(&messages).unwrap();
    assert!(bounded.len() <= 64 * 1024);
    assert!(frame(&bounded).len() <= 128 * 1024 + 256);
    let selected: Vec<Value> = serde_json::from_str(&bounded).unwrap();
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0]["text"], messages.last().unwrap().text);
    assert_eq!(connection::history_context(&messages[..11]).unwrap(), "[]");
}

#[tokio::test]
async fn manager_persistence_omits_pending_permissions_and_questions() {
    let (manager, _host, session, root) = manager_fixture();
    {
        let mut data = lock(&session.data);
        data.stored.session.permissions.push(permission_request());
        data.stored.session.questions.push(question_request());
    }
    manager.persist(&session).await.unwrap();
    let persisted: StoredSession =
        serde_json::from_slice(&std::fs::read(archive_path(root.path(), &session.id())).unwrap())
            .unwrap();
    assert!(persisted.session.permissions.is_empty());
    assert!(persisted.session.questions.is_empty());
    assert_eq!(session.snapshot().permissions.len(), 1);
    assert_eq!(session.snapshot().questions.len(), 1);
}

#[test]
fn loading_archives_revokes_pending_interactions_and_normalizes_live_states() {
    let root = tempfile::tempdir().unwrap();
    let storage = AssistantStorage::new(root.path());
    let mut active = stored_session(AssistantSessionState::WaitingForInput);
    active.session.permissions.push(permission_request());
    active.session.questions.push(question_request());
    storage.save(&active).unwrap();
    let closed = stored_session(AssistantSessionState::Closed);
    storage.save(&closed).unwrap();
    let original = std::fs::read(archive_path(root.path(), &active.session.info.id)).unwrap();
    let (loaded, notices) = storage.load();
    assert!(notices.is_empty());
    let restored = loaded
        .iter()
        .find(|item| item.session.info.id == active.session.info.id)
        .unwrap();
    assert_eq!(
        restored.session.info.state,
        AssistantSessionState::Disconnected
    );
    assert!(restored.session.permissions.is_empty());
    assert!(restored.session.questions.is_empty());
    assert_eq!(
        loaded
            .iter()
            .find(|item| item.session.info.id == closed.session.info.id)
            .unwrap()
            .session
            .info
            .state,
        AssistantSessionState::Closed
    );
    assert_eq!(
        std::fs::read(archive_path(root.path(), &active.session.info.id)).unwrap(),
        original
    );
}

#[test]
fn malformed_and_mismatched_archives_are_reported_without_overwriting_them() {
    let root = tempfile::tempdir().unwrap();
    let storage = AssistantStorage::new(root.path());
    let valid = stored_session(AssistantSessionState::Ready);
    storage.save(&valid).unwrap();
    let bad_id = uuid::Uuid::new_v4().to_string();
    let bad_path = archive_path(root.path(), &bad_id);
    std::fs::write(&bad_path, b"{broken-json").unwrap();
    let mismatched_id = uuid::Uuid::new_v4().to_string();
    let mismatched_path = archive_path(root.path(), &mismatched_id);
    let mismatched = serde_json::to_vec(&stored_session(AssistantSessionState::Running)).unwrap();
    std::fs::write(&mismatched_path, &mismatched).unwrap();
    let (loaded, notices) = storage.load();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].session.info.id, valid.session.info.id);
    assert_eq!(notices.len(), 2);
    assert_eq!(
        std::fs::read(&bad_path).unwrap().as_slice(),
        b"{broken-json"
    );
    assert_eq!(std::fs::read(&mismatched_path).unwrap(), mismatched);
}

#[test]
fn oversize_save_preserves_the_previous_archive() {
    let root = tempfile::tempdir().unwrap();
    let storage = AssistantStorage::new(root.path());
    let mut stored = stored_session(AssistantSessionState::Ready);
    storage.save(&stored).unwrap();
    let path = archive_path(root.path(), &stored.session.info.id);
    let previous = std::fs::read(&path).unwrap();
    stored.session.messages.push(AssistantMessage {
        id: "too-large".into(),
        role: AssistantMessageRole::User,
        text: "x".repeat(1024 * 1024),
        created_at_ms: 1.0,
    });
    assert!(storage.save(&stored).is_err());
    assert_eq!(std::fs::read(path).unwrap(), previous);
}

#[test]
fn only_canonical_uuid_ids_can_reach_archive_and_workspace_paths() {
    let root = tempfile::tempdir().unwrap();
    let storage = AssistantStorage::new(root.path());
    let id = uuid::Uuid::new_v4().to_string();
    assert!(storage.workspace(&id).unwrap().starts_with(root.path()));
    for invalid in [
        "../outside",
        "..\\outside",
        "/absolute",
        "",
        "{00000000-0000-0000-0000-000000000000}",
        "00000000000000000000000000000000",
    ] {
        assert!(storage.workspace(invalid).is_err());
        assert!(storage.delete(invalid).is_err());
    }
    assert!(
        storage
            .workspace("AAAAAAAA-AAAA-4AAA-8AAA-AAAAAAAAAAAA")
            .is_err()
    );
}

#[tokio::test]
async fn malformed_settings_are_reported_and_left_in_place_when_loading() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("config/assistant.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"{bad-settings").unwrap();
    let manager = AssistantManager::new(root.path().to_owned(), Arc::new(MockHost::default()));
    assert_eq!(
        serde_json::to_value(manager.settings().await.unwrap()).unwrap(),
        serde_json::to_value(AssistantSettings::default()).unwrap()
    );
    assert!(!manager.list_sessions().await.unwrap().notices.is_empty());
    assert_eq!(std::fs::read(path).unwrap().as_slice(), b"{bad-settings");
}

#[test]
fn oversized_archive_reads_are_reported_without_rewriting_the_source() {
    let root = tempfile::tempdir().unwrap();
    let storage = AssistantStorage::new(root.path());
    let stored = stored_session(AssistantSessionState::Ready);
    storage.save(&stored).unwrap();
    let path = archive_path(root.path(), &stored.session.info.id);
    let oversized = vec![b'x'; 1024 * 1024 + 1];
    std::fs::write(&path, &oversized).unwrap();
    let (sessions, notices) = storage.load();
    assert!(sessions.is_empty());
    assert_eq!(notices.len(), 1);
    assert_eq!(std::fs::read(path).unwrap(), oversized);
}

#[tokio::test]
async fn restored_archives_apply_per_item_memory_limits_before_becoming_visible() {
    let root = tempfile::tempdir().unwrap();
    let storage = AssistantStorage::new(root.path());
    let mut stored = stored_session(AssistantSessionState::Running);
    stored.session.messages.push(AssistantMessage {
        id: "long".into(),
        role: AssistantMessageRole::Assistant,
        text: "猫".repeat(20_000),
        created_at_ms: 1.0,
    });
    stored.session.tools.push(AssistantToolCall {
        id: "tool".into(),
        title: "猫".repeat(2000),
        details: "猫".repeat(5000),
        status: "completed".into(),
        created_at_ms: 1.0,
    });
    storage.save(&stored).unwrap();
    let manager = AssistantManager::new(root.path().to_owned(), Arc::new(MockHost::default()));
    let restored = manager.get_session(&stored.session.info.id).await.unwrap();
    assert!(restored.messages[0].text.len() <= 32 * 1024);
    assert!(restored.tools[0].title.len() <= 1024);
    assert!(restored.tools[0].details.len() <= 4096);
    assert_eq!(restored.info.state, AssistantSessionState::Disconnected);
}

fn answer(values: &[(&str, &str)]) -> AssistantQuestionAnswer {
    AssistantQuestionAnswer {
        cancelled: false,
        values: values
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect(),
    }
}

#[test]
fn form_answers_enforce_required_keys_enums_numeric_ranges_and_boolean_types() {
    let schema = json!({"type":"object", "required":["channel", "count", "enabled"], "properties":{
        "channel":{"type":"string", "enum":["logs", "status"]},
        "count":{"type":"integer", "minimum":1, "maximum":3}, "enabled":{"type":"boolean"}
    }});
    let valid = answer(&[("channel", "logs"), ("count", "2"), ("enabled", "false")]);
    let content = events::answer_content(&schema, &valid).unwrap();
    assert_eq!(content["count"], 2);
    assert_eq!(content["enabled"], false);
    let invalid = [
        answer(&[("channel", "other"), ("count", "2"), ("enabled", "false")]),
        answer(&[("channel", "logs"), ("count", "0"), ("enabled", "false")]),
        answer(&[("channel", "logs"), ("count", "4"), ("enabled", "false")]),
        answer(&[("channel", "logs"), ("count", "1.5"), ("enabled", "false")]),
        answer(&[("channel", "logs"), ("count", "2"), ("enabled", "yes")]),
        answer(&[("channel", "logs"), ("count", "2")]),
        answer(&[
            ("channel", "logs"),
            ("count", "2"),
            ("enabled", "false"),
            ("unexpected", "x"),
        ]),
    ];
    for invalid in invalid {
        assert!(events::answer_content(&schema, &invalid).is_none());
    }
}

#[test]
fn form_answer_lengths_count_unicode_characters_and_reject_nonfinite_numbers() {
    let schema = json!({"type":"object", "required":["text"], "properties":{
        "text":{"type":"string", "minLength":2, "maxLength":4}, "ratio":{"type":"number"}
    }});
    assert!(events::answer_content(&schema, &answer(&[("text", "中文🧪")])).is_some());
    for text in ["字", "一二三四五"] {
        assert!(events::answer_content(&schema, &answer(&[("text", text)])).is_none());
    }
    for number in ["NaN", "inf", "-inf", "1e999"] {
        assert!(
            events::answer_content(&schema, &answer(&[("text", "有效"), ("ratio", number)]))
                .is_none()
        );
    }
}

#[test]
fn unsupported_or_oversized_form_schemas_are_rejected() {
    assert!(events::question_fields(&json!({"type":"array"})).is_none());
    assert!(
        events::question_fields(
            &json!({"type":"object", "properties":{"nested":{"type":"object"}}})
        )
        .is_none()
    );
    let fields = (0..25)
        .map(|index| (format!("field-{index}"), json!({"type":"string"})))
        .collect::<serde_json::Map<String, Value>>();
    assert!(events::question_fields(&json!({"type":"object", "properties":fields})).is_none());
}

#[tokio::test(start_paused = true)]
async fn shutdown_cancels_a_creation_waiting_for_context_before_taking_admission() {
    struct BlockingContextHost {
        entered: tokio::sync::Notify,
        opens: AtomicUsize,
    }
    #[async_trait]
    impl AssistantToolHost for BlockingContextHost {
        async fn context(&self, _: &str) -> Result<AssistantContext, String> {
            self.entered.notify_one();
            std::future::pending().await
        }
        async fn open(
            &self,
            _: &str,
            _: &str,
            _: Arc<dyn AssistantInteraction>,
            _: CancellationToken,
        ) -> Result<AssistantToolConnection, String> {
            self.opens.fetch_add(1, Ordering::AcqRel);
            Err("测试不能启动 Agent".into())
        }
        async fn close(&self, _: &str) {}
        fn redact(&self, text: &str) -> String {
            text.to_owned()
        }
    }
    let root = tempfile::tempdir().unwrap();
    let host = Arc::new(BlockingContextHost {
        entered: tokio::sync::Notify::new(),
        opens: AtomicUsize::new(0),
    });
    let manager = Arc::new(AssistantManager::new(root.path().to_owned(), host.clone()));
    manager.loaded.store(true, Ordering::Release);
    let caller = manager.clone();
    let creating = tokio::spawn(async move {
        caller
            .create_session(AssistantProvider::Custom, "test-bot".into())
            .await
    });
    tokio::time::timeout(Duration::from_secs(1), host.entered.notified())
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(1), manager.shutdown())
        .await
        .unwrap();
    assert!(creating.await.unwrap().is_err());
    assert!(lock(&manager.sessions).is_empty());
    assert_eq!(host.opens.load(Ordering::Acquire), 0);
    assert!(
        manager
            .create_session(AssistantProvider::Custom, "test-bot".into())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn suspending_for_migration_does_not_permanently_disable_new_sessions() {
    let (manager, host, _, _root) = manager_fixture();
    manager.suspend_all().await;
    assert!(!manager.shutdown_cancel.is_cancelled());
    assert!(!manager.shutting_down.load(Ordering::Acquire));
    let created = manager
        .create_session(AssistantProvider::Custom, "test-bot".into())
        .await
        .unwrap();
    assert_eq!(created.info.state, AssistantSessionState::Failed);
    assert_eq!(host.contexts.load(Ordering::Acquire), 1);
    assert_eq!(host.opens.load(Ordering::Acquire), 0);
}

#[tokio::test]
async fn deleting_a_cold_archived_session_loads_it_before_lookup() {
    let root = tempfile::tempdir().unwrap();
    let storage = AssistantStorage::new(root.path());
    let stored = stored_session(AssistantSessionState::Ready);
    let id = stored.session.info.id.clone();
    storage.save(&stored).unwrap();
    let host = Arc::new(MockHost::default());
    let manager = AssistantManager::new(root.path().to_owned(), host.clone());
    manager.delete_session(&id).await.unwrap();
    assert!(!archive_path(root.path(), &id).exists());
    assert!(manager.list_sessions().await.unwrap().sessions.is_empty());
    assert_eq!(host.opens.load(Ordering::Acquire), 0);
}

#[tokio::test]
async fn archive_deletion_waits_for_an_admitted_write_and_blocks_later_persistence() {
    let (manager, _, session, root) = manager_fixture();
    let path = archive_path(root.path(), &session.id());
    let old_writer = session.persisting.lock().await;
    let deleting = manager.delete_archive(&session);
    tokio::pin!(deleting);
    std::future::poll_fn(|cx| {
        assert!(std::future::Future::poll(deleting.as_mut(), cx).is_pending());
        std::task::Poll::Ready(())
    })
    .await;
    assert!(!session.deleted.load(Ordering::Acquire));
    manager.storage.save(&session.stored()).unwrap();
    assert!(path.exists());
    drop(old_writer);
    deleting.await.unwrap();
    assert!(session.deleted.load(Ordering::Acquire));
    assert!(!path.exists());
    manager.persist(&session).await.unwrap();
    assert!(!path.exists());
}

#[tokio::test]
async fn failed_archive_deletion_restores_persistence_and_keeps_the_session_accessible() {
    let (manager, _, session, root) = manager_fixture();
    let path = archive_path(root.path(), &session.id());
    std::fs::create_dir_all(&path).unwrap();
    assert!(manager.delete_archive(&session).await.is_err());
    assert!(!session.deleted.load(Ordering::Acquire));
    assert!(manager.get_session(&session.id()).await.is_ok());
    std::fs::remove_dir(&path).unwrap();
    manager.persist(&session).await.unwrap();
    assert!(path.is_file());
}

#[test]
fn archive_loading_ignores_backup_and_junk_names_before_applying_its_candidate_limit() {
    let root = tempfile::tempdir().unwrap();
    let directory = root.path().join("state/assistant/sessions");
    std::fs::create_dir_all(&directory).unwrap();
    for index in 0..60 {
        std::fs::write(directory.join(format!("000-junk-{index}.txt")), b"ignored").unwrap();
        std::fs::write(
            directory.join(format!(
                "00000000-0000-4000-8000-000000000000.json.bak{index}"
            )),
            b"ignored",
        )
        .unwrap();
    }
    let expected = [
        "eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee",
        "ffffffff-ffff-4fff-8fff-ffffffffffff",
    ];
    for id in expected {
        let mut stored = stored_session(AssistantSessionState::Ready);
        stored.session.info.id = id.into();
        std::fs::write(
            archive_path(root.path(), id),
            serde_json::to_vec(&stored).unwrap(),
        )
        .unwrap();
    }
    let (sessions, notices) = AssistantStorage::new(root.path()).load();
    let actual = sessions
        .iter()
        .map(|stored| stored.session.info.id.as_str())
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(actual, expected.into_iter().collect());
    assert!(notices.is_empty());
}

#[test]
fn archive_loading_caps_canonical_candidates_and_reports_excess_archives() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("state/assistant/sessions")).unwrap();
    for index in 1..=storage::SESSION_LIMIT + 1 {
        let mut stored = stored_session(AssistantSessionState::Ready);
        stored.session.info.id = uuid::Uuid::from_u128(index as u128).to_string();
        std::fs::write(
            archive_path(root.path(), &stored.session.info.id),
            serde_json::to_vec(&stored).unwrap(),
        )
        .unwrap();
    }
    let (sessions, notices) = AssistantStorage::new(root.path()).load();
    assert_eq!(sessions.len(), storage::SESSION_LIMIT);
    assert_eq!(notices.len(), 1);
    assert!(notices[0].contains("超过50个"));
}
