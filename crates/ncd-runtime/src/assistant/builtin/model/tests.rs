use std::sync::Mutex;

use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

use super::*;

fn messages() -> Vec<ChatMessage> {
    vec![ChatMessage::User {
        content: "了解当前环境".into(),
    }]
}

fn client(server: &MockServer, timeout: Duration) -> ModelClient {
    ModelClient::new(
        &server.uri(),
        "fixture-model",
        "TEST_FIXTURE_KEY",
        2048,
        timeout,
    )
    .unwrap()
}

fn response_wire() -> String {
    format!(
        "data: {}\r\n\r\ndata: {}\r\n\r\ndata: [DONE]\r\n\r\n",
        json!({"choices":[{"index":0,"delta":{"content":"本地测试回答"},"finish_reason":null}]}),
        json!({"choices":[{"index":0,"delta":{},"finish_reason":"stop"}]})
    )
}

async fn mounted(status: u16, body: String, mime: &str) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(status).set_body_raw(body, mime))
        .expect(1)
        .mount(&server)
        .await;
    server
}

#[test]
fn endpoint_normalization_preserves_explicit_paths_and_normalizes_origins() {
    for (input, expected) in [
        (
            "https://Example.COM",
            "https://example.com/v1/chat/completions",
        ),
        (
            "https://example.com/v1/",
            "https://example.com/v1/chat/completions",
        ),
        (
            "https://example.com/proxy/v1",
            "https://example.com/proxy/v1/chat/completions",
        ),
        (
            "https://example.com/custom/completions",
            "https://example.com/custom/completions",
        ),
        (
            "http://localhost:1234/",
            "http://localhost:1234/v1/chat/completions",
        ),
        (
            "http://127.0.0.1:1234/v1",
            "http://127.0.0.1:1234/v1/chat/completions",
        ),
        ("http://[::1]:1234", "http://[::1]:1234/v1/chat/completions"),
    ] {
        assert_eq!(endpoint_url(input).unwrap().as_str(), expected);
    }
}

#[test]
fn endpoint_validation_rejects_insecure_or_credential_bearing_urls_without_echoing_them() {
    for input in [
        "",
        "/v1",
        "http://example.com/v1",
        "ftp://127.0.0.1/v1",
        "https://user:TEST_PRIVATE_KEY@example.com/v1",
        "https://@example.com/v1",
        "https://example.com/v1?api_key=TEST_PRIVATE_KEY",
        "https://example.com/v1?",
        "https://example.com/v1#TEST_PRIVATE_KEY",
        "https://example.com/v1#",
        "https://example.com/\nprivate",
        "https://example.com\\private",
        "file:///private/config",
    ] {
        let error = endpoint_url(input).err().unwrap();
        assert_eq!(error, ModelError::InvalidEndpoint);
        assert!(!error.to_string().contains("TEST_PRIVATE"));
        assert!(!error.to_string().contains("example.com"));
    }
}

