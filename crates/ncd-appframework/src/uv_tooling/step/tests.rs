use super::*;

fn policy() -> MonitorPolicy {
    MonitorPolicy {
        heartbeat: Duration::from_millis(10),
        quiet_warning: Duration::from_millis(25),
        timeout: Duration::from_secs(60),
    }
}

fn success() -> CommandOutput {
    CommandOutput {
        exit_code: Some(0),
        stdout: String::new(),
        stderr: String::new(),
    }
}

#[test]
fn uv_commands_report_resolution_and_bound_network_and_lock_waits() {
    let (ctx, _rx) = ActionCtx::new();
    let command = prepare_command(
        HostCommand::new("/root/ncd/tools/uv/uv").args(["pip", "install", "neobot-app"]),
        &ctx,
        Duration::from_secs(2400),
    );
    assert_eq!(command.args, ["--verbose", "pip", "install", "neobot-app"]);
    assert_eq!(command.environment["UV_HTTP_TIMEOUT"], "30");
    assert_eq!(command.environment["UV_LOCK_TIMEOUT"], "120");
    assert_eq!(command.timeout, Some(Duration::from_secs(2400)));
    assert!(command.cancel.is_some());
}

#[test]
fn framework_initializers_keep_their_command_line() {
    let (ctx, _rx) = ActionCtx::new();
    let command = prepare_command(
        HostCommand::new("/app/.venv/bin/astrbot").args(["init", "-y"]),
        &ctx,
        Duration::from_secs(60),
    );
    assert_eq!(command.args, ["init", "-y"]);
    assert!(!command.environment.contains_key("UV_HTTP_TIMEOUT"));
}

#[test]
fn diagnostic_urls_hide_credentials_and_signed_queries() {
    let result = redact_line(
        "DEBUG GET https://alice:secret@private.example/simple/pkg/?token=secret#fragment and http://user:pass@localhost:9000/",
    );
    assert_eq!(
        result,
        "DEBUG GET https://private.example/simple/pkg/ and http://localhost:9000/"
    );
    for secret in ["alice", "secret", "fragment", "user:", "pass"] {
        assert!(!result.contains(secret));
    }
}

#[tokio::test]
async fn silent_commands_keep_reporting_and_explain_the_wait() {
    let (ctx, mut progress) = ActionCtx::new();
    let (_tx, rx) = mpsc::channel(64);
    let run = async {
        tokio::time::sleep(Duration::from_millis(65)).await;
        Ok(success())
    };
    let output = monitor_output(&ctx, 4, "安装 neobot-app", run, rx, policy())
        .await
        .unwrap();
    assert!(output.success());
    let mut saw_wait = false;
    let mut warnings = 0;
    while let Ok(event) = progress.try_recv() {
        match event.kind {
            ProgressKind::StepProgress { message, .. } => {
                saw_wait |= message.contains("无新输出");
            }
            ProgressKind::Log {
                level: ncd_component::ProgressLogLevel::Warn,
                message,
            } => {
                warnings += 1;
                assert!(message.contains("Python 包源"));
            }
            _ => {}
        }
    }
    assert!(saw_wait);
    assert_eq!(warnings, 1, "同一次静默只提醒一次");
}

#[tokio::test]
async fn command_completion_drains_all_queued_diagnostics() {
    let (ctx, mut progress) = ActionCtx::with_capacity(64);
    let (tx, rx) = mpsc::channel(64);
    let run = async move {
        for line in [
            "DEBUG Solving dependencies",
            "error: package unavailable",
            "last cause",
        ] {
            tx.try_send(line.to_string()).unwrap();
        }
        Ok(success())
    };
    monitor_output(&ctx, 4, "安装", run, rx, policy())
        .await
        .unwrap();
    let mut lines = Vec::new();
    while let Ok(event) = progress.try_recv() {
        if let ProgressKind::Log { message, .. } = event.kind {
            lines.push(message);
        }
    }
    assert_eq!(
        lines,
        [
            "DEBUG Solving dependencies",
            "error: package unavailable",
            "last cause"
        ]
    );
}

#[tokio::test]
async fn cancellation_remains_cancellation_instead_of_an_install_error() {
    let (ctx, _progress) = ActionCtx::new();
    let (_tx, rx) = mpsc::channel(64);
    let result = monitor_output(
        &ctx,
        4,
        "安装",
        async { Err(HostError::Cancelled) },
        rx,
        policy(),
    )
    .await;
    assert!(matches!(result, Err(ActionError::Cancelled)));
}

#[tokio::test]
async fn full_progress_queue_cannot_block_command_cancellation() {
    let (ctx, _blocked_consumer) = ActionCtx::with_capacity(1);
    let (tx, rx) = mpsc::channel(64);
    let cancel = ctx.cancel_token();
    let trigger = ctx.clone();
    let run = async move {
        for _ in 0..10 {
            tx.try_send("DEBUG Resolving dependencies".to_string())
                .unwrap();
        }
        cancel.cancelled().await;
        Err(HostError::Cancelled)
    };
    let cancel_task = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(30)).await;
        trigger.cancel();
    });
    let result = tokio::time::timeout(
        Duration::from_secs(1),
        monitor_output(&ctx, 4, "安装", run, rx, policy()),
    )
    .await
    .expect("进度队列满时仍应及时取消 Host 命令");
    cancel_task.await.unwrap();
    assert!(matches!(result, Err(ActionError::Cancelled)));
}

#[tokio::test]
async fn timeout_includes_the_last_phase_and_recovery_action() {
    let (ctx, _progress) = ActionCtx::new();
    let (tx, rx) = mpsc::channel(64);
    let run = async move {
        tx.try_send("DEBUG Waiting to acquire lock for /app/.venv".into())
            .unwrap();
        Err(HostError::Timeout { operation: "test" })
    };
    let result = monitor_output(&ctx, 4, "安装", run, rx, policy()).await;
    let error = result.unwrap_err().to_string();
    assert!(error.contains("等待安装锁"));
    assert!(error.contains("Python 包源"));
}
