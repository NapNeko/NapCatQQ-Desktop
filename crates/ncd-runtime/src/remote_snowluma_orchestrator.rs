//! 远端 SnowLuma daemon / bot 启停：编排委托 [`remote_snowluma_stack`]，Bot 用 `QQComponent` + 短 detach。

use std::time::Duration;

use ncd_component::{Component, LaunchArgs, QQComponent, QQ_MAIN_NATIVE, set_remote_qq_package_main};
use ncd_host::{Host, HostPath};
use serde_json::json;

use crate::runtime_backend::BotBackendError;

pub use crate::remote_snowluma_stack::{resolve_remote_bash, run_remote_bash as run_sh};

use crate::remote_snowluma_layout::{
    DEFAULT_DISPLAY_NUM, DEFAULT_NOVNC_PORT, DEFAULT_VNC_PORT, DEFAULT_WEBUI_PORT,
    RemoteSnowLumaLayout, SnowLumaRemotePaths, shell_single_quote,
};
use crate::remote_snowluma_stack::{
    ensure_stack_running, is_stack_ready, run_remote_bash, stack_stop,
    wait_webui_tcp as wait_webui_tcp_on_host,
};

fn display_str(num: i32) -> String {
    format!(":{num}")
}

pub async fn remote_daemon_already_ready(
    host: &dyn Host,
    paths: &SnowLumaRemotePaths,
) -> Result<bool, BotBackendError> {
    is_stack_ready(host, paths).await
}

pub async fn daemon_start(host: &dyn Host, layout: &RemoteSnowLumaLayout) -> Result<(), BotBackendError> {
    ensure_stack_running(host, layout).await
}

pub async fn daemon_stop(host: &dyn Host, paths: &SnowLumaRemotePaths) -> Result<(), BotBackendError> {
    stack_stop(host, paths).await
}

pub async fn wait_webui_tcp(host: &dyn Host, port: i32, timeout: Duration) -> Result<(), BotBackendError> {
    wait_webui_tcp_on_host(host, port, timeout).await
}

async fn prepare_bot_launch_env(host: &dyn Host) -> Result<Option<String>, BotBackendError> {
    let script = r#"libgcc_path=$(ldconfig -p 2>/dev/null | grep -m1 'libgcc_s.so.1' | awk '{print $NF}') || true
if [ -n "$libgcc_path" ] && [ -L "$libgcc_path" ]; then
  real_path=$(readlink -f "$libgcc_path")
  rm -f "$libgcc_path" 2>/dev/null && ln "$real_path" "$libgcc_path" 2>/dev/null || true
  libgcc_path=$(ldconfig -p 2>/dev/null | grep -m1 'libgcc_s.so.1' | awk '{print $NF}') || true
fi
if [ -f /proc/sys/kernel/yama/ptrace_scope ]; then
  cur=$(cat /proc/sys/kernel/yama/ptrace_scope 2>/dev/null || echo "")
  if [ "$cur" != "0" ]; then
    sudo -n sysctl -w kernel.yama.ptrace_scope=0 >/dev/null 2>&1 || true
  fi
fi
echo "${libgcc_path:-}"
"#;
    let out = run_remote_bash(host, script).await?;
    let line = out.lines().last().unwrap_or("").trim();
    if line.is_empty() {
        Ok(None)
    } else {
        Ok(Some(line.to_string()))
    }
}

async fn bot_pid_if_running(host: &dyn Host, paths: &SnowLumaRemotePaths, qq_id: &str) -> Result<Option<u32>, BotBackendError> {
    let pidfile = shell_single_quote(&paths.pid_bot_path(qq_id));
    let script = format!(
        r#"pidfile={pidfile}
if [ -f "$pidfile" ]; then
  existing=$(cat "$pidfile" 2>/dev/null || echo "")
  if [ -n "$existing" ] && kill -0 "$existing" 2>/dev/null; then
    echo "$existing"
    exit 0
  fi
  rm -f "$pidfile"
fi
"#
    );
    let out = run_remote_bash(host, &script).await?;
    let line = out.lines().last().unwrap_or("").trim();
    if line.is_empty() {
        return Ok(None);
    }
    match line.parse::<u32>() {
        Ok(n) => Ok(Some(n)),
        Err(_) => Ok(None),
    }
}

