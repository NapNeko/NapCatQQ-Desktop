use std::sync::Mutex;

use ncd_domain::assistant::AssistantBuiltinSettings;
use serde_json::Value;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

use super::super::{ProtocolContent, ToolCall};
use super::*;
use genai::chat::ChatStreamGuard;
use genai::chat::{ContentPart, MessageContent, ToolCall as SdkToolCall};

fn profile(base: String, format: AssistantApiFormat) -> AssistantBuiltinSettings {
    AssistantBuiltinSettings {
        base_url: base,
        api_format: format,
        model: "fixture-model".into(),
        ..Default::default()
    }
}

fn sse(event: Option<&str>, data: Value) -> String {
    format!(
        "{}data: {data}\n\n",
        event
            .map(|event| format!("event: {event}\n"))
            .unwrap_or_default()
    )
}

fn wire(format: AssistantApiFormat, tool: bool) -> String {
    match format {
        AssistantApiFormat::OpenAiChatCompletions => {
            let delta = if tool {
                json!({"tool_calls":[{"index":0,"id":"call-1","type":"function","function":{"name":"inspect_bot","arguments":"{}"}}],"reasoning_content":"PRIVATE_REASONING"})
            } else {
                json!({"content":"已检查"})
            };
            format!(
                ": keep-alive\n\n{}{}data: [DONE]\n\n",
                sse(
                    None,
                    json!({"choices":[{"index":0,"delta":delta,"finish_reason":null}]})
                ),
                sse(
                    None,
                    json!({"choices":[{"index":0,"delta":{},"finish_reason":if tool {"tool_calls"} else {"stop"}}]})
                )
            )
        }
        AssistantApiFormat::OpenAiResponses => {
            let item = json!({"type":"function_call","id":"fc-1","call_id":"call-1","name":"inspect_bot","arguments":"{}","status":"completed"});
            let mut frames = if tool {
                sse(
                    None,
                    json!({"type":"response.output_item.done","output_index":1,"item":{"type":"reasoning","id":"rs-1","encrypted_content":"PRIVATE_SIGNATURE"}}),
                ) + &sse(
                    None,
                    json!({"type":"response.output_item.added","output_index":0,"item":{ "type":"function_call","id":"fc-1","call_id":"call-1","name":"inspect_bot","arguments":"" }}),
                ) + &sse(
                    None,
                    json!({"type":"response.function_call_arguments.delta","output_index":0,"delta":"{}"}),
                ) + &sse(
                    None,
                    json!({"type":"response.output_item.done","output_index":0,"item":item}),
                )
            } else {
                sse(
                    None,
                    json!({"type":"response.output_text.delta","output_index":0,"content_index":0,"delta":"已检查"}),
                )
            };
            frames += &sse(
                None,
                json!({"type":"response.completed","response":{"id":"resp-1","status":"completed","model":"fixture-model","output":if tool {vec![item]} else {vec![]}}}),
            );
            frames
        }
        AssistantApiFormat::AnthropicMessages => {
            let mut frames = sse(
                Some("message_start"),
                json!({"type":"message_start","message":{"id":"msg-1","type":"message","role":"assistant","content":[],"model":"fixture-model","usage":{"input_tokens":1,"output_tokens":1}}}),
            );
            if tool {
                frames += &sse(
                    Some("content_block_start"),
                    json!({"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":"PRIVATE_REASONING","signature":"PRIVATE_SIGNATURE"}}),
                );
                frames += &sse(
                    Some("content_block_stop"),
                    json!({"type":"content_block_stop","index":0}),
                );
                frames += &sse(
                    Some("content_block_start"),
                    json!({"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"call-1","name":"inspect_bot","input":{}}}),
                );
                frames += &sse(
                    Some("content_block_delta"),
                    json!({"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{}"}}),
                );
                frames += &sse(
                    Some("content_block_stop"),
                    json!({"type":"content_block_stop","index":1}),
                );
            } else {
                frames += &sse(
                    Some("content_block_start"),
                    json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
                );
                frames += &sse(
                    Some("content_block_delta"),
                    json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"已检查"}}),
                );
                frames += &sse(
                    Some("content_block_stop"),
                    json!({"type":"content_block_stop","index":0}),
                );
            }
            frames += &sse(
                Some("message_delta"),
                json!({"type":"message_delta","delta":{"stop_reason":if tool {"tool_use"} else {"end_turn"}},"usage":{"output_tokens":1}}),
            );
            frames + &sse(Some("message_stop"), json!({"type":"message_stop"}))
        }
        AssistantApiFormat::GeminiGenerateContent => sse(
            None,
            json!({"candidates":[{"index":0,"content":{"role":"model","parts":if tool {vec![json!({"functionCall":{"id":"call-1","name":"inspect_bot","args":{}},"thoughtSignature":"PRIVATE_SIGNATURE"})]} else {vec![json!({"text":"已检查"})]}},"finishReason":"STOP"}]}),
        ),
    }
}

