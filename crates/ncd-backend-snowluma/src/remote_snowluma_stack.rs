//! 远端 SnowLuma 图形栈:Rust 分步编排 + 短 bash -c detach(无内联 mega-script)
//!
//! SSH Host::spawn 的 exec channel 关闭后长驻进程会随 channel 结束(ProcessId.native == 0),
//! 因此 daemon 各角色用 run_to_string 投递 nohup setsid … & + pid 文件,不用单次 spawn

use std::time::Duration;

use ncd_host::{Host, HostCommand, HostPath};

use crate::remote_snowluma_layout::{
    DEFAULT_DISPLAY_NUM, DEFAULT_NOVNC_PORT, DEFAULT_VNC_PORT, DEFAULT_WEBUI_PORT,
    RemoteSnowLumaLayout, SnowLumaRemotePaths, shell_single_quote,
};
use ncd_traits::runtime_backend::BotBackendError;

fn display_str(num: i32) -> String {
    format!(":{num}")
}

/// 远端内联脚本依赖 bash(flock,nohup 数组等);/bin/sh 为 dash 时会失败
pub async fn resolve_remote_bash(host: &dyn Host) -> Result<String, BotBackendError> {
    let cmd = HostCommand::new("sh").arg("-c").arg("command -v bash");
    let out = host
        .run_to_string(cmd)
        .await
        .map_err(|e| BotBackendError::Io(e.to_string()))?;
    if out.success() {
        let line = out.stdout.lines().next().unwrap_or("").trim();
        if !line.is_empty() {
            return Ok(line.to_string());
        }
    }
    if host
        .run_to_string(HostCommand::new("sh").arg("-c").arg("test -x /bin/bash"))
        .await
        .ok()
        .is_some_and(|o| o.success())
    {
        return Ok("/bin/bash".into());
    }
    Err(BotBackendError::InvalidConfig(
        "远端 SnowLuma「直接运行」需要 bash。请安装：sudo apt install bash".into(),
    ))
}

/// 执行短 bash 脚本(单条 detach 或 flock),禁止拼接百行 heredoc
pub async fn run_remote_bash(host: &dyn Host, script: &str) -> Result<String, BotBackendError> {
    let bash = resolve_remote_bash(host).await?;
    let cmd = HostCommand::new(bash).arg("-c").arg(script);
    let out = host
        .run_to_string(cmd)
        .await
        .map_err(|e| BotBackendError::Io(e.to_string()))?;
    if !out.success() {
        return Err(BotBackendError::Io(format!(
            "远端命令失败: exit={:?} stderr={} stdout={}",
            out.exit_code,
            out.stderr.trim(),
            out.stdout.trim()
        )));
    }
    Ok(out.stdout)
}

/// dash-safe 单行(pgrep,test -x,kill -0)
pub async fn run_sh_dash(host: &dyn Host, script: &str) -> Result<String, BotBackendError> {
    let cmd = HostCommand::new("sh").arg("-c").arg(script);
    let out = host
        .run_to_string(cmd)
        .await
        .map_err(|e| BotBackendError::Io(e.to_string()))?;
    if !out.success() {
        return Err(BotBackendError::Io(format!(
            "远端 sh 失败: exit={:?} stderr={}",
            out.exit_code,
            out.stderr.trim()
        )));
    }
    Ok(out.stdout)
}

fn pid_file(paths: &SnowLumaRemotePaths, role: &str) -> String {
    format!("{}/pid_{role}", paths.runtime_dir)
}

async fn ensure_dirs(host: &dyn Host, paths: &SnowLumaRemotePaths) -> Result<(), BotBackendError> {
    for dir in [&paths.runtime_dir, &paths.log_dir] {
        host.create_dir_all(&HostPath::from_posix(dir))
            .await
            .map_err(|e| BotBackendError::Io(e.to_string()))?;
    }
    Ok(())
}


