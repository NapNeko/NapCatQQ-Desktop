//! 远端 SnowLuma daemon / bot 启停:编排委托 [remote_snowluma_stack],Bot 用 QQComponent + 短 detach

use std::collections::BTreeMap;

use ncd_component::{Component, LaunchArgs, QQComponent};
use ncd_host::{Host, HostCommand, HostPath, shell_single_quote};
use serde_json::json;

use ncd_traits::runtime_backend::BotBackendError;

pub use super::stack::{resolve_remote_bash, run_remote_bash as run_sh};

use super::layout::{
    DEFAULT_DISPLAY_NUM, DEFAULT_NOVNC_PORT, DEFAULT_VNC_PORT, DEFAULT_WEBUI_PORT,
    RemoteSnowLumaLayout, SnowLumaRemotePaths,
};
use super::stack::{
    LOG_PREV_MAX_BYTES, ensure_dbus_env, ensure_stack_running, is_stack_ready, run_remote_bash,
    stack_stop,
};

fn display_str(num: i32) -> String {
    format!(":{num}")
}

pub async fn remote_daemon_already_ready(
    host: &dyn Host,
    layout: &RemoteSnowLumaLayout,
) -> Result<bool, BotBackendError> {
    is_stack_ready(host, layout).await
}

pub async fn daemon_start(
    host: &dyn Host,
    layout: &RemoteSnowLumaLayout,
    metrics_env: Option<&BTreeMap<String, String>>,
) -> Result<u16, BotBackendError> {
    ensure_stack_running(host, layout, metrics_env).await
}

pub async fn daemon_stop(
    host: &dyn Host,
    paths: &SnowLumaRemotePaths,
) -> Result<(), BotBackendError> {
    stack_stop(host, paths).await
}

pub(crate) async fn daemon_start_cancellable(
    host: &dyn Host,
    layout: &RemoteSnowLumaLayout,
    metrics_env: Option<&BTreeMap<String, String>>,
    cancel: Option<&tokio_util::sync::CancellationToken>,
) -> Result<u16, BotBackendError> {
    super::stack::ensure_stack_running_cancellable(host, layout, metrics_env, cancel).await
}

async fn prepare_bot_launch_env(host: &dyn Host) -> Result<Option<String>, BotBackendError> {
    // 1. libgcc_s.so.1 LD_PRELOAD 修复(非特权)
    let script = r#"libgcc_path=$(ldconfig -p 2>/dev/null | grep -m1 'libgcc_s.so.1' | awk '{print $NF}') || true
	if [ -n "$libgcc_path" ] && [ -L "$libgcc_path" ]; then
	  real_path=$(readlink -f "$libgcc_path")
	  rm -f "$libgcc_path" 2>/dev/null && ln "$real_path" "$libgcc_path" 2>/dev/null || true
	  libgcc_path=$(ldconfig -p 2>/dev/null | grep -m1 'libgcc_s.so.1' | awk '{print $NF}') || true
	fi
	echo "${libgcc_path:-}"
	"#;
    let out = run_remote_bash(host, script).await?;
    let line = out.lines().last().unwrap_or("").trim();
    let ld_preload = if line.is_empty() {
        None
    } else {
        Some(line.to_string())
    };

    // 2. 关键:放宽 ptrace_scope
    //    旧脚本里用 sudo -n 静默尝试,这里改用 Host 的 elevation 机制(会喂已缓存的 sudo 密码)
    //    失败则给出清晰,可执行的错误提示
    relax_ptrace_scope(host).await?;

    Ok(ld_preload)
}

