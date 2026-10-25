use super::*;

fn policy() -> MonitorPolicy {
    MonitorPolicy {
        heartbeat: Duration::from_millis(10),
        quiet_warning: Duration::from_millis(25),
        timeout: Duration::from_secs(60),
        flush_interval: Duration::from_millis(10),
    }
}

fn success() -> CommandOutput {
    CommandOutput {
        exit_code: Some(0),
        stdout: String::new(),
        stderr: String::new(),
    }
}

fn completion_policy() -> MonitorPolicy {
    MonitorPolicy {
        heartbeat: Duration::from_secs(60),
        flush_interval: Duration::from_secs(60),
        ..policy()
    }
}

#[test]
fn uv_commands_use_normal_output_and_bound_network_and_lock_waits() {
    let (ctx, _rx) = ActionCtx::new();
    let command = prepare_command(
        HostCommand::new("/root/ncd/tools/uv/uv").args(["pip", "install", "neobot-app"]),
        &ctx,
        Duration::from_secs(2400),
    );
    assert_eq!(command.args, ["pip", "install", "neobot-app"]);
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
            lines.extend(message.lines().map(str::to_string));
        }
    }
    assert_eq!(lines, ["error: package unavailable", "last cause"]);
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
            tx.try_send("Resolved 1 package".to_string()).unwrap();
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
        tx.try_send("WARN Waiting to acquire lock for /app/.venv".into())
            .unwrap();
        Err(HostError::Timeout { operation: "test" })
    };
    let result = monitor_output(&ctx, 4, "安装", run, rx, policy()).await;
    let error = result.unwrap_err().to_string();
    assert!(error.contains("等待安装锁"));
    assert!(error.contains("Python 包源"));
}

#[test]
fn diagnostic_and_package_detail_lines_are_filtered_without_hiding_errors() {
    for line in [
        "DEBUG Resolving dependencies",
        "TRACE request headers",
        "2026-10-25T12:00:00Z DEBUG uv_client: cache miss",
        "\x1b[2mDEBUG\x1b[0m selecting package",
        "[DEBUG] downloaded metadata",
        " + anyio==4.0.0",
        "\x1b[32m + anyio==4.0.0\x1b[0m",
        " - old-package==1.0.0",
        " ~ neobot-app @ https://example.com/package.whl",
    ] {
        assert!(!visible_line(line), "不应发布：{line}");
    }
    for line in [
        "Resolved 41 packages in 3s",
        "Installed 41 packages in 2s",
        "warning: network unavailable",
        "error: package DEBUG is missing",
        "Traceback (most recent call last):",
        "- Failed to fetch metadata",
    ] {
        assert!(visible_line(line), "应保留：{line}");
    }
}

#[tokio::test]
async fn twenty_thousand_diagnostic_lines_publish_no_progress_or_log_events() {
    let (ctx, mut progress) = ActionCtx::new();
    let (tx, rx) = mpsc::channel(64);
    let run = async move {
        let mut relay = OutputRelay {
            tx,
            overflowed: false,
        };
        for i in 0..20_000 {
            relay.send(format!("DEBUG uv_resolver: selecting package {i}"));
            relay.send(format!(" + package-{i}==1.0.0"));
        }
        let mut result = success();
        result.stderr = "DEBUG cleanup complete\nTRACE request finished".into();
        Ok(result)
    };
    let output = monitor_output(&ctx, 4, "安装", run, rx, completion_policy())
        .await
        .unwrap();
    assert!(output.stderr.is_empty());
    assert!(progress.try_recv().is_err());
}

#[tokio::test]
async fn burst_output_is_bounded_and_keeps_error_header_and_tail() {
    let (ctx, mut progress) = ActionCtx::new();
    let (tx, rx) = mpsc::channel(2048);
    let run = async move {
        tx.try_send("error: failed to install neobot-app".into())
            .unwrap();
        for i in 0..1000 {
            let stage = if i % 2 == 0 {
                "Resolving"
            } else {
                "Downloading"
            };
            tx.try_send(format!("{stage} dependency {i}")).unwrap();
        }
        tx.try_send("Caused by: server refused the request".into())
            .unwrap();
        Ok(success())
    };
    monitor_output(&ctx, 4, "安装", run, rx, completion_policy())
        .await
        .unwrap();
    let mut events = Vec::new();
    let mut logs = Vec::new();
    while let Ok(event) = progress.try_recv() {
        if let ProgressKind::Log { message, .. } = &event.kind {
            logs.push(message.clone());
        }
        events.push(event);
    }
    assert!(events.len() <= 2, "同一突发只发布合并后的阶段和日志");
    assert_eq!(logs.len(), 1);
    assert!(logs[0].contains("error: failed to install neobot-app"));
    assert!(logs[0].contains("Caused by: server refused the request"));
    assert!(logs[0].len() < 2048);
}

#[tokio::test]
async fn burst_output_keeps_a_warning_before_normal_output() {
    let (ctx, mut progress) = ActionCtx::new();
    let (tx, rx) = mpsc::channel(128);
    let run = async move {
        tx.try_send("warning: using a fallback index".into())
            .unwrap();
        for i in 0..100 {
            tx.try_send(format!("Resolving dependency {i}")).unwrap();
        }
        Ok(success())
    };
    monitor_output(&ctx, 4, "安装", run, rx, completion_policy())
        .await
        .unwrap();
    let mut logs = Vec::new();
    while let Ok(event) = progress.try_recv() {
        if let ProgressKind::Log { message, .. } = event.kind {
            logs.push(message);
        }
    }
    assert_eq!(logs.len(), 1);
    assert!(logs[0].contains("warning: using a fallback index"));
    assert!(logs[0].contains("Resolving dependency 99"));
}
