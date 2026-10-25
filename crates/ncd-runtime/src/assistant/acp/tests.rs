use std::{env, path::Path};

use tokio::io::{AsyncWriteExt, BufReader};

use super::wire::Incoming;
use super::*;

#[test]
fn balance_failure_hint_requires_the_specific_phrase_and_never_forwards_secrets() {
    for (code, message, expected) in [
        (
            -32603,
            "Internal error: turn failed: request failed: Insufficient Balance fixture-secret",
            "Agent 报告模型服务余额不足，请在对应服务中检查额度后重试（错误码 -32603）",
        ),
        (
            -32603,
            "balance lookup failed fixture-secret",
            "Agent 请求失败（错误码 -32603）",
        ),
        (
            -32602,
            "Insufficient Balance fixture-secret",
            "Agent 请求失败（错误码 -32602）",
        ),
    ] {
        let frame = wire::encode(json!({"jsonrpc":"2.0", "id":1, "error":{
            "code":code, "message":message, "data":{"token":"fixture-secret-data"}
        }}))
        .unwrap();
        let Incoming::Response {
            result: Err(error), ..
        } = wire::parse(&frame).unwrap()
        else {
            panic!("expected error");
        };
        assert_eq!(error, expected);
        assert!(!error.contains("fixture-secret"));
    }
}

#[test]
fn dsh_missing_key_hint_never_forwards_upstream_message_tail_or_data() {
    let message = "Internal error: turn failed: llm-deepseek: no API key for provider route \"deepseek-official\"; store DEEPSEEK_API_KEY through credentials; fixture-secret-message";
    for (code, upstream, expected) in [
        (
            -32603,
            message,
            "DSH 的 deepseek-official 模型路由缺少 API Key，请先在 DSH 中配置该路由后重试（错误码 -32603）",
        ),
        (-32602, message, "Agent 请求失败（错误码 -32602）"),
        (
            -32603,
            "turn failed: unrelated fixture-secret-message",
            "Agent 请求失败（错误码 -32603）",
        ),
    ] {
        let frame = wire::encode(json!({"jsonrpc":"2.0", "id":1, "error":{
            "code":code, "message":upstream, "data":{"credentials":"fixture-secret-data"}
        }}))
        .expect("error frame");
        let Incoming::Response {
            result: Err(error), ..
        } = wire::parse(&frame).expect("valid response")
        else {
            panic!("expected error response");
        };
        assert_eq!(error, expected);
        assert!(!error.contains("fixture-secret") && !error.contains("DEEPSEEK_API_KEY"));
    }
}

fn in_memory_process() -> (
    Arc<AcpProcess>,
    mpsc::Receiver<AcpEvent>,
    mpsc::Receiver<Outbound>,
) {
    let (events, receiver) = mpsc::channel(EVENT_QUEUE_CAPACITY);
    let (outgoing, writes) = mpsc::channel(WRITE_QUEUE_CAPACITY);
    let shared = Arc::new(Shared {
        closed: AtomicBool::new(false),
        next_id: AtomicU64::new(1),
        pending: Mutex::new(HashMap::new()),
        agent_requests: Mutex::new(HashSet::new()),
        event_gate: Mutex::new(()),
        events,
        stop: CancellationToken::new(),
        stopped: CancellationToken::new(),
        stdout_done: CancellationToken::new(),
    });
    (Arc::new(AcpProcess { shared, outgoing }), receiver, writes)
}

#[tokio::test]
async fn reads_fragmented_utf8_frames_without_consuming_the_next_message() {
    let first = json!({"jsonrpc":"2.0", "id":1, "result":{"text":"中文🧪"}});
    let second = json!({"jsonrpc":"2.0", "id":2, "result":{}});
    let mut bytes = wire::encode(first.clone()).unwrap();
    bytes.extend(wire::encode(second.clone()).unwrap());
    let (mut writer, reader) = tokio::io::duplex(3);
    let task = tokio::spawn(async move {
        for chunk in bytes.chunks(2) {
            writer.write_all(chunk).await.unwrap();
        }
    });
    let mut reader = BufReader::with_capacity(3, reader);
    let actual: Value =
        serde_json::from_slice(&wire::read_frame(&mut reader).await.unwrap().unwrap()).unwrap();
    assert_eq!(actual, first);
    let actual: Value =
        serde_json::from_slice(&wire::read_frame(&mut reader).await.unwrap().unwrap()).unwrap();
    assert_eq!(actual, second);
    task.await.unwrap();
    assert!(wire::read_frame(&mut reader).await.unwrap().is_none());
}

