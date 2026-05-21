use std::collections::BTreeMap;
use std::sync::Mutex;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::ids::BotId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellCmd {
    pub program: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<String>,
    #[serde(default)]
    pub environment: BTreeMap<String, String>,
}

impl ShellCmd {
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            working_dir: None,
            environment: BTreeMap::new(),
        }
    }

    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TunnelSpec {
    pub local_port: u16,
    pub remote_host: String,
    pub remote_port: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bind_addr: Option<String>,
}

impl TunnelSpec {
    pub fn new(local_port: u16, remote_host: impl Into<String>, remote_port: u16) -> Self {
        Self {
            local_port,
            remote_host: remote_host.into(),
            remote_port,
            bind_addr: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TunnelHandle {
    pub local_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessNode {
    pub pid: u32,
    pub name: String,
    #[serde(default)]
    pub children: Vec<ProcessNode>,
}

impl ProcessNode {
    pub fn new(pid: u32, name: impl Into<String>) -> Self {
        Self {
            pid,
            name: name.into(),
            children: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessTree {
    pub root: ProcessNode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteFileEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteInstallInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub napcat_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snowluma_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_root: Option<String>,
}

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum RemoteHostError {
    #[error("remote host unavailable")]
    Unavailable,
    #[error("remote file not found: {0}")]
    NotFound(String),
    #[error("remote command failed: {0}")]
    CommandFailed(String),
    #[error("remote tunnel failed: {0}")]
    TunnelFailed(String),
    #[error("remote process tree failed: {0}")]
    ProcessTreeFailed(String),
    #[error("remote io error: {0}")]
    Io(String),
}

#[async_trait]
pub trait RemoteHost: Send + Sync {
    async fn exec(&self, cmd: ShellCmd) -> Result<ExecResult, RemoteHostError>;
    async fn read_file(&self, path: &str) -> Result<Vec<u8>, RemoteHostError>;
    async fn write_file(&self, path: &str, data: &[u8], mode: u32) -> Result<(), RemoteHostError>;
    async fn list_dir(&self, path: &str) -> Result<Vec<RemoteFileEntry>, RemoteHostError>;
    async fn open_tunnel(&self, spec: TunnelSpec) -> Result<TunnelHandle, RemoteHostError>;
    async fn process_tree(&self, bot_id: BotId) -> Result<ProcessTree, RemoteHostError>;
    async fn detect_installation(&self) -> Result<RemoteInstallInfo, RemoteHostError>;
}

#[derive(Debug, Default)]
pub struct MockRemoteHost {
    pub files: Mutex<BTreeMap<String, Vec<u8>>>,
    pub directories: BTreeMap<String, Vec<RemoteFileEntry>>,
    pub exec_results: BTreeMap<String, ExecResult>,
    pub tunnel: Option<TunnelHandle>,
    pub process_tree: Option<ProcessTree>,
    pub install_info: Option<RemoteInstallInfo>,
}

impl MockRemoteHost {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_file(self, path: impl Into<String>, data: impl Into<Vec<u8>>) -> Self {
        self.files.lock().unwrap().insert(path.into(), data.into());
        self
    }

    pub fn with_dir(mut self, path: impl Into<String>, entries: Vec<RemoteFileEntry>) -> Self {
        self.directories.insert(path.into(), entries);
        self
    }

    pub fn with_exec_result(mut self, program: impl Into<String>, result: ExecResult) -> Self {
        self.exec_results.insert(program.into(), result);
        self
    }
}

#[async_trait]
impl RemoteHost for MockRemoteHost {
    async fn exec(&self, cmd: ShellCmd) -> Result<ExecResult, RemoteHostError> {
        self.exec_results
            .get(&cmd.program)
            .cloned()
            .ok_or_else(|| RemoteHostError::CommandFailed(cmd.program))
    }

    async fn read_file(&self, path: &str) -> Result<Vec<u8>, RemoteHostError> {
        self.files
            .lock()
            .unwrap()
            .get(path)
            .cloned()
            .ok_or_else(|| RemoteHostError::NotFound(path.to_string()))
    }

    async fn write_file(&self, path: &str, data: &[u8], _mode: u32) -> Result<(), RemoteHostError> {
        self.files
            .lock()
            .unwrap()
            .insert(path.to_string(), data.to_vec());
        Ok(())
    }

    async fn list_dir(&self, path: &str) -> Result<Vec<RemoteFileEntry>, RemoteHostError> {
        Ok(self.directories.get(path).cloned().unwrap_or_default())
    }

    async fn open_tunnel(&self, spec: TunnelSpec) -> Result<TunnelHandle, RemoteHostError> {
        self.tunnel.clone().ok_or_else(|| {
            RemoteHostError::TunnelFailed(format!("{}:{}", spec.remote_host, spec.remote_port))
        })
    }

    async fn process_tree(&self, _bot_id: BotId) -> Result<ProcessTree, RemoteHostError> {
        self.process_tree.clone().ok_or_else(|| {
            RemoteHostError::ProcessTreeFailed("missing process tree fixture".to_string())
        })
    }

    async fn detect_installation(&self) -> Result<RemoteInstallInfo, RemoteHostError> {
        self.install_info
            .clone()
            .ok_or(RemoteHostError::Unavailable)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PosixPath(pub String);

impl From<&str> for PosixPath {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mock_remote_host_returns_fixtures() {
        let host = MockRemoteHost::new()
            .with_file("/etc/test.txt", b"hello".to_vec())
            .with_dir(
                "/etc",
                vec![RemoteFileEntry {
                    name: "test.txt".to_string(),
                    is_dir: false,
                    size: 5,
                }],
            )
            .with_exec_result(
                "echo",
                ExecResult {
                    exit_code: 0,
                    stdout: "ok".to_string(),
                    stderr: String::new(),
                },
            );

        let file = host.read_file("/etc/test.txt").await.unwrap();
        assert_eq!(file, b"hello".to_vec());
        let entries = host.list_dir("/etc").await.unwrap();
        assert_eq!(entries.len(), 1);
        let exec = host.exec(ShellCmd::new("echo")).await.unwrap();
        assert_eq!(exec.stdout, "ok");
    }
}
