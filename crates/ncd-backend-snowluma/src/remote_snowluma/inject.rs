//! 隧道 WebUI 注入与 QQ 进程探测

use std::sync::Arc;
use std::time::{Duration, Instant};

use ncd_component::linux_qq_running_pid_script;
use ncd_host::{Host, HostCommand, HostPath};
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
    remote_qq_running_pid_with_hint(host, qq_id, None, None, false).await
}

/// `pid_file`：Desktop 写下的 `pid_bot_<qq>`，导入接管时优先认。
/// `qq_bin`：库存/布局里的 QQ 可执行文件，用来对齐 /proc/pid/exe。
/// `allow_single_main_fallback`：无任何账号证据时允许按「整机唯一 QQ 主进程」归因，
/// 只有 reconcile（导入接管 / 冷启动恢复）传 true；start/status/stop 必须传 false。
pub async fn remote_qq_running_pid_with_hint(
    host: &dyn Host,
    qq_id: u64,
    pid_file: Option<&str>,
    qq_bin: Option<&str>,
    allow_single_main_fallback: bool,
) -> Result<Option<u32>, BotBackendError> {
    let script = linux_qq_running_pid_script(qq_id, pid_file, qq_bin, allow_single_main_fallback);
    let cmd = HostCommand::new("sh").arg("-c").arg(script);
    let out = host
        .run_to_string(cmd)
        .await
        .map_err(|e| BotBackendError::Io(e.to_string()))?;
    if !out.success() {
        return Err(BotBackendError::Io(format!(
            "远端 QQ 进程探测失败: exit={:?} stderr={}",
            out.exit_code,
            out.stderr.trim()
        )));
    }
    let line = out.stdout.lines().next().unwrap_or("").trim();
    if line.is_empty() {
        return Ok(None);
    }
    line.parse()
        .map(Some)
        .map_err(|_| BotBackendError::InvalidConfig(format!("invalid qq pid: {line}")))
}

// 重登只接受主进程的精确账号参数，不能沿用 Ptlogin 或命令行子串归因。
pub(crate) async fn remote_qq_relogin_pid(
    host: &dyn Host,
    qq_id: u64,
    qq_bin: &str,
) -> Result<Option<u32>, BotBackendError> {
    let Some(pid) = remote_qq_running_pid_with_hint(host, qq_id, None, Some(qq_bin), false).await?
    else {
        return Ok(None);
    };
    if pid == 0 {
        return Ok(None);
    }
    let cmdline = match host
        .read_file(&HostPath::from_posix(format!("/proc/{pid}/cmdline")))
        .await
    {
        Ok(value) => value,
        Err(ncd_host::HostError::PathNotFound { .. }) => return Ok(None),
        Err(error) => return Err(BotBackendError::Io(error.to_string())),
    };
    if !relogin_cmdline_matches(&cmdline, &qq_id.to_string()) {
        return Ok(None);
    }
    let executable = host
        .run_to_string(
            HostCommand::new("readlink")
                .args(["-f", "--", qq_bin, &format!("/proc/{pid}/exe")])
                .timeout(Duration::from_secs(3)),
        )
        .await
        .map_err(|error| BotBackendError::Io(error.to_string()))?;
    let paths: Vec<_> = executable.stdout.lines().collect();
    if !executable.success() || paths.len() != 2 || paths[0].is_empty() || paths[0] != paths[1] {
        return Ok(None);
    }
    Ok(Some(pid))
}

fn relogin_cmdline_matches(bytes: &[u8], expected_uin: &str) -> bool {
    if bytes.len() > 64 * 1024 || bytes.last() != Some(&0) {
        return false;
    }
    let Ok(cmdline) = std::str::from_utf8(bytes) else {
        return false;
    };
    let mut args = cmdline.trim_end_matches('\0').split('\0');
    if args.next().is_none_or(str::is_empty) {
        return false;
    }
    let mut found = false;
    while let Some(arg) = args.next() {
        if arg == "--type" || arg.starts_with("--type=") {
            return false;
        }
        let account = if matches!(arg, "-q" | "--qq" | "--qq-id") {
            args.next()
        } else {
            arg.strip_prefix("-q")
                .or_else(|| arg.strip_prefix("--qq="))
                .or_else(|| arg.strip_prefix("--qq-id="))
        };
        if matches!(arg, "-q" | "--qq" | "--qq-id") && account.is_none() {
            return false;
        }
        if let Some(account) = account {
            if account != expected_uin {
                return false;
            }
            found = true;
        }
    }
    found
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
        .wait_ready(Duration::from_secs(20), Box::new(|| false))
        .await
        .map_err(|e| BotBackendError::Io(format!("SnowLuma WebUI wait_ready: {e}")))?;
    client
        .login()
        .await
        .map_err(|e| BotBackendError::Io(format!("SnowLuma WebUI login: {e}")))?;

    // 冷启动刚 spawn QQ 后,SnowLuma daemon 侧扫描器需要时间把该 PID 识别为 Available
    // 立即 load_process 极大概率拿到 "server rejected"(success=false,error 常为空)
    wait_process_available(&client, qq_pid, Duration::from_secs(12)).await;

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

#[cfg(test)]
mod tests {
    use super::relogin_cmdline_matches;

    #[test]
    fn relogin_accepts_only_exact_account_arguments_of_the_main_process() {
        for args in [
            vec!["/opt/QQ/qq", "-q", "2550419068"],
            vec!["/opt/QQ/qq", "-q2550419068"],
            vec!["/opt/QQ/qq", "--qq", "2550419068"],
            vec!["/opt/QQ/qq", "--qq=2550419068"],
            vec!["/opt/QQ/qq", "--qq-id", "2550419068"],
            vec!["/opt/QQ/qq", "--qq-id=2550419068", "--no-sandbox"],
        ] {
            let bytes = format!("{}\0", args.join("\0"));
            assert!(relogin_cmdline_matches(bytes.as_bytes(), "2550419068"));
        }
    }

    #[test]
    fn relogin_rejects_other_accounts_helpers_and_ambiguous_arguments() {
        for args in [
            vec!["/opt/QQ/qq"],
            vec!["/opt/QQ/qq", "-q", "2707600964"],
            vec!["/opt/QQ/qq", "-q", "25504190680"],
            vec!["/opt/QQ/qq", "--qq=25504190680"],
            vec!["/opt/QQ/qq", "--user-data-dir=/tmp/2550419068"],
            vec!["/opt/QQ/qq", "2550419068"],
            vec!["/opt/QQ/qq", "-q", "2550419068", "--type=renderer"],
            vec!["/opt/QQ/qq", "--type", "utility", "-q2550419068"],
            vec!["/opt/QQ/qq", "-q2550419068", "--qq=2707600964"],
            vec!["/opt/QQ/qq", "-q"],
            vec!["/opt/QQ/qq", "-q", "", "2550419068"],
        ] {
            let bytes = format!("{}\0", args.join("\0"));
            assert!(!relogin_cmdline_matches(bytes.as_bytes(), "2550419068"));
        }
        assert!(!relogin_cmdline_matches(
            b"/opt/QQ/qq\0-q2550419068",
            "2550419068"
        ));
        assert!(!relogin_cmdline_matches(
            b"\xff\0-q2550419068\0",
            "2550419068"
        ));
    }
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