fn route(format: AssistantApiFormat) -> &'static str {
    match format {
        AssistantApiFormat::OpenAiChatCompletions => "/v1/chat/completions",
        AssistantApiFormat::OpenAiResponses => "/v1/responses",
        AssistantApiFormat::AnthropicMessages => "/v1/messages",
        AssistantApiFormat::GeminiGenerateContent => {
            "/v1beta/models/fixture-model:streamGenerateContent"
        }
    }
}

const FORMATS: [AssistantApiFormat; 4] = [
    AssistantApiFormat::OpenAiChatCompletions,
    AssistantApiFormat::OpenAiResponses,
    AssistantApiFormat::AnthropicMessages,
    AssistantApiFormat::GeminiGenerateContent,
];

fn anonymous_gemini_frame(part: usize, done: bool) -> String {
    let mut candidate = json!({"content":{"role":"model","parts":[{
        "functionCall":{"name":"inspect_bot","args":{"part":part}},
        "thoughtSignature":format!("signature-{part}")
    }]}});
    if done {
        candidate["finishReason"] = json!("STOP");
    }
    sse(None, json!({"candidates":[candidate]}))
}

fn append_tool_round(messages: &mut Vec<ChatMessage>, reply: ModelReply) {
    let results = reply
        .tool_calls
        .iter()
        .map(|call| ChatMessage::Tool {
            tool_call_id: call.id.clone(),
            content: "{\"state\":\"ready\"}".into(),
        })
        .collect::<Vec<_>>();
    messages.push(ChatMessage::Assistant {
        content: reply.text,
        reasoning_content: reply.reasoning_content,
        tool_calls: reply.tool_calls,
        protocol: reply.protocol,
    });
    messages.extend(results);
}

fn assert_gemini_omits_synthetic_native_ids(payload: &Value) {
    for content in payload["contents"].as_array().unwrap() {
        for part in content["parts"].as_array().unwrap() {
            for field in ["functionCall", "functionResponse"] {
                if let Some(native) = part.get(field) {
                    assert!(native.get("id").is_none());
                }
            }
        }
    }
}

