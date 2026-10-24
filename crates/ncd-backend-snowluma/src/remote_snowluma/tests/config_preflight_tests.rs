use super::*;
use std::sync::atomic::Ordering;

fn paths() -> SnowLumaRemotePaths {
    SnowLumaRemotePaths::from_remote_home("/home/fixture")
}

fn prerequisite_paths(paths: &SnowLumaRemotePaths) -> [String; 4] {
    [
        format!("{}/runtime.json", paths.config_dir),
        format!("{}/webui.json", paths.config_dir),
        paths.webui_secret.clone(),
        paths.vnc_secret.clone(),
    ]
}

#[tokio::test]
async fn prerequisite_files_begin_concurrently_and_failed_read_cannot_generate_credentials() {
    let mut host = FileHost::new();
    let paths = paths();
    let expected = prerequisite_paths(&paths);
    host.read_barrier = Some(tokio::sync::Barrier::new(4));
    host.failures
        .lock()
        .unwrap()
        .insert(expected[0].clone(), ReadFailure::Denied);
    let result = tokio::time::timeout(
        Duration::from_secs(1),
        ensure_remote_daemon_prereqs(&host, "/home/fixture", &paths, "/opt/QQ/qq"),
    )
    .await
    .unwrap();
    assert!(result.is_err());
    let mut actual = host.reads.lock().unwrap().clone();
    let mut expected = expected.to_vec();
    actual.sort();
    expected.sort();
    assert_eq!(actual, expected);
    assert!(host.writes.lock().unwrap().is_empty());
    assert_eq!(host.directories.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn unreadable_prerequisite_is_never_treated_as_missing() {
    let paths = paths();
    for failure in [ReadFailure::Denied, ReadFailure::Timeout] {
        for path in prerequisite_paths(&paths) {
            let host = FileHost::new();
            host.failures.lock().unwrap().insert(path, failure);
            assert!(
                ensure_remote_daemon_prereqs(&host, "/home/fixture", &paths, "/opt/QQ/qq")
                    .await
                    .is_err()
            );
            assert!(host.writes.lock().unwrap().is_empty());
            assert_eq!(host.directories.load(Ordering::SeqCst), 0);
        }
    }
}

#[tokio::test(start_paused = true)]
async fn prerequisite_file_read_has_its_own_deadline() {
    let host = FileHost::new();
    host.failures
        .lock()
        .unwrap()
        .insert("/pending".into(), ReadFailure::Pending);
    let error = read_optional_config(&host, "/pending").await.unwrap_err();
    assert!(error.to_string().contains("超时"));
    assert!(host.writes.lock().unwrap().is_empty());
}

#[tokio::test]
async fn cancellation_interrupts_pending_prerequisite_reads_without_writes() {
    let host = FileHost::new();
    let paths = paths();
    host.failures.lock().unwrap().insert(
        format!("{}/runtime.json", paths.config_dir),
        ReadFailure::Pending,
    );
    let cancel = tokio_util::sync::CancellationToken::new();
    let prepare = ensure_remote_daemon_prereqs_cancellable(
        &host,
        "/home/fixture",
        &paths,
        "/opt/QQ/qq",
        Some(&cancel),
    );
    tokio::pin!(prepare);
    tokio::select! {
        biased;
        result = &mut prepare => panic!("read unexpectedly completed: {result:?}"),
        _ = std::future::ready(()) => {}
    }
    assert_eq!(host.reads.lock().unwrap().len(), 4);
    cancel.cancel();
    let error = tokio::time::timeout(Duration::from_secs(1), prepare)
        .await
        .expect("cancellation must not await the file deadline")
        .unwrap_err();
    assert!(error.to_string().contains("取消"));
    assert!(host.writes.lock().unwrap().is_empty());
    assert_eq!(host.directories.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn cancellation_during_credential_commit_finishes_the_pair_and_returns_restart_evidence() {
    let cancel = tokio_util::sync::CancellationToken::new();
    let mut host = FileHost::new();
    host.cancel_on_write = Some(cancel.clone());
    let paths = paths();
    let result = sync_loaded_webui_credentials(&host, &paths, None, String::new(), Some(&cancel))
        .await
        .unwrap();
    assert!(cancel.is_cancelled());
    assert!(result.wrote);
    assert_eq!(
        host.writes.lock().unwrap().as_slice(),
        [
            paths.webui_secret.clone(),
            format!("{}/webui.json", paths.config_dir),
        ]
    );
}

#[tokio::test]
async fn webui_secret_is_read_once_and_existing_non_bootstrap_auth_is_not_overwritten() {
    let host = FileHost::new();
    let paths = paths();
    host.files.lock().unwrap().insert(format!("{}/webui.json", paths.config_dir), br#"{"passwordHash":"fixture-hash","passwordSalt":"fixture-salt","mustChangePassword":false}"#.to_vec());
    host.files
        .lock()
        .unwrap()
        .insert(paths.webui_secret.clone(), Vec::new());
    let result = sync_remote_webui_credentials(&host, &paths).await.unwrap();
    assert!(!result.wrote);
    assert_eq!(
        host.reads
            .lock()
            .unwrap()
            .iter()
            .filter(|path| *path == &paths.webui_secret)
            .count(),
        1
    );
    assert!(host.writes.lock().unwrap().is_empty());
}

#[test]
fn invalid_nonempty_webui_config_cannot_be_replaced_as_an_empty_bootstrap() {
    for bytes in [
        b"broken".as_slice(),
        b"[]",
        br#"{"passwordHash":1,"passwordSalt":"x"}"#,
        br#"{"passwordHash":"nonempty"}"#,
    ] {
        assert!(parse_webui_disk_creds(Some(bytes)).is_err());
    }
    assert!(parse_webui_disk_creds(None).unwrap().is_none());
    assert!(parse_webui_disk_creds(Some(b" ")).unwrap().is_none());
}

#[tokio::test]
async fn onebot_config_preserves_unknown_fields_and_skips_unchanged_writes_and_mkdir() {
    let host = FileHost::new();
    let paths = paths();
    let config = ncd_test_support::BotConfigBuilder::new()
        .backend_type(BackendType::SnowLuma)
        .qq_id(10001)
        .build();
    let bot = BotId::new("10001");
    let path = format!("{}/onebot_10001.json", paths.config_dir);
    host.files.lock().unwrap().insert(
        path.clone(),
        br#"{"fixtureExisting":{"nested":true}}"#.to_vec(),
    );
    render_native_snowluma_config_on_host(&host, &bot, &config, &paths)
        .await
        .unwrap();
    assert_eq!(host.writes.lock().unwrap().as_slice(), [path.clone()]);
    let payload: Value = serde_json::from_slice(&host.files.lock().unwrap()[&path]).unwrap();
    assert_eq!(payload["fixtureExisting"]["nested"], Value::Bool(true));
    render_native_snowluma_config_on_host(&host, &bot, &config, &paths)
        .await
        .unwrap();
    assert_eq!(host.writes.lock().unwrap().len(), 1);
    assert_eq!(host.directories.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn onebot_read_failures_and_invalid_json_never_overwrite_existing_configuration() {
    let paths = paths();
    let config = ncd_test_support::BotConfigBuilder::new()
        .backend_type(BackendType::SnowLuma)
        .qq_id(10001)
        .build();
    let bot = BotId::new("10001");
    let path = format!("{}/onebot_10001.json", paths.config_dir);
    for failure in [ReadFailure::Denied, ReadFailure::Timeout] {
        let host = FileHost::new();
        host.failures.lock().unwrap().insert(path.clone(), failure);
        assert!(
            render_native_snowluma_config_on_host(&host, &bot, &config, &paths)
                .await
                .is_err()
        );
        assert!(host.writes.lock().unwrap().is_empty());
    }
    let host = FileHost::new();
    host.files
        .lock()
        .unwrap()
        .insert(path.clone(), b"broken".to_vec());
    assert!(
        render_native_snowluma_config_on_host(&host, &bot, &config, &paths)
            .await
            .is_err()
    );
    assert_eq!(host.files.lock().unwrap()[&path], b"broken");
    assert!(host.writes.lock().unwrap().is_empty());
}
