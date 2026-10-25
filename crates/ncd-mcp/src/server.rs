//! MCP 服务生命周期、仅回环监听与密钥存储。

use std::future::Future;
use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex, MutexGuard, PoisonError};
use std::task::{Context, Poll};
use std::time::Duration;

use ncd_domain::{McpServerSettings, McpServerStatus};
use ncd_runtime::onebot_debug::DebugManager;
use ncd_traits::SecretStore;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore};
use tokio::task::JoinHandle;
use tokio::time::Sleep;
use tokio_util::sync::CancellationToken;

use crate::gate::GateKeeper;
use crate::tools::ToolsCtx;

// v1 曾允许回落到文件，不能迁移其中可能已被撤销的系统凭据副本。
const TOKEN_SECRET_KEY: &str = "mcp:bearer_token:v2";
const MAX_CONNECTIONS: usize = 64;
const CONNECTION_TIMEOUT: Duration = Duration::from_secs(120);

// HTTP 中间件看不到尚未读完 header 的连接，必须在 accept 和 IO 层限制它们。
pub(crate) struct LimitedListener {
    listener: TcpListener,
    slots: Arc<Semaphore>,
    cancel: CancellationToken,
}

impl LimitedListener {
    pub(crate) fn new(listener: TcpListener, cancel: CancellationToken) -> Self {
        Self {
            listener,
            slots: Arc::new(Semaphore::new(MAX_CONNECTIONS)),
            cancel,
        }
    }
}

impl axum::serve::Listener for LimitedListener {
    type Io = LimitedConnection;
    type Addr = SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        let permit = self
            .slots
            .clone()
            .acquire_owned()
            .await
            .expect("MCP connection semaphore remains open");
        let (socket, address) = axum::serve::Listener::accept(&mut self.listener).await;
        (
            LimitedConnection {
                socket,
                _permit: permit,
                deadline: Box::pin(tokio::time::sleep(CONNECTION_TIMEOUT)),
                cancelled: Box::pin(self.cancel.clone().cancelled_owned()),
            },
            address,
        )
    }

    fn local_addr(&self) -> io::Result<Self::Addr> {
        self.listener.local_addr()
    }
}

pub(crate) struct LimitedConnection {
    socket: TcpStream,
    _permit: OwnedSemaphorePermit,
    deadline: Pin<Box<Sleep>>,
    cancelled: Pin<Box<dyn Future<Output = ()> + Send>>,
}

impl LimitedConnection {
    fn poll_expired(&mut self, cx: &mut Context<'_>) -> io::Result<()> {
        if self.cancelled.as_mut().poll(cx).is_ready() {
            return Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "MCP service stopped",
            ));
        }
        if self.deadline.as_mut().poll(cx).is_ready() {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "MCP connection expired",
            ));
        }
        Ok(())
    }
}

impl AsyncRead for LimitedConnection {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        self.poll_expired(cx)?;
        Pin::new(&mut self.socket).poll_read(cx, buf)
    }
}

impl AsyncWrite for LimitedConnection {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.poll_expired(cx)?;
        Pin::new(&mut self.socket).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.poll_expired(cx)?;
        Pin::new(&mut self.socket).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.socket).poll_shutdown(cx)
    }
}

pub(crate) struct ServerCtx {
    pub tools: ToolsCtx,
    pub token: String,
    pub bound_port: u16,
    pub requests: Semaphore,
    pub cancel: CancellationToken,
}

struct Run {
    /// 当时设置里要求的端口（0 = 随机）
    requested_port: u16,
    bound_port: u16,
    token: String,
    cancel: CancellationToken,
    task: JoinHandle<()>,
}

pub struct McpApplyOutcome {
    /// 起服务成功后的实际监听端口；没在听（关着或起失败）为空
    pub bound_port: Option<u16>,
}

pub struct McpServer {
    debug: Arc<DebugManager>,
    inspection: Option<Arc<crate::inspection::McpInspection>>,
    secrets: Arc<dyn SecretStore + Send + Sync>,
    gate: Arc<GateKeeper>,
    run: StdMutex<Option<Run>>,
    /// 最近一次 apply 的设置（status 报用）
    config: StdMutex<McpServerSettings>,
    /// 服务刚被打开过（用于收起「从没起过」和「起过停了」的分歧）
    ever_started: AtomicBool,
    last_error: StdMutex<Option<String>>,
    apply_lock: Mutex<()>,
}

