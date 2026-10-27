use super::*;

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Instant;

struct ProbeFixture {
    directory: tempfile::TempDir,
}

impl ProbeFixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        std::fs::create_dir(directory.path().join("home")).unwrap();
        Self { directory }
    }

    fn path(&self, relative: &str) -> PathBuf {
        self.directory.path().join(relative)
    }

    fn posix(&self, relative: &str) -> String {
        let path = self.path(relative);
        #[cfg(windows)]
        return ncd_host::HostPath::from_windows(&path.to_string_lossy())
            .as_posix()
            .into();
        #[cfg(not(windows))]
        path.to_string_lossy().into_owned()
    }

    fn write(&self, relative: &str, contents: &[u8]) {
        let path = self.path(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }

    fn entry(&self, directory: &str, package: &[u8]) {
        self.write(&format!("{directory}/index.mjs"), b"// fixture entry\n");
        self.write(&format!("{directory}/package.json"), package);
    }

    fn bundled_node(&self, relative: &str) {
        self.write(
            relative,
            b"#!/bin/sh\necho UNEXPECTED_NODE_EXECUTION\nexit 97\n",
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(self.path(relative), std::fs::Permissions::from_mode(0o755))
                .unwrap();
        }
    }

    fn probe(
        &self,
        overrides: Option<&RemotePathOverrides>,
        previous: Option<&str>,
        process_directory: Option<&str>,
    ) -> RemoteInventory {
        let script = build_probe_script_with_previous(overrides, previous);
        let (functions, _) = script.split_once("\nif [ -n \"$OV_QQ_BASE\" ]").unwrap();
        // Run the emitted functions and SnowLuma candidate calls without scanning this machine.
        let calls = script
            .lines()
            .filter(|line| {
                line.starts_with("if [ -n \"$OV_SL\" ]")
                    || line.starts_with("emit_sl \"$HOME/")
                    || line.starts_with("if [ -n \"$PREV_SL\" ]")
            })
            .collect::<Vec<_>>()
            .join("\n");
        let process_call = process_directory
            .map(|directory| {
                format!(
                    "emit_sl {} process\n",
                    ncd_host::shell_single_quote(directory)
                )
            })
            .unwrap_or_default();
        let input = format!("{functions}\n{calls}\n{process_call}exit 0\n");
        let mut child = posix_shell()
            .arg("-s")
            .env("HOME", self.posix("home"))
            .env("LC_ALL", "C")
            .env("PATH", "/usr/bin:/bin")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("script regression requires POSIX sh (Git Bash on Windows), or NCD_TEST_SH");
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        let mut stdout = child.stdout.take().unwrap();
        let stdout_reader = std::thread::spawn(move || {
            let mut bytes = Vec::new();
            stdout.read_to_end(&mut bytes).unwrap();
            bytes
        });
        let mut stderr = child.stderr.take().unwrap();
        let stderr_reader = std::thread::spawn(move || {
            let mut bytes = Vec::new();
            stderr.read_to_end(&mut bytes).unwrap();
            bytes
        });
        let deadline = Instant::now() + Duration::from_secs(5);
        while child.try_wait().unwrap().is_none() {
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("SnowLuma inventory script exceeded five seconds");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let status = child.wait().unwrap();
        let stderr = stderr_reader.join().unwrap();
        assert!(status.success(), "{}", String::from_utf8_lossy(&stderr));
        let stdout = String::from_utf8(stdout_reader.join().unwrap()).unwrap();
        assert!(!stdout.contains("UNEXPECTED_NODE_EXECUTION"));
        inventory_from_stdout(&stdout, None, "2026-10-27T00:00:00Z").unwrap()
    }
}

fn posix_shell() -> Command {
    if let Some(shell) = std::env::var_os("NCD_TEST_SH") {
        return Command::new(shell);
    }
    #[cfg(windows)]
    for variable in ["ProgramW6432", "ProgramFiles", "ProgramFiles(x86)"] {
        if let Some(root) = std::env::var_os(variable) {
            let shell = std::path::Path::new(&root).join("Git/bin/bash.exe");
            if shell.is_file() {
                return Command::new(shell);
            }
        }
    }
    Command::new("sh")
}

#[test]
fn emitted_probe_finds_fresh_default_package_and_bundled_node_without_config() {
    let fixture = ProbeFixture::new();
    let directory = "home/snowluma-remote/workspace/snowluma";
    fixture.entry(
        directory,
        b"{\n  \"name\": \"@snowluma/runtime\",\n  \"version\": \"1.0.0\"\n}\n",
    );
    fixture.bundled_node(&format!("{directory}/node"));

    let inventory = fixture.probe(None, None, None);
    assert_eq!(
        inventory.selected.snowluma_dir,
        Some(fixture.posix(directory))
    );
    assert_eq!(
        inventory.selected.node_bin,
        Some(fixture.posix(&format!("{directory}/node")))
    );
    assert!(!fixture.path(&format!("{directory}/config")).exists());
}

#[test]
fn emitted_probe_prefers_fresh_override_and_reads_minified_top_level_name() {
    let fixture = ProbeFixture::new();
    fixture.entry(
        "home/snowluma-remote/workspace/snowluma",
        br#"{"name":"@snowluma/runtime"}"#,
    );
    fixture.entry(
        "custom/sl",
        br#"{"dependencies":{"name":"other"},"name" : "@snowluma/runtime"}"#,
    );
    fixture.bundled_node("custom/sl/node/bin/node");
    let overrides = RemotePathOverrides {
        snowluma_dir: Some(fixture.posix("custom/sl")),
        ..RemotePathOverrides::default()
    };

    let inventory = fixture.probe(Some(&overrides), None, None);
    assert_eq!(inventory.selected.snowluma_dir, overrides.snowluma_dir);
    assert_eq!(
        inventory.selected.node_bin,
        Some(fixture.posix("custom/sl/node/bin/node"))
    );
}

#[test]
fn emitted_probe_rediscovers_previous_fresh_package_without_running_process() {
    let fixture = ProbeFixture::new();
    fixture.entry("previous/sl", br#"{"name":"@snowluma/runtime"}"#);
    let previous = fixture.posix("previous/sl");

    let inventory = fixture.probe(None, Some(&previous), None);
    assert_eq!(
        inventory.selected.snowluma_dir.as_deref(),
        Some(previous.as_str())
    );
    assert_eq!(inventory.items[0].source, RemoteInventorySource::PathLookup);
}

#[test]
fn emitted_probe_keeps_legacy_runtime_and_webui_identity_markers() {
    for marker in ["runtime", "webui"] {
        let fixture = ProbeFixture::new();
        fixture.write("legacy/sl/index.mjs", b"// legacy entry");
        let contents: &[u8] = if marker == "runtime" {
            br#"{"webuiPort":5099}"#
        } else {
            b"{}"
        };
        fixture.write(&format!("legacy/sl/config/{marker}.json"), contents);
        let overrides = RemotePathOverrides {
            snowluma_dir: Some(fixture.posix("legacy/sl")),
            ..RemotePathOverrides::default()
        };

        let inventory = fixture.probe(Some(&overrides), None, None);
        assert_eq!(
            inventory.selected.snowluma_dir, overrides.snowluma_dir,
            "{marker}"
        );
    }
}

#[test]
fn emitted_probe_rejects_unrelated_process_cwd_and_invalid_package_identity() {
    for package in [
        br#"{"name":"other","dependency":{"name":"@snowluma/runtime"}}"#.as_slice(),
        br#"{"name":"@snowluma/runtime-other"}"#,
        br#"{"name":"@snowluma/runtime""#,
        br#"{"name":"@snowluma/runtime","name":"other"}"#,
        br#"{"name":"@snowluma/runtime",}"#,
        br#"{"name":"@snowluma/runtime"]"#,
    ] {
        let fixture = ProbeFixture::new();
        fixture.entry("project", package);
        fixture.bundled_node("project/node");
        fixture.write("project/config/onebot_12345.json", b"{}");
        let directory = fixture.posix("project");

        let inventory = fixture.probe(None, Some(&directory), Some(&directory));
        assert!(inventory.selected.snowluma_dir.is_none(), "{package:?}");
        assert!(inventory.selected.node_bin.is_none(), "{package:?}");
        assert!(inventory.bots.is_empty(), "{package:?}");
    }
}

#[test]
fn emitted_probe_requires_entry_even_when_package_identity_is_correct() {
    let fixture = ProbeFixture::new();
    fixture.write(
        "incomplete/package.json",
        br#"{"name":"@snowluma/runtime"}"#,
    );
    let overrides = RemotePathOverrides {
        snowluma_dir: Some(fixture.posix("incomplete")),
        ..RemotePathOverrides::default()
    };

    let inventory = fixture.probe(Some(&overrides), None, None);
    assert!(inventory.selected.snowluma_dir.is_none());
}

#[test]
fn emitted_probe_rejects_manifests_larger_than_transport_limit() {
    let fixture = ProbeFixture::new();
    let mut package = br#"{"name":"@snowluma/runtime"}"#.to_vec();
    package.resize(SNOWLUMA_MANIFEST_MAX_BYTES + 1, b' ');
    fixture.entry("oversized", &package);
    fixture.bundled_node("oversized/node");
    let overrides = RemotePathOverrides {
        snowluma_dir: Some(fixture.posix("oversized")),
        ..RemotePathOverrides::default()
    };

    let inventory = fixture.probe(Some(&overrides), None, None);
    assert!(inventory.items.is_empty());
}
