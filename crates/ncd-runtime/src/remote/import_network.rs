//! 导入已有远端 Bot 时，按 NC/SL × Native/Docker 读取框架网络配置。

use ncd_domain::{BackendType, DeploymentType, ImportedNetworkConfig, RemoteSelectedPaths};
use ncd_host::Host;

use crate::native_deployment_adapter::docker_helpers::read_docker_imported_network;

/// 按库存选中路径与导入行分发读取。文件不存在返回 Ok(None)；路径不足或 JSON 损坏返回 Err。
pub async fn fetch_imported_network(
    host: &dyn Host,
    selected: &RemoteSelectedPaths,
    backend: BackendType,
    deployment: DeploymentType,
    qq_id: &str,
    docker_name: Option<&str>,
) -> Result<Option<ImportedNetworkConfig>, String> {
    match (backend, deployment) {
        (BackendType::SnowLuma, DeploymentType::Native) => {
            let dir = selected
                .snowluma_dir
                .as_deref()
                .filter(|s| !s.is_empty())
                .ok_or_else(|| "该主机未发现 SnowLuma 安装，无法迁移网络配置".to_string())?;
            ncd_backend_snowluma::remote_snowluma::read_remote_onebot_connect(host, dir, qq_id)
                .await
        }
        (BackendType::NapCat, DeploymentType::Native) => {
            let root = napcat_root_for_import(selected)?;
            ncd_backend_napcat::read_remote_napcat_connect(host, &root, qq_id).await
        }
        (_, DeploymentType::Docker) => {
            let name = docker_name
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| "缺少 Docker 容器名，无法迁移网络配置".to_string())?;
            let home = selected.home.trim();
            if home.is_empty() {
                return Err("尚未发现该主机的安装库存，请先在导入对话框点「重新发现」".into());
            }
            read_docker_imported_network(host, home, name, backend, qq_id).await
        }
    }
}