#[tokio::test]
async fn sdk_gemini_anonymous_calls_are_unique_across_consecutive_tool_rounds() {
    let server = MockServer::start().await;
    let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let hit = count.clone();
    Mock::given(method("POST"))
        .and(path(route(AssistantApiFormat::GeminiGenerateContent)))
        .respond_with(move |_: &wiremock::Request| {
            let index = hit.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let body = if index < 2 {
                anonymous_gemini_frame(index, true)
            } else {
                wire(AssistantApiFormat::GeminiGenerateContent, false)
            };
            ResponseTemplate::new(200).set_body_raw(body, "text/event-stream")
        })
        .expect(3)
        .mount(&server)
        .await;
    let client = SdkClient::new(
        &profile(server.uri(), AssistantApiFormat::GeminiGenerateContent),
        "FIXTURE_PRIVATE_KEY",
        Duration::from_secs(5),
    )
    .unwrap();
    let tools = vec![ToolSpec {
        name: "inspect_bot".into(),
        description: "读取状态".into(),
        parameters: json!({"type":"object","properties":{"part":{"type":"integer"}}}),
    }];
    let mut messages = vec![ChatMessage::User {
        content: "检查状态".into(),
    }];
    let mut ids = Vec::new();
    for _ in 0..2 {
        let reply = client
            .complete(
                &messages,
                &tools,
                Arc::new(|_| {}),
                &CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(reply.tool_calls.len(), 1);
        ids.push(reply.tool_calls[0].id.clone());
        append_tool_round(&mut messages, reply);
    }
    assert_ne!(ids[0], ids[1]);
    let final_reply = client
        .complete(
            &messages,
            &tools,
            Arc::new(|_| {}),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(final_reply.text, "已检查");
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 3);
    for request in &requests {
        let payload: Value = serde_json::from_slice(&request.body).unwrap();
        assert_gemini_omits_synthetic_native_ids(&payload);
    }
    let continuation = String::from_utf8(requests[2].body.clone()).unwrap();
    assert!(continuation.contains("signature-0") && continuation.contains("signature-1"));
}

#[tokio::test]
async fn sdk_gemini_anonymous_calls_in_separate_frames_keep_distinct_ids_and_signatures() {
    let server = MockServer::start().await;
    let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let hit = count.clone();
    Mock::given(method("POST"))
        .and(path(route(AssistantApiFormat::GeminiGenerateContent)))
        .respond_with(move |_: &wiremock::Request| {
            let body = if hit.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                anonymous_gemini_frame(1, false) + &anonymous_gemini_frame(2, true)
            } else {
                wire(AssistantApiFormat::GeminiGenerateContent, false)
            };
            ResponseTemplate::new(200).set_body_raw(body, "text/event-stream")
        })
        .expect(2)
        .mount(&server)
        .await;
    let client = SdkClient::new(
        &profile(server.uri(), AssistantApiFormat::GeminiGenerateContent),
        "FIXTURE_PRIVATE_KEY",
        Duration::from_secs(5),
    )
    .unwrap();
    let tools = vec![ToolSpec {
        name: "inspect_bot".into(),
        description: "读取状态".into(),
        parameters: json!({"type":"object","properties":{"part":{"type":"integer"}}}),
    }];
    let mut messages = vec![ChatMessage::User {
        content: "检查状态".into(),
    }];
    let reply = client
        .complete(
            &messages,
            &tools,
            Arc::new(|_| {}),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    assert_eq!(reply.tool_calls.len(), 2);
    assert_ne!(reply.tool_calls[0].id, reply.tool_calls[1].id);
    append_tool_round(&mut messages, reply);
    client
        .complete(
            &messages,
            &tools,
            Arc::new(|_| {}),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    let requests = server.received_requests().await.unwrap();
    let payload: Value = serde_json::from_slice(&requests[1].body).unwrap();
    assert_gemini_omits_synthetic_native_ids(&payload);
    assert!(
        payload.to_string().contains("signature-1") && payload.to_string().contains("signature-2")
    );
    let parts: Vec<_> = payload["contents"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|content| content["parts"].as_array().unwrap())
        .collect();
    assert_eq!(
        parts
            .iter()
            .filter(|part| part.get("functionCall").is_some())
            .count(),
        2
    );
    assert_eq!(
        parts
            .iter()
            .filter(|part| part.get("functionResponse").is_some())
            .count(),
        2
    );
}

#[tokio::test]
async fn sdk_downgrades_old_protocol_tools_but_preserves_current_native_continuations() {
    let server = MockServer::start().await;
    let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let hit = count.clone();
    Mock::given(method("POST"))
        .and(path(route(AssistantApiFormat::GeminiGenerateContent)))
        .respond_with(move |_: &wiremock::Request| {
            ResponseTemplate::new(200).set_body_raw(
                wire(
                    AssistantApiFormat::GeminiGenerateContent,
                    hit.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0,
                ),
                "text/event-stream",
            )
        })
        .expect(2)
        .mount(&server)
        .await;
    let client = SdkClient::new(
        &profile(server.uri(), AssistantApiFormat::GeminiGenerateContent),
        "FIXTURE_PRIVATE_KEY",
        Duration::from_secs(5),
    )
    .unwrap();
    let old_call = ToolCall {
        id: "old-call".into(),
        name: "inspect_bot".into(),
        arguments: "{}".into(),
    };
    let old_protocol = ProtocolContent {
        format: AssistantApiFormat::AnthropicMessages,
        content: MessageContent::from_parts(vec![
            ContentPart::ThoughtSignature("OLD_PROVIDER_SIGNATURE".into()),
            ContentPart::ReasoningContent("OLD_PRIVATE_REASONING".into()),
            ContentPart::Text("先前检查完成。".into()),
            ContentPart::ToolCall(SdkToolCall {
                call_id: old_call.id.clone(),
                fn_name: old_call.name.clone(),
                fn_arguments: json!({}),
                thought_signatures: None,
            }),
        ]),
    };
    let mut messages = vec![
        ChatMessage::User {
            content: "旧问题".into(),
        },
        ChatMessage::Assistant {
            content: "<think>OLD_PRIVATE_REASONING</think>先前检查完成。".into(),
            reasoning_content: Some("OLD_PRIVATE_REASONING".into()),
            tool_calls: vec![old_call],
            protocol: Some(old_protocol),
        },
        ChatMessage::Tool {
            tool_call_id: "old-call".into(),
            content: "{\"state\":\"old-ready\"}".into(),
        },
        ChatMessage::User {
            content: "继续检查".into(),
        },
    ];
    let tools = vec![ToolSpec {
        name: "inspect_bot".into(),
        description: "读取状态".into(),
        parameters: json!({"type":"object","properties":{}}),
    }];
    let first = client
        .complete(
            &messages,
            &tools,
            Arc::new(|_| {}),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    let id = first.tool_calls[0].id.clone();
    messages.push(ChatMessage::Assistant {
        content: first.text,
        reasoning_content: first.reasoning_content,
        tool_calls: first.tool_calls,
        protocol: first.protocol,
    });
    messages.push(ChatMessage::Tool {
        tool_call_id: id,
        content: "{\"state\":\"new-ready\"}".into(),
    });
    client
        .complete(
            &messages,
            &tools,
            Arc::new(|_| {}),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    let requests = server.received_requests().await.unwrap();
    let original: Value = serde_json::from_slice(&requests[0].body).unwrap();
    let original_text = original.to_string();
    assert!(original_text.contains("先前检查完成") && original_text.contains("old-ready"));
    assert!(
        !original_text.contains("OLD_PROVIDER_SIGNATURE")
            && !original_text.contains("OLD_PRIVATE_REASONING")
    );
    assert!(
        !original_text.contains("\"functionCall\"")
            && !original_text.contains("\"functionResponse\"")
    );
    let continuation: Value = serde_json::from_slice(&requests[1].body).unwrap();
    let parts: Vec<_> = continuation["contents"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|content| content["parts"].as_array().unwrap())
        .collect();
    assert_eq!(
        parts
            .iter()
            .filter(|part| part.get("functionCall").is_some())
            .count(),
        1
    );
    assert_eq!(
        parts
            .iter()
            .filter(|part| part.get("functionResponse").is_some())
            .count(),
        1
    );
    assert!(continuation.to_string().contains("PRIVATE_SIGNATURE"));
    assert!(!continuation.to_string().contains("OLD_PROVIDER_SIGNATURE"));
}

#[test]
fn sdk_legacy_unsigned_tools_are_plain_history_for_non_chat_formats() {
    for format in [
        AssistantApiFormat::OpenAiResponses,
        AssistantApiFormat::AnthropicMessages,
        AssistantApiFormat::GeminiGenerateContent,
    ] {
        let client = SdkClient::new(
            &profile("http://127.0.0.1:1".into(), format),
            "FIXTURE_PRIVATE_KEY",
            Duration::from_secs(5),
        )
        .unwrap();
        let messages = vec![
            ChatMessage::User {
                content: "旧问题".into(),
            },
            ChatMessage::Assistant {
                content: "先前检查".into(),
                reasoning_content: Some("OLD_PRIVATE_REASONING".into()),
                tool_calls: vec![ToolCall {
                    id: "old-call".into(),
                    name: "inspect_bot".into(),
                    arguments: "{}".into(),
                }],
                protocol: None,
            },
            ChatMessage::Tool {
                tool_call_id: "old-call".into(),
                content: "已就绪".into(),
            },
        ];
        let request = client.request(&messages, &[]).unwrap();
        assert!(
            request
                .messages
                .iter()
                .flat_map(|message| message.content.parts())
                .all(|part| !matches!(
                    part,
                    ContentPart::ToolCall(_)
                        | ContentPart::ToolResponse(_)
                        | ContentPart::ReasoningContent(_)
                        | ContentPart::ThoughtSignature(_)
                ))
        );
        let rendered = serde_json::to_string(&request).unwrap();
        assert!(
            rendered.contains("历史工具调用")
                && rendered.contains("历史工具结果")
                && rendered.contains("已就绪")
        );
        assert!(!rendered.contains("OLD_PRIVATE_REASONING"));
    }
}

#[test]
fn sdk_anthropic_rejects_interleaved_tool_blocks_before_sdk_state_can_be_overwritten() {
    let guard = ModelStreamGuard::new(AssistantApiFormat::AnthropicMessages);
    for index in 0..2 {
        let frame = json!({
            "type": "content_block_start",
            "index": index,
            "content_block": { "type": "tool_use", "id": format!("call-{index}"), "name": "inspect_bot", "input": {} }
        });
        let result = guard.validate_frame(Some("content_block_start"), &frame.to_string());
        assert_eq!(result.is_ok(), index == 0);
    }
    assert!(guard.completed().is_err());
}

#[tokio::test]
async fn sdk_minimax_preserves_completion_token_settings_without_null_max_tokens() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            wire(AssistantApiFormat::OpenAiChatCompletions, false),
            "text/event-stream",
        ))
        .expect(1)
        .mount(&server)
        .await;
    let mut settings = profile(server.uri(), AssistantApiFormat::OpenAiChatCompletions);
    settings.model = "minimax-m3.1-flash".into();
    let client = SdkClient::new(&settings, "FIXTURE_PRIVATE_KEY", Duration::from_secs(5)).unwrap();
    client
        .complete(
            &[ChatMessage::User {
                content: "检查状态".into(),
            }],
            &[],
            Arc::new(|_| {}),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    let requests = server.received_requests().await.unwrap();
    let payload: Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert!(payload.get("max_tokens").is_none());
    assert_eq!(payload["max_completion_tokens"], settings.max_output_tokens);
    assert_eq!(payload["reasoning_split"], true);
    assert_eq!(payload["reasoning_effort"], "low");
}

#[tokio::test]
async fn sdk_accepts_an_anonymous_local_model_and_preserves_an_explicit_endpoint() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/gateway/custom-chat"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            wire(AssistantApiFormat::OpenAiChatCompletions, false),
            "text/event-stream",
        ))
        .expect(1)
        .mount(&server)
        .await;
    let settings = profile(
        format!("{}/gateway/custom-chat", server.uri()),
        AssistantApiFormat::OpenAiChatCompletions,
    );
    let client = SdkClient::new(&settings, "", Duration::from_secs(5)).unwrap();
    client
        .complete(
            &[ChatMessage::User {
                content: "检查状态".into(),
            }],
            &[],
            Arc::new(|_| {}),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
    let requests = server.received_requests().await.unwrap();
    assert!(requests[0].headers.get("authorization").is_none());
}

