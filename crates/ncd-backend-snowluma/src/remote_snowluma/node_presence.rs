// 未确认 Node 缺失时禁止补启动；提权只检查原用户枚举并绑定的进程身份。
use std::time::Duration;

use ncd_host::{Host, HostCommand};
use ncd_traits::runtime_backend::BotBackendError;
use serde::{Deserialize, Serialize};

use super::super::layout::RemoteSnowLumaLayout;

const NODE_PRESENCE: &str = include_str!("../../resources/node-presence.py");

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
struct Birth {
    pid: u32,
    start_time: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct VerifiedWebuiNode {
    pub pid: u32,
    pub uid: u32,
    pub start_time: String,
    pub exe: String,
    #[serde(default)]
    pub cwd: String,
    #[serde(default)]
    pub entry: String,
    #[serde(default)]
    pub ns_pids: Vec<u32>,
    #[serde(default)]
    pub pid_namespace: String,
}

#[derive(Clone, Debug)]
pub struct ResolvedWebuiEndpoint {
    pub host: String,
    pub port: u16,
    pub node: VerifiedWebuiNode,
}

#[derive(Deserialize)]
struct Listener {
    host: std::net::IpAddr,
    port: u16,
}

#[derive(Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
enum Reply {
    Present {
        node: VerifiedWebuiNode,
        snapshot: Vec<Birth>,
        host: Option<String>,
        port: Option<u16>,
        listeners: Option<Vec<Listener>>,
    },
    Absent {
        snapshot: Vec<Birth>,
    },
    PermissionDenied {
        snapshot: Vec<Birth>,
    },
    Snapshot {
        snapshot: Vec<Birth>,
    },
    Error {
        message: String,
    },
}

fn unknown() -> BotBackendError {
    BotBackendError::Io("无法确认远端 SnowLuma Node 身份，未启动新实例".into())
}

fn valid_snapshot(snapshot: &[Birth]) -> bool {
    snapshot.len() <= 256
        && snapshot.iter().enumerate().all(|(index, item)| {
            item.pid != 0
                && !item.start_time.is_empty()
                && item.start_time.bytes().all(|byte| byte.is_ascii_digit())
                && snapshot[..index].iter().all(|other| other.pid != item.pid)
        })
}

async fn run(
    host: &dyn Host,
    command: HostCommand,
    phase: &'static str,
) -> Result<Reply, BotBackendError> {
    let started = std::time::Instant::now();
    let output = host.run_to_string(command).await.map_err(|error| {
        BotBackendError::Io(format!(
            "检查远端 Node 身份失败（{phase}，{}ms）：{error}",
            started.elapsed().as_millis()
        ))
    })?;
    tracing::info!(
        target: "ncd_backend_snowluma::remote",
        "Node 只读探针 {phase} 完成，耗时 {}ms",
        started.elapsed().as_millis()
    );
    if output.stdout.len() > 64 * 1024 {
        return Err(unknown());
    }
    let reply: Reply = serde_json::from_str(output.stdout.trim()).map_err(|_| unknown())?;
    if let Reply::Error { message } = reply {
        return Err(BotBackendError::Io(message));
    }
    if !output.success() {
        return Err(unknown());
    }
    Ok(reply)
}

async fn checked_reply(
    host: &dyn Host,
    layout: &RemoteSnowLumaLayout,
    endpoint: bool,
) -> Result<(u32, Reply), BotBackendError> {
    let output = host
        .run_to_string(
            HostCommand::new("id")
                .arg("-u")
                .timeout(Duration::from_secs(5)),
        )
        .await
        .map_err(|_| unknown())?;
    let uid = output
        .stdout
        .trim()
        .parse::<u32>()
        .ok()
        .filter(|_| output.success())
        .ok_or_else(unknown)?;
    let command = |mode: &str| {
        HostCommand::new("python3")
            .arg("-I")
            .arg("-B")
            .arg("-c")
            .arg(NODE_PRESENCE)
            .arg(mode)
            .arg(uid.to_string())
            .arg(&layout.node_bin)
            .arg(&layout.paths.snowluma_dir)
            .timeout(Duration::from_secs(if endpoint { 12 } else { 8 }))
    };
    let mut reply = run(
        host,
        command(if endpoint { "endpoint_scan" } else { "scan" }),
        if endpoint { "endpoint_scan" } else { "scan" },
    )
    .await?;
    if let Reply::PermissionDenied { snapshot } = reply {
        if snapshot.is_empty() || !valid_snapshot(&snapshot) {
            return Err(unknown());
        }
        let candidates = serde_json::to_string(&snapshot).map_err(|_| unknown())?;
        // The elevated helper never enumerates /proc or launches a service.
        reply = run(
            host,
            command(if endpoint {
                "endpoint_inspect"
            } else {
                "inspect"
            })
            .arg(candidates)
            .elevated(),
            if endpoint {
                "endpoint_inspect"
            } else {
                "inspect"
            },
        )
        .await?;
        let inspected = match &reply {
            Reply::Present { snapshot, .. } | Reply::Absent { snapshot } => snapshot,
            _ => {
                return Err(BotBackendError::Io(
                    "只读提权无法确认远端 Node 身份，请检查 sudo 权限；未启动新实例".into(),
                ));
            }
        };
        if *inspected != snapshot {
            return Err(unknown());
        }
        // 此预算也包含 SSH 队列和传输；跨地域连接的正常快照可能超过两秒。
        match run(
            host,
            command("snapshot").timeout(Duration::from_secs(8)),
            "snapshot",
        )
        .await?
        {
            Reply::Snapshot { snapshot: current } if current == snapshot => {}
            _ => {
                return Err(BotBackendError::Io(
                    "Node 进程在只读验证期间变化，未启动新实例".into(),
                ));
            }
        }
    }
    Ok((uid, reply))
}

fn valid_node(node: &VerifiedWebuiNode, uid: u32, snapshot: &[Birth]) -> bool {
    valid_snapshot(snapshot)
        && node.uid == uid
        && node.exe.starts_with('/')
        && snapshot
            .iter()
            .any(|item| item.pid == node.pid && item.start_time == node.start_time)
}

pub(super) async fn is_present(
    host: &dyn Host,
    layout: &RemoteSnowLumaLayout,
) -> Result<bool, BotBackendError> {
    let (uid, reply) = checked_reply(host, layout, false).await?;
    match reply {
        Reply::Present { node, snapshot, .. } if valid_node(&node, uid, &snapshot) => Ok(true),
        Reply::Absent { snapshot } if valid_snapshot(&snapshot) => Ok(false),
        _ => Err(unknown()),
    }
}

pub(super) async fn resolve_endpoint(
    host: &dyn Host,
    layout: &RemoteSnowLumaLayout,
) -> Result<ResolvedWebuiEndpoint, BotBackendError> {
    let started = std::time::Instant::now();
    let (uid, reply) = checked_reply(host, layout, true).await?;
    let endpoint = endpoint_from_reply(uid, reply)?;
    tracing::info!(
        target: "ncd_backend_snowluma::remote",
        node_pid = endpoint.node.pid,
        remote_host = %endpoint.host,
        port = endpoint.port,
        elapsed_ms = started.elapsed().as_millis() as u64,
        "已按 Node socket 归属和 HTTP 响应确认 SnowLuma WebUI 端口"
    );
    Ok(endpoint)
}

fn endpoint_from_reply(uid: u32, reply: Reply) -> Result<ResolvedWebuiEndpoint, BotBackendError> {
    match reply {
        Reply::Present {
            node,
            snapshot,
            host: Some(host),
            port: Some(port),
            listeners: Some(listeners),
        } if valid_node(&node, uid, &snapshot)
            && node.cwd.starts_with('/')
            && node.entry.starts_with('/')
            && node.entry.ends_with("/index.mjs")
            && node.ns_pids.first() == Some(&node.pid)
            && node.ns_pids.len() <= 32
            && node.ns_pids.iter().all(|pid| *pid != 0)
            && node
                .pid_namespace
                .strip_prefix("pid:[")
                .and_then(|value| value.strip_suffix(']'))
                .is_some_and(|value| {
                    !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
                })
            && port != 0
            && !listeners.is_empty()
            && listeners.len() <= 16
            && listeners
                .iter()
                .all(|listener| listener.port != 0 && !listener.host.is_unspecified())
            && host
                .parse::<std::net::IpAddr>()
                .ok()
                .is_some_and(|address| {
                    listeners
                        .iter()
                        .any(|listener| listener.port == port && listener.host == address)
                }) =>
        {
            Ok(ResolvedWebuiEndpoint { host, port, node })
        }
        Reply::Absent { snapshot } if valid_snapshot(&snapshot) => Err(BotBackendError::Io(
            "已确认远端 SnowLuma Node 未运行，无法确定 WebUI 端口".into(),
        )),
        _ => Err(unknown()),
    }
}

#[cfg(test)]
#[path = "node_endpoint_tests.rs"]
mod endpoint_tests;
