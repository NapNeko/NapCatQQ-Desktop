//! 远端 bash 的路径。`Host::which` 在远端按连接记住找到的结果，每条短脚本不会都多一趟 SSH

use ncd_host::{Host, HostCommand};
use ncd_traits::runtime_backend::BotBackendError;

/// 解析远端 bash：PATH 里找不到时再看 /bin/bash（非交互 SSH 的 PATH 可能很短）
pub async fn resolve_remote_bash(host: &dyn Host) -> Result<String, BotBackendError> {
    if let Some(path) = host
        .which("bash")
        .await
        .map_err(|e| BotBackendError::Io(e.to_string()))?
    {
        return Ok(path);
    }
    if host
        .run_to_string(HostCommand::new("sh").arg("-c").arg("test -x /bin/bash"))
        .await
        .ok()
        .is_some_and(|o| o.success())
    {
        return Ok("/bin/bash".into());
    }
    Err(BotBackendError::InvalidConfig(
        "远端 SnowLuma「直接运行」需要 bash。请安装：sudo apt install bash".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use bytes::Bytes;
    use ncd_host::{
        Arch, ArchiveKind, CommandOutput, DirEntry, HostError, HostPath, HostProcess, HostShell,
        Locality, Os, ShellKind,
    };
    use std::path::Path;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct NoopShell;
    impl HostShell for NoopShell {
        fn kind(&self) -> ShellKind {
            ShellKind::Bash
        }
        fn escape(&self, arg: &str) -> String {
            arg.to_string()
        }
        fn line_separator(&self) -> &'static str {
            "\n"
        }
    }
    static NOOP_SHELL: NoopShell = NoopShell;

    #[derive(Clone, Copy)]
    enum Bash {
        InPath,
        OnlyAtBinBash,
        Missing,
        SshDown,
    }

    struct CountingHost {
        id: String,
        hits: AtomicUsize,
        bash: Bash,
    }

    impl CountingHost {
        fn new(id: &str, bash: Bash) -> Self {
            Self {
                id: id.to_string(),
                hits: AtomicUsize::new(0),
                bash,
            }
        }
    }

    #[async_trait]
    impl Host for CountingHost {
        fn os(&self) -> Os {
            Os::Linux
        }
        fn arch(&self) -> Arch {
            Arch::X86_64
        }
        fn locality(&self) -> Locality {
            Locality::Remote
        }
        fn id(&self) -> &str {
            &self.id
        }
        fn shell(&self) -> &dyn HostShell {
            &NOOP_SHELL
        }
        async fn read_file(&self, _: &HostPath) -> Result<Bytes, HostError> {
            Err(HostError::Unsupported { operation: "mock" })
        }
        async fn write_file(&self, _: &HostPath, _: &[u8]) -> Result<(), HostError> {
            Ok(())
        }
        async fn list_dir(&self, _: &HostPath) -> Result<Vec<DirEntry>, HostError> {
            Err(HostError::Unsupported { operation: "mock" })
        }
        async fn create_dir_all(&self, _: &HostPath) -> Result<(), HostError> {
            Ok(())
        }
        async fn remove_file(&self, _: &HostPath) -> Result<(), HostError> {
            Ok(())
        }
        async fn remove_dir_all(&self, _: &HostPath) -> Result<(), HostError> {
            Ok(())
        }
        async fn exists(&self, _: &HostPath) -> Result<bool, HostError> {
            Ok(false)
        }
        async fn upload(&self, _: &Path, _: &HostPath) -> Result<(), HostError> {
            Err(HostError::Unsupported { operation: "mock" })
        }
        async fn download(&self, _: &HostPath, _: &Path) -> Result<(), HostError> {
            Err(HostError::Unsupported { operation: "mock" })
        }
        async fn extract_archive(
            &self,
            _: &HostPath,
            _: &HostPath,
            _: ArchiveKind,
        ) -> Result<(), HostError> {
            Err(HostError::Unsupported { operation: "mock" })
        }
        async fn spawn(&self, _: HostCommand) -> Result<Box<dyn HostProcess>, HostError> {
            Err(HostError::Unsupported { operation: "mock" })
        }
        async fn run_to_string(&self, cmd: HostCommand) -> Result<CommandOutput, HostError> {
            self.hits.fetch_add(1, Ordering::SeqCst);
            let script = cmd.args.last().cloned().unwrap_or_default();
            let found = |stdout: &str| CommandOutput {
                exit_code: Some(0),
                stdout: stdout.into(),
                stderr: String::new(),
            };
            let absent = CommandOutput {
                exit_code: Some(1),
                stdout: String::new(),
                stderr: String::new(),
            };
            match (self.bash, script.starts_with("command -v")) {
                (Bash::SshDown, _) => Err(HostError::Unsupported { operation: "ssh down" }),
                (Bash::InPath, true) => Ok(found("/usr/bin/bash\n")),
                (Bash::OnlyAtBinBash, false) => Ok(found("")),
                _ => Ok(absent),
            }
        }
    }

    #[tokio::test]
    async fn bash_in_path_is_used() {
        let host = CountingHost::new("h", Bash::InPath);
        assert_eq!(resolve_remote_bash(&host).await.unwrap(), "/usr/bin/bash");
        assert_eq!(host.hits.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn falls_back_to_bin_bash_when_path_is_short() {
        let host = CountingHost::new("h", Bash::OnlyAtBinBash);
        assert_eq!(resolve_remote_bash(&host).await.unwrap(), "/bin/bash");
    }

    #[tokio::test]
    async fn missing_bash_says_so() {
        let host = CountingHost::new("h", Bash::Missing);
        let err = resolve_remote_bash(&host).await.unwrap_err();
        assert!(matches!(err, BotBackendError::InvalidConfig(_)));
    }

    // 连不上不能报成「缺 bash」，不然用户会去装一个早就有的东西
    #[tokio::test]
    async fn ssh_failure_is_io_not_missing_bash() {
        let host = CountingHost::new("h", Bash::SshDown);
        let err = resolve_remote_bash(&host).await.unwrap_err();
        assert!(matches!(err, BotBackendError::Io(_)));
    }
}