/// 读取当前 ptrace_scope,若不为 0 则尝试用已缓存的 sudo 密码(elevation)将其设为 0
/// 设完后再次校验;仍不为 0 则返回带明确操作指引的错误(不再静默失败导致后续 PTRACE_ATTACH 报 Operation not permitted)
///
/// 改进:
/// - 初始读失败或非 0 都尝试设置(不只看初始读)
/// - 同时尝试 sysctl -w 和直接 echo 写 proc(某些系统一个可行另一个不行)
/// - 捕获设置命令的完整输出(stdout/stderr/exit),失败时一并带进错误消息,便于诊断 elevation 是否真的生效
async fn relax_ptrace_scope(host: &dyn Host) -> Result<(), BotBackendError> {
    // 读当前值(非特权读即可)
    let read_scope = || async {
        let cmd = HostCommand::new("cat").arg("/proc/sys/kernel/yama/ptrace_scope");
        host.run_to_string(cmd)
            .await
            .ok()
            .filter(|o| o.success())
            .map(|o| o.stdout.trim().to_string())
    };

    let before = read_scope().await;

    if before.as_deref() == Some("0") {
        return Ok(());
    }

    // 准备两套设置命令,都走 elevated(会用已缓存的 sudo 密码喂 sudo -S)
    // 1) sysctl -w
    let set_sysctl = HostCommand::new("sysctl")
        .arg("-w")
        .arg("kernel.yama.ptrace_scope=0")
        .elevated();

    // 2) 直接写 proc(fallback)
    let set_proc = HostCommand::new("sh")
        .arg("-c")
        .arg("echo 0 > /proc/sys/kernel/yama/ptrace_scope")
        .elevated();

    // 执行设置,记录输出
    let mut set_attempts: Vec<String> = Vec::new();

    for (label, cmd) in [("sysctl -w", set_sysctl), ("echo > proc", set_proc)] {
        let out = host.run_to_string(cmd).await;
        match out {
            Ok(o) => {
                set_attempts.push(format!(
                    "{}: exit={:?} stdout={} stderr={}",
                    label,
                    o.exit_code,
                    o.stdout.trim(),
                    o.stderr.trim()
                ));
            }
            Err(e) => {
                set_attempts.push(format!("{}: error={}", label, e));
            }
        }
    }

    // 再读一次
    let after = read_scope().await;

    if after.as_deref() == Some("0") {
        return Ok(());
    }

    let seen_before = before.unwrap_or_else(|| "unknown".to_string());
    let seen_after = after.unwrap_or_else(|| "unknown".to_string());
    let attempts = set_attempts.join("\n  ");

    Err(BotBackendError::InvalidConfig(format!(
        "远端 kernel.yama.ptrace_scope 当前为 {seen_after}（启动前为 {seen_before}），SnowLuma 注入 trampoline 需要 PTRACE_ATTACH 权限（必须为 0）。\n\
         已尝试使用已保存的 sudo 密码通过 elevation 自动修改，但未成功。\n\
         设置尝试输出：\n  {attempts}\n\
         请在远端手动执行（需要 root 或 sudo 权限）：\n\
           sudo sysctl -w kernel.yama.ptrace_scope=0\n\
         或（持久化）：\n\
           echo 'kernel.yama.ptrace_scope = 0' | sudo tee /etc/sysctl.d/99-ptrace.conf && sudo sysctl --system\n\
         如仍不行，可尝试给 node 二进制加 capability：\n\
           sudo setcap 'cap_sys_ptrace+ep' $(readlink -f $(command -v node))\n\
         修改后立即生效。建议为该 sysctl 配置免密 sudo，以便冷启动 Bot 时无需交互。"
    )))
}

pub(crate) fn node_already_has_sys_ptrace(getcap_stdout: &str) -> bool {
    getcap_stdout
        .to_ascii_lowercase()
        .contains("cap_sys_ptrace")
}

