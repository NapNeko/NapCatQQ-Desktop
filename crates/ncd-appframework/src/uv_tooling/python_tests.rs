use std::collections::VecDeque;
use std::path::Path;
use std::sync::Mutex;

use bytes::Bytes;
use ncd_component::ProgressKind;
use ncd_host::shell::PowerShellShell;
use ncd_host::{Arch, ArchiveKind, CommandOutput, DirEntry, HostError, HostProcess, HostShell};

use super::*;

const LINK_ERROR: &str = "error: Failed to create Python minor version link directory\n  Caused by: 无法遍历该路径，因为它包含不受信任的装入点。 (os error 448)\n";
const PYTHON_PATH: &str = "C:\\Users\\test\\AppData\\Roaming\\uv\\python\\cpython-3.13.17-windows-x86_64-none\\python.exe\n";

struct ScriptedHost {
    locality: Locality,
    replies: Mutex<VecDeque<Result<CommandOutput, HostError>>>,
    commands: Mutex<Vec<HostCommand>>,
}

impl ScriptedHost {
    fn new(locality: Locality, replies: Vec<Result<CommandOutput, HostError>>) -> Self {
        Self {
            locality,
            replies: Mutex::new(replies.into()),
            commands: Mutex::new(Vec::new()),
        }
    }

    fn commands(&self) -> Vec<HostCommand> {
        self.commands.lock().unwrap().clone()
    }
}

#[async_trait::async_trait]
impl Host for ScriptedHost {
    fn os(&self) -> Os {
        Os::Windows
    }

    fn arch(&self) -> Arch {
        Arch::X86_64
    }

    fn locality(&self) -> Locality {
        self.locality
    }

    fn id(&self) -> &str {
        "python-test"
    }

    fn shell(&self) -> &dyn HostShell {
        &PowerShellShell
    }

    async fn read_file(&self, _: &HostPath) -> Result<Bytes, HostError> {
        panic!("Python recovery must not read files")
    }

    async fn write_file(&self, _: &HostPath, _: &[u8]) -> Result<(), HostError> {
        panic!("Python recovery must not write files")
    }

    async fn list_dir(&self, _: &HostPath) -> Result<Vec<DirEntry>, HostError> {
        panic!("Python recovery must use uv discovery")
    }

    async fn create_dir_all(&self, _: &HostPath) -> Result<(), HostError> {
        panic!("Python recovery must not create directories")
    }

    async fn remove_file(&self, _: &HostPath) -> Result<(), HostError> {
        panic!("Python recovery must not remove files")
    }

    async fn remove_dir_all(&self, _: &HostPath) -> Result<(), HostError> {
        panic!("Python recovery must not remove directories")
    }

    async fn exists(&self, _: &HostPath) -> Result<bool, HostError> {
        panic!("Python recovery must use uv discovery")
    }

    async fn upload(&self, _: &Path, _: &HostPath) -> Result<(), HostError> {
        panic!("An untrusted mount point must not trigger mirror upload")
    }

    async fn download(&self, _: &HostPath, _: &Path) -> Result<(), HostError> {
        panic!("An untrusted mount point must not trigger mirror download")
    }

    async fn extract_archive(
        &self,
        _: &HostPath,
        _: &HostPath,
        _: ArchiveKind,
    ) -> Result<(), HostError> {
        panic!("Python recovery must not extract archives")
    }

    async fn spawn(&self, _: HostCommand) -> Result<Box<dyn HostProcess>, HostError> {
        panic!("Python recovery must not start a persistent process")
    }

    async fn run_to_string(&self, command: HostCommand) -> Result<CommandOutput, HostError> {
        self.commands.lock().unwrap().push(command);
        self.replies
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected Python command")
    }
}

fn output(exit_code: i32, stdout: &str, stderr: &str) -> Result<CommandOutput, HostError> {
    Ok(CommandOutput {
        exit_code: Some(exit_code),
        stdout: stdout.into(),
        stderr: stderr.into(),
    })
}

async fn ensure(host: &dyn Host, ctx: &ActionCtx) -> Result<(), ActionError> {
    ensure_python(
        host,
        &HostPath::from_windows(r"C:\tools\uv.exe"),
        "3.13",
        &HostPath::from_windows(r"C:\apps\test\.ncd-python"),
        ctx,
        2,
    )
    .await
}

fn assert_recovery_commands(host: &ScriptedHost) {
    let commands = host.commands();
    assert_eq!(
        commands
            .iter()
            .map(|cmd| cmd.args.clone())
            .collect::<Vec<_>>(),
        [
            vec!["python", "find", "3.13"],
            vec!["python", "install", "3.13"],
            vec!["python", "find", "3.13"],
        ]
    );
    for command in &commands {
        assert_eq!(command.program, commands[0].program);
        assert_eq!(command.environment["UV_NO_PROGRESS"], "1");
        assert_eq!(command.environment["UV_PYTHON_INSTALL_BIN"], "0");
        assert_eq!(command.environment["UV_PYTHON_INSTALL_REGISTRY"], "0");
        assert!(command.cancel.is_some());
    }
    assert_eq!(commands[2].timeout, Some(Duration::from_secs(30)));
}

