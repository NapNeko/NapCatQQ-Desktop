//! 远端 Linux QQ 是否在跑：看进程和 Ptlogin 账号，不写死 WebUI 口、不只认 `-q`。
//!
//! 导入 / SnowLuma 自启的 QQ 经常没有 `qq -q <uin>`。
//! 顺序：pid 文件 → cmdline `-q/--qq` → 本机 Ptlogin2（4301/4303/…）→ cmdline 里的 UIN。

/// 生成 dash-safe 探测脚本。stdout 第一行是 pid，没有则空。
pub fn linux_qq_running_pid_script(
    qq_id: u64,
    pid_file: Option<&str>,
    qq_bin: Option<&str>,
) -> String {
    let pid_file = pid_file.unwrap_or("").replace('\'', "'\"'\"'");
    let qq_bin = qq_bin.unwrap_or("").replace('\'', "'\"'\"'");
    format!(
        r#"pidfile='{pid_file}'
qqbin='{qq_bin}'
qid='{qq_id}'
if [ -n "$pidfile" ] && [ -f "$pidfile" ]; then
  pid=$(cat "$pidfile" 2>/dev/null || true)
  if [ -n "$pid" ] && kill -0 "$pid" 2>/dev/null; then
    echo "$pid"
    exit 0
  fi
fi
alive() {{
  [ -n "$1" ] && [ "$1" != "0" ] && kill -0 "$1" 2>/dev/null
}}
is_helper() {{
  case "$1" in *"--type="*) return 0 ;; esac
  return 1
}}
collect_qq_pids() {{
  pgrep -x qq 2>/dev/null
  pgrep -x QQ 2>/dev/null
  if [ -n "$qqbin" ] && [ -x "$qqbin" ]; then
    for proc in /proc/[0-9]*; do
      [ -d "$proc" ] || continue
      pid=${{proc#/proc/}}
      alive "$pid" || continue
      exe=$(readlink -f "$proc/exe" 2>/dev/null || true)
      [ "$exe" = "$qqbin" ] || continue
      echo "$pid"
    done
  fi
}}
http_get() {{
  url="$1"
  if command -v curl >/dev/null 2>&1; then
    curl -sk --max-time 2 \
      -H 'Host: localhost.ptlogin2.qq.com' \
      -H 'Referer: https://xui.ptlogin2.qq.com/' \
      -H 'Cookie: pt_local_token=0' \
      "$url" 2>/dev/null
    return
  fi
  if command -v python3 >/dev/null 2>&1; then
    python3 -c "import ssl,urllib.request,sys
ctx=ssl._create_unverified_context()
req=urllib.request.Request(sys.argv[1], headers={{'Host':'localhost.ptlogin2.qq.com','Referer':'https://xui.ptlogin2.qq.com/','Cookie':'pt_local_token=0'}})
try:
    print(urllib.request.urlopen(req, context=ctx, timeout=2).read().decode('utf-8','replace'))
except Exception:
    pass" "$url" 2>/dev/null
  fi
}}
fetch_ptlogin() {{
  port="$1"
  body=$(http_get "https://127.0.0.1:${{port}}/pt_get_uins?callback=ptui_getuins_CB&pt_local_tk=0")
  [ -n "$body" ] || body=$(http_get "http://127.0.0.1:${{port}}/pt_get_uins?callback=ptui_getuins_CB&pt_local_tk=0")
  echo "$body"
}}
pid_on_port() {{
  port="$1"
  if command -v ss >/dev/null 2>&1; then
    ss -ltnp 2>/dev/null | grep -E ":${{port}}[[:space:]]" | sed -n 's/.*pid=\([0-9][0-9]*\).*/\1/p' | head -n 1
    return
  fi
  if command -v fuser >/dev/null 2>&1; then
    fuser -n tcp "$port" 2>/dev/null | awk '{{print $1; exit}}'
  fi
}}
# cmdline 明确带账号（Desktop 冷启动）
for pid in $(collect_qq_pids | awk 'NF && !a[$1]++'); do
  alive "$pid" || continue
  cmd=$(tr '\0' ' ' < /proc/$pid/cmdline 2>/dev/null || true)
  is_helper "$cmd" && continue
  case "$cmd" in
    *"-q $qid"*|*-q"$qid"*|*"--qq $qid"*|*"--qq=$qid"*|*"--qq-id=$qid"*)
      echo "$pid"
      exit 0
      ;;
  esac
done
# Ptlogin2：QQ NT 本机快捷登录口，不依赖 -q
for port in 4301 4303 4305 4307 4309; do
  body=$(fetch_ptlogin "$port")
  [ -n "$body" ] || continue
  if echo "$body" | grep -qE '"uin"[[:space:]]*:[[:space:]]*"?'"$qid"; then
    pid=$(pid_on_port "$port")
    if alive "$pid"; then
      echo "$pid"
      exit 0
    fi
  fi
done
# 主进程 cmdline 里出现该 UIN
for pid in $(collect_qq_pids | awk 'NF && !a[$1]++'); do
  alive "$pid" || continue
  cmd=$(tr '\0' ' ' < /proc/$pid/cmdline 2>/dev/null || true)
  is_helper "$cmd" && continue
  if echo "$cmd" | grep -qE "(^|[^0-9])$qid([^0-9]|$)"; then
    echo "$pid"
    exit 0
  fi
done
exit 0
"#
    )
}

#[cfg(test)]
mod tests {
    use super::linux_qq_running_pid_script;

    #[test]
    fn script_uses_ptlogin_not_fixed_webui_port() {
        let script = linux_qq_running_pid_script(
            2703401480,
            Some("/tmp/pid_bot_2703401480"),
            Some("/root/Napcat/opt/QQ/qq"),
        );
        assert!(script.contains("pt_get_uins"));
        assert!(script.contains("4301"));
        assert!(script.contains("pgrep -x qq"));
        assert!(script.contains("-q $qid"));
        assert!(!script.contains("5099"));
        assert!(script.contains("2703401480"));
    }

    #[test]
    fn script_escapes_quotes_in_paths() {
        let script = linux_qq_running_pid_script(1, Some("/tmp/a'b"), Some("/opt/x'y"));
        assert!(script.contains("a'\"'\"'b"));
    }
}
