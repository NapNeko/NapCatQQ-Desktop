// QQ 元数据受 /proc 限制时，只提权读取已归因进程；恢复服务仍由原用户启动。
use std::time::Duration;

use ncd_host::{Host, HostCommand};
use ncd_traits::runtime_backend::BotBackendError;
use serde::{Deserialize, Serialize};

const INSPECT_TARGET: &str = include_str!("../../resources/novnc-metadata.py");

#[derive(Deserialize, Serialize)]
pub(super) struct Target {
    pid: u32,
    uid: u32,
    start_time: String,
    exe: String,
    display: String,
    xauthority: Option<String>,
}

#[derive(Deserialize)]
struct ProbeError {
    code: String,
    message: String,
}

#[derive(Deserialize)]
struct Reply {
    target: Option<Target>,
    error: Option<ProbeError>,
}

fn invalid_reply() -> BotBackendError {
    BotBackendError::Io("目标 QQ 图形会话检查未返回有效身份".into())
}

pub(super) async fn inspect(
    host: &dyn Host,
    qq_exe: &str,
    pid: u32,
    start_time: &str,
    qq_id: u64,
) -> Result<Target, BotBackendError> {
    let identity = host
        .run_to_string(HostCommand::new("id").arg("-u"))
        .await
        .map_err(|_| BotBackendError::Io("无法确认当前 SSH 用户身份，未恢复 QQ 界面".into()))?;
    let uid = identity.stdout.trim().parse::<u32>().ok();
    let uid = uid
        .filter(|_| identity.success())
        .ok_or_else(invalid_reply)?;
    let command = HostCommand::new("python3")
        .arg("-I")
        .arg("-B")
        .arg("-c")
        .arg(INSPECT_TARGET)
        .arg(qq_exe)
        .arg(pid.to_string())
        .arg(start_time)
        .arg(qq_id.to_string())
        .arg(uid.to_string())
        .timeout(Duration::from_secs(8));
    let output = host
        .run_to_string(command.clone())
        .await
        .map_err(|error| BotBackendError::Io(format!("检查 QQ 图形会话失败：{error}")))?;
    let mut reply: Reply =
        serde_json::from_str(output.stdout.trim()).map_err(|_| invalid_reply())?;
    if reply
        .error
        .as_ref()
        .is_some_and(|error| error.code == "permission_denied")
    {
        // 只重跑固定的只读探针；不能把含 Popen 的恢复脚本整体以 root 执行。
        let elevated = host
            .run_to_string(command.elevated())
            .await
            .map_err(|error| {
                BotBackendError::Io(format!(
                    "读取 QQ 图形会话元数据需要只读提权，验证失败：{error}"
                ))
            })?;
        reply = serde_json::from_str(elevated.stdout.trim()).map_err(|_| {
            BotBackendError::Io("只读提权未能读取 QQ 图形会话，请检查 sudo 权限".into())
        })?;
    }
    if let Some(error) = reply.error {
        return Err(BotBackendError::Io(error.message));
    }
    let target = reply.target.ok_or_else(invalid_reply)?;
    if target.pid != pid
        || target.uid != uid
        || target.start_time != start_time
        || !target.exe.starts_with('/')
        || target.display.is_empty()
    {
        return Err(invalid_reply());
    }
    Ok(target)
}