fn napcat_root_for_import(selected: &RemoteSelectedPaths) -> Result<String, String> {
    if let Some(root) = selected
        .napcat_root
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return Ok(root.to_string());
    }
    if let Some(base) = selected
        .qq_install_base
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return Ok(format!(
            "{}/opt/QQ/resources/app/app_launcher/napcat",
            base.trim_end_matches('/')
        ));
    }
    Err("该主机未发现 NapCat 安装，无法迁移网络配置".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::Path;
    use std::sync::Mutex;

    use async_trait::async_trait;
    use bytes::Bytes;
    use ncd_host::{
        Arch, ArchiveKind, CommandOutput, DirEntry, HostCommand, HostError, HostPath, HostProcess,
        HostShell, Locality, Os, PackageManager, ShellKind,
    };
    use serde_json::json;

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

    struct FileHost {
        files: Mutex<HashMap<String, Vec<u8>>>,
        container_files: HashMap<String, Vec<u8>>,
    }

    impl FileHost {
        fn with_file(path: &str, body: &[u8]) -> Self {
            let mut files = HashMap::new();
            files.insert(path.to_string(), body.to_vec());
            Self {
                files: Mutex::new(files),
                container_files: HashMap::new(),
            }
        }

        fn with_container_file(container_src: &str, body: &[u8]) -> Self {
            let mut container_files = HashMap::new();
            container_files.insert(container_src.to_string(), body.to_vec());
            Self {
                files: Mutex::new(HashMap::new()),
                container_files,
            }
        }
    }

    fn output(code: i32, stdout: &str, stderr: &str) -> CommandOutput {
        CommandOutput {
            exit_code: Some(code),
            stdout: stdout.into(),
            stderr: stderr.into(),
        }
    }

    #[async_trait]
    impl Host for FileHost {
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
            "import-mock"
        }
        fn shell(&self) -> &dyn HostShell {
            &NOOP_SHELL
        }
        fn pkg_manager(&self) -> Option<&dyn PackageManager> {
            None
        }
        async fn read_file(&self, path: &HostPath) -> Result<Bytes, HostError> {
            self.files
                .lock()
                .unwrap()
                .get(path.as_posix())
                .cloned()
                .map(Bytes::from)
                .ok_or_else(|| HostError::PathNotFound { path: path.clone() })
        }
        async fn write_file(&self, path: &HostPath, bytes: &[u8]) -> Result<(), HostError> {
            self.files
                .lock()
                .unwrap()
                .insert(path.as_posix().to_string(), bytes.to_vec());
            Ok(())
        }
        async fn list_dir(&self, _: &HostPath) -> Result<Vec<DirEntry>, HostError> {
            Err(HostError::Unsupported { operation: "mock" })
        }
        async fn create_dir_all(&self, _: &HostPath) -> Result<(), HostError> {
            Ok(())
        }
        async fn remove_file(&self, path: &HostPath) -> Result<(), HostError> {
            self.files.lock().unwrap().remove(path.as_posix());
            Ok(())
        }
        async fn remove_dir_all(&self, _: &HostPath) -> Result<(), HostError> {
            Ok(())
        }
        async fn exists(&self, path: &HostPath) -> Result<bool, HostError> {
            Ok(self.files.lock().unwrap().contains_key(path.as_posix()))
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
            if cmd.program != "docker" {
                return Ok(output(0, "", ""));
            }
            match cmd.args.first().map(String::as_str) {
                Some("version") | Some("info") => Ok(output(0, "27.0.0\n", "")),
                Some("compose") => Ok(output(0, "ok\n", "")),
                Some("cp") => {
                    let spec = cmd.args.get(1).cloned().unwrap_or_default();
                    let dest = cmd.args.get(2).cloned().unwrap_or_default();
                    if let Some(body) = self.container_files.get(&spec) {
                        self.files.lock().unwrap().insert(dest, body.clone());
                        Ok(output(0, "", ""))
                    } else {
                        Ok(output(1, "", "Could not find the file"))
                    }
                }
                _ => Ok(output(0, "", "")),
            }
        }
    }

    fn sl_onebot() -> Vec<u8> {
        serde_json::to_vec(&json!({
            "networks": {
                "httpServers": [{
                    "name": "hs", "accessToken": "t", "host": "0.0.0.0", "port": 3000,
                    "enabled": true
                }],
                "httpClients": [],
                "wsServers": [],
                "wsClients": []
            },
            "musicSignUrl": "https://sl.sign"
        }))
        .unwrap()
    }

    fn nc_onebot() -> Vec<u8> {
        serde_json::to_vec(&json!({
            "network": {
                "httpServers": [{
                    "name": "hs", "token": "nt", "host": "0.0.0.0", "port": 3100
                }]
            },
            "musicSignUrl": "https://nc.sign",
            "parseMultMsg": true
        }))
        .unwrap()
    }

    #[test]
    fn napcat_root_prefers_selected_then_install_base() {
        let selected = RemoteSelectedPaths {
            home: "/home/u".into(),
            napcat_root: Some("/opt/custom/napcat".into()),
            qq_install_base: Some("/data/qq".into()),
            ..RemoteSelectedPaths::default()
        };
        assert_eq!(
            napcat_root_for_import(&selected).unwrap(),
            "/opt/custom/napcat"
        );

        let fallback = RemoteSelectedPaths {
            home: "/home/u".into(),
            qq_install_base: Some("/data/qq".into()),
            ..RemoteSelectedPaths::default()
        };
        assert_eq!(
            napcat_root_for_import(&fallback).unwrap(),
            "/data/qq/opt/QQ/resources/app/app_launcher/napcat"
        );

        let empty = RemoteSelectedPaths {
            home: "/home/u".into(),
            ..RemoteSelectedPaths::default()
        };
        assert!(
            napcat_root_for_import(&empty)
                .unwrap_err()
                .contains("NapCat")
        );
    }

    #[tokio::test]
    async fn native_snowluma_reads_onebot_file() {
        let host = FileHost::with_file("/opt/sl/config/onebot_10001.json", &sl_onebot());
        let selected = RemoteSelectedPaths {
            home: "/home/u".into(),
            snowluma_dir: Some("/opt/sl".into()),
            ..RemoteSelectedPaths::default()
        };
        let imported = fetch_imported_network(
            &host,
            &selected,
            BackendType::SnowLuma,
            DeploymentType::Native,
            "10001",
            None,
        )
        .await
        .unwrap()
        .expect("should migrate");
        assert_eq!(imported.connect.http_servers[0].port, 3000);
        assert_eq!(imported.music_sign_url.as_deref(), Some("https://sl.sign"));
    }

    #[tokio::test]
    async fn native_napcat_reads_from_napcat_root() {
        let host = FileHost::with_file(
            "/opt/custom/napcat/config/onebot11_10001.json",
            &nc_onebot(),
        );
        let selected = RemoteSelectedPaths {
            home: "/home/u".into(),
            napcat_root: Some("/opt/custom/napcat".into()),
            ..RemoteSelectedPaths::default()
        };
        let imported = fetch_imported_network(
            &host,
            &selected,
            BackendType::NapCat,
            DeploymentType::Native,
            "10001",
            None,
        )
        .await
        .unwrap()
        .expect("should migrate");
        assert_eq!(imported.connect.http_servers[0].port, 3100);
        assert_eq!(imported.parse_mult_msg, Some(true));
    }

    #[tokio::test]
    async fn docker_napcat_reads_host_bind_without_docker_cp() {
        let path = "/home/u/.napcat-bots/ncbot-10001/napcat/config/onebot11_10001.json";
        let host = FileHost::with_file(path, &nc_onebot());
        let selected = RemoteSelectedPaths {
            home: "/home/u".into(),
            ..RemoteSelectedPaths::default()
        };
        let imported = fetch_imported_network(
            &host,
            &selected,
            BackendType::NapCat,
            DeploymentType::Docker,
            "10001",
            Some("ncbot-10001"),
        )
        .await
        .unwrap()
        .expect("should migrate");
        assert_eq!(imported.connect.http_servers[0].base.token, "nt");
    }

    #[tokio::test]
    async fn docker_napcat_falls_back_to_docker_cp_when_host_bind_missing() {
        let spec = "ncbot-10001:/app/napcat/config/onebot11_10001.json";
        let host = FileHost::with_container_file(spec, &nc_onebot());
        let selected = RemoteSelectedPaths {
            home: "/home/u".into(),
            ..RemoteSelectedPaths::default()
        };
        let imported = fetch_imported_network(
            &host,
            &selected,
            BackendType::NapCat,
            DeploymentType::Docker,
            "10001",
            Some("ncbot-10001"),
        )
        .await
        .unwrap()
        .expect("should migrate via docker cp");
        assert_eq!(imported.connect.http_servers[0].port, 3100);
    }

    #[tokio::test]
    async fn docker_snowluma_reads_via_docker_cp() {
        let spec = "slbot-10001:/app/snowluma-data/config/onebot_10001.json";
        let host = FileHost::with_container_file(spec, &sl_onebot());
        let selected = RemoteSelectedPaths {
            home: "/home/u".into(),
            ..RemoteSelectedPaths::default()
        };
        let imported = fetch_imported_network(
            &host,
            &selected,
            BackendType::SnowLuma,
            DeploymentType::Docker,
            "10001",
            Some("slbot-10001"),
        )
        .await
        .unwrap()
        .expect("should migrate");
        assert_eq!(imported.connect.http_servers[0].port, 3000);
    }

    #[tokio::test]
    async fn missing_snowluma_dir_is_error() {
        let host = FileHost::with_file("/x", b"{}");
        let selected = RemoteSelectedPaths {
            home: "/home/u".into(),
            ..RemoteSelectedPaths::default()
        };
        let err = fetch_imported_network(
            &host,
            &selected,
            BackendType::SnowLuma,
            DeploymentType::Native,
            "1",
            None,
        )
        .await
        .unwrap_err();
        assert!(err.contains("SnowLuma"));
    }
}
