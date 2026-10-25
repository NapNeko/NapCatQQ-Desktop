use std::sync::Mutex;

use serde_json::json;

use super::*;

fn event(delta: Value, reason: Value) -> String {
    format!(
        "data: {}\r\n\r\n",
        json!({"choices":[{"index":0,"delta":delta,"finish_reason":reason}]})
    )
}

fn complete_wire(delta: Value, reason: &str) -> Vec<u8> {
    format!(
        "{}{}data: [DONE]\r\n\r\n",
        event(delta, Value::Null),
        event(json!({}), json!(reason))
    )
    .into_bytes()
}

fn parse(wire: &[u8], width: usize) -> Result<(ModelReply, Vec<String>), ModelError> {
    let mut decoder = SseDecoder::default();
    let mut parser = CompletionParser::default();
    let text = Mutex::new(Vec::new());
    for chunk in wire.chunks(width) {
        decoder.push(chunk, |data| {
            parser.event(data, &|delta| text.lock().unwrap().push(delta.to_owned()))
        })?;
    }
    decoder.finish()?;
    Ok((parser.finish()?, text.into_inner().unwrap()))
}

#[test]
fn fragmented_utf8_crlf_and_comments_preserve_visible_text_and_ignore_reasoning() {
    let wire = format!(
        "\u{feff}: heartbeat\r\n\r\n{}{}{}data: [DONE]\r\n\r\n",
        event(
            json!({"role":"assistant","reasoning_content":"TEST_PRIVATE_REASONING"}),
            Value::Null
        ),
        event(
            json!({"content":"中文🧪","reasoning":"TEST_PRIVATE_REASONING"}),
            Value::Null
        ),
        event(json!({}), json!("stop"))
    );
    let (reply, text) = parse(wire.as_bytes(), 1).unwrap();
    assert_eq!(reply.text, "中文🧪");
    assert_eq!(text, ["中文🧪"]);
    assert_eq!(reply.finish_reason, FinishReason::Stop);
}

#[test]
fn multiple_data_lines_form_one_json_event_and_bare_cr_boundaries_are_supported() {
    let wire = b"data: {\"choices\":\rdata: [{\"index\":0,\"delta\":{\"content\":\"answer\"},\"finish_reason\":\"stop\"}]}\r\rdata: [DONE]\r\r";
    let (reply, _) = parse(wire, 3).unwrap();
    assert_eq!(reply.text, "answer");
}

#[test]
fn interleaved_tool_indices_assemble_id_name_and_json_arguments_before_returning() {
    let wire = format!(
        "{}{}{}{}data: [DONE]\n\n",
        event(
            json!({"tool_calls":[{"index":1,"id":"call_","type":"function","function":{"name":"read_","arguments":"{\"bot\":"}}]}),
            Value::Null
        ),
        event(
            json!({"tool_calls":[{"index":0,"id":"first","type":"function","function":{"name":"inspect","arguments":"{}"}}]}),
            Value::Null
        ),
        event(
            json!({"tool_calls":[{"index":1,"id":"second","function":{"name":"logs","arguments":"\"测试\"}"}}]}),
            Value::Null
        ),
        event(json!({}), json!("tool_calls"))
    );
    let (reply, text) = parse(wire.as_bytes(), 2).unwrap();
    assert!(text.is_empty());
    assert_eq!(reply.tool_calls.len(), 2);
    assert_eq!(reply.tool_calls[0].id, "first");
    assert_eq!(reply.tool_calls[1].id, "call_second");
    assert_eq!(reply.tool_calls[1].name, "read_logs");
    assert_eq!(
        serde_json::from_str::<Value>(&reply.tool_calls[1].arguments).unwrap(),
        json!({"bot":"测试"})
    );
}

#[test]
fn done_without_finish_reason_is_incomplete_even_with_complete_tool_json() {
    let wire = format!(
        "{}data: [DONE]\n\n",
        event(
            json!({"tool_calls":[{
                "index":0,"id":"a","function":{"name":"read","arguments":"{}"}
            }]}),
            Value::Null
        )
    );
    assert!(matches!(
        parse(wire.as_bytes(), 7),
        Err(ModelError::IncompleteResponse)
    ));
}

#[test]
fn explicit_done_is_required_before_a_stream_can_deliver_tool_calls() {
    let wire = event(
        json!({"tool_calls":[{"index":0,"id":"a","function":{"name":"read","arguments":"{}"}}]}),
        json!("tool_calls"),
    );
    assert!(matches!(
        parse(wire.as_bytes(), 8),
        Err(ModelError::IncompleteResponse)
    ));
}

#[test]
fn incomplete_sse_event_at_eof_is_rejected() {
    let mut decoder = SseDecoder::default();
    decoder
        .push(b"data: {\"choices\":[]}\n", |_| Ok(()))
        .unwrap();
    assert_eq!(decoder.finish(), Err(ModelError::IncompleteResponse));
}