impl McpServer {
    pub fn new(debug: Arc<DebugManager>, secrets: Arc<dyn SecretStore + Send + Sync>) -> Self {
        Self {
            debug,
            inspection: None,
            secrets,
            gate: Arc::new(GateKeeper::new()),
            run: StdMutex::new(None),
            config: StdMutex::new(McpServerSettings::default()),
            ever_started: AtomicBool::new(false),
            last_error: StdMutex::new(None),
            apply_lock: Mutex::new(()),
        }
    }

    pub fn with_inspection(mut self, inspection: Arc<crate::inspection::McpInspection>) -> Self {
        self.inspection = Some(inspection);
        self
    }

    /// 权限闸的危险级开关跟设置走，和服务开不开无关（掉了服务也照配，工具到不了闸也无妨）
    pub async fn apply(self: &Arc<Self>, settings: &McpServerSettings) -> McpApplyOutcome {
        // bind 会让出执行权，串行化避免并发保存设置留下未受管理的监听器。
        let _apply = self.apply_lock.lock().await;
        self.apply_locked(settings).await
    }

    async fn apply_locked(self: &Arc<Self>, settings: &McpServerSettings) -> McpApplyOutcome {
        if !settings.enabled {
            // 同次保存可能放宽权限；先撤销旧服务，再应用离线设置。
            self.stop_run_and_wait().await;
            self.gate
                .set_policy(settings.read_only, settings.allow_dangerous);
            *lock_std(&self.config) = settings.clone();
            return McpApplyOutcome { bound_port: None };
        }
        let permissions_changed = self
            .gate
            .set_policy(settings.read_only, settings.allow_dangerous);
        if permissions_changed {
            self.clear_subscriptions().await;
        }
        *lock_std(&self.config) = settings.clone();

        if let Some(bound) = self.current_run_port(settings.port) {
            return McpApplyOutcome {
                bound_port: Some(bound),
            };
        }
        self.stop_run_and_wait().await;

        let token = match self.ensure_token() {
            Ok(token) => token,
            Err(reason) => {
                *lock_std(&self.last_error) = Some(reason);
                return McpApplyOutcome { bound_port: None };
            }
        };
        let listener = match TcpListener::bind(SocketAddr::new(
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            settings.port,
        ))
        .await
        {
            Ok(listener) => listener,
            Err(err) => {
                *lock_std(&self.last_error) = Some(format!(
                    "MCP 服务绑定 127.0.0.1:{} 失败：{err}",
                    settings.port
                ));
                return McpApplyOutcome { bound_port: None };
            }
        };
        let bound_port = listener
            .local_addr()
            .map(|addr| addr.port())
            .unwrap_or(settings.port);

        let tools = ToolsCtx {
            debug: Arc::clone(&self.debug),
            runtime: self.inspection.clone(),
            gate: Arc::clone(&self.gate),
            status: {
                let me = Arc::downgrade(self);
                Arc::new(move || {
                    me.upgrade()
                        .map(|server| server.status())
                        .unwrap_or(McpServerStatus {
                            enabled: false,
                            listening: false,
                            port: None,
                            last_error: None,
                        })
                })
            },
        };
        let cancel = CancellationToken::new();
        let ctx = Arc::new(ServerCtx {
            tools,
            token: token.clone(),
            bound_port,
            requests: Semaphore::new(crate::http::MAX_CONCURRENT_REQUESTS),
            cancel: cancel.clone(),
        });
        let shutdown = cancel.clone();
        let listener = LimitedListener {
            listener,
            slots: Arc::new(Semaphore::new(MAX_CONNECTIONS)),
            cancel: cancel.clone(),
        };
        let task = tokio::spawn(async move {
            let serve = axum::serve(listener, crate::http::router(ctx))
                .with_graceful_shutdown(shutdown.cancelled_owned());
            if let Err(err) = serve.await {
                tracing::warn!(error = %err, "MCP 服务异常退出");
            }
        });

        self.ever_started.store(true, Ordering::SeqCst);
        *lock_std(&self.last_error) = None;
        *lock_std(&self.run) = Some(Run {
            requested_port: settings.port,
            bound_port,
            token,
            cancel,
            task,
        });
        McpApplyOutcome {
            bound_port: Some(bound_port),
        }
    }

