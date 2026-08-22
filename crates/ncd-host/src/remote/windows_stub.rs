//! RemoteWindowsHost:接口预留 stub
//!
//! 把"远端 Windows 主机"接口预留出来,所有方法返回 [HostError::Unsupported],
//! 真实实装留给未来(走 OpenSSH Server + PowerShell session)
//!
//! 当前状态:
//! - 类型定义,Host trait 实装框架已就位
//! - 所有方法返回 HostError::Unsupported { operation: "RemoteWindowsHost: ..." }
//! - 集成测试用 #[ignore = "RemoteWindowsHost real impl pending"] 占位
//!
//! 这样做的好处:
//! - 上层 Component 代码可以提前 match host.os() 写好分支,不用等接口落地才能开工
//! - 编译期就能拒绝调用方"假设 RemoteWindowsHost 已实装"的错误用法
//! - 实装时只要把每个方法的 unimplemented 替换成真实代码,trait 签名零变更

use std::path::Path;

use async_trait::async_trait;
use bytes::Bytes;

use crate::command::{CommandOutput, HostCommand};
use crate::error::HostError;
use crate::host::{Arch, Host, Locality, Os};
use crate::package_manager::PackageManager;
use crate::path::{ArchiveKind, DirEntry, HostPath};
use crate::process::HostProcess;
use crate::shell::{HostShell, PowerShellShell};

use super::connection::ConnectionConfig;

/// 远端 Windows 主机(stub)
pub struct RemoteWindowsHost {
    id: String,
    shell: PowerShellShell,
    #[allow(dead_code)] // 实装时会用到
    config: ConnectionConfig,
}

impl RemoteWindowsHost {
    /// 创建 stub,不实际建立任何 SSH 连接
    /// 实装时本方法会换成 connect() 走 SSH 握手
    pub fn new_stub(id: impl Into<String>, config: ConnectionConfig) -> Self {
        Self {
            id: id.into(),
            shell: PowerShellShell,
            config,
        }
    }
}

fn unsupported(op: &'static str) -> HostError {
    HostError::Unsupported { operation: op }
}

#[async_trait]
impl Host for RemoteWindowsHost {
    fn os(&self) -> Os {
        Os::Windows
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
        &self.shell
    }
    fn pkg_manager(&self) -> Option<&dyn PackageManager> {
        None
    }

    async fn read_file(&self, _path: &HostPath) -> Result<Bytes, HostError> {
        Err(unsupported("RemoteWindowsHost: read_file"))
    }
    async fn write_file(&self, _path: &HostPath, _bytes: &[u8]) -> Result<(), HostError> {
        Err(unsupported("RemoteWindowsHost: write_file"))
    }
    async fn list_dir(&self, _path: &HostPath) -> Result<Vec<DirEntry>, HostError> {
        Err(unsupported("RemoteWindowsHost: list_dir"))
    }
    async fn create_dir_all(&self, _path: &HostPath) -> Result<(), HostError> {
        Err(unsupported("RemoteWindowsHost: create_dir_all"))
    }
    async fn remove_file(&self, _path: &HostPath) -> Result<(), HostError> {
        Err(unsupported("RemoteWindowsHost: remove_file"))
    }
    async fn remove_dir_all(&self, _path: &HostPath) -> Result<(), HostError> {
        Err(unsupported("RemoteWindowsHost: remove_dir_all"))
    }
    async fn exists(&self, _path: &HostPath) -> Result<bool, HostError> {
        Err(unsupported("RemoteWindowsHost: exists"))
    }
    async fn upload(&self, _local: &Path, _remote: &HostPath) -> Result<(), HostError> {
        Err(unsupported("RemoteWindowsHost: upload"))
    }
    async fn download(&self, _remote: &HostPath, _local: &Path) -> Result<(), HostError> {
        Err(unsupported("RemoteWindowsHost: download"))
    }
    async fn extract_archive(
        &self,
        _archive: &HostPath,
        _dest: &HostPath,
        _kind: ArchiveKind,
    ) -> Result<(), HostError> {
        Err(unsupported("RemoteWindowsHost: extract_archive"))
    }
    async fn spawn(&self, _cmd: HostCommand) -> Result<Box<dyn HostProcess>, HostError> {
        Err(unsupported("RemoteWindowsHost: spawn"))
    }
    async fn run_to_string(&self, _cmd: HostCommand) -> Result<CommandOutput, HostError> {
        Err(unsupported("RemoteWindowsHost: run_to_string"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remote::credentials::SshCredentials;
    use crate::remote::host_key::HostKeyPolicy;

    fn stub() -> RemoteWindowsHost {
        let cfg = ConnectionConfig::new(
            "win-vm.example.com",
            22,
            SshCredentials::password("u", "p"),
            HostKeyPolicy::Insecure,
        );
        RemoteWindowsHost::new_stub("remote-win", cfg)
    }

    #[tokio::test]
    async fn identity_methods_work() {
        let h = stub();
        assert_eq!(h.os(), Os::Windows);
        assert_eq!(h.locality(), Locality::Remote);
        assert_eq!(h.id(), "remote-win");
        assert!(h.pkg_manager().is_none());
    }

    #[tokio::test]
    async fn all_io_methods_return_unsupported() {
        let h = stub();
        let p = HostPath::from_posix("/tmp/x");
        assert!(matches!(
            h.read_file(&p).await,
            Err(HostError::Unsupported { .. })
        ));
        assert!(matches!(
            h.write_file(&p, b"").await,
            Err(HostError::Unsupported { .. })
        ));
        assert!(matches!(
            h.list_dir(&p).await,
            Err(HostError::Unsupported { .. })
        ));
        assert!(matches!(
            h.create_dir_all(&p).await,
            Err(HostError::Unsupported { .. })
        ));
        assert!(matches!(
            h.exists(&p).await,
            Err(HostError::Unsupported { .. })
        ));
        let cmd = HostCommand::new("dir");
        assert!(matches!(
            h.run_to_string(cmd.clone()).await,
            Err(HostError::Unsupported { .. })
        ));
        assert!(matches!(
            h.spawn(cmd).await,
            Err(HostError::Unsupported { .. })
        ));
    }
}