#[tokio::test]
async fn python_448_reuses_discoverable_interpreter_on_local_and_remote_windows() {
    for locality in [Locality::Local, Locality::Remote] {
        let host = ScriptedHost::new(
            locality,
            vec![
                output(1, "", "not found"),
                output(2, "", LINK_ERROR),
                output(0, PYTHON_PATH, ""),
            ],
        );
        let (ctx, mut progress) = ActionCtx::new();

        ensure(&host, &ctx).await.unwrap();

        assert_recovery_commands(&host);
        let mut announced_continuation = false;
        while let Ok(event) = progress.try_recv() {
            if let ProgressKind::Log { message, .. } = event.kind {
                announced_continuation |= message.contains("继续") && message.contains("Python");
            }
        }
        assert!(announced_continuation, "用户应能看到安装继续的原因");
    }
}

#[tokio::test]
async fn python_448_is_detected_before_the_last_stderr_line() {
    let host = ScriptedHost::new(
        Locality::Local,
        vec![
            output(1, "", "not found"),
            output(2, "", &format!("{LINK_ERROR}additional diagnostic\n")),
            output(0, PYTHON_PATH, ""),
        ],
    );
    let (ctx, _progress) = ActionCtx::new();

    ensure(&host, &ctx).await.unwrap();

    assert_recovery_commands(&host);
}

#[tokio::test]
async fn python_448_without_usable_python_gives_repair_steps_and_skips_remote_mirror() {
    for locality in [Locality::Local, Locality::Remote] {
        let host = ScriptedHost::new(
            locality,
            vec![
                output(1, "", "not found"),
                output(2, "", LINK_ERROR),
                output(1, "", "not found after installation"),
            ],
        );
        let (ctx, _progress) = ActionCtx::new();

        let error = ensure(&host, &ctx).await.unwrap_err().to_string();

        for instruction in [
            "448",
            "setx UV_PYTHON_INSTALL_DIR",
            "LOCALAPPDATA",
            "Python 3.13",
        ] {
            assert!(error.contains(instruction), "缺少 {instruction}：{error}");
        }
        match locality {
            Locality::Local => {
                assert!(error.contains("彻底退出"), "{error}");
                assert!(error.contains("重新打开"), "{error}");
            }
            Locality::Remote => {
                assert!(error.contains("安装主机"), "{error}");
                assert!(error.contains("重启"), "{error}");
            }
        }
        assert_recovery_commands(&host);
    }
}

#[tokio::test]
async fn python_448_recheck_host_failure_still_gives_actionable_repair_steps() {
    let host = ScriptedHost::new(
        Locality::Local,
        vec![
            output(1, "", "not found"),
            output(2, "", LINK_ERROR),
            Err(HostError::Timeout {
                operation: "python-find",
            }),
        ],
    );
    let (ctx, _progress) = ActionCtx::new();

    let error = ensure(&host, &ctx).await.unwrap_err().to_string();

    assert!(error.contains("448"), "{error}");
    assert!(error.contains("UV_PYTHON_INSTALL_DIR"), "{error}");
    assert_recovery_commands(&host);
}

#[tokio::test]
async fn ordinary_python_install_failures_do_not_run_a_recovery_probe() {
    for stderr in [
        "error: connection timed out (os error 10060)",
        "error: invalid response from https://example.com/448",
        "error: another failure (os error 4480)",
    ] {
        let host = ScriptedHost::new(
            Locality::Local,
            vec![output(1, "", "not found"), output(2, "", stderr)],
        );
        let (ctx, _progress) = ActionCtx::new();

        let error = ensure(&host, &ctx).await.unwrap_err().to_string();

        assert!(error.contains(stderr), "{error}");
        assert!(!error.contains("UV_PYTHON_INSTALL_DIR"), "{error}");
        assert_eq!(host.commands().len(), 2);
    }
}

#[tokio::test]
async fn an_existing_python_skips_installation_and_recovery() {
    let host = ScriptedHost::new(Locality::Local, vec![output(0, PYTHON_PATH, "")]);
    let (ctx, _progress) = ActionCtx::new();

    ensure(&host, &ctx).await.unwrap();

    assert_eq!(host.commands().len(), 1);
}

#[tokio::test]
async fn cancellation_during_python_install_does_not_trigger_448_recovery() {
    let host = ScriptedHost::new(
        Locality::Local,
        vec![output(1, "", "not found"), Err(HostError::Cancelled)],
    );
    let (ctx, _progress) = ActionCtx::new();

    assert!(matches!(
        ensure(&host, &ctx).await,
        Err(ActionError::Cancelled)
    ));
    assert_eq!(host.commands().len(), 2);
}

#[tokio::test]
async fn cancellation_during_python_448_recheck_remains_cancellation() {
    let host = ScriptedHost::new(
        Locality::Remote,
        vec![
            output(1, "", "not found"),
            output(2, "", LINK_ERROR),
            Err(HostError::Cancelled),
        ],
    );
    let (ctx, _progress) = ActionCtx::new();

    assert!(matches!(
        ensure(&host, &ctx).await,
        Err(ActionError::Cancelled)
    ));
    assert_recovery_commands(&host);
}
