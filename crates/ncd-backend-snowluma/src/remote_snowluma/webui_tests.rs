use super::*;

use std::path::Path;
use std::sync::Mutex as StdMutex;
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicUsize, Ordering};

use async_trait::async_trait;
use bytes::Bytes;
use ncd_host::shell::BashShell;
use ncd_host::{
    Arch, ArchiveKind, CommandOutput, DirEntry, HostError, HostPath, HostProcess, HostShell,
    Locality, Os,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Semaphore;

struct WebuiHost {
    files: StdMutex<BTreeMap<String, Vec<u8>>>,
    specs: StdMutex<Vec<TunnelSpec>>,
    servers: StdMutex<Vec<tokio::task::JoinHandle<()>>>,
    requests: Arc<StdMutex<Vec<String>>>,
    status: Arc<AtomicU16>,
    reads: AtomicUsize,
    endpoint_host: StdMutex<String>,
    endpoint_port: AtomicU16,
    gate_reads: AtomicBool,
    read_entered: Semaphore,
    read_release: Semaphore,
    exists_checks: AtomicUsize,
    invalidations: AtomicUsize,
}

impl WebuiHost {
    fn new(status: u16) -> Self {
        Self {
            files: StdMutex::new(BTreeMap::new()),
            specs: StdMutex::new(Vec::new()),
            servers: StdMutex::new(Vec::new()),
            requests: Arc::new(StdMutex::new(Vec::new())),
            status: Arc::new(AtomicU16::new(status)),
            reads: AtomicUsize::new(0),
            endpoint_host: StdMutex::new("127.0.0.1".into()),
            endpoint_port: AtomicU16::new(5099),
            gate_reads: AtomicBool::new(false),
            read_entered: Semaphore::new(0),
            read_release: Semaphore::new(0),
            exists_checks: AtomicUsize::new(0),
            invalidations: AtomicUsize::new(0),
        }
    }

    fn set_file(&self, path: &str, value: &[u8]) {
        self.files.lock().unwrap().insert(path.into(), value.into());
    }

    fn set_credentials(&self, primary: &str, actual: &str) {
        self.set_file(&layout().paths.webui_secret, primary.as_bytes());
        self.set_file(
            &format!("{}/webui.json", layout().paths.config_dir),
            &serde_json::to_vec(
                &crate::snowluma::session::build_webui_json_payload(actual, false).unwrap(),
            )
            .unwrap(),
        );
    }

    async fn disconnect_tunnels(&self) {
        let servers = std::mem::take(&mut *self.servers.lock().unwrap());
        for server in servers {
            server.abort();
            let _ = server.await;
        }
    }
}

impl Drop for WebuiHost {
    fn drop(&mut self) {
        for server in self.servers.get_mut().unwrap().drain(..) {
            server.abort();
        }
    }
}

#[async_trait]
impl Host for WebuiHost {
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
        "webui-host"
    }
    fn shell(&self) -> &dyn HostShell {
        &BashShell
    }

    async fn read_file(&self, path: &HostPath) -> Result<Bytes, HostError> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        if self.gate_reads.load(Ordering::SeqCst) {
            self.read_entered.add_permits(1);
            self.read_release.acquire().await.unwrap().forget();
        }
        self.files
            .lock()
            .unwrap()
            .get(path.as_posix())
            .cloned()
            .map(Bytes::from)
            .ok_or_else(|| HostError::PathNotFound { path: path.clone() })
    }
    async fn exists(&self, path: &HostPath) -> Result<bool, HostError> {
        self.exists_checks.fetch_add(1, Ordering::SeqCst);
        Ok(self.files.lock().unwrap().contains_key(path.as_posix()))
    }
    async fn invalidate_connection(&self) {
        self.invalidations.fetch_add(1, Ordering::SeqCst);
    }
    async fn list_dir(&self, _: &HostPath) -> Result<Vec<DirEntry>, HostError> {
        Ok(vec![])
    }
    async fn which(&self, _: &str) -> Result<Option<String>, HostError> {
        Ok(None)
    }
    async fn run_to_string(&self, command: HostCommand) -> Result<CommandOutput, HostError> {
        assert!(matches!(
            command.program.as_str(),
            "sh" | "tail" | "id" | "python3"
        ));
        assert!(!command.args.iter().any(|arg| arg.contains("node_pid=$!")));
        Ok(CommandOutput {
            exit_code: Some(0),
            stdout: if command.program == "id" {
                "1000\n".into()
            } else if command.program == "python3" {
                let endpoint_host = self.endpoint_host.lock().unwrap().clone();
                let endpoint_port = self.endpoint_port.load(Ordering::SeqCst);
                serde_json::json!({
                    "state":"present",
                    "node":{"pid":901,"uid":1000,"start_time":"12345","exe":"/framework/snowluma/node","cwd":"/framework/snowluma","entry":"/framework/snowluma/index.mjs","ns_pids":[901],"pid_namespace":"pid:[4026532569]"},
                    "snapshot":[{"pid":901,"start_time":"12345"}],
                    "host":endpoint_host,"port":endpoint_port,
                    "listeners":[{"host":endpoint_host,"port":endpoint_port}]
                }).to_string()
            } else {
                String::new()
            },
            stderr: String::new(),
        })
    }
    async fn open_tunnel(&self, spec: TunnelSpec) -> Result<TunnelHandle, HostError> {
        self.specs.lock().unwrap().push(spec);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let port = listener.local_addr()?.port();
        let requests = Arc::clone(&self.requests);
        let status = Arc::clone(&self.status);
        let server = tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    return;
                };
                let mut bytes = [0; 4096];
                let count = stream.read(&mut bytes).await.unwrap();
                requests.lock().unwrap().push(
                    String::from_utf8_lossy(&bytes[..count])
                        .lines()
                        .next()
                        .unwrap_or("")
                        .into(),
                );
                let response = format!(
                    "HTTP/1.1 {} Test\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                    status.load(Ordering::SeqCst),
                );
                let _ = stream.write_all(response.as_bytes()).await;
            }
        });
        self.servers.lock().unwrap().push(server);
        Ok(TunnelHandle::detached(port, 0))
    }
    async fn write_file(&self, _: &HostPath, _: &[u8]) -> Result<(), HostError> {
        panic!("WebUI preparation must not write remote files")
    }
    async fn create_dir_all(&self, _: &HostPath) -> Result<(), HostError> {
        panic!("WebUI preparation must not create remote directories")
    }
    async fn remove_file(&self, _: &HostPath) -> Result<(), HostError> {
        panic!("WebUI preparation must not remove remote files")
    }
    async fn remove_dir_all(&self, _: &HostPath) -> Result<(), HostError> {
        panic!("WebUI preparation must not remove remote directories")
    }
    async fn upload(&self, _: &Path, _: &HostPath) -> Result<(), HostError> {
        panic!("WebUI preparation must not upload")
    }
    async fn download(&self, _: &HostPath, _: &Path) -> Result<(), HostError> {
        panic!("WebUI preparation must not download")
    }
    async fn extract_archive(
        &self,
        _: &HostPath,
        _: &HostPath,
        _: ArchiveKind,
    ) -> Result<(), HostError> {
        panic!("WebUI preparation must not install")
    }
    async fn spawn(&self, _: HostCommand) -> Result<Box<dyn HostProcess>, HostError> {
        panic!("WebUI preparation must not start QQ")
    }
}