pub async fn bot_cold_start(
    host: &dyn Host,
    layout: &RemoteSnowLumaLayout,
    qq_id: &str,
    uin: &str,
) -> Result<u32, BotBackendError> {
    let paths = &layout.paths;
    if let Some(pid) = bot_pid_if_running(host, paths, qq_id).await? {
        return Ok(pid);
    }

    let rotate = format!(
        r#"mkdir -p {log_dir}
if [ -f {log} ]; then mv -f {log} "{log}.prev" 2>/dev/null || true; fi
: > {log}"#,
        log_dir = shell_single_quote(&paths.log_dir),
        log = shell_single_quote(&paths.log_bot_path(qq_id)),
    );
    run_remote_bash(host, &rotate).await?;

    let install_base = HostPath::from_posix(format!("{}/Napcat", layout.home));
    set_remote_qq_package_main(host, &install_base, QQ_MAIN_NATIVE)
        .await
        .map_err(|e| BotBackendError::InvalidConfig(format!("SnowLuma 启动前切换 QQ 入口失败: {e}")))?;

    let qq = QQComponent::default_v3_2_25(install_base);
    let mut launch_args = LaunchArgs::default();
    launch_args.extra_args = vec![
        "--no-sandbox".into(),
        "--disable-gpu-sandbox".into(),
        "-q".into(),
        qq_id.to_string(),
    ];
    launch_args.extra_env.push((
        "DISPLAY".to_string(),
        display_str(DEFAULT_DISPLAY_NUM),
    ));
    let cmd = qq
        .launch_command(host, &launch_args)
        .map_err(|e| BotBackendError::InvalidConfig(e.to_string()))?;

    let ld_preload = prepare_bot_launch_env(host).await?;
    let rt = shell_single_quote(&paths.runtime_dir);
    let log_dir = shell_single_quote(&paths.log_dir);
    let log = shell_single_quote(&paths.log_bot_path(qq_id));
    let pidfile = shell_single_quote(&paths.pid_bot_path(qq_id));
    let prog = shell_single_quote(&cmd.program);
    let mut qq_parts = vec![prog];
    for a in &cmd.args {
        qq_parts.push(shell_single_quote(a));
    }
    let qq_invoke = qq_parts.join(" ");
    let display = display_str(DEFAULT_DISPLAY_NUM);
    let ld_fragment = ld_preload
        .as_ref()
        .map(|p| format!("LD_PRELOAD={} ", shell_single_quote(p)))
        .unwrap_or_default();
    let dbus = shell_single_quote(&paths.dbus_env);
    let script = format!(
        r#"umask 077
mkdir -p {rt} {log_dir}
if [ -f {dbus} ]; then . {dbus}; fi
DISPLAY="{display}" nohup env {ld_fragment}{qq_invoke} > {log} 2>&1 </dev/null &
echo $! > {pidfile}
sleep 0.5
pid=$(cat {pidfile})
if ! kill -0 "$pid" 2>/dev/null; then
  echo "bot 启动后立即退出" >&2
  exit 3
fi
echo "$pid"
"#
    );
    let out = run_remote_bash(host, &script).await?;
    let pid: u32 = out
        .lines()
        .last()
        .unwrap_or("")
        .trim()
        .parse()
        .map_err(|_| BotBackendError::Io(format!("bot start 未返回 pid: {out}")))?;

    write_status_bot_json(host, paths, qq_id, uin, pid, true).await?;
    Ok(pid)
}

async fn write_status_bot_json(
    host: &dyn Host,
    paths: &SnowLumaRemotePaths,
    qq_id: &str,
    uin: &str,
    pid: u32,
    running: bool,
) -> Result<(), BotBackendError> {
    let started_at = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let uin_val: serde_json::Value = if uin.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::Value::String(uin.to_string())
    };
    let payload = json!({
        "qq_id": qq_id,
        "uin": uin_val,
        "pid": pid,
        "running": running,
        "started_at": started_at,
    });
    let bytes = serde_json::to_vec_pretty(&payload).map_err(|e| BotBackendError::Json(e.to_string()))?;
    let path = HostPath::from_posix(paths.status_bot_path(qq_id));
    host.write_file(&path, &bytes)
        .await
        .map_err(|e| BotBackendError::Io(e.to_string()))?;
    Ok(())
}

pub async fn bot_stop(host: &dyn Host, paths: &SnowLumaRemotePaths, qq_id: &str) -> Result<(), BotBackendError> {
    let rt = shell_single_quote(&paths.runtime_dir);
    let qq_id_q = shell_single_quote(qq_id);
    let status = shell_single_quote(&paths.status_bot_path(qq_id));
    let script = format!(
        r#"RUNTIME_DIR={rt}
qq_id={qq_id_q}
pidfile="$RUNTIME_DIR/pid_bot_$qq_id"
if [ -f "$pidfile" ]; then
  pid=$(cat "$pidfile" 2>/dev/null || echo "")
  if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
    kill "$pid" 2>/dev/null || true
    i=0
    while [ "$i" -lt 20 ] && kill -0 "$pid" 2>/dev/null; do sleep 0.5; i=$((i+1)); done
    kill -9 "$pid" 2>/dev/null || true
  fi
  rm -f "$pidfile"
fi
echo '{{"qq_id":"'"$qq_id"'","running":false}}' > {status}
"#
    );
    let _ = run_remote_bash(host, &script).await;
    Ok(())
}

pub async fn write_status_daemon_json(
    host: &dyn Host,
    paths: &SnowLumaRemotePaths,
    running: bool,
    ready: bool,
) -> Result<(), BotBackendError> {
    let payload = json!({
        "running": running,
        "ready": ready,
        "ports": {
            "vnc": DEFAULT_VNC_PORT,
            "novnc": DEFAULT_NOVNC_PORT,
            "webui": DEFAULT_WEBUI_PORT,
        },
        "display": display_str(DEFAULT_DISPLAY_NUM),
    });
    let bytes = serde_json::to_vec_pretty(&payload).map_err(|e| BotBackendError::Json(e.to_string()))?;
    host.write_file(&HostPath::from_posix(&paths.status_daemon), &bytes)
        .await
        .map_err(|e| BotBackendError::Io(e.to_string()))?;
    Ok(())
}