#[tokio::test]
async fn sdk_uses_explicit_formats_and_custom_model_without_probe_or_credentials_in_url() {
    for format in FORMATS {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path(route(format)))
            .respond_with(
                ResponseTemplate::new(200).set_body_raw(wire(format, false), "text/event-stream"),
            )
            .expect(1)
            .mount(&server)
            .await;
        let client = SdkClient::new(
            &profile(server.uri(), format),
            "FIXTURE_PRIVATE_KEY",
            Duration::from_secs(5),
        )
        .unwrap();
        assert!(server.received_requests().await.unwrap().is_empty());
        let visible = Arc::new(Mutex::new(String::new()));
        let capture = visible.clone();
        let reply = client
            .complete(
                &[ChatMessage::User {
                    content: "检查状态".into(),
                }],
                &[],
                Arc::new(move |text| capture.lock().unwrap().push_str(text)),
                &CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(reply.text, "已检查");
        assert_eq!(visible.lock().unwrap().as_str(), "已检查");
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1);
        assert!(!requests[0].url.as_str().contains("FIXTURE_PRIVATE_KEY"));
        let payload: Value = serde_json::from_slice(&requests[0].body).unwrap();
        if format != AssistantApiFormat::GeminiGenerateContent {
            assert_eq!(payload["model"], "fixture-model");
        }
        let header = match format {
            AssistantApiFormat::AnthropicMessages => "x-api-key",
            AssistantApiFormat::GeminiGenerateContent => "x-goog-api-key",
            _ => "authorization",
        };
        assert!(
            requests[0]
                .headers
                .get(header)
                .unwrap()
                .to_str()
                .unwrap()
                .contains("FIXTURE_PRIVATE_KEY")
        );
    }
}