#[tokio::test]
async fn construction_is_pure_and_does_not_probe_or_list_models() {
    let server = MockServer::start().await;
    let _client = client(&server, Duration::from_secs(5));
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn one_post_streams_visible_text_and_uses_the_manually_selected_model() {
    let server = mounted(200, response_wire(), "text/event-stream; charset=utf-8").await;
    let output = Arc::new(Mutex::new(String::new()));
    let capture = Arc::clone(&output);
    let reply = client(&server, Duration::from_secs(5))
        .complete(
            &messages(),
            &[],
            Arc::new(move |text| capture.lock().unwrap().push_str(text)),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(reply.text, "本地测试回答");
    assert_eq!(output.lock().unwrap().as_str(), "本地测试回答");
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method.as_str(), "POST");
    assert_eq!(
        requests[0]
            .headers
            .get("authorization")
            .unwrap()
            .to_str()
            .unwrap(),
        "Bearer TEST_FIXTURE_KEY"
    );
    let body: Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(body["model"], "fixture-model");
    assert_eq!(body["stream"], true);
    assert_eq!(body["max_tokens"], 2048);
}

#[tokio::test]
async fn assistant_tool_history_serializes_in_openai_format_without_losing_call_ids() {
    let server = mounted(200, response_wire(), "text/event-stream").await;
    let mut messages = messages();
    messages.push(ChatMessage::Assistant {
        content: "<think>TEST_PRIVATE_CONTENT</think>".into(),
        reasoning_content: Some("原始推理\nTEST_PRIVATE_REASONING \\ \"内容\"".into()),
        tool_calls: vec![ToolCall {
            id: "call-1".into(),
            name: "inspect_bot".into(),
            arguments: "{\"bot\":\"test\"}".into(),
        }],
    });
    messages.push(ChatMessage::Tool {
        tool_call_id: "call-1".into(),
        content: "{\"state\":\"stopped\"}".into(),
    });
    let tools = vec![ToolSpec {
        name: "inspect_bot".into(),
        description: "读取目标状态".into(),
        parameters: json!({"type":"object","properties":{"bot":{"type":"string"}}}),
    }];
    client(&server, Duration::from_secs(5))
        .complete(
            &messages,
            &tools,
            Arc::new(|_| {}),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    let requests = server.received_requests().await.unwrap();
    let body: Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(
        body["messages"][1]["tool_calls"][0]["function"]["name"],
        "inspect_bot"
    );
    assert_eq!(
        body["messages"][1]["tool_calls"][0]["function"]["arguments"],
        "{\"bot\":\"test\"}"
    );
    assert_eq!(body["messages"][2]["tool_call_id"], "call-1");
    assert_eq!(
        body["messages"][1]["content"],
        "<think>TEST_PRIVATE_CONTENT</think>"
    );
    assert_eq!(
        body["messages"][1]["reasoning_content"],
        "原始推理\nTEST_PRIVATE_REASONING \\ \"内容\""
    );
    assert_eq!(
        body["tools"][0]["function"]["parameters"],
        tools[0].parameters
    );
}

#[tokio::test]
async fn http_failures_stop_after_one_post_and_never_expose_the_body_url_or_key() {
    for status in [401, 429, 500] {
        let server = mounted(
            status,
            "TEST_PRIVATE_BODY https://private.invalid TEST_FIXTURE_KEY".into(),
            "application/json",
        )
        .await;
        let error = client(&server, Duration::from_secs(5))
            .complete(
                &messages(),
                &[],
                Arc::new(|_| {}),
                &CancellationToken::new(),
            )
            .await
            .err()
            .unwrap();
        assert_eq!(error, ModelError::Http(status));
        assert!(!error.to_string().contains("TEST_"));
        assert!(!error.to_string().contains("private.invalid"));
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
    }
}

#[tokio::test]
async fn redirect_is_returned_as_an_error_without_forwarding_authorization_to_another_origin() {
    let target = MockServer::start().await;
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(307)
                .insert_header("location", format!("{}/stolen", target.uri())),
        )
        .expect(1)
        .mount(&server)
        .await;
    let error = client(&server, Duration::from_secs(5))
        .complete(
            &messages(),
            &[],
            Arc::new(|_| {}),
            &CancellationToken::new(),
        )
        .await
        .err()
        .unwrap();
    assert_eq!(error, ModelError::Http(307));
    assert!(target.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn nonstream_json_fallback_requires_a_complete_response() {
    let body = json!({"choices":[{"index":0,"message":{"role":"assistant","content":"JSON 完整回答"},"finish_reason":"stop"}]}).to_string();
    let server = mounted(200, body, "application/json").await;
    let reply = client(&server, Duration::from_secs(5))
        .complete(
            &messages(),
            &[],
            Arc::new(|_| {}),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(reply.text, "JSON 完整回答");
    assert_eq!(reply.finish_reason, FinishReason::Stop);
}

#[tokio::test]
async fn cancelled_request_drops_the_only_http_attempt() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(response_wire(), "text/event-stream")
                .set_delay(Duration::from_secs(30)),
        )
        .expect(1)
        .mount(&server)
        .await;
    let client = client(&server, Duration::from_secs(60));
    let cancel = CancellationToken::new();
    let token = cancel.clone();
    let task = tokio::spawn(async move {
        client
            .complete(&messages(), &[], Arc::new(|_| {}), &token)
            .await
    });
    tokio::time::timeout(Duration::from_secs(2), async {
        while server.received_requests().await.unwrap().is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    cancel.cancel();
    let result = tokio::time::timeout(Duration::from_secs(1), task)
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(result, Err(ModelError::Cancelled)));
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn timeout_does_not_retry_the_delayed_request() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(response_wire(), "text/event-stream")
                .set_delay(Duration::from_secs(10)),
        )
        .expect(1)
        .mount(&server)
        .await;
    let error = client(&server, Duration::from_secs(1))
        .complete(
            &messages(),
            &[],
            Arc::new(|_| {}),
            &CancellationToken::new(),
        )
        .await
        .err()
        .unwrap();
    assert_eq!(error, ModelError::Timeout);
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn oversized_response_is_rejected_before_unbounded_buffering() {
    let server = mounted(200, "x".repeat(MAX_RESPONSE_BYTES + 1), "application/json").await;
    let error = client(&server, Duration::from_secs(5))
        .complete(
            &messages(),
            &[],
            Arc::new(|_| {}),
            &CancellationToken::new(),
        )
        .await
        .err()
        .unwrap();
    assert_eq!(error, ModelError::ResponseTooLarge);
}

#[tokio::test]
async fn invalid_outgoing_tool_history_is_rejected_without_any_http_request() {
    let server = MockServer::start().await;
    let messages = vec![ChatMessage::Assistant {
        content: String::new(),
        reasoning_content: None,
        tool_calls: vec![ToolCall {
            id: "call".into(),
            name: "read".into(),
            arguments: "{incomplete".into(),
        }],
    }];
    let error = client(&server, Duration::from_secs(5))
        .complete(&messages, &[], Arc::new(|_| {}), &CancellationToken::new())
        .await
        .err()
        .unwrap();
    assert_eq!(error, ModelError::InvalidToolCall);
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn minimax_m31_uses_split_low_reasoning_and_completion_tokens_without_changing_other_models()
{
    for model in [
        "MiniMax-M3.1-Flash",
        "minimax-m3.1-flash",
        "fixture-model",
        "MiniMax-M3.1-Flash-preview",
    ] {
        let client = ModelClient::new(
            "https://example.invalid/v1",
            model,
            "fixture",
            2048,
            Duration::from_secs(5),
        )
        .unwrap();
        let body: Value =
            serde_json::from_slice(&client.request_body(&messages(), &[]).unwrap()).unwrap();
        if model.eq_ignore_ascii_case("minimax-m3.1-flash") {
            assert_eq!(body["reasoning_split"], true);
            assert_eq!(body["reasoning_effort"], "low");
            assert_eq!(body["max_completion_tokens"], 2048);
            assert!(body.get("max_tokens").is_none());
        } else {
            assert_eq!(body["max_tokens"], 2048);
            for field in [
                "reasoning_split",
                "reasoning_effort",
                "max_completion_tokens",
            ] {
                assert!(body.get(field).is_none());
            }
        }
        assert!(body.get("thinking").is_none());
    }
}

#[test]
fn optional_reasoning_is_omitted_for_legacy_history_and_has_an_outgoing_budget() {
    let legacy: ChatMessage =
        serde_json::from_value(json!({"role":"assistant","content":"旧回答","tool_calls":[]}))
            .unwrap();
    assert!(
        wire_message(&legacy)
            .unwrap()
            .get("reasoning_content")
            .is_none()
    );
    let oversized = ChatMessage::Assistant {
        content: "answer".into(),
        reasoning_content: Some("x".repeat(MAX_REASONING_BYTES + 1)),
        tool_calls: Vec::new(),
    };
    assert!(matches!(
        wire_message(&oversized),
        Err(ModelError::RequestTooLarge)
    ));
}

#[tokio::test]
async fn json_reasoning_and_raw_think_content_remain_private_while_visible_answer_is_streamed() {
    let raw = "<think>TEST_PRIVATE_CONTENT</think>公开的中文回答";
    let private = "原始推理 TEST_PRIVATE_REASONING\n\\\"";
    let body = json!({"choices":[{"index":0,"message":{"role":"assistant","content":raw,"reasoning_content":private},"finish_reason":"stop"}]}).to_string();
    let server = mounted(200, body, "application/json").await;
    let visible = Arc::new(Mutex::new(String::new()));
    let capture = visible.clone();
    let reply = client(&server, Duration::from_secs(5))
        .complete(
            &messages(),
            &[],
            Arc::new(move |text| capture.lock().unwrap().push_str(text)),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(reply.text, raw);
    assert_eq!(reply.reasoning_content.as_deref(), Some(private));
    assert_eq!(visible.lock().unwrap().as_str(), "公开的中文回答");
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}