#[tokio::test]
async fn accepts_the_frame_limit_and_rejects_a_larger_unterminated_frame() {
    let mut exact = vec![b' '; wire::MAX_FRAME_BYTES];
    exact.push(b'\n');
    assert_eq!(
        wire::read_frame(&mut BufReader::new(exact.as_slice()))
            .await
            .unwrap()
            .unwrap()
            .len(),
        wire::MAX_FRAME_BYTES
    );
    let too_large = vec![b' '; wire::MAX_FRAME_BYTES + 1];
    assert!(
        wire::read_frame(&mut BufReader::new(too_large.as_slice()))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn rejects_empty_and_incomplete_lines_but_accepts_crlf() {
    assert!(
        wire::read_frame(&mut BufReader::new(&b"\n"[..]))
            .await
            .is_err()
    );
    assert!(
        wire::read_frame(&mut BufReader::new(&b"{\"jsonrpc\":\"2.0\"}"[..]))
            .await
            .is_err()
    );
    let frame = wire::read_frame(&mut BufReader::new(&b"{}\r\n"[..]))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(frame.as_slice(), b"{}");
}

#[test]
fn rejects_invalid_utf8_batch_messages_and_ambiguous_envelopes() {
    let invalid = [
        &b"\xff"[..],
        &b"[]"[..],
        &b"[{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{}}]"[..],
        &b"{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"text\":\"\xff\"}}"[..],
        &b"{\"jsonrpc\":\"1.0\",\"id\":1,\"result\":{}}"[..],
        &b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"x\",\"result\":{}}"[..],
        &b"{\"jsonrpc\":\"2.0\",\"id\":1.5,\"result\":{}}"[..],
        &b"{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{},\"error\":{}}"[..],
        &b"{\"jsonrpc\":\"2.0\",\"method\":\"x\",\"params\":42}"[..],
    ];
    for frame in invalid {
        assert!(wire::parse(frame).is_err());
    }
}

#[test]
fn preserves_numeric_string_and_null_reverse_request_ids() {
    for id in [json!(42), json!(-3), json!("permission-42"), Value::Null] {
        let frame = wire::encode(
            json!({"jsonrpc":"2.0", "id":id, "method":"session/request_permission", "params":{}}),
        )
        .unwrap();
        let Incoming::Request { id: parsed, .. } = wire::parse(&frame).unwrap() else {
            panic!("expected a request");
        };
        assert_eq!(parsed.value(), id);
    }
    assert!(RequestId::parse(&json!("x".repeat(257))).is_err());
}

#[test]
fn encoding_escapes_newlines_and_rejects_oversized_payloads() {
    let frame = wire::encode(json!({"text":"first\nsecond"})).unwrap();
    assert_eq!(frame.iter().filter(|byte| **byte == b'\n').count(), 1);
    let large = json!({"text":"x".repeat(wire::MAX_FRAME_BYTES)});
    assert!(wire::encode(large).is_err());
}

#[test]
fn rpc_errors_do_not_forward_upstream_messages_or_data() {
    let frame = wire::encode(json!({"jsonrpc":"2.0", "id":1, "error":{
        "code":-32000, "message":"secret=TEST_PRIVATE_VALUE", "data":{"token":"TEST_PRIVATE_VALUE"}
    }}))
    .unwrap();
    let Incoming::Response {
        result: Err(error), ..
    } = wire::parse(&frame).unwrap()
    else {
        panic!("expected error");
    };
    assert!(error.contains("-32000"));
    assert!(!error.contains("TEST_PRIVATE_VALUE"));
}

#[test]
fn authentication_error_hint_uses_only_the_code_and_does_not_relabel_other_errors() {
    for (code, expected) in [
        (
            -32000,
            "Agent 返回需要认证，请在该 Agent 中检查登录状态后重试（错误码 -32000）",
        ),
        (-32603, "Agent 请求失败（错误码 -32603）"),
    ] {
        let frame = wire::encode(json!({"jsonrpc":"2.0", "id":1, "error":{
            "code":code, "message":"upstream text TEST_PRIVATE_VALUE", "data":{"token":"TEST_PRIVATE_VALUE"}
        }})).unwrap();
        let Incoming::Response {
            result: Err(error), ..
        } = wire::parse(&frame).unwrap()
        else {
            panic!("expected error");
        };
        assert_eq!(error, expected);
    }
}

#[tokio::test(start_paused = true)]
async fn request_timeout_releases_pending_and_sends_request_cancellation() {
    let (process, _events, mut writes) = in_memory_process();
    let caller = Arc::clone(&process);
    let task = tokio::spawn(async move {
        caller
            .request("hang", json!({}), Duration::from_secs(1))
            .await
    });
    let frame: Value = serde_json::from_slice(&writes.recv().await.unwrap().bytes).unwrap();
    time::advance(Duration::from_secs(2)).await;
    assert!(task.await.unwrap().is_err());
    assert!(process.shared.pending.lock().unwrap().is_empty());
    let cancellation: Value = serde_json::from_slice(&writes.recv().await.unwrap().bytes).unwrap();
    assert_eq!(cancellation["method"], "$/cancel_request");
    assert_eq!(cancellation["params"]["requestId"], frame["id"]);
}

#[tokio::test]
async fn dropping_request_future_releases_pending_and_late_response_is_ignored() {
    let (process, _events, mut writes) = in_memory_process();
    let caller = Arc::clone(&process);
    let task = tokio::spawn(async move {
        caller
            .request("hang", json!({}), Duration::from_secs(60))
            .await
    });
    let frame: Value = serde_json::from_slice(&writes.recv().await.unwrap().bytes).unwrap();
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert!(process.shared.pending.lock().unwrap().is_empty());
    let id = RequestId::parse(&frame["id"]).unwrap();
    io_tasks::dispatch_message(
        &process.shared,
        Incoming::Response {
            id,
            result: Ok(json!({"late":true})),
        },
    )
    .unwrap();
    assert!(!process.is_closed());
}

#[tokio::test]
async fn closing_connection_fails_every_pending_request() {
    let (process, mut events, mut writes) = in_memory_process();
    let mut calls = Vec::new();
    for _ in 0..3 {
        let caller = Arc::clone(&process);
        calls.push(tokio::spawn(async move {
            caller
                .request("hang", json!({}), Duration::from_secs(60))
                .await
        }));
        writes.recv().await.unwrap();
    }
    process.shared.close("protocol closed");
    for call in calls {
        assert_eq!(call.await.unwrap().unwrap_err(), "protocol closed");
    }
    assert!(matches!(events.recv().await, Some(AcpEvent::Closed { .. })));
}

#[tokio::test]
async fn pending_request_budget_rejects_new_calls_without_writing_them() {
    let (process, _events, mut writes) = in_memory_process();
    let mut calls = Vec::new();
    for _ in 0..MAX_PENDING_REQUESTS {
        let caller = Arc::clone(&process);
        calls.push(tokio::spawn(async move {
            caller
                .request("hang", json!({}), Duration::from_secs(60))
                .await
        }));
        writes.recv().await.unwrap();
    }
    assert!(
        process
            .request("over-budget", json!({}), Duration::from_secs(5))
            .await
            .is_err()
    );
    assert!(writes.try_recv().is_err());
    process.shared.close("budget cleanup");
    for call in calls {
        assert!(call.await.unwrap().is_err());
    }
}

#[test]
fn reverse_request_ids_cannot_be_reused_and_have_a_bounded_pending_set() {
    let (process, _events, _writes) = in_memory_process();
    let request = |number| Incoming::Request {
        id: RequestId::Number(number),
        method: "session/request_permission".into(),
        params: json!({}),
    };
    io_tasks::dispatch_message(&process.shared, request(0)).unwrap();
    assert!(io_tasks::dispatch_message(&process.shared, request(0)).is_err());
    for number in 1..MAX_AGENT_REQUESTS {
        io_tasks::dispatch_message(&process.shared, request(number as i64)).unwrap();
    }
    assert!(io_tasks::dispatch_message(&process.shared, request(100)).is_err());
    assert_eq!(
        process.shared.agent_requests.lock().unwrap().len(),
        MAX_AGENT_REQUESTS
    );
}

#[tokio::test]
async fn a_full_event_queue_still_delivers_one_final_closed_event() {
    let (process, mut events, _writes) = in_memory_process();
    for _ in 0..EVENT_QUEUE_CAPACITY - 1 {
        process
            .shared
            .publish(AcpEvent::Notification {
                method: "session/update".into(),
                params: json!({}),
            })
            .unwrap();
    }
    assert!(
        process
            .shared
            .publish(AcpEvent::Notification {
                method: "session/update".into(),
                params: json!({})
            })
            .is_err()
    );
    process.shared.close("second close");
    for _ in 0..EVENT_QUEUE_CAPACITY - 1 {
        assert!(matches!(
            events.recv().await,
            Some(AcpEvent::Notification { .. })
        ));
    }
    assert!(matches!(events.recv().await, Some(AcpEvent::Closed { .. })));
    assert!(events.try_recv().is_err());
}

#[tokio::test]
async fn launch_rejects_explicit_interpreter_injection_case_insensitively() {
    let launch = AcpLaunch {
        executable: env::current_exe().unwrap(),
        args: Vec::new(),
        cwd: env::current_dir().unwrap(),
        env: BTreeMap::from([("node_options".into(), "--require=unexpected.js".into())]),
    };
    let error = AcpProcess::spawn(launch)
        .await
        .err()
        .expect("must reject before spawning");
    assert!(error.contains("注入环境变量"));
}

fn find_node() -> Option<PathBuf> {
    let name = if cfg!(windows) { "node.exe" } else { "node" };
    env::split_paths(&env::var_os("PATH")?)
        .map(|path| path.join(name))
        .filter(|path| path.is_file())
        .find_map(|path| path.canonicalize().ok())
}

async fn fake_process(
    mode: &str,
) -> (Arc<AcpProcess>, mpsc::Receiver<AcpEvent>, tempfile::TempDir) {
    let node =
        find_node().expect("Node.js 未在 PATH 中；此用例需要真实 Node 子进程，不能以跳过代替通过");
    let cwd = tempfile::tempdir().unwrap();
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/assistant/acp/fixtures/fake-agent.cjs");
    let launch = AcpLaunch {
        executable: node,
        args: vec![fixture.to_string_lossy().into_owned(), mode.into()],
        env: BTreeMap::new(),
        cwd: cwd.path().to_owned(),
    };
    let (process, events) = AcpProcess::spawn(launch)
        .await
        .expect("cannot launch the Node protocol fixture");
    (process, events, cwd)
}

async fn next_event(events: &mut mpsc::Receiver<AcpEvent>) -> AcpEvent {
    time::timeout(Duration::from_secs(5), events.recv())
        .await
        .unwrap()
        .expect("event channel ended")
}

#[tokio::test]
#[ignore = "需要 PATH 中的 Node.js；使用 --include-ignored 验证真实子进程协议"]
async fn node_fixture_completes_initialize_session_prompt_and_notification_roundtrip() {
    let (process, mut events, _cwd) = fake_process("normal").await;
    let initialized = process
        .request(
            "initialize",
            json!({"protocolVersion":1, "clientCapabilities":{}}),
            Duration::from_secs(5),
        )
        .await
        .unwrap();
    assert_eq!(initialized["protocolVersion"], 1);
    let servers =
        json!([{"name":"ncd-test", "type":"http", "url":"http://127.0.0.1:1/mcp", "headers":[]}]);
    let session = process
        .request(
            "session/new",
            json!({"cwd":_cwd.path(), "mcpServers":servers}),
            Duration::from_secs(5),
        )
        .await
        .unwrap();
    assert_eq!(session["mcpServers"], servers);
    let response = process
        .request(
            "session/prompt",
            json!({"sessionId":"fake-session", "prompt":[{"type":"text", "text":"hello"}]}),
            Duration::from_secs(5),
        )
        .await
        .unwrap();
    assert_eq!(response["stopReason"], "end_turn");
    let AcpEvent::Notification { method, params } = next_event(&mut events).await else {
        panic!("missing streamed notification");
    };
    assert_eq!(method, "session/update");
    assert_eq!(params["update"]["content"]["text"], "本地协议夹具回复");
    process.shutdown().await;
}

#[tokio::test]
#[ignore = "需要 PATH 中的 Node.js；使用 --include-ignored 验证真实子进程协议"]
async fn node_fixture_reverse_permission_does_not_block_the_original_response() {
    let (process, mut events, _cwd) = fake_process("normal").await;
    let result = process
        .request("permission-roundtrip", json!({}), Duration::from_secs(5))
        .await
        .unwrap();
    assert_eq!(result["awaitingPermission"], true);
    let AcpEvent::Request { id, method, .. } = next_event(&mut events).await else {
        panic!("missing reverse request");
    };
    assert_eq!(method, "session/request_permission");
    process
        .respond(
            id.clone(),
            json!({"outcome":{"outcome":"selected", "optionId":"once"}}),
        )
        .await
        .unwrap();
    assert!(process.respond(id, json!({})).await.is_err());
    let result = process
        .request("last-permission", json!({}), Duration::from_secs(5))
        .await
        .unwrap();
    assert_eq!(result["permission"]["outcome"]["optionId"], "once");
    process.shutdown().await;
}

#[tokio::test]
#[ignore = "需要 PATH 中的 Node.js；使用 --include-ignored 验证真实子进程协议"]
async fn node_fixture_ignores_a_late_response_and_uses_a_new_request_id() {
    let (process, _events, _cwd) = fake_process("ignore-cancel").await;
    let first = process
        .request("echo", json!({}), Duration::from_secs(5))
        .await
        .unwrap();
    assert!(
        process
            .request("delay", json!({"delayMs":100}), Duration::from_millis(20))
            .await
            .is_err()
    );
    time::sleep(Duration::from_millis(200)).await;
    let next = process
        .request("echo", json!({"afterTimeout":true}), Duration::from_secs(5))
        .await
        .unwrap();
    assert!(next["requestId"].as_i64().unwrap() > first["requestId"].as_i64().unwrap());
    assert_eq!(next["params"]["afterTimeout"], true);
    assert!(process.shared.pending.lock().unwrap().is_empty());
    process.shutdown().await;
}

#[tokio::test]
#[ignore = "需要 PATH 中的 Node.js；使用 --include-ignored 验证真实子进程协议"]
async fn node_fixture_eof_fails_all_outstanding_calls() {
    let (process, mut events, _cwd) = fake_process("exit-after-two").await;
    let mut calls = Vec::new();
    for _ in 0..2 {
        let caller = Arc::clone(&process);
        calls.push(tokio::spawn(async move {
            caller
                .request("wait", json!({}), Duration::from_secs(5))
                .await
        }));
    }
    for call in calls {
        assert!(call.await.unwrap().is_err());
    }
    assert!(matches!(
        next_event(&mut events).await,
        AcpEvent::Closed { .. }
    ));
    assert!(process.is_closed());
    process.shutdown().await;
}

#[tokio::test]
#[ignore = "需要 PATH 中的 Node.js；使用 --include-ignored 验证真实子进程协议"]
async fn node_fixture_final_response_is_drained_before_process_exit() {
    let (process, mut events, _cwd) = fake_process("normal").await;
    let response = process
        .request("final", json!({}), Duration::from_secs(5))
        .await
        .unwrap();
    assert_eq!(response["final"], true);
    assert!(matches!(
        next_event(&mut events).await,
        AcpEvent::Closed { .. }
    ));
    process.shutdown().await;
}

#[tokio::test]
#[ignore = "需要 PATH 中的 Node.js；使用 --include-ignored 验证真实子进程协议"]
async fn node_fixture_oversized_output_is_rejected_without_forwarding_it() {
    let (process, mut events, _cwd) = fake_process("normal").await;
    assert!(
        process
            .request("oversized", json!({}), Duration::from_secs(5))
            .await
            .is_err()
    );
    let AcpEvent::Closed { reason } = next_event(&mut events).await else {
        panic!("expected connection closure");
    };
    assert!(reason.contains("限制"));
    assert!(reason.len() < 128);
    process.shutdown().await;
}

#[tokio::test]
#[ignore = "需要 PATH 中的 Node.js；使用 --include-ignored 验证真实子进程协议"]
async fn node_fixture_stderr_never_reaches_the_event_stream() {
    let (process, mut events, _cwd) = fake_process("normal").await;
    assert!(
        process
            .request("stderr-exit", json!({}), Duration::from_secs(5))
            .await
            .is_err()
    );
    let AcpEvent::Closed { reason } = next_event(&mut events).await else {
        panic!("stderr was forwarded as an event");
    };
    assert!(!reason.contains("TEST_PRIVATE_VALUE"));
    assert!(!reason.contains("do-not-display"));
    process.shutdown().await;
}

#[cfg(windows)]
#[expect(
    unsafe_code,
    reason = "Test-only ownership transfer of a query handle for the spawned fixture process"
)]
fn open_process_for_exit_wait(pid: u32) -> std::os::windows::io::OwnedHandle {
    use std::os::windows::io::{FromRawHandle, OwnedHandle};
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, SYNCHRONIZATION_SYNCHRONIZE,
    };
    // SAFETY: 仅为夹具返回的进程申请查询与等待权限，句柄不继承。
    let raw = unsafe {
        OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZATION_SYNCHRONIZE,
            0,
            pid,
        )
    };
    assert!(!raw.is_null(), "cannot open fixture child process");
    // SAFETY: OpenProcess 成功，OwnedHandle 唯一持有返回句柄。
    unsafe { OwnedHandle::from_raw_handle(raw) }
}