#[tokio::test]
async fn sdk_tool_continuations_preserve_private_reasoning_and_signatures_without_exposing_them() {
    for format in FORMATS {
        let server = MockServer::start().await;
        let hit = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let count = hit.clone();
        Mock::given(method("POST"))
            .and(path(route(format)))
            .respond_with(move |_: &wiremock::Request| {
                ResponseTemplate::new(200).set_body_raw(
                    wire(
                        format,
                        count.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0,
                    ),
                    "text/event-stream",
                )
            })
            .expect(2)
            .mount(&server)
            .await;
        let client = SdkClient::new(
            &profile(server.uri(), format),
            "FIXTURE_PRIVATE_KEY",
            Duration::from_secs(5),
        )
        .unwrap();
        let visible = Arc::new(Mutex::new(String::new()));
        let capture = visible.clone();
        let user = ChatMessage::User {
            content: "检查状态".into(),
        };
        let tools = vec![ToolSpec {
            name: "inspect_bot".into(),
            description: "读取状态".into(),
            parameters: json!({"type":"object","properties":{},"additionalProperties":false}),
        }];
        let first = client
            .complete(
                &[user.clone()],
                &tools,
                Arc::new(move |text| capture.lock().unwrap().push_str(text)),
                &CancellationToken::new(),
            )
            .await
            .unwrap();
        assert_eq!(first.finish_reason, FinishReason::ToolCalls);
        assert_eq!(first.tool_calls.len(), 1);
        let id = first.tool_calls[0].id.clone();
        let follow = vec![
            user,
            ChatMessage::Assistant {
                content: first.text,
                reasoning_content: first.reasoning_content,
                tool_calls: first.tool_calls,
                protocol: first.protocol,
            },
            ChatMessage::Tool {
                tool_call_id: id,
                content: "{\"state\":\"ready\"}".into(),
            },
        ];
        client
            .complete(&follow, &tools, Arc::new(|_| {}), &CancellationToken::new())
            .await
            .unwrap();
        assert!(!visible.lock().unwrap().contains("PRIVATE_"));
        let requests = server.received_requests().await.unwrap();
        let payload: Value = serde_json::from_slice(&requests[1].body).unwrap();
        let rendered = payload.to_string();
        assert!(rendered.contains("inspect_bot"));
        if matches!(
            format,
            AssistantApiFormat::OpenAiResponses
                | AssistantApiFormat::AnthropicMessages
                | AssistantApiFormat::GeminiGenerateContent
        ) {
            assert!(rendered.contains("PRIVATE_SIGNATURE"));
        }
        if format == AssistantApiFormat::OpenAiChatCompletions {
            assert!(rendered.contains("PRIVATE_REASONING"));
        }
        if format == AssistantApiFormat::GeminiGenerateContent {
            assert!(rendered.contains("functionResponse"));
        }
    }
}