    /// 设置里的端口和当前跑着的一致（且没在半路死掉）就续用
    fn current_run_port(&self, requested_port: u16) -> Option<u16> {
        let run = lock_std(&self.run);
        let run = run.as_ref()?;
        if (run.requested_port == requested_port || run.bound_port == requested_port)
            && !run.task.is_finished()
        {
            Some(run.bound_port)
        } else {
            None
        }
    }

    fn stop_run(&self) -> Option<JoinHandle<()>> {
        let run = lock_std(&self.run).take();
        if let Some(run) = &run {
            run.cancel.cancel();
            run.task.abort();
        }
        self.gate.reset();
        run.map(|run| run.task)
    }

    async fn stop_run_and_wait(&self) {
        // 等监听 socket 真正释放后才能在固定端口轮换凭据。
        if let Some(task) = self.stop_run() {
            let _ = task.await;
        }
        self.clear_subscriptions().await;
    }

    async fn clear_subscriptions(&self) {
        for subscription in self.gate.take_subscriptions() {
            self.debug.unsubscribe(&subscription).await;
        }
    }

    /// 仅桌面端显式复制时读取；工具与状态查询不暴露凭据。
    pub fn connection_config(&self) -> Result<String, String> {
        let run = lock_std(&self.run);
        let run = run
            .as_ref()
            .filter(|run| !run.task.is_finished())
            .ok_or_else(|| "请先开启 MCP 服务".to_owned())?;
        serde_json::to_string_pretty(&serde_json::json!({
            "mcpServers": {
                "ncd": {
                    "url": format!("http://127.0.0.1:{}/mcp", run.bound_port),
                    "headers": { "Authorization": format!("Bearer {}", run.token) },
                },
            },
        }))
        .map_err(|_| "无法生成 MCP 连接配置".to_owned())
    }

    pub async fn rotate_token(self: &Arc<Self>) -> Result<(), String> {
        let _apply = self.apply_lock.lock().await;
        let token = hex::encode(rand::random::<[u8; 32]>());
        self.secrets
            .put(TOKEN_SECRET_KEY, &token)
            .map_err(|err| format!("MCP token 写入密钥存储失败：{err}"))?;
        let mut settings = lock_std(&self.config).clone();
        if settings.port == 0 {
            settings.port = lock_std(&self.run).as_ref().map_or(0, |run| run.bound_port);
        }
        self.stop_run_and_wait().await;
        let outcome = self.apply_locked(&settings).await;
        if settings.enabled && outcome.bound_port.is_none() {
            return Err(lock_std(&self.last_error)
                .clone()
                .unwrap_or_else(|| "MCP 密钥已更新，但服务重启失败".to_owned()));
        }
        Ok(())
    }

    /// 只复用服务生成的 32 字节 hex 凭据，损坏或手工写入的弱口令会被替换。
    fn ensure_token(&self) -> Result<String, String> {
        match self.secrets.get(TOKEN_SECRET_KEY) {
            Ok(Some(token))
                if token.len() == 64 && token.bytes().all(|byte| byte.is_ascii_hexdigit()) =>
            {
                Ok(token)
            }
            Ok(_) => {
                let token = hex::encode(rand::random::<[u8; 32]>());
                self.secrets
                    .put(TOKEN_SECRET_KEY, &token)
                    .map_err(|err| format!("MCP token 写入密钥存储失败：{err}"))?;
                Ok(token)
            }
            Err(err) => Err(format!("MCP token 读取密钥存储失败：{err}")),
        }
    }

    pub fn status(&self) -> McpServerStatus {
        let enabled = lock_std(&self.config).enabled;
        let run = lock_std(&self.run);
        let listening = run.as_ref().is_some_and(|r| !r.task.is_finished());
        McpServerStatus {
            enabled,
            listening,
            port: run.as_ref().filter(|_| listening).map(|r| r.bound_port),
            // 从没起成功过、或用户关着：错误不报（那不是失败，是没开过）
            last_error: if self.ever_started.load(Ordering::SeqCst) || enabled {
                lock_std(&self.last_error).clone()
            } else {
                None
            },
        }
    }
}

impl Drop for McpServer {
    fn drop(&mut self) {
        let _ = self.stop_run();
    }
}