/// Best-effort: grant cap_sys_ptrace+ep to the remote's node binary via elevation.
/// Already granted → skip setcap（避免每次冷启动都 elevated）。
pub async fn try_grant_node_ptrace_cap(host: &dyn Host) {
    let probe = HostCommand::new("sh").arg("-c").arg(
        r#"node=$(command -v node 2>/dev/null || true)
[ -n "$node" ] || exit 0
echo "node=$node"
getcap "$node" 2>/dev/null || true
"#,
    );
    let out = match host.run_to_string(probe).await {
        Ok(o) if o.success() => o.stdout,
        _ => return,
    };
    let node = out
        .lines()
        .find_map(|l| l.strip_prefix("node="))
        .map(str::trim);
    let Some(node) = node.filter(|p| !p.is_empty()) else {
        return;
    };
    if node_already_has_sys_ptrace(&out) {
        tracing::info!(
            target: "ncd_runtime::remote_snowluma",
            node = %node,
            "node already has cap_sys_ptrace; skip setcap"
        );
        return;
    }

    let set = HostCommand::new("setcap")
        .arg("cap_sys_ptrace+ep")
        .arg(node)
        .elevated();
    let _ = host.run_to_string(set).await;
}

/// 冷启 QQ 的远端脚本。ptrace / libgcc 已在 spawn 前的 host 侧做过，这里不再重复。
#[allow(clippy::too_many_arguments)]
pub(crate) fn bot_cold_start_script(
    rt: &str,
    log_dir: &str,
    log: &str,
    pidfile: &str,
    qq_invoke: &str,
    display: &str,
    ld_fragment: &str,
    dbus: &str,
    qq_bin: &str,
) -> String {
    format!(
        r#"umask 077
mkdir -p {rt} {log_dir}
if [ -f {dbus} ]; then . {dbus}; fi
if [ "${{NCD_QR_ACCESSIBILITY:-}}" = 1 ]; then
  if command -v gdbus >/dev/null 2>&1; then
    timeout 3 gdbus call --session --timeout 2 --dest org.a11y.Bus --object-path /org/a11y/bus --method org.freedesktop.DBus.Properties.Set org.a11y.Status IsEnabled '<true>' >/dev/null 2>&1 || true
  elif command -v dbus-send >/dev/null 2>&1; then
    timeout 3 dbus-send --session --print-reply --reply-timeout=1500 --dest=org.a11y.Bus /org/a11y/bus org.freedesktop.DBus.Properties.Set string:org.a11y.Status string:IsEnabled variant:boolean:true >/dev/null 2>&1 || true
  fi
fi
DISPLAY="{display}" nohup env {ld_fragment}{qq_invoke} > {log} 2>&1 </dev/null &
echo $! > {pidfile}
sleep 0.5
pid=$(cat {pidfile})
if ! kill -0 "$pid" 2>/dev/null; then
  echo "bot 启动后立即退出" >&2
  missing_libs=$(ldd {qq_bin} 2>/dev/null | grep 'not found' | awk '{{print $1}}' | tr '\n' ' ')
  if [ -n "$missing_libs" ]; then
    echo "缺少系统依赖库: $missing_libs" >&2
    echo "请到「组件」页按提示修复 QQ 系统依赖，或手动安装后重试" >&2
  fi
  echo "--- 启动日志末尾 ---" >&2
  tail -n 20 {log} >&2 2>/dev/null || true
  exit 3
fi
echo "$pid"
"#
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn bot_cold_start_cancellable(
    host: &dyn Host,
    layout: &RemoteSnowLumaLayout,
    qq_id: &str,
    uin: &str,
    login: Option<&super::login_helper::PreparedLogin>,
    verified_running_pid: Option<u32>,
    cancel: Option<&tokio_util::sync::CancellationToken>,
    on_spawned: Option<&(dyn Fn(u32, Option<String>) + Send + Sync)>,
) -> Result<u32, BotBackendError> {
    super::startup_cancel::check(cancel)?;
    let paths = &layout.paths;
    if let Some(pid) = verified_running_pid {
        return Ok(pid);
    }

    // 冷启 Bot：bot_*.log 只留一代 .prev（过大截尾），current 清空
    let log = shell_single_quote(&paths.log_bot_path(qq_id));
    let log_dir = shell_single_quote(&paths.log_dir);
    let prev_max = LOG_PREV_MAX_BYTES;
    let rotate = format!(
        r#"mkdir -p {log_dir}
if [ -f {log} ]; then
  rm -f {log}.prev
  mv -f {log} {log}.prev 2>/dev/null || true
  if [ -f {log}.prev ]; then
    sz=$(wc -c < {log}.prev 2>/dev/null || echo 0)
    if [ "$sz" -gt {prev_max} ]; then
      tail -c {prev_max} {log}.prev > {log}.prev.tmp 2>/dev/null \
        && mv -f {log}.prev.tmp {log}.prev \
        || rm -f {log}.prev.tmp
    fi
  fi
fi
: > {log}
"#,
        prev_max = prev_max,
    );
    run_remote_bash(host, &rotate).await?;
    super::startup_cancel::check(cancel)?;

    let install_base = HostPath::from_posix(layout.qq_install_base.clone());
    let qq = QQComponent::default_v3_2_25(install_base);
    let mut launch_args = LaunchArgs {
        extra_args: vec![
            "--no-sandbox".into(),
            "--disable-gpu-sandbox".into(),
            "-q".into(),
            qq_id.to_string(),
        ],
        ..Default::default()
    };
    launch_args
        .extra_env
        .push(("DISPLAY".to_string(), display_str(DEFAULT_DISPLAY_NUM)));
    if login.is_some() {
        launch_args
            .extra_args
            .push("--force-renderer-accessibility".into());
    }
    let cmd = qq
        .launch_command(host, &launch_args)
        .map_err(|e| BotBackendError::InvalidConfig(e.to_string()))?;

    let ld_preload = prepare_bot_launch_env(host).await?;
    super::startup_cancel::check(cancel)?;

    // Also try to grant cap_sys_ptrace to the remote node binary (best-effort).
    // This complements lowering ptrace_scope and is recommended in our own error messages.
    try_grant_node_ptrace_cap(host).await;
    super::startup_cancel::check(cancel)?;

    // 共享 daemon 就绪不代表保存的 bus 还活着；仅新 QQ 读取修复后的环境。
    ensure_dbus_env(host, paths).await?;
    super::startup_cancel::check(cancel)?;

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
    let mut ld_fragment = ld_preload
        .as_ref()
        .map(|p| format!("LD_PRELOAD={} ", shell_single_quote(p)))
        .unwrap_or_default();
    if let Some(login) = login {
        for (key, value) in &login.environment {
            ld_fragment.push_str(&format!("{key}={} ", shell_single_quote(value)));
        }
    }
    let dbus = shell_single_quote(&paths.dbus_env);
    let qq_bin = shell_single_quote(&cmd.program);

    let mut script = bot_cold_start_script(
        &rt,
        &log_dir,
        &log,
        &pidfile,
        &qq_invoke,
        &display,
        &ld_fragment,
        &dbus,
        &qq_bin,
    );
    if login.is_some() {
        script = format!("NCD_QR_ACCESSIBILITY=1\n{script}");
    }
    super::startup_cancel::check(cancel)?;
    if let Some(record) = on_spawned {
        record(0, None);
    }
    let out = run_remote_bash(host, &script).await?;
    let pid: u32 = out
        .lines()
        .last()
        .unwrap_or("")
        .trim()
        .parse()
        .map_err(|_| BotBackendError::Io(format!("bot start 未返回 pid: {out}")))?;

    if let Some(record) = on_spawned {
        record(pid, None);
    }
    let birth = super::login_helper::process_start_time(host, pid).await?;
    if let Some(record) = on_spawned {
        record(pid, Some(birth));
    }
    super::startup_cancel::check(cancel)?;
    write_status_bot_json(host, paths, qq_id, uin, pid, true).await?;
    super::startup_cancel::check(cancel)?;
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
    let bytes =
        serde_json::to_vec_pretty(&payload).map_err(|e| BotBackendError::Json(e.to_string()))?;
    let path = HostPath::from_posix(paths.status_bot_path(qq_id));
    host.write_file(&path, &bytes)
        .await
        .map_err(|e| BotBackendError::Io(e.to_string()))?;
    Ok(())
}

/// 导入接管时记下 QQ pid，后续 stop / status 不再只靠 Desktop 自己拉起时写的文件。
pub async fn remember_remote_bot_pid(
    host: &dyn Host,
    paths: &SnowLumaRemotePaths,
    qq_id: &str,
    pid: u32,
) -> Result<(), BotBackendError> {
    let pidfile = HostPath::from_posix(paths.pid_bot_path(qq_id));
    host.write_file(&pidfile, format!("{pid}\n").as_bytes())
        .await
        .map_err(|e| BotBackendError::Io(e.to_string()))?;
    write_status_bot_json(host, paths, qq_id, qq_id, pid, true).await
}

pub async fn bot_stop(
    host: &dyn Host,
    paths: &SnowLumaRemotePaths,
    qq_id: &str,
) -> Result<(), BotBackendError> {
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
    if let Ok(qid) = qq_id.parse::<u64>() {
        if let Ok(Some(pid)) = super::inject::remote_qq_running_pid(host, qid).await {
            let kill = format!(
                r#"pid={pid}
if kill -0 "$pid" 2>/dev/null; then
  kill "$pid" 2>/dev/null || true
  i=0
  while [ "$i" -lt 10 ] && kill -0 "$pid" 2>/dev/null; do sleep 0.5; i=$((i+1)); done
  kill -9 "$pid" 2>/dev/null || true
fi
"#
            );
            let _ = run_remote_bash(host, &kill).await;
        }
    }
    Ok(())
}

pub async fn write_status_daemon_json(
    host: &dyn Host,
    paths: &SnowLumaRemotePaths,
    running: bool,
    ready: bool,
    webui_port: u16,
) -> Result<(), BotBackendError> {
    let webui = if webui_port == 0 {
        DEFAULT_WEBUI_PORT
    } else {
        webui_port as i32
    };
    let payload = json!({
        "running": running,
        "ready": ready,
        "ports": {
            "vnc": DEFAULT_VNC_PORT,
            "novnc": DEFAULT_NOVNC_PORT,
            "webui": webui,
        },
        "display": display_str(DEFAULT_DISPLAY_NUM),
    });
    let bytes =
        serde_json::to_vec_pretty(&payload).map_err(|e| BotBackendError::Json(e.to_string()))?;
    host.write_file(&HostPath::from_posix(&paths.status_daemon), &bytes)
        .await
        .map_err(|e| BotBackendError::Io(e.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_already_has_sys_ptrace_reads_getcap() {
        assert!(node_already_has_sys_ptrace(
            "/usr/bin/node cap_sys_ptrace=ep"
        ));
        assert!(node_already_has_sys_ptrace("cap_sys_ptrace+ep"));
        assert!(!node_already_has_sys_ptrace(""));
        assert!(!node_already_has_sys_ptrace(
            "/usr/bin/node cap_net_bind_service=ep"
        ));
    }

    #[test]
    fn bot_cold_start_script_does_not_repeat_ptrace_or_libgcc() {
        let script = bot_cold_start_script(
            "'/ws/runtime'",
            "'/ws/log'",
            "'/ws/log/bot_1.log'",
            "'/ws/runtime/pid_bot_1'",
            "'/opt/QQ/qq' --no-sandbox",
            ":0",
            "",
            "'/ws/runtime/dbus.env'",
            "'/opt/QQ/qq'",
        );
        assert!(
            !script.contains("ptrace_scope"),
            "ptrace is relaxed on the host before spawn"
        );
        assert!(
            !script.contains("libgcc"),
            "libgcc hardlink is done on the host before spawn"
        );
        assert!(script.contains("nohup env"));
        assert!(script.contains("echo $!"));
    }
}