#[cfg(windows)]
#[expect(
    unsafe_code,
    reason = "Test-only wait on an owned Windows process handle to verify Job termination"
)]
fn assert_process_exited(handle: &std::os::windows::io::OwnedHandle) {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::{Foundation::WAIT_OBJECT_0, System::Threading::WaitForSingleObject};
    // SAFETY: 测试持有原进程句柄，不会受 PID 重用影响。
    assert_eq!(
        unsafe { WaitForSingleObject(handle.as_raw_handle(), 5000) },
        WAIT_OBJECT_0
    );
}

#[cfg(windows)]
#[tokio::test]
#[ignore = "需要 Windows 与 PATH 中的 Node.js；使用 --include-ignored 验证 Job 进程树"]
async fn windows_shutdown_terminates_the_spawned_grandchild_in_the_same_job() {
    let (process, _events, _cwd) = fake_process("spawn-child").await;
    let child = process
        .request("child-pid", json!({}), Duration::from_secs(5))
        .await
        .unwrap();
    let pid = u32::try_from(child["pid"].as_u64().unwrap()).unwrap();
    let handle = open_process_for_exit_wait(pid);
    process.shutdown().await;
    assert_process_exited(&handle);
}

#[cfg(windows)]
#[tokio::test]
#[ignore = "需要 Windows 与 PATH 中的 Node.js；使用 --include-ignored 验证 Job 进程树"]
async fn windows_dropping_the_last_handle_terminates_the_spawned_grandchild() {
    let (process, _events, _cwd) = fake_process("spawn-child").await;
    let child = process
        .request("child-pid", json!({}), Duration::from_secs(5))
        .await
        .unwrap();
    let pid = u32::try_from(child["pid"].as_u64().unwrap()).unwrap();
    let handle = open_process_for_exit_wait(pid);
    let stopped = process.shared.stopped.clone();
    drop(process);
    let exited = tokio::task::spawn_blocking(move || assert_process_exited(&handle));
    time::timeout(Duration::from_secs(6), exited)
        .await
        .unwrap()
        .unwrap();
    time::timeout(Duration::from_secs(6), stopped.cancelled())
        .await
        .unwrap();
}