#[tokio::test]
async fn sdk_rejects_truncated_streams_and_never_uses_partial_tool_calls() {
    for format in FORMATS {
        let server = MockServer::start().await;
        let body = match format {
            AssistantApiFormat::OpenAiChatCompletions => sse(
                None,
                json!({"choices":[{"index":0,"delta":{"content":"部分结果"},"finish_reason":null}]}),
            ),
            AssistantApiFormat::OpenAiResponses => sse(
                None,
                json!({"type":"response.output_text.delta","delta":"部分结果"}),
            ),
            AssistantApiFormat::AnthropicMessages => sse(
                Some("message_start"),
                json!({"type":"message_start","message":{"role":"assistant"}}),
            ),
            AssistantApiFormat::GeminiGenerateContent => sse(
                None,
                json!({"candidates":[{"content":{"role":"model","parts":[{"text":"部分结果"}]}}]}),
            ),
        };
        Mock::given(method("POST"))
            .and(path(route(format)))
            .respond_with(ResponseTemplate::new(200).set_body_raw(body, "text/event-stream"))
            .expect(1)
            .mount(&server)
            .await;
        let error = SdkClient::new(
            &profile(server.uri(), format),
            "FIXTURE_PRIVATE_KEY",
            Duration::from_secs(5),
        )
        .unwrap()
        .complete(
            &[ChatMessage::User {
                content: "检查状态".into(),
            }],
            &[],
            Arc::new(|_| {}),
            &CancellationToken::new(),
        )
        .await
        .err()
        .unwrap();
        assert!(matches!(
            error,
            ModelError::IncompleteResponse | ModelError::InvalidResponse
        ));
    }
}

