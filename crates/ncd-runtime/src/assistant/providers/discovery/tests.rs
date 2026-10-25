use super::*;

fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let temp = tempfile::tempdir().expect("fixture directory");
    let prefix = temp.path().join("npm prefix with spaces");
    let package = prefix.join("node_modules/@deepseek-ai/dsh");
    std::fs::create_dir_all(package.join("lib")).expect("package fixture");
    std::fs::write(
        prefix.join(if cfg!(windows) { "node.exe" } else { "node" }),
        b"fixture, never executed",
    )
    .expect("node fixture");
    (temp, prefix, package)
}

fn manifest(package: &Path, bin: serde_json::Value) {
    let contents = serde_json::json!({
        "name": "@deepseek-ai/dsh", "bin": bin,
        "scripts": {"prestart": "node --eval fixture-injection", "start": "arbitrary shell command"}
    });
    std::fs::write(package.join("package.json"), contents.to_string()).expect("manifest fixture");
}

#[tokio::test]
async fn resolves_known_npm_wrapper_to_declared_bin_without_executing_scripts() {
    let (_temp, prefix, package) = fixture();
    let entry = package.join("lib/bin.js");
    std::fs::write(&entry, b"throw new Error('fixture must never execute')")
        .expect("script fixture");
    manifest(&package, serde_json::json!({"dsh": "lib/bin.js"}));
    let wrapper = prefix.join("dsh.cmd");
    std::fs::write(&wrapper, b"arbitrary wrapper content that must not execute")
        .expect("wrapper fixture");
    let executable = resolve_file(AssistantProvider::Dsh, &wrapper)
        .await
        .expect("known package");
    let expected_entry = tokio::fs::canonicalize(&entry)
        .await
        .expect("canonical fixture");
    assert_eq!(
        executable.prefix_args,
        vec![node_script_argument(&expected_entry).expect("Node-compatible fixture path")]
    );
    assert_eq!(executable.display_path, expected_entry);
    assert_eq!(
        executable.program,
        tokio::fs::canonicalize(prefix.join(if cfg!(windows) { "node.exe" } else { "node" }))
            .await
            .expect("node path")
    );
    assert!(
        !executable
            .prefix_args
            .iter()
            .any(|argument| argument.contains("eval") || argument.contains("injection"))
    );
    let saved = resolve_file(AssistantProvider::Dsh, &executable.display_path)
        .await
        .expect("saved discovered path");
    assert_eq!(saved.prefix_args, executable.prefix_args);
}