fn layout() -> RemoteSnowLumaLayout {
    RemoteSnowLumaLayout {
        home: "/home/test".into(),
        paths: SnowLumaRemotePaths::from_workspace_and_dir("/workspace", "/framework/snowluma"),
        node_bin: "/framework/snowluma/node".into(),
        qq_bin: "/opt/QQ/qq".into(),
        qq_install_base: "/".into(),
    }
}

fn daemon(host: Arc<WebuiHost>) -> RemoteSnowLumaDaemon {
    RemoteSnowLumaDaemon::from_layout(
        "server".into(),
        host,
        Arc::new(RemoteSnowLumaTunnelRegistry::new()),
        Arc::new(BroadcastEventBus::default()),
        layout(),
    )
}

#[tokio::test]
async fn changing_only_the_verified_listen_address_rebuilds_the_forward() {
    let host = Arc::new(WebuiHost::new(200));
    *host.endpoint_host.lock().unwrap() = "::1".into();
    let daemon = daemon(Arc::clone(&host));
    daemon.prepare_webui().await.unwrap();
    *host.endpoint_host.lock().unwrap() = "127.0.0.1".into();
    daemon.prepare_webui().await.unwrap();
    let specs = host.specs.lock().unwrap();
    assert_eq!(specs.len(), 2);
    assert_eq!(specs[0].remote_host, "::1");
    assert_eq!(specs[1].remote_host, "127.0.0.1");
    assert_eq!(specs[0].remote_port, specs[1].remote_port);
}