fn lock_std<T>(mutex: &StdMutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::io::{Read, Write};
    use std::time::Duration;

    use ncd_domain::SecretError;
    use tempfile::tempdir;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;
    use crate::tests_support::test_debug_manager;

    #[derive(Default)]
    struct MemorySecrets {
        map: StdMutex<HashMap<String, String>>,
        fail_put: AtomicBool,
    }

    impl SecretStore for MemorySecrets {
        fn get(&self, key: &str) -> Result<Option<String>, SecretError> {
            Ok(lock_std(&self.map).get(key).cloned())
        }

        fn put(&self, key: &str, value: &str) -> Result<(), SecretError> {
            if self.fail_put.load(Ordering::SeqCst) {
                return Err(SecretError::Unavailable);
            }
            lock_std(&self.map).insert(key.to_owned(), value.to_owned());
            Ok(())
        }

        fn delete(&self, key: &str) -> Result<(), SecretError> {
            lock_std(&self.map).remove(key);
            Ok(())
        }
    }

    fn server(root: &std::path::Path) -> (Arc<McpServer>, Arc<MemorySecrets>) {
        let secrets = Arc::new(MemorySecrets::default());
        let server = Arc::new(McpServer::new(
            test_debug_manager(root.to_path_buf()),
            secrets.clone(),
        ));
        (server, secrets)
    }

    #[tokio::test]
    async fn disabled_by_default_and_apply_starts_listening() {
        let root = tempdir().unwrap();
        let (server, secrets) = server(root.path());
        assert!(!server.status().enabled);
        assert!(!server.status().listening);

        // 随机端口：apply 返回实际端口，状态一致
        let outcome = server
            .apply(&McpServerSettings {
                enabled: true,
                port: 0,
                allow_dangerous: false,
                read_only: true,
            })
            .await;
        let bound = outcome.bound_port.expect("开着的设置应起服务");
        assert!(bound > 0);
        let status = server.status();
        assert!(status.enabled && status.listening);
        assert_eq!(status.port, Some(bound));

        // 同设置再 apply：不重启（同一个端口）
        let again = server
            .apply(&McpServerSettings {
                enabled: true,
                port: 0,
                allow_dangerous: false,
                read_only: true,
            })
            .await;
        assert_eq!(again.bound_port, Some(bound));

        // 关掉：不再听
        server
            .apply(&McpServerSettings {
                enabled: false,
                port: 0,
                allow_dangerous: false,
                read_only: true,
            })
            .await;
        let status = server.status();
        assert!(!status.listening);
        assert_eq!(status.port, None);

        // token 已在假存储里，且是 64 位 hex
        let stored = secrets.get(TOKEN_SECRET_KEY).unwrap().unwrap();
        assert_eq!(stored.len(), 64);
        assert!(stored.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[tokio::test]
    async fn start_failure_surfaces_in_status_error() {
        let root = tempdir().unwrap();
        let (server, _secrets) = server(root.path());
        // 用一个已占用的端口让 bind 失败
        let busy = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let busy_port = busy.local_addr().unwrap().port();
        let outcome = server
            .apply(&McpServerSettings {
                enabled: true,
                port: busy_port,
                allow_dangerous: false,
                read_only: true,
            })
            .await;
        assert_eq!(outcome.bound_port, None);
        let status = server.status();
        assert!(status.enabled);
        assert!(!status.listening);
        assert!(
            status
                .last_error
                .as_deref()
                .is_some_and(|e| e.contains("绑定")),
            "{:?}",
            status.last_error
        );
    }

    async fn ping_status(port: u16, token: String) -> String {
        tokio::task::spawn_blocking(move || {
            let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
            stream.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
            stream.set_write_timeout(Some(Duration::from_secs(3))).unwrap();
            let body = r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#;
            let request = format!(
                "POST /mcp HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len(),
            );
            stream.write_all(request.as_bytes()).unwrap();
            let mut response = String::new();
            stream.read_to_string(&mut response).unwrap();
            response.lines().next().unwrap().to_owned()
        }).await.unwrap()
    }

    #[tokio::test]
    async fn token_rotation_invalidates_old_token_and_retains_endpoint() {
        let root = tempdir().unwrap();
        let (server, secrets) = server(root.path());
        assert!(server.connection_config().is_err());
        let settings = McpServerSettings {
            enabled: true,
            port: 0,
            ..Default::default()
        };
        let port = server.apply(&settings).await.bound_port.unwrap();
        let old_token = secrets.get(TOKEN_SECRET_KEY).unwrap().unwrap();
        let old_cancel = lock_std(&server.run).as_ref().unwrap().cancel.clone();
        assert!(ping_status(port, old_token.clone()).await.contains("200"));

        server.rotate_token().await.unwrap();
        assert!(old_cancel.is_cancelled());
        assert_eq!(server.status().port, Some(port));
        let new_token = secrets.get(TOKEN_SECRET_KEY).unwrap().unwrap();
        assert_ne!(old_token, new_token);
        assert!(ping_status(port, old_token.clone()).await.contains("401"));
        assert!(ping_status(port, new_token.clone()).await.contains("200"));

        let config: serde_json::Value =
            serde_json::from_str(&server.connection_config().unwrap()).unwrap();
        assert_eq!(
            config["mcpServers"]["ncd"]["url"],
            format!("http://127.0.0.1:{port}/mcp")
        );
        assert_eq!(
            config["mcpServers"]["ncd"]["headers"]["Authorization"],
            format!("Bearer {new_token}")
        );
        assert!(!config.to_string().contains(&old_token));
        server.apply(&McpServerSettings::default()).await;
        assert!(server.connection_config().is_err());
    }

    #[tokio::test]
    async fn concurrent_apply_has_one_listener_and_shutdown_releases_port() {
        let root = tempdir().unwrap();
        let (server, _) = server(root.path());
        let settings = McpServerSettings {
            enabled: true,
            port: 0,
            ..Default::default()
        };
        let (first, second) = tokio::join!(server.apply(&settings), server.apply(&settings));
        assert_eq!(first.bound_port, second.bound_port);
        let port = first.bound_port.unwrap();
        server.apply(&McpServerSettings::default()).await;
        let _listener = TcpListener::bind(("127.0.0.1", port)).await.unwrap();
    }

    #[tokio::test]
    async fn disabling_revokes_the_old_run_before_relaxing_its_policy() {
        let root = tempdir().unwrap();
        let (server, _) = server(root.path());
        let old_cancel = CancellationToken::new();
        let old_epoch = server.gate.epoch();
        let (entered, ready) = tokio::sync::oneshot::channel();
        let (release, hold) = std::sync::mpsc::channel();
        // 已开始的阻塞任务不会被 abort 立即终止，使停服等待边界可被确定地观测。
        let task = tokio::task::spawn_blocking(move || {
            let _ = entered.send(());
            let _ = hold.recv();
        });
        ready.await.unwrap();
        *lock_std(&server.run) = Some(Run {
            requested_port: 0,
            bound_port: 3999,
            token: "test-token".to_owned(),
            cancel: old_cancel.clone(),
            task,
        });
        let disabling = tokio::spawn({
            let server = server.clone();
            async move {
                server
                    .apply(&McpServerSettings {
                        enabled: false,
                        read_only: false,
                        allow_dangerous: true,
                        ..Default::default()
                    })
                    .await
            }
        });
        tokio::time::timeout(Duration::from_secs(1), old_cancel.cancelled())
            .await
            .unwrap();
        assert!(old_epoch.is_cancelled());
        assert!(lock_std(&server.run).is_none());
        assert!(server.gate.read_only());
        assert!(!server.gate.allow_dangerous());
        assert!(!disabling.is_finished());
        release.send(()).unwrap();
        assert!(disabling.await.unwrap().bound_port.is_none());
        assert!(!server.gate.read_only());
        assert!(server.gate.allow_dangerous());
        assert!(!server.status().listening);
    }

    #[tokio::test]
    async fn failed_token_write_preserves_the_active_credentials_and_epoch() {
        let root = tempdir().unwrap();
        let (server, secrets) = server(root.path());
        let port = server
            .apply(&McpServerSettings {
                enabled: true,
                port: 0,
                ..Default::default()
            })
            .await
            .bound_port
            .unwrap();
        let token = secrets.get(TOKEN_SECRET_KEY).unwrap().unwrap();
        let epoch = server.gate.epoch();
        let cancel = lock_std(&server.run).as_ref().unwrap().cancel.clone();
        secrets.fail_put.store(true, Ordering::SeqCst);
        assert!(server.rotate_token().await.is_err());
        assert!(!epoch.is_cancelled());
        assert!(!cancel.is_cancelled());
        assert_eq!(server.status().port, Some(port));
        assert_eq!(secrets.get(TOKEN_SECRET_KEY).unwrap(), Some(token.clone()));
        assert!(ping_status(port, token).await.contains("200"));
    }

    #[tokio::test]
    async fn rotating_token_closes_connections_with_incomplete_headers() {
        let root = tempdir().unwrap();
        let (server, _) = server(root.path());
        let port = server
            .apply(&McpServerSettings {
                enabled: true,
                port: 0,
                ..Default::default()
            })
            .await
            .bound_port
            .unwrap();
        let mut socket = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
        socket
            .write_all(b"POST /mcp HTTP/1.1\r\nHost: ")
            .await
            .unwrap();
        tokio::task::yield_now().await;
        server.rotate_token().await.unwrap();
        let mut byte = [0u8; 1];
        let read = tokio::time::timeout(Duration::from_secs(1), socket.read(&mut byte))
            .await
            .expect("old connection must close after rotation");
        assert!(matches!(read, Ok(0) | Err(_)));
    }

    #[tokio::test]
    async fn connections_hold_capacity_until_drop_and_expire_without_headers() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let address = listener.local_addr().unwrap();
        let slots = Arc::new(Semaphore::new(1));
        let mut listener = LimitedListener {
            listener,
            slots: slots.clone(),
            cancel: CancellationToken::new(),
        };
        let _first = TcpStream::connect(address).await.unwrap();
        let (first, _) = axum::serve::Listener::accept(&mut listener).await;
        assert_eq!(slots.available_permits(), 0);
        let _second = TcpStream::connect(address).await.unwrap();
        assert!(
            tokio::time::timeout(
                Duration::from_millis(10),
                axum::serve::Listener::accept(&mut listener)
            )
            .await
            .is_err()
        );
        drop(first);
        let (mut second, _) = axum::serve::Listener::accept(&mut listener).await;
        second.deadline = Box::pin(tokio::time::sleep(Duration::from_millis(1)));
        let mut byte = [0u8; 1];
        let error = tokio::time::timeout(Duration::from_secs(1), second.read(&mut byte))
            .await
            .unwrap()
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        drop(second);
        assert_eq!(slots.available_permits(), 1);
    }

    #[test]
    fn invalid_stored_token_is_replaced_before_starting() {
        let root = tempdir().unwrap();
        let (server, secrets) = server(root.path());
        secrets.put(TOKEN_SECRET_KEY, "weak-token").unwrap();
        let token = server.ensure_token().unwrap();
        assert_eq!(token.len(), 64);
        assert!(token.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert_eq!(server.ensure_token().unwrap(), token);
    }

    #[test]
    fn legacy_fallback_credentials_are_never_reused() {
        let root = tempdir().unwrap();
        let (server, secrets) = server(root.path());
        let legacy = "ab".repeat(32);
        secrets.put("mcp:bearer_token", &legacy).unwrap();
        let token = server.ensure_token().unwrap();
        assert_ne!(token, legacy);
        assert_eq!(secrets.get(TOKEN_SECRET_KEY).unwrap(), Some(token));
        assert_eq!(secrets.get("mcp:bearer_token").unwrap(), Some(legacy));
    }
}

/// rpc / http 测试共用的最小上下文：空 Bot 表的 DebugManager + 固定 token
#[cfg(test)]
pub(crate) mod tests_support {
    use std::path::PathBuf;
    use std::sync::Arc;

    use ncd_domain::McpServerStatus;

    use super::ServerCtx;
    use crate::gate::GateKeeper;
    use crate::tools::ToolsCtx;

    pub(crate) fn test_server_ctx() -> Arc<ServerCtx> {
        // keep() 不删目录：DebugManager 的落盘是懒加载，测试跑完目录被清会让存储写失败
        let root: PathBuf = tempfile::tempdir().unwrap().keep();
        let debug = crate::tests_support::test_debug_manager(root);
        Arc::new(ServerCtx {
            tools: ToolsCtx {
                debug,
                runtime: None,
                gate: Arc::new(GateKeeper::new()),
                status: Arc::new(|| McpServerStatus {
                    enabled: true,
                    listening: true,
                    port: Some(3999),
                    last_error: None,
                }),
            },
            token: "test-token".to_owned(),
            bound_port: 3999,
            requests: tokio::sync::Semaphore::new(crate::http::MAX_CONCURRENT_REQUESTS),
            cancel: tokio_util::sync::CancellationToken::new(),
        })
    }
}