#[tokio::test]
async fn sdk_http_errors_are_fixed_categories_without_retry_or_body_echo() {
    for status in [302, 400, 401, 429, 500] {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(status)
                    .insert_header("Location", "http://127.0.0.1:1/private")
                    .set_body_string("PRIVATE_BODY FIXTURE_PRIVATE_KEY"),
            )
            .expect(1)
            .mount(&server)
            .await;
        let error = SdkClient::new(
            &profile(server.uri(), AssistantApiFormat::OpenAiResponses),
            "FIXTURE_PRIVATE_KEY",
            Duration::from_secs(5),
        )
        .unwrap()
        .complete(
            &[ChatMessage::User {
                content: "检查状态".into(),
            }],
            &[],
            Arc::new(|_| {}),
            &CancellationToken::new(),
        )
        .await
        .err()
        .unwrap();
        assert_eq!(error, ModelError::Http(status));
        assert!(!error.to_string().contains("PRIVATE_"));
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
    }
}

#[tokio::test]
async fn sdk_cancellation_drops_the_only_pending_request() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_secs(5))
                .set_body_raw(
                    wire(AssistantApiFormat::OpenAiChatCompletions, false),
                    "text/event-stream",
                ),
        )
        .expect(1)
        .mount(&server)
        .await;
    let client = SdkClient::new(
        &profile(server.uri(), AssistantApiFormat::OpenAiChatCompletions),
        "FIXTURE_PRIVATE_KEY",
        Duration::from_secs(10),
    )
    .unwrap();
    let cancel = CancellationToken::new();
    let child = cancel.clone();
    let stopping = async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        child.cancel();
    };
    let messages = [ChatMessage::User {
        content: "检查状态".into(),
    }];
    let request = client.complete(&messages, &[], Arc::new(|_| {}), &cancel);
    let (reply, ()) = tokio::join!(request, stopping);
    assert_eq!(reply.err(), Some(ModelError::Cancelled));
}

#[test]
fn raw_guard_rejects_huge_indexes_before_sdk_allocation_and_requires_explicit_terminators() {
    let guard = ModelStreamGuard::new(AssistantApiFormat::OpenAiChatCompletions);
    assert!(guard.validate_frame(None, &json!({"choices":[{"index":0,"delta":{"tool_calls":[{"index":4_000_000_000u64,"id":"call","function":{"name":"inspect_bot","arguments":"{}"}}]},"finish_reason":null}]}).to_string()).is_err());
    assert_eq!(guard.error(), Some(ModelError::InvalidToolCall));
    for format in FORMATS {
        assert!(ModelStreamGuard::new(format).completed().is_err());
    }
}
