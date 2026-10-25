use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use ncd_domain::SecretError;
use ncd_traits::SecretStore;
use ncd_traits::assistant::{AssistantContext, AssistantInteraction, AssistantToolConnection};
use serde_json::{Value, json};
use wiremock::{
    Mock, MockServer, Request, ResponseTemplate,
    matchers::{method, path},
};

use super::*;

const MODEL_KEY: &str = "fixture-private-model-value";
const MCP_KEY: &str = "fixture-local-mcp-value";

#[derive(Default)]
struct MemorySecrets(Mutex<HashMap<String, String>>);
impl SecretStore for MemorySecrets {
    fn get(&self, key: &str) -> Result<Option<String>, SecretError> {
        Ok(lock(&self.0).get(key).cloned())
    }
    fn put(&self, key: &str, value: &str) -> Result<(), SecretError> {
        lock(&self.0).insert(key.into(), value.into());
        Ok(())
    }
    fn delete(&self, key: &str) -> Result<(), SecretError> {
        lock(&self.0).remove(key);
        Ok(())
    }
}

struct ToolHost(String);
#[async_trait]
impl AssistantToolHost for ToolHost {
    async fn context(&self, _: &str) -> Result<AssistantContext, String> {
        Ok(AssistantContext {
            bot_name: "只读测试环境".into(),
            summary: "没有真实 Bot；只读本地合成摘要。".into(),
        })
    }
    async fn open(
        &self,
        _: &str,
        _: &str,
        _: Arc<dyn AssistantInteraction>,
        _: CancellationToken,
    ) -> Result<AssistantToolConnection, String> {
        Ok(AssistantToolConnection {
            url: format!("{}/mcp", self.0),
            bearer_token: MCP_KEY.into(),
        })
    }
    async fn close(&self, _: &str) {}
    fn redact(&self, text: &str) -> String {
        text.replace(MCP_KEY, "[REDACTED]")
    }
}

async fn mount_mcp(server: &MockServer, calls: Arc<AtomicUsize>) {
    Mock::given(method("POST")).and(path("/mcp")).respond_with(move |request: &Request| {
        let value: Value = request.body_json().unwrap();
        let result = match value["method"].as_str().unwrap() {
            "initialize" => json!({"protocolVersion":"2025-11-25","capabilities":{"tools":{}},"serverInfo":{"name":"test","version":"1"}}),
            "tools/list" => json!({"tools":[{"name":"get_bot_runtime","description":"读取本次环境摘要","inputSchema":{"type":"object","properties":{},"additionalProperties":false}}]}),
            "tools/call" => {
                calls.fetch_add(1, Ordering::SeqCst);
                let content = json!({"evidence":"X".repeat(4000),"private":MODEL_KEY,"local":MCP_KEY});
                json!({"content":[{"type":"text","text":content.to_string()}],"structuredContent":content,"isError":false})
            }
            _ => Value::Null,
        };
        ResponseTemplate::new(200).set_body_json(json!({"jsonrpc":"2.0","id":value["id"],"result":result}))
    }).mount(server).await;
}

async fn manager(server: &MockServer) -> (tempfile::TempDir, Arc<AssistantManager>) {
    let root = tempfile::tempdir().unwrap();
    let manager = Arc::new(
        AssistantManager::new(root.path().to_owned(), Arc::new(ToolHost(server.uri())))
            .with_secret_store(Arc::new(MemorySecrets::default())),
    );
    let mut settings = manager.settings().await.unwrap();
    settings.builtin.base_url = server.uri();
    settings.builtin.model = "fixture-model".into();
    manager.save_settings(settings).await.unwrap();
    manager.set_api_key(MODEL_KEY.into()).await.unwrap();
    (root, manager)
}