#[test]
fn malformed_utf8_is_rejected_without_replacement_characters() {
    let mut decoder = SseDecoder::default();
    assert_eq!(
        decoder.push(b"data: \xff\n\n", |_| Ok(())),
        Err(ModelError::InvalidResponse)
    );
}

#[test]
fn sse_line_and_multiline_event_budgets_are_bounded() {
    let mut decoder = SseDecoder::default();
    assert_eq!(
        decoder.push(&vec![b'x'; MAX_SSE_LINE_BYTES + 1], |_| Ok(())),
        Err(ModelError::ResponseTooLarge)
    );
    let mut decoder = SseDecoder::default();
    let line = format!("data: {}\n", "x".repeat(MAX_SSE_EVENT_BYTES / 2));
    decoder.push(line.as_bytes(), |_| Ok(())).unwrap();
    assert_eq!(
        decoder.push(line.as_bytes(), |_| Ok(())),
        Err(ModelError::ResponseTooLarge)
    );
}

#[test]
fn duplicate_tool_ids_and_sparse_indices_are_rejected() {
    for indices in [vec![0, 1], vec![1]] {
        let calls = indices.iter().map(|index| json!({"index":index,"id":"same","function":{"name":"read","arguments":"{}"}})).collect::<Vec<_>>();
        let wire = complete_wire(json!({"tool_calls":calls}), "tool_calls");
        assert!(matches!(
            parse(&wire, 1024),
            Err(ModelError::InvalidToolCall)
        ));
    }
}

#[test]
fn incomplete_or_nonobject_tool_arguments_never_become_executable() {
    for arguments in ["{\"value\":", "[]", "null", "\"text\"", "{} trailing"] {
        let wire = complete_wire(
            json!({"tool_calls":[{"index":0,"id":"call","function":{"name":"read","arguments":arguments}}]}),
            "tool_calls",
        );
        assert!(matches!(parse(&wire, 17), Err(ModelError::InvalidToolCall)));
    }
}

#[test]
fn truncated_or_blocked_responses_cannot_return_tool_calls() {
    for finish in ["length", "content_filter", "stop"] {
        let wire = complete_wire(
            json!({"tool_calls":[{"index":0,"id":"call","function":{"name":"read","arguments":"{}"}}]}),
            finish,
        );
        assert!(matches!(
            parse(&wire, 13),
            Err(ModelError::IncompleteResponse)
        ));
    }
}

#[test]
fn tool_argument_and_visible_text_limits_apply_across_deltas() {
    let wire = complete_wire(
        json!({"tool_calls":[{"index":0,"id":"a","function":{"name":"read","arguments":"x".repeat(MAX_ARGUMENT_BYTES + 1)}}]}),
        "tool_calls",
    );
    assert!(matches!(
        parse(&wire, 4096),
        Err(ModelError::ResponseTooLarge)
    ));
    let wire = format!(
        "{}{}{}data: [DONE]\n\n",
        event(
            json!({"content":"x".repeat(MAX_TEXT_BYTES / 2)}),
            Value::Null
        ),
        event(
            json!({"content":"x".repeat(MAX_TEXT_BYTES / 2 + 1)}),
            Value::Null
        ),
        event(json!({}), json!("stop"))
    );
    assert!(matches!(
        parse(wire.as_bytes(), 4096),
        Err(ModelError::ResponseTooLarge)
    ));
}

#[test]
fn unknown_finish_reasons_multiple_choices_and_data_after_done_are_rejected() {
    let wire = complete_wire(json!({"content":"answer"}), "unexpected");
    assert!(matches!(parse(&wire, 31), Err(ModelError::InvalidResponse)));
    let mut parser = CompletionParser::default();
    assert_eq!(
        parser.event(
            br#"{"choices":[{"index":0,"delta":{}},{"index":1,"delta":{}}]}"#,
            &|_| {}
        ),
        Err(ModelError::InvalidResponse)
    );
    let wire = format!(
        "{}data: [DONE]\n\ndata: {{\"choices\":[]}}\n\n",
        event(json!({"content":"answer"}), json!("stop"))
    );
    assert!(matches!(
        parse(wire.as_bytes(), wire.len()),
        Err(ModelError::InvalidResponse)
    ));
}

