//! MCP 服务的生命周期：按设置启停、绑 127.0.0.1、管 bearer token。
//!
//! `apply` 幂等：开着且端口没变就不重启服务；关掉 / 换端口先停旧服务再开。
//! 随机端口（设置里 `port = 0`）把实际端口放进 [`McpApplyOutcome`]，
//! 由接线的一方回填进设置（之后重启就固定了）。

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex, MutexGuard, PoisonError};

use ncd_domain::{McpServerSettings, McpServerStatus};
use ncd_runtime::onebot_debug::DebugManager;
use ncd_traits::SecretStore;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::gate::GateKeeper;
use crate::tools::ToolsCtx;

/// bearer token 在 SecretStore 里的键（和 `app:github_pat` 同一命名风格）
const TOKEN_SECRET_KEY: &str = "mcp:bearer_token";

pub(crate) struct ServerCtx {
    pub tools: ToolsCtx,
    pub token: String,
}

struct Run {
    /// 当时设置里要求的端口（0 = 随机）
    requested_port: u16,
    bound_port: u16,
    cancel: CancellationToken,
    task: JoinHandle<()>,
}

pub struct McpApplyOutcome {
    /// 起服务成功后的实际监听端口；没在听（关着或起失败）为空
    pub bound_port: Option<u16>,
}

pub struct McpServer {
    debug: Arc<DebugManager>,
    secrets: Arc<dyn SecretStore + Send + Sync>,
    gate: Arc<GateKeeper>,
    run: StdMutex<Option<Run>>,
    /// 最近一次 apply 的设置（status 报用）
    config: StdMutex<McpServerSettings>,
    /// 服务刚被打开过（用于收起「从没起过」和「起过停了」的分歧）
    ever_started: AtomicBool,
    last_error: StdMutex<Option<String>>,
}

impl McpServer {
    pub fn new(debug: Arc<DebugManager>, secrets: Arc<dyn SecretStore + Send + Sync>) -> Self {
        Self {
            debug,
            secrets,
            gate: Arc::new(GateKeeper::new()),
            run: StdMutex::new(None),
            config: StdMutex::new(McpServerSettings::default()),
            ever_started: AtomicBool::new(false),
            last_error: StdMutex::new(None),
        }
    }

    /// 权限闸的危险级开关跟设置走，和服务开不开无关（掉了服务也照配，工具到不了闸也无妨）
    pub async fn apply(self: &Arc<Self>, settings: &McpServerSettings) -> McpApplyOutcome {
        self.gate.set_allow_dangerous(settings.allow_dangerous);
        *lock_std(&self.config) = settings.clone();

        if !settings.enabled {
            self.stop_run();
            return McpApplyOutcome { bound_port: None };
        }
        if let Some(bound) = self.current_run_port(settings.port) {
            return McpApplyOutcome {
                bound_port: Some(bound),
            };
        }
        self.stop_run();

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
        let ctx = Arc::new(ServerCtx { tools, token });
        let cancel = CancellationToken::new();
        let shutdown = cancel.clone();
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
        if run.requested_port == requested_port && !run.task.is_finished() {
            Some(run.bound_port)
        } else {
            None
        }
    }

    fn stop_run(&self) {
        if let Some(run) = lock_std(&self.run).take() {
            run.cancel.cancel();
            run.task.abort();
        }
    }

    /// 空着就生成一个 32 字节随机 token 存进 SecretStore（只写一次，之后照读）
    fn ensure_token(&self) -> Result<String, String> {
        match self.secrets.get(TOKEN_SECRET_KEY) {
            Ok(Some(token)) if !token.is_empty() => Ok(token),
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
        self.stop_run();
    }
}

fn lock_std<T>(mutex: &StdMutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use ncd_domain::SecretError;
    use tempfile::tempdir;

    use super::*;
    use crate::tests_support::test_debug_manager;

    #[derive(Default)]
    struct MemorySecrets {
        map: StdMutex<HashMap<String, String>>,
    }

    impl SecretStore for MemorySecrets {
        fn get(&self, key: &str) -> Result<Option<String>, SecretError> {
            Ok(lock_std(&self.map).get(key).cloned())
        }

        fn put(&self, key: &str, value: &str) -> Result<(), SecretError> {
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
            })
            .await;
        assert_eq!(again.bound_port, Some(bound));

        // 关掉：不再听
        server
            .apply(&McpServerSettings {
                enabled: false,
                port: 0,
                allow_dangerous: false,
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
                gate: Arc::new(GateKeeper::new()),
                status: Arc::new(|| McpServerStatus {
                    enabled: true,
                    listening: true,
                    port: Some(3999),
                    last_error: None,
                }),
            },
            token: "test-token".to_owned(),
        })
    }
}