async fn finished(manager: &AssistantManager, id: &str) -> AssistantSession {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let snapshot = manager.get_session(id).await.unwrap();
            let replied = snapshot
                .messages
                .iter()
                .any(|message| message.role == AssistantMessageRole::Assistant);
            if snapshot.info.state == AssistantSessionState::Failed
                || (replied && snapshot.info.state == AssistantSessionState::Ready)
            {
                return snapshot;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("one local model task settles")
}

#[tokio::test]
async fn builtin_manager_discovers_tools_only_on_user_task_and_persists_full_redacted_tool_history()
{
    let server = MockServer::start().await;
    let tools = Arc::new(AtomicUsize::new(0));
    mount_mcp(&server, tools.clone()).await;
    let steps = Arc::new(AtomicUsize::new(0));
    let complete_tool_result = Arc::new(AtomicBool::new(false));
    let body_leaked = Arc::new(AtomicBool::new(false));
    let count = steps.clone();
    let full = complete_tool_result.clone();
    let leak = body_leaked.clone();
    Mock::given(method("POST")).and(path("/v1/chat/completions")).respond_with(move |request: &Request| {
        let value: Value = request.body_json().unwrap();
        if String::from_utf8_lossy(&request.body).contains(MODEL_KEY) || String::from_utf8_lossy(&request.body).contains(MCP_KEY) { leak.store(true, Ordering::SeqCst); }
        let step = count.fetch_add(1, Ordering::SeqCst);
        let choice = if step == 0 {
            json!({"index":0,"message":{"role":"assistant","content":format!("先检查 {MODEL_KEY}"),"tool_calls":[{"id":"call-environment","type":"function","function":{"name":"get_bot_runtime","arguments":"{}"}}]},"finish_reason":"tool_calls"})
        } else {
            full.store(value["messages"].as_array().unwrap().iter().any(|message| message["role"] == "tool" && message["tool_call_id"] == "call-environment" && message["content"].as_str().is_some_and(|content| content.contains(&"X".repeat(4000)))), Ordering::SeqCst);
            json!({"index":0,"message":{"role":"assistant","content":"环境检查完成，工具信息完整。"},"finish_reason":"stop"})
        };
        ResponseTemplate::new(200).set_body_json(json!({"choices":[choice]}))
    }).expect(2).mount(&server).await;
    let (_root, manager) = manager(&server).await;
    let created = manager
        .create_session(AssistantProvider::Builtin, "environment".into())
        .await
        .unwrap();
    assert_eq!(created.info.state, AssistantSessionState::Ready);
    assert!(server.received_requests().await.unwrap().is_empty());
    manager
        .send_message(&created.info.id, format!("了解当前环境 {MODEL_KEY}"))
        .await
        .unwrap();
    let result = finished(&manager, &created.info.id).await;
    assert_eq!(
        result.info.state,
        AssistantSessionState::Ready,
        "{:?}",
        result.info.error
    );
    assert_eq!(steps.load(Ordering::SeqCst), 2);
    assert_eq!(tools.load(Ordering::SeqCst), 1);
    assert!(complete_tool_result.load(Ordering::SeqCst));
    assert!(!body_leaked.load(Ordering::SeqCst));
    let snapshot = serde_json::to_string(&result).unwrap();
    assert!(!snapshot.contains(MODEL_KEY) && !snapshot.contains(MCP_KEY));
    let (archives, notices) = manager.storage.load();
    assert!(notices.is_empty());
    assert_eq!(archives.len(), 1);
    assert!(archives[0].builtin_history.validate().is_ok());
    let saved = serde_json::to_string(&archives[0]).unwrap();
    assert!(saved.contains("call-environment") && saved.contains(&"X".repeat(4000)));
    assert!(!saved.contains(MODEL_KEY) && !saved.contains(MCP_KEY));
    assert!(
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .all(|request| request.method.as_str() == "POST")
    );
    manager.shutdown().await;
}

#[tokio::test]
async fn builtin_manager_stops_after_first_model_error_without_status_reads_triggering_retries() {
    let server = MockServer::start().await;
    mount_mcp(&server, Arc::new(AtomicUsize::new(0))).await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(503).set_body_json(json!({"error":{"message":MODEL_KEY}})),
        )
        .expect(1)
        .mount(&server)
        .await;
    let (_root, manager) = manager(&server).await;
    let created = manager
        .create_session(AssistantProvider::Builtin, "environment".into())
        .await
        .unwrap();
    manager
        .send_message(&created.info.id, "了解当前环境".into())
        .await
        .unwrap();
    let result = finished(&manager, &created.info.id).await;
    assert_eq!(result.info.state, AssistantSessionState::Failed);
    assert!(!result.info.error.unwrap().contains(MODEL_KEY));
    for _ in 0..3 {
        manager.model_status().await.unwrap();
        manager.get_session(&created.info.id).await.unwrap();
    }
    let requests = server.received_requests().await.unwrap();
    assert_eq!(
        requests
            .iter()
            .filter(|request| request.url.path() == "/v1/chat/completions")
            .count(),
        1
    );
    manager.shutdown().await;
}

#[tokio::test]
async fn clearing_model_key_cancels_the_active_round_before_it_can_submit_another_request() {
    let server = MockServer::start().await;
    mount_mcp(&server, Arc::new(AtomicUsize::new(0))).await;
    Mock::given(method("POST")).and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(2)).set_body_json(json!({"choices":[{"index":0,"message":{"role":"assistant","content":"过时回答"},"finish_reason":"stop"}]})))
        .expect(1).mount(&server).await;
    let (_root, manager) = manager(&server).await;
    let created = manager
        .create_session(AssistantProvider::Builtin, "environment".into())
        .await
        .unwrap();
    manager
        .send_message(&created.info.id, "了解当前环境".into())
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if server
                .received_requests()
                .await
                .unwrap()
                .iter()
                .any(|request| request.url.path() == "/v1/chat/completions")
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let status = tokio::time::timeout(Duration::from_secs(2), manager.clear_api_key())
        .await
        .expect("clear cancels immediately")
        .unwrap();
    assert!(!status.has_api_key);
    assert!(!manager.updating_credentials.load(Ordering::Acquire));
    assert_eq!(
        manager
            .get_session(&created.info.id)
            .await
            .unwrap()
            .info
            .state,
        AssistantSessionState::Disconnected
    );
    manager
        .send_message(&created.info.id, "继续了解当前环境".into())
        .await
        .unwrap();
    let result = finished(&manager, &created.info.id).await;
    assert_eq!(result.info.state, AssistantSessionState::Failed);
    assert!(
        !result
            .messages
            .iter()
            .any(|message| message.text.contains("过时回答"))
    );
    assert_eq!(
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|request| request.url.path() == "/v1/chat/completions")
            .count(),
        1
    );
    manager.shutdown().await;
}