#[tokio::test]
async fn rejects_npm_bin_traversal_wrong_package_and_unknown_wrappers() {
    let (temp, prefix, package) = fixture();
    std::fs::write(package.join("outside.js"), b"fixture").expect("outside fixture");
    for bin in [
        "../outside.js",
        "/absolute/injected.js",
        "lib/../../injected.js",
    ] {
        manifest(&package, serde_json::json!({"dsh": bin}));
        assert!(npm_entry(AssistantProvider::Dsh, &prefix).await.is_err());
    }
    std::fs::write(
        package.join("package.json"),
        r#"{"name":"unrelated-package","bin":{"dsh":"lib/bin.js"}}"#,
    )
    .expect("wrong package");
    assert!(npm_entry(AssistantProvider::Dsh, &prefix).await.is_err());
    let wrapper = temp.path().join("unknown.ps1");
    std::fs::write(&wrapper, b"Write-Output fixture").expect("unknown wrapper");
    assert!(
        resolve_file(AssistantProvider::Dsh, &wrapper)
            .await
            .is_err()
    );
    assert!(
        resolve_file(AssistantProvider::Custom, &wrapper)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn npm_metadata_cannot_turn_node_bin_into_arguments_or_another_command() {
    let (_temp, prefix, package) = fixture();
    for bin in ["--eval", "lib/bin.js --require injected.cjs", "lib/bin.cmd"] {
        manifest(&package, serde_json::json!({"dsh": bin}));
        if bin == "lib/bin.cmd" {
            std::fs::write(package.join(bin), b"fixture").expect("batch fixture");
        }
        assert!(npm_entry(AssistantProvider::Dsh, &prefix).await.is_err());
    }
    std::fs::write(package.join("package.json"), vec![b' '; 65537]).expect("oversized manifest");
    assert!(npm_entry(AssistantProvider::Dsh, &prefix).await.is_err());
}

#[cfg(unix)]
#[tokio::test]
async fn npm_entry_cannot_escape_package_through_symlink() {
    let (temp, prefix, package) = fixture();
    let outside = temp.path().join("outside.js");
    std::fs::write(&outside, b"fixture").expect("outside fixture");
    std::os::unix::fs::symlink(&outside, package.join("lib/bin.js")).expect("symlink fixture");
    manifest(&package, serde_json::json!({"dsh": "lib/bin.js"}));
    assert!(npm_entry(AssistantProvider::Dsh, &prefix).await.is_err());
}

#[tokio::test]
async fn version_output_memory_is_bounded_while_both_pipes_are_drained() {
    let bytes = vec![b'x'; 100_000];
    let mut stdout = bytes.as_slice();
    let mut stderr = bytes.as_slice();
    let (stdout_saved, stderr_saved) = tokio::join!(
        read_bounded(&mut stdout, VERSION_STDOUT_LIMIT),
        read_bounded(&mut stderr, VERSION_STDERR_LIMIT)
    );
    assert_eq!(
        stdout_saved.expect("stdout").len() + stderr_saved.expect("stderr").len(),
        4096
    );
    assert!(stdout.is_empty() && stderr.is_empty());
}

#[tokio::test(start_paused = true)]
async fn unresponsive_version_probe_reaches_its_deadline() {
    let started = time::Instant::now();
    let result = probe_deadline(std::future::pending::<Result<(), String>>()).await;
    assert!(matches!(result, Err(ProbeFailure::TimedOut)));
    assert_eq!(time::Instant::now() - started, Duration::from_secs(5));
    assert!(
        matches!(probe_deadline(async { Err::<(), _>("fixture".into()) }).await, Err(ProbeFailure::Failed(message)) if message == "fixture")
    );
}

#[test]
fn parses_versions_without_returning_surrounding_diagnostics() {
    for (output, expected) in [
        ("dsh 0.2.0-rc.2\n", "0.2.0-rc.2"),
        ("v2.1.1", "2.1.1"),
        ("Qwen Code 0.26.1 (build)", "0.26.1"),
        ("mcode version 0.6.9", "0.6.9"),
    ] {
        assert_eq!(extract_version(output).as_deref(), Some(expected));
    }
    assert!(extract_version("fixture-secret: not a version").is_none());
    assert!(extract_version("version 2.1").is_none());
}

#[cfg(windows)]
#[test]
fn node_arguments_simplify_only_typed_disk_and_unc_prefixes() {
    for (canonical, expected) in [
        (
            r"\\?\C:\Program Files\agent\bin.js",
            r"C:\Program Files\agent\bin.js",
        ),
        (
            r"\\?\UNC\server\share\agent\bin.js",
            r"\\server\share\agent\bin.js",
        ),
        (r"D:\agent\bin.js", r"D:\agent\bin.js"),
    ] {
        assert_eq!(
            node_script_argument(Path::new(canonical)).expect("Node path"),
            expected
        );
    }
    assert!(node_script_argument(Path::new(r"C:relative.js")).is_err());
    assert!(node_script_argument(Path::new(r"\\?\Volume{fixture}\bin.js")).is_err());
}

#[tokio::test]
async fn installed_node_can_read_a_canonical_synthetic_entry_without_network() {
    let temp = tempfile::tempdir().expect("Node regression fixture");
    let Some(node) = find_node(temp.path()).await else {
        return;
    };
    let directory = temp.path().join("Node entry ' 中 with spaces");
    tokio::fs::create_dir(&directory)
        .await
        .expect("entry directory");
    let entry = directory.join("version.cjs");
    tokio::fs::write(&entry, b"if (process.argv[2] !== '--version') process.exit(12); process.stdout.write('0.0.0-smoke\\n');\n").await.expect("synthetic entry");
    let entry = tokio::fs::canonicalize(entry)
        .await
        .expect("canonical entry");
    let executable = Executable {
        program: node,
        prefix_args: vec![node_script_argument(&entry).expect("Node-compatible argument")],
        display_path: entry,
    };
    assert_eq!(
        version(&executable)
            .await
            .expect("synthetic Node --version"),
        "0.0.0-smoke"
    );
}