#[tokio::test]
async fn repeated_open_refreshes_empty_password_without_vnc_or_new_leases() {
    let host = Arc::new(WebuiHost::new(200));
    host.set_credentials("", "current-test-password");
    let daemon = daemon(Arc::clone(&host));
    let first = daemon.prepare_webui().await.unwrap();
    assert!(first.1.is_empty());
    host.set_credentials("current-test-password", "current-test-password");
    let second = daemon.prepare_webui().await.unwrap();
    assert_eq!(first.0, second.0);
    assert_eq!(second.1, "current-test-password");
    assert_eq!(host.specs.lock().unwrap().len(), 1);
    assert_eq!(daemon.leases.lock().await.count(), 0);
    assert!(
        daemon
            .tunnels
            .endpoints_for_server("server")
            .await
            .is_none()
    );
    assert!(daemon.tunnel_endpoints().await.is_none());
    assert!(
        host.requests
            .lock()
            .unwrap()
            .iter()
            .all(|line| line == "GET / HTTP/1.1")
    );
}

#[tokio::test]
async fn matching_fallback_replaces_stale_password_for_the_injection_endpoint() {
    let host = Arc::new(WebuiHost::new(200));
    host.set_credentials("old-test-password", "fresh-test-password");
    host.set_file("/framework/webui.secret", b"fresh-test-password");
    let daemon = daemon(Arc::clone(&host));
    *daemon.tunnel_eps.lock().await = Some(RemoteSnowLumaTunnelEndpoints {
        webui_local_port: 1234,
        novnc_local_port: 5678,
        webui_password: "cached-old-test-password".into(),
        vnc_password: "vnc-test".into(),
    });
    let prepared = daemon.prepare_webui().await.unwrap();
    let endpoint = daemon.tunnel_endpoints().await.unwrap();
    assert_eq!(prepared.1, "fresh-test-password");
    assert_eq!(endpoint.webui_local_port, prepared.0);
    assert_eq!(endpoint.webui_password, prepared.1);
    assert_eq!(endpoint.novnc_local_port, 5678);
    host.set_credentials("rotated-test-password", "rotated-test-password");
    let refreshed = daemon.prepare_webui().await.unwrap();
    assert_eq!(refreshed.0, prepared.0);
    assert_eq!(
        daemon.tunnel_endpoints().await.unwrap().webui_password,
        "rotated-test-password",
    );
    assert_eq!(host.specs.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn http_authentication_errors_still_allow_opening_without_a_login_request() {
    for status in [401, 429] {
        let host = Arc::new(WebuiHost::new(status));
        let daemon = daemon(Arc::clone(&host));
        let endpoint = daemon.prepare_webui().await.unwrap();
        assert_ne!(endpoint.0, 0);
        assert!(endpoint.1.is_empty());
        assert_eq!(*host.requests.lock().unwrap(), vec!["GET / HTTP/1.1"]);
    }
}

#[tokio::test]
async fn known_invalid_secret_is_not_copied_or_sent_to_login() {
    let host = Arc::new(WebuiHost::new(401));
    host.set_credentials("stale-test-password", "other-test-password");
    let daemon = daemon(Arc::clone(&host));
    *daemon.tunnel_eps.lock().await = Some(RemoteSnowLumaTunnelEndpoints {
        webui_local_port: 1234,
        novnc_local_port: 5678,
        webui_password: "cached-old-test-password".into(),
        vnc_password: "vnc-test".into(),
    });
    daemon.prepare_webui().await.unwrap();
    let endpoint = daemon.tunnel_endpoints().await.unwrap();
    assert!(endpoint.webui_password.is_empty());
    let result = super::super::super::inject::inject_via_tunnel(&endpoint, 42, 42).await;
    assert!(result.is_err());
    assert_eq!(*host.requests.lock().unwrap(), vec!["GET / HTTP/1.1"]);
}

#[tokio::test]
async fn failed_cached_http_tunnel_is_rebuilt_on_the_current_host() {
    let host = Arc::new(WebuiHost::new(200));
    let daemon = daemon(Arc::clone(&host));
    daemon.prepare_webui().await.unwrap();
    host.disconnect_tunnels().await;
    let second = daemon.prepare_webui().await.unwrap();
    assert_ne!(second.0, 0);
    assert_eq!(host.specs.lock().unwrap().len(), 2);
}

#[tokio::test]
async fn replacement_host_discards_the_old_http_tunnel_and_password() {
    let old = Arc::new(WebuiHost::new(200));
    old.set_credentials("old-test-password", "old-test-password");
    let daemon = daemon(Arc::clone(&old));
    daemon.prepare_webui().await.unwrap();
    let reads = old.reads.load(Ordering::SeqCst);
    let fresh = Arc::new(WebuiHost::new(200));
    fresh.set_credentials("fresh-test-password", "fresh-test-password");
    daemon
        .replace_host_if_current(fresh.clone(), async { true })
        .await
        .unwrap();
    assert!(daemon.webui_tunnel.lock().await.is_none());
    let endpoint = daemon.prepare_webui().await.unwrap();
    assert_eq!(endpoint.1, "fresh-test-password");
    assert_eq!(old.reads.load(Ordering::SeqCst), reads);
    assert_eq!(fresh.specs.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn changing_remote_http_port_replaces_only_the_http_tunnel() {
    let host = Arc::new(WebuiHost::new(200));
    host.endpoint_port.store(15123, Ordering::SeqCst);
    host.set_file(
        "/framework/snowluma/config/runtime.json",
        br#"{"webuiPort":15123}"#,
    );
    let daemon = daemon(Arc::clone(&host));
    daemon.prepare_webui().await.unwrap();
    host.set_file(
        "/framework/snowluma/config/runtime.json",
        br#"{"webuiPort":15123}"#,
    );
    host.endpoint_port.store(15124, Ordering::SeqCst);
    daemon.prepare_webui().await.unwrap();
    let ports: Vec<_> = host
        .specs
        .lock()
        .unwrap()
        .iter()
        .map(|spec| spec.remote_port)
        .collect();
    assert_eq!(ports, vec![15123, 15124]);
}

#[tokio::test]
async fn detaching_local_sessions_clears_the_http_endpoint_without_remote_writes() {
    let host = Arc::new(WebuiHost::new(200));
    let daemon = daemon(host);
    daemon.prepare_webui().await.unwrap();
    daemon.detach_local_sessions().await;
    assert!(daemon.tunnel_endpoints().await.is_none());
}

#[tokio::test]
async fn prepare_after_reconcile_cleanup_restores_http_without_claiming_vnc_is_ready() {
    let host = Arc::new(WebuiHost::new(200));
    let daemon = daemon(Arc::clone(&host));
    daemon.prepare_webui().await.unwrap();
    daemon.clear_local_startup().await;
    assert!(daemon.webui_tunnel.lock().await.is_none());
    let endpoint = daemon.prepare_webui().await.unwrap();
    assert_ne!(endpoint.0, 0);
    assert_eq!(host.specs.lock().unwrap().len(), 2);
    assert!(daemon.tunnel_endpoints().await.is_none());
}

#[tokio::test]
async fn credential_files_are_read_concurrently_without_an_exists_round_trip() {
    let host = Arc::new(WebuiHost::new(200));
    host.set_file(&layout().paths.webui_secret, b"current-fixture-password");
    host.gate_reads.store(true, Ordering::SeqCst);
    let daemon = Arc::new(daemon(Arc::clone(&host)));
    let pending = tokio::spawn({
        let host = Arc::clone(&host);
        let daemon = Arc::clone(&daemon);
        async move { daemon.read_current_webui_credentials(host.as_ref()).await }
    });
    tokio::time::timeout(Duration::from_secs(1), host.read_entered.acquire_many(3))
        .await
        .expect("all independent reads must start before any one completes")
        .unwrap()
        .forget();
    assert_eq!(host.exists_checks.load(Ordering::SeqCst), 0);
    host.read_release.add_permits(3);
    assert_eq!(
        pending.await.unwrap().unwrap().0,
        "current-fixture-password"
    );
}

#[tokio::test(start_paused = true)]
async fn credential_read_timeout_keeps_existing_tunnel_without_using_its_old_password() {
    let host = Arc::new(WebuiHost::new(200));
    host.gate_reads.store(true, Ordering::SeqCst);
    let daemon = daemon(Arc::clone(&host));
    *daemon.webui_tunnel.lock().await = Some(WebuiTunnel {
        handle: TunnelHandle::detached(12345, 0),
        remote_port: 5099,
        remote_host: "127.0.0.1".into(),
        password: "cached-fixture-password".into(),
    });
    let error = daemon
        .read_current_webui_credentials(host.as_ref())
        .await
        .unwrap_err();
    assert!(error.to_string().contains("超时"));
    let cached = daemon.webui_tunnel.lock().await;
    assert_eq!(cached.as_ref().unwrap().handle.local_port(), 12345);
    assert_eq!(cached.as_ref().unwrap().password, "cached-fixture-password");
    assert_eq!(host.invalidations.load(Ordering::SeqCst), 0);
    assert!(host.specs.lock().unwrap().is_empty());
}
