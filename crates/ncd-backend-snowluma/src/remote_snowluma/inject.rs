//! 隧道 WebUI 注入与 QQ 进程探测

use std::sync::Arc;
use std::time::{Duration, Instant};

use ncd_host::{Host, HostCommand};
use ncd_traits::runtime_backend::BotBackendError;

use super::tunnel::RemoteSnowLumaTunnelEndpoints;
use crate::snowluma::error::SnowLumaWebUiError;
use crate::snowluma::webui_client::{
    HookProcessStatus, ReqwestSnowLumaWebUiClient, SnowLumaWebUiClient,
    snowluma_error_requires_consent,
};

pub async fn remote_qq_running_pid(
    host: &dyn Host,
    qq_id: u64,
) -> Result<Option<u32>, BotBackendError> {
    let script = format!(
        r#"pid="$(pgrep -f -- "qq --no-sandbox -q {qq_id}$" 2>/dev/null | head -n 1)"
if [ -z "$pid" ]; then
  pid="$(pgrep -f -- "qq.*-q {qq_id}$" 2>/dev/null | head -n 1)"
fi
echo "$pid"
"#
    );
    let cmd = HostCommand::new("sh").arg("-c").arg(script);
    let out = host
        .run_to_string(cmd)
        .await
        .map_err(|e| BotBackendError::Io(e.to_string()))?;
    // pgrep 无匹配时常 exit 1;stdout 空视为未运行
    let line = out.stdout.lines().next().unwrap_or("").trim();
    if line.is_empty() {
        return Ok(None);
    }
    line.parse()
        .map(Some)
        .map_err(|_| BotBackendError::InvalidConfig(format!("invalid pgrep pid: {line}")))
}

pub(crate) async fn inject_via_tunnel(
    endpoints: &RemoteSnowLumaTunnelEndpoints,
    qq_pid: u32,
) -> Result<Arc<dyn SnowLumaWebUiClient>, BotBackendError> {
    let client = ReqwestSnowLumaWebUiClient::new(
        endpoints.webui_local_port,
        endpoints.webui_password.clone(),
    )
    .map_err(|e: SnowLumaWebUiError| BotBackendError::Io(e.to_string()))?;
    client
        .wait_ready(Duration::from_secs(90), Box::new(|| false))
        .await
        .map_err(|e| BotBackendError::Io(format!("SnowLuma WebUI wait_ready: {e}")))?;
    client
        .login()
        .await
        .map_err(|e| BotBackendError::Io(format!("SnowLuma WebUI login: {e}")))?;

    // 冷启动刚 spawn QQ 后,SnowLuma daemon 侧扫描器需要时间把该 PID 识别为 Available
    // 立即 load_process 极大概率拿到 "server rejected"(success=false,error 常为空)
    wait_process_available(&client, qq_pid, Duration::from_secs(25)).await;

    // "already" 视为注入成功;其它拒绝若 error 为空则补 /api/processes 快照
    let load_res = client.load_process(qq_pid).await;
    match load_res {
        Ok(_) => {}
        Err(SnowLumaWebUiError::ServerRejected { ref message, .. }) => {
            let m = message.to_lowercase();
            if m.contains("already") {
                tracing::info!(
                    target: "ncd_runtime::remote_snowluma",
                    pid = qq_pid,
                    "load_process reported already loaded/injected; proceeding with existing client"
                );
            } else {
                let e = SnowLumaWebUiError::ServerRejected {
                    endpoint: format!("/api/processes/{qq_pid}/load"),
                    message: message.clone(),
                };
                let mut msg = format!("SnowLuma load_process: {e}");
                if message.trim().is_empty() {
                    match client.list_processes().await {
                        Ok(list) => {
                            if let Some(p) = list.into_iter().find(|p| p.pid == qq_pid) {
                                msg.push_str(&format!(
                                    "; observed pid {} in /api/processes: status={:?} error={:?}",
                                    p.pid, p.status, p.error
                                ));
                            } else {
                                msg.push_str(
                                    "; pid not present in current /api/processes snapshot",
                                );
                            }
                        }
                        Err(list_err) => {
                            msg.push_str(&format!(
                                "; list_processes after rejection also failed: {list_err}"
                            ));
                        }
                    }
                }
                return Err(BotBackendError::Io(msg));
            }
        }
        Err(e) => {
            if snowluma_error_requires_consent(&e) {
                return Err(BotBackendError::Io(format!(
                    "SNOWLUMA_CONSENT_REQUIRED: SnowLuma load_process: {e}"
                )));
            }
            return Err(BotBackendError::Io(format!("SnowLuma load_process: {e}")));
        }
    }

    Ok(Arc::new(client) as Arc<dyn SnowLumaWebUiClient>)
}

/// 等待 /api/processes 出现指定 pid(best-effort,超时不抛错)
async fn wait_process_available(client: &dyn SnowLumaWebUiClient, pid: u32, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    loop {
        if Instant::now() >= deadline {
            return;
        }
        if let Ok(list) = client.list_processes().await {
            if let Some(p) = list.into_iter().find(|p| p.pid == pid) {
                match p.status {
                    HookProcessStatus::Error | HookProcessStatus::Disconnected => return,
                    HookProcessStatus::Available
                    | HookProcessStatus::Loaded
                    | HookProcessStatus::Online
                    | HookProcessStatus::Connecting
                    | HookProcessStatus::Loading => return,
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(700)).await;
    }
}
