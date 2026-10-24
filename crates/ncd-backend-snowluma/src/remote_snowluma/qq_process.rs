//! 固定 PID 与启动时间的 QQ 身份验证和停止。

use std::time::Duration;

use ncd_host::{Host, HostCommand};
use ncd_traits::runtime_backend::BotBackendError;
use serde::{Deserialize, Serialize};

const PROBE: &str = ncd_component::LINUX_QQ_PROCESS_PROBE;

#[derive(Serialize)]
struct Request<'a> {
    v: u32,
    action: &'a str,
    pid: u32,
    start_time: &'a str,
    qq_bin: &'a str,
    qq_id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    expected_uid: Option<u32>,
}

#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
enum State {
    Verified,
    Absent,
    Replaced,
    Permission,
    Stopped,
    ManagedService,
    Error,
}

#[derive(Deserialize)]
struct Reply {
    v: u32,
    pid: u32,
    start_time: String,
    state: State,
    unit: Option<String>,
    uid: u32,
}

#[derive(Debug, PartialEq)]
pub(super) enum Identity {
    Verified,
    Absent,
    Replaced,
}

fn failed(message: &str) -> BotBackendError {
    BotBackendError::Io(message.to_owned())
}

fn validate_reply(
    request: &Request<'_>,
    elevated: bool,
    reply: &Reply,
) -> Result<(), BotBackendError> {
    if reply.v != 1 || reply.pid != request.pid || reply.start_time != request.start_time {
        return Err(failed("QQ 进程操作返回了不匹配的身份"));
    }
    if (elevated && (request.expected_uid.is_none() || reply.uid != 0))
        || (!elevated && request.expected_uid.is_some_and(|uid| uid != reply.uid))
    {
        return Err(failed("QQ 进程操作返回了不匹配的用户身份"));
    }
    Ok(())
}

async fn run(
    host: &dyn Host,
    request: &Request<'_>,
    elevated: bool,
) -> Result<(State, u32), BotBackendError> {
    if request.pid == 0
        || request.qq_id == 0
        || request.start_time.is_empty()
        || !request.start_time.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(failed("尚未确认目标 QQ 进程身份，请先刷新状态"));
    }
    let timeout = Duration::from_secs(if request.action == "stop" { 12 } else { 4 });
    let mut command = HostCommand::new("python3")
        .args(["-I", "-B", "-c", PROBE])
        .stdin(serde_json::to_vec(request).map_err(|_| failed("QQ 进程请求无效"))?)
        .timeout(timeout);
    if elevated {
        command = command.elevated();
    }
    let output = host
        .run_to_string(command)
        .await
        .map_err(|error| failed(&format!("QQ 进程操作失败：{error}")))?;
    if !output.success() || output.stdout.len() > 4096 {
        return Err(failed("QQ 进程身份验证或停止失败，请检查目标进程权限"));
    }
    let reply: Reply =
        serde_json::from_str(&output.stdout).map_err(|_| failed("QQ 进程操作未返回有效确认"))?;
    validate_reply(request, elevated, &reply)?;
    if reply.state == State::ManagedService {
        let unit = reply.unit.as_deref().unwrap_or("未知单元");
        return Err(failed(&format!(
            "目标 QQ 由系统服务 {unit} 管理；请先停止或接管该服务后重试"
        )));
    }
    Ok((reply.state, reply.uid))
}

pub(super) async fn inspect(
    host: &dyn Host,
    qq_bin: &str,
    qq_id: u64,
    pid: u32,
    start_time: &str,
) -> Result<Identity, BotBackendError> {
    let mut request = Request {
        v: 1,
        action: "inspect",
        pid,
        start_time,
        qq_bin,
        qq_id,
        expected_uid: None,
    };
    let (mut state, uid) = run(host, &request, false).await?;
    if state == State::Permission {
        request.expected_uid = Some(uid);
        state = run(host, &request, true).await?.0;
        if state == State::Verified {
            request.action = "snapshot";
            state = run(host, &request, false).await?.0;
        }
    }
    match state {
        State::Verified => Ok(Identity::Verified),
        State::Absent => Ok(Identity::Absent),
        State::Replaced => Ok(Identity::Replaced),
        _ => Err(failed("无法验证目标 QQ 进程身份，请检查远端权限")),
    }
}

pub(super) async fn stop(
    host: &dyn Host,
    qq_bin: &str,
    qq_id: u64,
    pid: u32,
    start_time: &str,
) -> Result<(), BotBackendError> {
    let mut request = Request {
        v: 1,
        action: "stop",
        pid,
        start_time,
        qq_bin,
        qq_id,
        expected_uid: None,
    };
    let (mut state, uid) = run(host, &request, false).await?;
    if state == State::Permission {
        request.expected_uid = Some(uid);
        state = run(host, &request, true).await?.0;
    }
    match state {
        State::Stopped | State::Absent => Ok(()),
        State::Replaced => Err(failed("目标 QQ 进程已变化，已拒绝停止其它进程")),
        _ => Err(failed("目标 QQ 进程未确认退出，停止操作未完成")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn elevated_validation_requires_the_original_users_binding() {
        let mut request = Request {
            v: 1,
            action: "inspect",
            pid: 42,
            start_time: "12345",
            qq_bin: "/opt/QQ/qq",
            qq_id: 10001,
            expected_uid: None,
        };
        let mut reply = Reply {
            v: 1,
            pid: 42,
            start_time: "12345".into(),
            state: State::Permission,
            unit: None,
            uid: 1001,
        };
        assert!(validate_reply(&request, false, &reply).is_ok());
        request.expected_uid = Some(reply.uid);
        reply.uid = 0;
        reply.state = State::Verified;
        assert!(validate_reply(&request, true, &reply).is_ok());
        request.action = "snapshot";
        assert!(validate_reply(&request, false, &reply).is_err());
        reply.uid = 1001;
        assert!(validate_reply(&request, false, &reply).is_ok());
        reply.uid = 2002;
        assert!(validate_reply(&request, true, &reply).is_err());
        assert_eq!(request.expected_uid, Some(1001));
    }

    #[test]
    fn uid_validation_never_replaces_pid_or_birth_validation() {
        let request = Request {
            v: 1,
            action: "stop",
            pid: 42,
            start_time: "12345",
            qq_bin: "/opt/QQ/qq",
            qq_id: 10001,
            expected_uid: Some(1001),
        };
        for (pid, birth) in [(43, "12345"), (42, "54321")] {
            let reply = Reply {
                v: 1,
                pid,
                start_time: birth.into(),
                state: State::Stopped,
                unit: None,
                uid: 0,
            };
            assert!(validate_reply(&request, true, &reply).is_err());
        }
    }
}