#[test]
fn nonstream_json_requires_an_explicit_finish_reason_and_validates_tool_arguments() {
    let complete = json!({"choices":[{"index":0,"message":{"role":"assistant","content":null,"tool_calls":[{
        "id":"call","type":"function","function":{"name":"read","arguments":"{\"value\":1}"}
    }]},"finish_reason":"tool_calls"}]});
    let reply = CompletionParser::json(&serde_json::to_vec(&complete).unwrap(), &|_| {}).unwrap();
    assert_eq!(reply.tool_calls[0].id, "call");
    let incomplete = json!({"choices":[{"index":0,"message":{"content":"answer"}}]});
    assert!(matches!(
        CompletionParser::json(&serde_json::to_vec(&incomplete).unwrap(), &|_| {}),
        Err(ModelError::IncompleteResponse)
    ));
}

#[test]
fn provider_error_text_and_reasoning_do_not_appear_in_safe_errors() {
    let mut parser = CompletionParser::default();
    let error = parser
        .event(
            br#"{"error":{"message":"TEST_PRIVATE_VALUE","data":"TEST_PRIVATE_REASONING"}}"#,
            &|_| {},
        )
        .err()
        .unwrap();
    assert_eq!(error, ModelError::Provider);
    assert!(!error.to_string().contains("TEST_PRIVATE"));
}

#[test]
fn reasoning_fragments_are_preserved_privately_and_think_tags_are_filtered_across_chunks() {
    let wire = format!(
        "{}{}{}{}{}{}data: [DONE]\n\n",
        event(json!({"reasoning_content":"私有"}), Value::Null),
        event(
            json!({"reasoning_content":"推理\\\nTEST_PRIVATE_REASONING"}),
            Value::Null
        ),
        event(json!({"content":"<thi"}), Value::Null),
        event(
            json!({"content":"nk>TEST_PRIVATE_CONTENT</th"}),
            Value::Null
        ),
        event(json!({"content":"ink>公开中文回答"}), Value::Null),
        event(json!({}), json!("stop"))
    );
    let (reply, visible) = parse(wire.as_bytes(), 1).unwrap();
    assert_eq!(
        reply.reasoning_content.as_deref(),
        Some("私有推理\\\nTEST_PRIVATE_REASONING")
    );
    assert_eq!(
        reply.text,
        "<think>TEST_PRIVATE_CONTENT</think>公开中文回答"
    );
    assert_eq!(visible.concat(), "公开中文回答");
}

#[test]
fn reasoning_budget_applies_to_sse_fragments_and_nonstream_json() {
    let wire = format!(
        "{}{}{}data: [DONE]\n\n",
        event(
            json!({"reasoning_content":"x".repeat(MAX_REASONING_BYTES / 2)}),
            Value::Null
        ),
        event(
            json!({"reasoning_content":"x".repeat(MAX_REASONING_BYTES / 2 + 1)}),
            Value::Null
        ),
        event(json!({"content":"answer"}), json!("stop"))
    );
    assert!(matches!(
        parse(wire.as_bytes(), 4096),
        Err(ModelError::ResponseTooLarge)
    ));
    let body = json!({"choices":[{"index":0,"message":{"content":"answer","reasoning_content":"x".repeat(MAX_REASONING_BYTES + 1)},"finish_reason":"stop"}]}).to_string();
    assert!(matches!(
        CompletionParser::json(body.as_bytes(), &|_| {}),
        Err(ModelError::ResponseTooLarge)
    ));
}

#[test]
fn think_filter_handles_nested_tags_case_and_literal_prefixes_with_constant_pending_space() {
    let raw = "before<ThInK>hidden<think>nested</think>hidden</tHiNk>after <theme>literal<";
    for width in 1..=raw.len() {
        let mut filter = ThinkFilter::default();
        let visible = Mutex::new(String::new());
        for chunk in raw.as_bytes().chunks(width) {
            filter.push(std::str::from_utf8(chunk).unwrap(), &|text| {
                visible.lock().unwrap().push_str(text)
            });
            assert!(filter.pending.len() < "</think>".len());
        }
        filter.finish(&|text| visible.lock().unwrap().push_str(text));
        assert_eq!(visible.into_inner().unwrap(), "beforeafter <theme>literal<");
    }
}

#[test]
fn hidden_only_stop_is_not_a_visible_answer_but_tool_steps_can_preserve_private_reasoning() {
    let hidden = complete_wire(json!({"content":"<think>hidden forever"}), "stop");
    assert!(matches!(
        parse(&hidden, 2),
        Err(ModelError::InvalidResponse)
    ));
    let body = json!({"choices":[{"index":0,"message":{"content":null,"reasoning_content":"private chain","tool_calls":[{
        "id":"call","type":"function","function":{"name":"read","arguments":"{}"}
    }]},"finish_reason":"tool_calls"}]}).to_string();
    let reply = CompletionParser::json(body.as_bytes(), &|_| {
        panic!("reasoning must remain private")
    })
    .unwrap();
    assert_eq!(reply.reasoning_content.as_deref(), Some("private chain"));
    assert_eq!(reply.tool_calls.len(), 1);
}
