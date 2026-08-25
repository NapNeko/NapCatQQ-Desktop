//! 远端 bash 路径按 `Host::id` 缓存，避免每条短脚本都 SSH `command -v bash`

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use ncd_host::{Host, HostCommand};
use ncd_traits::runtime_backend::BotBackendError;

static BASH_BY_HOST: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();

fn bash_cache() -> &'static Mutex<HashMap<String, String>> {
    BASH_BY_HOST.get_or_init(|| Mutex::new(HashMap::new()))
}

fn cached_bash(host_id: &str) -> Option<String> {
    bash_cache()
        .lock()
        .ok()
        .and_then(|g| g.get(host_id).cloned())
}

fn remember_bash(host_id: &str, bash: &str) {
    if let Ok(mut g) = bash_cache().lock() {
        g.insert(host_id.to_string(), bash.to_string());
    }
}

#[cfg(test)]
pub(crate) fn clear_remote_bash_cache() {
    if let Ok(mut g) = bash_cache().lock() {
        g.clear();
    }
}

/// 解析远端 bash；同一 `host.id()` 只探测一次。
pub async fn resolve_remote_bash(host: &dyn Host) -> Result<String, BotBackendError> {
    let id = host.id();
    if let Some(cached) = cached_bash(id) {
        return Ok(cached);
    }

    let cmd = HostCommand::new("sh").arg("-c").arg("command -v bash");
    let out = host
        .run_to_string(cmd)
        .await
        .map_err(|e| BotBackendError::Io(e.to_string()))?;
    if out.success() {
        let line = out.stdout.lines().next().unwrap_or("").trim();
        if !line.is_empty() {
            remember_bash(id, line);
            return Ok(line.to_string());
        }
    }
    if host
        .run_to_string(HostCommand::new("sh").arg("-c").arg("test -x /bin/bash"))
        .await
        .ok()
        .is_some_and(|o| o.success())
    {
        remember_bash(id, "/bin/bash");
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
        Locality, Os, PackageManager, ShellKind,
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

    struct CountingHost {
        id: String,
        hits: AtomicUsize,
    }

    impl CountingHost {
        fn new(id: &str) -> Self {
            Self {
                id: id.to_string(),
                hits: AtomicUsize::new(0),
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
        fn pkg_manager(&self) -> Option<&dyn PackageManager> {
            None
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
        async fn run_to_string(&self, _: HostCommand) -> Result<CommandOutput, HostError> {
            self.hits.fetch_add(1, Ordering::SeqCst);
            Ok(CommandOutput {
                exit_code: Some(0),
                stdout: "/bin/bash\n".into(),
                stderr: String::new(),
            })
        }
    }

    #[test]
    fn cache_returns_stored_path() {
        clear_remote_bash_cache();
        remember_bash("h1", "/usr/bin/bash");
        assert_eq!(cached_bash("h1").as_deref(), Some("/usr/bin/bash"));
        assert_eq!(cached_bash("h2"), None);
    }

    #[tokio::test]
    async fn resolve_remote_bash_hits_host_once_per_id() {
        clear_remote_bash_cache();
        let host = CountingHost::new("bash-cache-once");
        let first = resolve_remote_bash(&host).await.expect("first");
        let second = resolve_remote_bash(&host).await.expect("second");
        assert_eq!(first, "/bin/bash");
        assert_eq!(second, "/bin/bash");
        assert_eq!(host.hits.load(Ordering::SeqCst), 1);
    }
}
