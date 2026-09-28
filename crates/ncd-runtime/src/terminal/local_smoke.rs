//! 本机真 shell 的冒烟：标记脚本在 PowerShell / cmd / Git Bash 里真能报目录和退出码
//!
//! 要真起 shell（会读用户自己的 profile），默认不跑：
//!   cargo test -p ncd-runtime --lib terminal::local_smoke -- --ignored --nocapture

use std::collections::BTreeMap;
use std::time::Duration;

use bytes::Bytes;
use ncd_domain::LocalShellKind;
use ncd_host::local::LocalWindowsHost;
use ncd_host::{Host, HostPath, PtyProgram, PtyRequest, PtySession, PtySize};

use super::integration::git_bash_rc;
use super::shells::{detect_local_shells, launch_for};

async fn read_until(session: &mut PtySession, needle: &str) -> String {
    let mut all = Vec::new();
    let _ = tokio::time::timeout(Duration::from_secs(25), async {
        while let Some(chunk) = session.output.recv().await {
            all.extend_from_slice(&chunk);
            if String::from_utf8_lossy(&all).contains(needle) {
                break;
            }
        }
    })
    .await;
    String::from_utf8_lossy(&all).into_owned()
}

async fn open(kind: LocalShellKind, rc: Option<&str>) -> Option<PtySession> {
    let shell = detect_local_shells().into_iter().find(|s| s.kind == kind)?;
    let launch = launch_for(&shell, rc);
    let mut request = PtyRequest::new(
        PtyProgram::Program {
            program: launch.program,
            args: launch.args,
        },
        PtySize::new(120, 30),
    );
    let mut env = BTreeMap::new();
    env.extend(launch.env);
    request.env = env;
    request.cwd = Some(HostPath::from_windows(
        &std::env::temp_dir().to_string_lossy(),
    ));
    LocalWindowsHost::new().open_pty(request).await.ok()
}

/// `cwd_mark` 要写完整的序列正文：回显里本来就有目录本身，只等目录会在标记出来之前就返回
async fn check_marks(mut session: PtySession, cd: &str, cwd_mark: &str, fail: Option<&str>) {
    let first = read_until(&mut session, "633;B").await;
    assert!(first.contains("633;A"), "no prompt mark: {first:?}");
    session
        .control
        .write(Bytes::from(format!("{cd}\r")))
        .unwrap();
    let after_cd = read_until(&mut session, cwd_mark).await;
    assert!(after_cd.contains(cwd_mark), "no cwd mark: {after_cd:?}");
    if let Some(fail) = fail {
        session
            .control
            .write(Bytes::from(format!("{fail}\r")))
            .unwrap();
        let after_fail = read_until(&mut session, "633;D;5").await;
        assert!(
            after_fail.contains("633;D;5"),
            "no exit mark: {after_fail:?}"
        );
    }
    session.control.close();
}

#[tokio::test]
#[ignore]
async fn powershell_reports_cwd_and_exit_code() {
    for kind in [LocalShellKind::Pwsh, LocalShellKind::WindowsPowershell] {
        let Some(session) = open(kind, None).await else {
            continue;
        };
        // 633 的目录里反斜杠转义成两个
        check_marks(
            session,
            "Set-Location C:\\Windows",
            "633;P;Cwd=C:\\\\Windows",
            Some("cmd /c exit 5"),
        )
        .await;
    }
}

#[tokio::test]
#[ignore]
async fn cmd_reports_cwd() {
    let Some(session) = open(LocalShellKind::Cmd, None).await else {
        return;
    };
    check_marks(session, "cd /d C:\\Windows", "9;9;C:\\Windows", None).await;
}

#[tokio::test]
#[ignore]
async fn git_bash_reports_cwd_and_exit_code() {
    let dir = tempfile::tempdir().unwrap();
    let rc = dir.path().join("ncd-bashrc.sh");
    std::fs::write(&rc, git_bash_rc()).unwrap();
    let rc_posix = HostPath::from_windows(&rc.to_string_lossy())
        .as_posix()
        .to_string();
    let Some(session) = open(LocalShellKind::GitBash, Some(&rc_posix)).await else {
        return;
    };
    check_marks(
        session,
        "cd /c/Windows",
        "633;P;Cwd=/c/Windows",
        Some("sh -c 'exit 5'"),
    )
    .await;
}
