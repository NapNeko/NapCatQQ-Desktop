//! 将已归因的 QQ 绑定到 PID、启动时间和入口，再用共享 pidfd 探针停止。
use std::time::Duration;

use ncd_host::{Host, HostCommand};
use ncd_traits::runtime_backend::BotBackendError;
use serde::{Deserialize, Serialize};

const IDENTITY: &str = r#"import json, os, sys
pid = int(sys.argv[1])
try:
    path = '/proc/' + str(pid)
    def birth():
        with open(path + '/stat') as stream:
            fields = stream.read(8192).rsplit(') ', 1)[1].split()
        if fields[0] in ('Z', 'X'):
            raise ProcessLookupError()
        return fields[19]
    started = birth()
    with open(path + '/comm') as stream:
        if stream.read(256).strip() not in ('qq', 'QQ'):
            raise ValueError('not_qq')
    with open(path + '/cmdline', 'rb') as stream:
        executable = stream.read(4097).split(b'\0', 1)[0].decode()
    if len(executable) > 4096 or not os.path.isabs(executable) or started != birth():
        raise ValueError('identity_changed')
    print(json.dumps({'pid': pid, 'start_time': started, 'qq_bin': executable}))
except (FileNotFoundError, ProcessLookupError):
    print('null')
"#;

#[derive(Deserialize, Serialize)]
struct Identity {
    pid: u32,
    start_time: String,
    qq_bin: String,
}

#[derive(Deserialize)]
struct Reply {
    v: u32,
    pid: u32,
    start_time: String,
    state: String,
    unit: Option<String>,
    uid: u32,
}

fn failed(message: &str) -> BotBackendError {
    BotBackendError::Io(message.to_owned())
}

pub(super) async fn stop_attributed(
    host: &dyn Host,
    qq_id: u64,
    pid: u32,
) -> Result<(), BotBackendError> {
    let output = host
        .run_to_string(
            HostCommand::new("python3")
                .args(["-I", "-B", "-c", IDENTITY])
                .arg(pid.to_string())
                .timeout(Duration::from_secs(8)),
        )
        .await
        .map_err(|error| failed(&format!("读取 QQ 身份失败：{error}")))?;
    if !output.success() || output.stdout.len() > 8192 {
        return Err(failed("无法确认目标 QQ 的启动身份，已取消停止"));
    }
    let Some(identity) = serde_json::from_str::<Option<Identity>>(&output.stdout)
        .map_err(|_| failed("QQ 身份返回无效，已取消停止"))?
    else {
        return Ok(());
    };
    if identity.pid != pid
        || identity.start_time.is_empty()
        || !identity
            .start_time
            .bytes()
            .all(|byte| byte.is_ascii_digit())
        || !identity.qq_bin.starts_with('/')
        || identity.qq_bin.contains('\0')
    {
        return Err(failed("QQ 身份不匹配，已取消停止"));
    }
    let mut request = serde_json::json!({
        "v": 1, "action": "stop", "pid": pid,
        "start_time": identity.start_time, "qq_bin": identity.qq_bin, "qq_id": qq_id,
    });
    for elevated in [false, true] {
        let mut command = HostCommand::new("python3")
            .args(["-I", "-B", "-c", ncd_component::LINUX_QQ_PROCESS_PROBE])
            .stdin(serde_json::to_vec(&request).map_err(|_| failed("QQ 停止请求无效"))?)
            .timeout(Duration::from_secs(12));
        if elevated {
            command = command.elevated();
        }
        let output = host
            .run_to_string(command)
            .await
            .map_err(|error| failed(&format!("停止 QQ 失败：{error}")))?;
        if !output.success() || output.stdout.len() > 4096 {
            return Err(failed("QQ 停止未返回有效确认"));
        }
        let reply: Reply =
            serde_json::from_str(&output.stdout).map_err(|_| failed("QQ 停止返回无效"))?;
        if reply.v != 1 || reply.pid != identity.pid || reply.start_time != identity.start_time {
            return Err(failed("QQ 停止返回身份不匹配"));
        }
        if elevated && reply.uid != 0 {
            return Err(failed("QQ 停止返回了不匹配的提权用户身份"));
        }
        match reply.state.as_str() {
            "stopped" | "absent" => return Ok(()),
            "permission" if !elevated => {
                request["expected_uid"] = serde_json::json!(reply.uid);
                continue;
            }
            "replaced" => return Err(failed("目标 QQ 进程已变化，已拒绝停止其它进程")),
            "managed_service" => {
                let unit = reply.unit.filter(|unit| {
                    unit.len() <= 256
                        && unit.ends_with(".service")
                        && unit
                            .bytes()
                            .all(|byte| byte.is_ascii_alphanumeric() || b"-_.@".contains(&byte))
                });
                return Err(failed(&format!(
                    "目标 QQ 由原服务 {} 管理，请先停止或接管该服务",
                    unit.as_deref().unwrap_or("systemd")
                )));
            }
            _ => return Err(failed("目标 QQ 未确认退出，请检查远端权限或原服务重启策略")),
        }
    }
    Err(failed("无法停止目标 QQ"))
}
