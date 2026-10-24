// 把已绑定的宿主机 QQ PID 映射到 SnowLuma Node 可见的 PID 层级。
use std::time::Duration;

use ncd_host::{Host, HostCommand, Os};
use ncd_traits::runtime_backend::BotBackendError;
use serde::{Deserialize, Serialize};

use super::layout::RemoteSnowLumaLayout;
use super::probe::VerifiedWebuiNode;

const PROCESS_PID: &str = include_str!("../../resources/webui-process-pid.py");

#[derive(Serialize)]
struct BoundNode<'a> {
    pid: u32,
    uid: u32,
    start_time: &'a str,
    exe: &'a str,
    cwd: &'a str,
    entry: &'a str,
    ns_pids: &'a [u32],
    pid_namespace: &'a str,
}

#[derive(Serialize)]
struct BoundQq<'a> {
    pid: u32,
    start_time: &'a str,
    exe: &'a str,
}

#[derive(Serialize)]
struct Request<'a> {
    node: BoundNode<'a>,
    qq: BoundQq<'a>,
    node_exe: &'a str,
    install_dir: &'a str,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct Identity {
    pid: u32,
    uid: u32,
    start_time: String,
    ns_pids: Vec<u32>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
struct Snapshot {
    node: Identity,
    qq: Identity,
}

#[derive(Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
enum Reply {
    Mapped { process_pid: u32, snapshot: Snapshot },
    PermissionDenied,
    Snapshot { snapshot: Snapshot },
    Error { message: String },
}

fn unknown() -> BotBackendError {
    BotBackendError::Io("无法确认 QQ 在 SnowLuma Node 中的 PID，未尝试注入".into())
}

fn valid_birth(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())
}

fn valid_pids(pids: &[u32], host_pid: u32) -> bool {
    !pids.is_empty() && pids.len() <= 32 && pids[0] == host_pid && pids.iter().all(|pid| *pid > 0)
}

fn mapped(
    reply: Reply,
    node: &VerifiedWebuiNode,
    qq_pid: u32,
    qq_start_time: &str,
) -> Result<(u32, Snapshot), BotBackendError> {
    let Reply::Mapped { process_pid, snapshot } = reply else {
        return Err(unknown());
    };
    if snapshot.node.pid != node.pid
        || snapshot.node.uid != node.uid
        || snapshot.node.start_time != node.start_time
        || snapshot.node.ns_pids != node.ns_pids
        || snapshot.qq.pid != qq_pid
        || snapshot.qq.uid != node.uid
        || snapshot.qq.start_time != qq_start_time
        || !valid_pids(&snapshot.node.ns_pids, node.pid)
        || !valid_pids(&snapshot.qq.ns_pids, qq_pid)
        || snapshot.node.ns_pids.len() != snapshot.qq.ns_pids.len()
        || snapshot.qq.ns_pids.last().copied() != Some(process_pid)
    {
        return Err(unknown());
    }
    Ok((process_pid, snapshot))
}

async fn run(host: &dyn Host, command: HostCommand) -> Result<Reply, BotBackendError> {
    let output = host.run_to_string(command).await.map_err(|_| unknown())?;
    let reply: Reply = serde_json::from_str(output.stdout.trim()).map_err(|_| unknown())?;
    if let Reply::Error { message } = reply {
        return Err(BotBackendError::Io(message));
    }
    if !output.success() {
        return Err(unknown());
    }
    Ok(reply)
}

pub(super) async fn resolve(
    host: &dyn Host,
    layout: &RemoteSnowLumaLayout,
    node: &VerifiedWebuiNode,
    qq_pid: u32,
    qq_start_time: &str,
) -> Result<u32, BotBackendError> {
    if host.os() != Os::Linux
        || node.pid == 0
        || qq_pid == 0
        || qq_pid == node.pid
        || !valid_birth(&node.start_time)
        || !valid_birth(qq_start_time)
        || !valid_pids(&node.ns_pids, node.pid)
    {
        return Err(unknown());
    }
    let request = serde_json::to_string(&Request {
        node: BoundNode {
            pid: node.pid,
            uid: node.uid,
            start_time: &node.start_time,
            exe: &node.exe,
            cwd: &node.cwd,
            entry: &node.entry,
            ns_pids: &node.ns_pids,
            pid_namespace: &node.pid_namespace,
        },
        qq: BoundQq { pid: qq_pid, start_time: qq_start_time, exe: &layout.qq_bin },
        node_exe: &layout.node_bin,
        install_dir: &layout.paths.snowluma_dir,
    })
    .map_err(|_| unknown())?;
    let command = |mode: &str| {
        HostCommand::new("python3")
            .arg("-I")
            .arg("-B")
            .arg("-c")
            .arg(PROCESS_PID)
            .arg(mode)
            .arg(&request)
            .timeout(Duration::from_secs(8))
    };
    let mut reply = run(host, command("inspect")).await?;
    let elevated = matches!(&reply, Reply::PermissionDenied);
    if elevated {
        reply = run(host, command("elevated").elevated()).await?;
    }
    let (process_pid, verified) = mapped(reply, node, qq_pid, qq_start_time)?;
    if elevated {
        match run(host, command("snapshot")).await? {
            Reply::Snapshot { snapshot } if snapshot == verified => {}
            _ => return Err(unknown()),
        }
    }
    Ok(process_pid)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node() -> VerifiedWebuiNode {
        VerifiedWebuiNode {
            pid: 429940,
            uid: 1001,
            start_time: "111".into(),
            exe: "/opt/sl/node".into(),
            cwd: "/opt/sl".into(),
            entry: "/opt/sl/index.mjs".into(),
            ns_pids: vec![429940, 19],
            pid_namespace: "pid:[42]".into(),
        }
    }

    fn snapshot(node: &VerifiedWebuiNode) -> Snapshot {
        Snapshot {
            node: Identity {
                pid: node.pid,
                uid: node.uid,
                start_time: node.start_time.clone(),
                ns_pids: node.ns_pids.clone(),
            },
            qq: Identity {
                pid: 430336,
                uid: 1001,
                start_time: "222".into(),
                ns_pids: vec![430336, 46],
            },
        }
    }

    #[test]
    fn accepts_node_visible_pid_without_changing_host_identity() {
        let node = node();
        let reply = Reply::Mapped { process_pid: 46, snapshot: snapshot(&node) };
        let (pid, verified) = mapped(reply, &node, 430336, "222").unwrap();
        assert_eq!(pid, 46);
        assert_eq!(verified.qq.pid, 430336);
    }

    #[test]
    fn unisolated_processes_keep_the_host_pid() {
        let mut node = node();
        node.ns_pids = vec![node.pid];
        let mut snapshot = snapshot(&node);
        snapshot.qq.ns_pids = vec![snapshot.qq.pid];
        let reply = Reply::Mapped { process_pid: 430336, snapshot };
        assert_eq!(mapped(reply, &node, 430336, "222").unwrap().0, 430336);
    }

    #[test]
    fn rejects_wrong_uid_birth_namespace_depth_or_mapping() {
        let node = node();
        for changed in 0..5 {
            let mut snapshot = snapshot(&node);
            let mut process_pid = 46;
            match changed {
                0 => snapshot.qq.uid = 2000,
                1 => snapshot.qq.start_time = "333".into(),
                2 => snapshot.node.ns_pids = vec![429940, 20],
                3 => snapshot.qq.ns_pids = vec![430336],
                _ => process_pid = 430336,
            }
            let reply = Reply::Mapped { process_pid, snapshot };
            assert!(mapped(reply, &node, 430336, "222").is_err());
        }
    }
}