async fn read_pid_file(host: &dyn Host, path: &str) -> Result<Option<u32>, BotBackendError> {
    let p = shell_single_quote(path);
    let script = format!(r#"test -f {p} && cat {p} || true"#);
    let out = run_sh_dash(host, &script).await.unwrap_or_default();
    let line = out.lines().next().unwrap_or("").trim();
    if line.is_empty() || line == "0" {
        return Ok(None);
    }
    match line.parse::<u32>() {
        Ok(0) => Ok(None),
        Ok(n) => Ok(Some(n)),
        Err(_) => Ok(None),
    }
}

/// 清理可能占用 VNC 端口的残留 x11vnc(半残留栈场景)
async fn cleanup_stale_x11vnc(host: &dyn Host, layout: &RemoteSnowLumaLayout) -> Result<(), BotBackendError> {
    let paths = &layout.paths;
    let vnc = DEFAULT_VNC_PORT;
    let pid_path = shell_single_quote(&pid_file(paths, "x11vnc"));
    let script = format!(
        r#"if [ -f {pid_path} ]; then
  old=$(cat {pid_path} 2>/dev/null || echo "")
  if [ -n "$old" ] && kill -0 "$old" 2>/dev/null; then kill "$old" 2>/dev/null || true; sleep 0.3; fi
  rm -f {pid_path}
fi
pids=$(pgrep -f "x11vnc.*-rfbport {vnc}" 2>/dev/null || true)
if [ -n "$pids" ]; then kill $pids 2>/dev/null || true; sleep 0.3; fi
"#
    );
    let _ = run_sh_dash(host, &script).await;
    Ok(())
}

/// 清理可能占用 noVNC 端口的残留 websockify(半残留栈场景)
async fn cleanup_stale_websockify(host: &dyn Host, layout: &RemoteSnowLumaLayout) -> Result<(), BotBackendError> {
    let paths = &layout.paths;
    let novnc = DEFAULT_NOVNC_PORT;
    let pid_path = shell_single_quote(&pid_file(paths, "websockify"));
    let script = format!(
        r#"if [ -f {pid_path} ]; then
  old=$(cat {pid_path} 2>/dev/null || echo "")
  if [ -n "$old" ] && kill -0 "$old" 2>/dev/null; then kill "$old" 2>/dev/null || true; sleep 0.3; fi
  rm -f {pid_path}
fi
pids=$(pgrep -f "websockify.*{novnc}" 2>/dev/null || true)
if [ -n "$pids" ]; then kill $pids 2>/dev/null || true; sleep 0.3; fi
"#
    );
    let _ = run_sh_dash(host, &script).await;
    Ok(())
}

async fn kill_pid_graceful(host: &dyn Host, pid: u32) -> Result<(), BotBackendError> {
    let script = format!(
        r#"pid={pid}
if kill -0 "$pid" 2>/dev/null; then
  kill "$pid" 2>/dev/null || true
  i=0
  while [ "$i" -lt 10 ] && kill -0 "$pid" 2>/dev/null; do sleep 0.5; i=$((i+1)); done
  kill -9 "$pid" 2>/dev/null || true
fi
"#
    );
    let _ = run_sh_dash(host, &script).await;
    Ok(())
}

pub async fn ensure_dbus_env(host: &dyn Host, paths: &SnowLumaRemotePaths) -> Result<(), BotBackendError> {
    let dbus = shell_single_quote(&paths.dbus_env);
    let script = format!(
        r#"if [ ! -f {dbus} ] || ! pgrep -f 'dbus-daemon.*--config-file' >/dev/null 2>&1; then
  dbus-launch --sh-syntax --exit-with-session > {dbus}
fi
"#
    );
    run_remote_bash(host, &script).await?;
    Ok(())
}

fn source_dbus_prefix(paths: &SnowLumaRemotePaths) -> String {
    let dbus = shell_single_quote(&paths.dbus_env);
    format!("# shellcheck disable=SC1090\n. {dbus}\n")
}

pub async fn start_xvfb(host: &dyn Host, layout: &RemoteSnowLumaLayout) -> Result<u32, BotBackendError> {
    let paths = &layout.paths;
    let display = display_str(DEFAULT_DISPLAY_NUM);
    let log = shell_single_quote(&format!("{}/xvfb.log", paths.log_dir));
    let pid_path = shell_single_quote(&pid_file(paths, "xvfb"));
    let dbus = source_dbus_prefix(paths);
    let script = format!(
        r#"{dbus}nohup setsid Xvfb "{display}" -screen 0 1280x720x24 -nolisten tcp > {log} 2>&1 </dev/null &
echo $! > {pid_path}
sleep 0.5
cat {pid_path}
"#
    );
    let out = run_remote_bash(host, &script).await?;
    parse_last_u32(&out, "Xvfb")
}

pub async fn start_wm(host: &dyn Host, layout: &RemoteSnowLumaLayout) -> Result<u32, BotBackendError> {
    let paths = &layout.paths;
    let display = display_str(DEFAULT_DISPLAY_NUM);
    let log = shell_single_quote(&format!("{}/wm.log", paths.log_dir));
    let pid_path = shell_single_quote(&pid_file(paths, "fluxbox"));
    let script = format!(
        r#"DISPLAY="{display}"
if command -v fluxbox >/dev/null 2>&1; then
  nohup setsid fluxbox > {log} 2>&1 </dev/null &
  echo $! > {pid_path}
elif command -v openbox >/dev/null 2>&1; then
  nohup setsid openbox > {log} 2>&1 </dev/null &
  echo $! > {pid_path}
else
  echo 0 > {pid_path}
fi
cat {pid_path}
"#
    );
    let out = run_remote_bash(host, &script).await?;
    parse_last_u32(&out, "WM")
}

pub async fn start_x11vnc(host: &dyn Host, layout: &RemoteSnowLumaLayout) -> Result<u32, BotBackendError> {
    let paths = &layout.paths;
    let display = display_str(DEFAULT_DISPLAY_NUM);
    let vnc = DEFAULT_VNC_PORT;
    let log = shell_single_quote(&format!("{}/x11vnc.log", paths.log_dir));
    let vnc_sec = shell_single_quote(&paths.vnc_secret);
    let pid_path = shell_single_quote(&pid_file(paths, "x11vnc"));
    let script = format!(
        r#"DISPLAY="{display}"
DISPLAY="{display}" x11vnc -display "{display}" -rfbport {vnc} -passwdfile {vnc_sec} -forever -shared -noxdamage -noxfixes -bg \
  -o {log} >/dev/null 2>&1 || {{ echo "x11vnc 启动失败" >&2; exit 3; }}
sleep 0.5
pgrep -nf "x11vnc.*-rfbport {vnc}" > {pid_path} || {{ echo "x11vnc pid 抓取失败" >&2; exit 31; }}
cat {pid_path}
"#
    );
    let out = run_remote_bash(host, &script).await?;
    parse_last_u32(&out, "x11vnc")
}

pub async fn start_websockify(host: &dyn Host, layout: &RemoteSnowLumaLayout) -> Result<u32, BotBackendError> {
    let paths = &layout.paths;
    let ws = shell_single_quote(&paths.workspace_dir);
    let log = shell_single_quote(&format!("{}/websockify.log", paths.log_dir));
    let pid_path = shell_single_quote(&pid_file(paths, "websockify"));
    let novnc = DEFAULT_NOVNC_PORT;
    let vnc = DEFAULT_VNC_PORT;
    let script = format!(
        r#"WORKSPACE_DIR={ws}
novnc_web="/usr/share/novnc"
if [ -d "$WORKSPACE_DIR/novnc" ] && [ -f "$WORKSPACE_DIR/novnc/vnc.html" ]; then
  novnc_web="$WORKSPACE_DIR/novnc"
elif [ -d /usr/share/noVNC ]; then novnc_web="/usr/share/noVNC"; fi
websockify --daemon --web "$novnc_web" --log-file {log} "{novnc}" "localhost:{vnc}" \
  || {{ echo "websockify 启动失败" >&2; exit 4; }}
sleep 0.3
pgrep -nf "websockify.*{novnc}" > {pid_path} || {{ echo "websockify pid 抓取失败" >&2; exit 32; }}
cat {pid_path}
"#
    );
    let out = run_remote_bash(host, &script).await?;
    parse_last_u32(&out, "websockify")
}


/// 排障用上一代日志上限（daemon/bot 的 `.prev`）
pub(crate) const LOG_PREV_MAX_BYTES: u64 = 2 * 1024 * 1024;
/// 仍在跑时磁盘上的活跃日志软上限（仅 stop 时收口；跑中靠 UI 裁会话）
pub(crate) const LOG_ACTIVE_ARCHIVE_MAX_BYTES: u64 = 8 * 1024 * 1024;

/// 轮转：current → .prev（只留一代），.prev 过大则 tail 截尾；再截断 current。
/// path 须已 shell 单引号包装。进程必须未持有该路径写句柄。
fn shell_rotate_log_file(path_q: &str) -> String {
    format!(
        r#"if [ -f {path_q} ]; then
  rm -f {path_q}.prev
  mv -f {path_q} {path_q}.prev 2>/dev/null || true
  if [ -f {path_q}.prev ]; then
    sz=$(wc -c < {path_q}.prev 2>/dev/null || echo 0)
    if [ "$sz" -gt {max} ]; then
      tail -c {max} {path_q}.prev > {path_q}.prev.tmp 2>/dev/null \
        && mv -f {path_q}.prev.tmp {path_q}.prev \
        || rm -f {path_q}.prev.tmp
    fi
  fi
fi
: > {path_q}
"#,
        path_q = path_q,
        max = LOG_PREV_MAX_BYTES,
    )
}

/// 进程已停：把活跃日志收成一代 `.prev`（截尾），删除 current，避免下次 UI 扫到古董。
fn shell_archive_log_on_stop(path_q: &str) -> String {
    format!(
        r#"if [ -f {path_q} ]; then
  rm -f {path_q}.prev
  sz=$(wc -c < {path_q} 2>/dev/null || echo 0)
  if [ "$sz" -gt {max} ]; then
    tail -c {max} {path_q} > {path_q}.prev 2>/dev/null || mv -f {path_q} {path_q}.prev 2>/dev/null || true
  else
    mv -f {path_q} {path_q}.prev 2>/dev/null || true
  fi
  rm -f {path_q}
fi
"#,
        path_q = path_q,
        max = LOG_ACTIVE_ARCHIVE_MAX_BYTES,
    )
}

pub async fn start_node(host: &dyn Host, layout: &RemoteSnowLumaLayout) -> Result<u32, BotBackendError> {
    let paths = &layout.paths;
    let display = display_str(DEFAULT_DISPLAY_NUM);
    let sl = shell_single_quote(&paths.snowluma_dir);
    let node = shell_single_quote(&layout.node_bin);
    let log_daemon = shell_single_quote(&paths.log_daemon);
    let pid_node = shell_single_quote(&pid_file(paths, "node"));
    let pid_daemon = shell_single_quote(&paths.pid_daemon);
    // node 未启动时轮转：只留一代 .prev 并截尾，current 清空。
    let rotate = shell_rotate_log_file(&log_daemon);
    let script = format!(
        r#"cd {sl}
{rotate}
DISPLAY="{display}" nohup setsid {node} --experimental-sqlite index.mjs >> {log_daemon} 2>&1 </dev/null &
node_pid=$!
echo "$node_pid" > {pid_node}
echo "$node_pid" > {pid_daemon}
echo "$node_pid"
"#
    );
    let out = run_remote_bash(host, &script).await?;
    parse_last_u32(&out, "node")
}

fn parse_last_u32(out: &str, label: &str) -> Result<u32, BotBackendError> {
    let line = out.lines().last().unwrap_or("").trim();
    line.parse::<u32>().map_err(|_| {
        BotBackendError::Io(format!("{label} 未返回 pid: {out}"))
    })
}

/// WebUI 端口就绪(bash /dev/tcp,短脚本)
pub async fn wait_webui_tcp(host: &dyn Host, port: i32, timeout: Duration) -> Result<(), BotBackendError> {
    let secs = timeout.as_secs().max(1);
    let script = format!(
        r#"port={port}
deadline=$(( $(date +%s) + {secs} ))
while [ $(date +%s) -lt $deadline ]; do
  if (: > "/dev/tcp/127.0.0.1/$port") 2>/dev/null; then exit 0; fi
  sleep 1
done
exit 1
"#
    );
    run_remote_bash(host, &script)
        .await
        .map_err(|_| BotBackendError::Io(format!("SnowLuma WebUI 端口 {port} 在时限内未就绪")))?;
    Ok(())
}

/// daemon 是否已在远端就绪(pid + WebUI),dash-safe 探测
pub async fn is_stack_ready(host: &dyn Host, paths: &SnowLumaRemotePaths) -> Result<bool, BotBackendError> {
    let pid_path = shell_single_quote(&paths.pid_daemon);
    let port = DEFAULT_WEBUI_PORT;
    let script = format!(
        r#"PID_PATH={pid_path}
port={port}
if [ -f "$PID_PATH" ]; then
  pid=$(cat "$PID_PATH" 2>/dev/null || echo "")
  if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
    if command -v bash >/dev/null 2>&1 && bash -c "(: > /dev/tcp/127.0.0.1/$port) 2>/dev/null"; then exit 0; fi
    if command -v nc >/dev/null 2>&1 && nc -z 127.0.0.1 "$port" 2>/dev/null; then exit 0; fi
  fi
fi
exit 1
"#
    );
    let cmd = HostCommand::new("sh").arg("-c").arg(script);
    let out = host
        .run_to_string(cmd)
        .await
        .map_err(|e| BotBackendError::Io(e.to_string()))?;
    Ok(out.success())
}

pub async fn stack_stop(host: &dyn Host, paths: &SnowLumaRemotePaths) -> Result<(), BotBackendError> {
    for role in ["node", "websockify", "x11vnc", "fluxbox", "xvfb"] {
        let pf = pid_file(paths, role);
        if let Ok(Some(pid)) = read_pid_file(host, &pf).await {
            if pid != 0 {
                let _ = kill_pid_graceful(host, pid).await;
            }
        }
        let p = shell_single_quote(&pf);
        let _ = run_sh_dash(host, &format!("rm -f {p}")).await;
    }
    let pid_daemon = shell_single_quote(&paths.pid_daemon);
    let status = shell_single_quote(&paths.status_daemon);
    let log_daemon = shell_single_quote(&paths.log_daemon);
    let log_dir = shell_single_quote(&paths.log_dir);
    let archive_daemon = shell_archive_log_on_stop(&log_daemon);
    // 整栈停掉后：daemon + bot_*.log 各留一代 .prev（截尾），删掉 current
    let _ = run_sh_dash(
        host,
        &format!(
            r#"rm -f {pid_daemon}
echo '{{"running":false,"ready":false}}' > {status}
{archive_daemon}
for f in {log_dir}/bot_*.log; do
  [ -f "$f" ] || continue
  rm -f "$f.prev"
  sz=$(wc -c < "$f" 2>/dev/null || echo 0)
  if [ "$sz" -gt {max_active} ]; then
    tail -c {max_active} "$f" > "$f.prev" 2>/dev/null || mv -f "$f" "$f.prev" 2>/dev/null || true
  else
    mv -f "$f" "$f.prev" 2>/dev/null || true
  fi
  rm -f "$f"
done
"#,
            max_active = LOG_ACTIVE_ARCHIVE_MAX_BYTES,
        ),
    )
    .await;
    Ok(())
}

/// 启动完整图形栈 + node(RemoteSnowlumaStackOrchestrator 入口)
pub async fn ensure_stack_running(host: &dyn Host, layout: &RemoteSnowLumaLayout) -> Result<(), BotBackendError> {
    let paths = &layout.paths;
    if is_stack_ready(host, paths).await? {
        return Ok(());
    }
    ensure_dirs(host, paths).await?;

    let pid_daemon = shell_single_quote(&paths.pid_daemon);
    let script_check = format!(
        r#"if [ -f {pid_daemon} ] && kill -0 "$(cat {pid_daemon} 2>/dev/null || echo 0)" 2>/dev/null; then
  echo "daemon already running"
  exit 0
fi
"#
    );
    if run_remote_bash(host, &script_check)
        .await
        .map(|o| o.contains("daemon already running"))
        .unwrap_or(false)
    {
        return Ok(());
    }

    ensure_dbus_env(host, paths).await?;
    start_xvfb(host, layout).await?;
    start_wm(host, layout).await?;
    // 给 WM 一点时间再抓屏,减轻 noVNC 全黑(QQ 尚未启动时属正常,冷启 QQ 后应能看到界面)
    tokio::time::sleep(Duration::from_millis(800)).await;
    cleanup_stale_x11vnc(host, layout).await?;
    start_x11vnc(host, layout).await?;
    cleanup_stale_websockify(host, layout).await?;
    start_websockify(host, layout).await?;
    start_node(host, layout).await?;
    wait_webui_tcp(host, DEFAULT_WEBUI_PORT, Duration::from_secs(60)).await?;
    Ok(())
}