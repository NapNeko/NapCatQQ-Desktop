//! 两台远端对接的主机常驻 ssh:授权行与 run.sh 正文,编排走 Host 命令
//!
//! Desktop 不握这条隧道;钥匙只在应用机生成,公钥进 Bot 机 authorized_keys。
//! 反向对接 `-R` 把应用口挂到 Bot 机回环上;正向对接(麦麦)`-L` 把 Bot 的服务口挂到应用机回环上

use std::net::Ipv4Addr;
use std::time::Duration;

use ncd_domain::{AppInstance, AppInstanceId};
use ncd_host::{Host, HostCommand, HostError, HostPath, Os, SshDialTarget, shell_single_quote};
use ncd_traits::AppFrameworkError;

use super::listen_port::allocate_listen_port;

#[path = "resident_link_windows.rs"]
mod windows;

pub const REMOTE_LINKS_REL: &str = "ncd/links";
pub const LINK_PID_FILE: &str = ".ncd-link.pid";
pub const LINK_STOP_FILE: &str = ".ncd-link.stop";
pub const LINK_KEY_NAME: &str = "id_ed25519";
pub const LINK_KEY_PUB: &str = "id_ed25519.pub";
pub const LINK_KNOWN_HOSTS: &str = "known_hosts";
pub const LINK_RUN_SH: &str = "run.sh";
pub const LINK_LOG: &str = "run.log";
const LINK_CONTROL: &str = "control.sock";

const COMMENT_PREFIX: &str = "ncd-link:";

pub struct ResidentLinkSpec<'a> {
    pub instance: &'a AppInstance,
    /// 听口开在哪台机上看 `forward`：反向在 Bot 机，正向在应用机；`taken_ports` 也按那台机算
    pub reuse_port: Option<u16>,
    pub taken_ports: &'a [u16],
    pub forward: ResidentForward,
}

/// 常驻 ssh 都在应用机上跑、连 Bot 机的 sshd，按对接方向决定把哪一端挂到另一端的回环上
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResidentForward {
    /// 反向 WS，Bot 是客户端：`-R 127.0.0.1:<Bot 机听口>:127.0.0.1:<应用口>`
    ExposeApp { app_port: u16 },
    /// 正向 WS，应用端是客户端：`-L 127.0.0.1:<应用机听口>:127.0.0.1:<Bot 服务口>`
    ReachBot { bot_port: u16 },
    /// Bot 宿主机自 SSH：只在 Docker 网桥上监听，连接宿主机回环目标。
    BridgeLoopback {
        listen_host: Ipv4Addr,
        target_port: u16,
    },
}

pub fn link_comment(instance_id: &AppInstanceId) -> String {
    format!("{COMMENT_PREFIX}{}", instance_id.as_str())
}

pub fn pubkey_type_and_blob(pubkey_line: &str) -> Result<(String, String), String> {
    let line = pubkey_line
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("");
    let mut parts = line.split_whitespace();
    let kind = parts.next().ok_or("公钥为空")?.to_string();
    let blob = parts.next().ok_or("公钥缺少 key 本体")?.to_string();
    if !(kind.starts_with("ssh-") || kind.starts_with("ecdsa-") || kind.contains("sk-")) {
        return Err(format!("不支持的公钥类型: {kind}"));
    }
    Ok((kind, blob))
}

/// 两个转发方向都限制到本次对接的同一回环端点，避免只限制一侧时另一侧任意转发。
/// OpenSSH 的 per-key 选项不能单独禁用某个方向；不能用 `permitopen/listen="none"`，
/// 它们会使整条公钥认证失败。`-L` 的客户端听口不受服务端 authorized_keys 控制。
pub fn authorized_keys_line(
    pubkey_line: &str,
    instance_id: &AppInstanceId,
    forward: ResidentForward,
    listen_port: u16,
) -> Result<String, String> {
    let permit = match forward {
        ResidentForward::ExposeApp { .. } if listen_port != 0 => {
            format!(
                "permitlisten=\"127.0.0.1:{listen_port}\",permitopen=\"127.0.0.1:{listen_port}\""
            )
        }
        ResidentForward::ReachBot { bot_port } if bot_port != 0 && listen_port != 0 => {
            format!("permitopen=\"127.0.0.1:{bot_port}\",permitlisten=\"127.0.0.1:{bot_port}\"")
        }
        ResidentForward::BridgeLoopback { target_port, .. }
            if target_port != 0 && listen_port != 0 =>
        {
            format!(
                "permitopen=\"127.0.0.1:{target_port}\",permitlisten=\"127.0.0.1:{target_port}\""
            )
        }
        _ => return Err("常驻隧道听口无效".into()),
    };
    let (kind, blob) = pubkey_type_and_blob(pubkey_line)?;
    Ok(format!(
        "restrict,port-forwarding,{permit},command=\"/bin/false\" {kind} {blob} {}",
        link_comment(instance_id)
    ))
}

fn line_belongs_to_instance(line: &str, instance_id: &AppInstanceId) -> bool {
    let needle = link_comment(instance_id);
    line.split_whitespace().any(|w| w == needle)
}

pub fn upsert_authorized_key(
    existing: &str,
    new_line: &str,
    instance_id: &AppInstanceId,
) -> String {
    let mut out: Vec<&str> = existing
        .lines()
        .filter(|l| !line_belongs_to_instance(l, instance_id))
        .collect();
    let line = new_line.trim_end();
    if !line.is_empty() {
        out.push(line);
    }
    join_lines(&out)
}

pub fn remove_authorized_key(existing: &str, instance_id: &AppInstanceId) -> String {
    let out: Vec<&str> = existing
        .lines()
        .filter(|l| !line_belongs_to_instance(l, instance_id))
        .collect();
    join_lines(&out)
}

fn join_lines(lines: &[&str]) -> String {
    if lines.is_empty() {
        return String::new();
    }
    let mut s = lines.join("\n");
    if !s.ends_with('\n') {
        s.push('\n');
    }
    s
}

pub struct ResidentLinkScriptInput<'a> {
    pub forward: ResidentForward,
    pub listen_port: u16,
    pub dial: &'a SshDialTarget,
}

pub fn render_run_sh(input: &ResidentLinkScriptInput<'_>) -> String {
    let user_host = shell_single_quote(&format!("{}@{}", input.dial.username, input.dial.host));
    let ssh_port = input.dial.port;
    let listen = input.listen_port;
    let tunnel = match input.forward {
        ResidentForward::ExposeApp { app_port } => {
            format!("-R 127.0.0.1:{listen}:127.0.0.1:{app_port}")
        }
        ResidentForward::ReachBot { bot_port } => {
            format!("-L 127.0.0.1:{listen}:127.0.0.1:{bot_port}")
        }
        ResidentForward::BridgeLoopback {
            listen_host,
            target_port,
        } => {
            format!("-L {listen_host}:{listen}:127.0.0.1:{target_port}")
        }
    };
    let control = "-M -S \"$DIR/control.sock\" ";
    format!(
        "#!/bin/sh\n\
         DIR=$(CDPATH= cd -- \"$(dirname -- \"$0\")\" && pwd)\n\
         PIDFILE=\"$DIR/{pid}\"\n\
         STOP=\"$DIR/{stop}\"\n\
         KEY=\"$DIR/{key}\"\n\
         KNOWN=\"$DIR/{known}\"\n\
         SSH_PID=\n\
         echo $$ > \"$PIDFILE\"\n\
         cleanup() {{\n\
           if [ -n \"$SSH_PID\" ]; then kill \"$SSH_PID\" 2>/dev/null; fi\n\
           rm -f \"$PIDFILE\"\n\
         }}\n\
         trap cleanup EXIT INT TERM\n\
         while [ ! -f \"$STOP\" ]; do\n\
           ssh {control}-i \"$KEY\" \\\n\
             -o IdentitiesOnly=yes \\\n\
             -o ExitOnForwardFailure=yes \\\n\
             -o ServerAliveInterval=30 \\\n\
             -o ConnectTimeout=10 \\\n\
             -o ServerAliveCountMax=3 \\\n\
             -o StrictHostKeyChecking=yes \\\n\
             -o \"UserKnownHostsFile=\\\"$KNOWN\\\"\" \\\n\
             -o GlobalKnownHostsFile=/dev/null \\\n\
             -o BatchMode=yes \\\n\
             -N {tunnel} \\\n\
             -p {ssh_port} -- {user_host} &\n\
           SSH_PID=$!\n\
           wait \"$SSH_PID\" || true\n\
           SSH_PID=\n\
           [ -f \"$STOP\" ] && break\n\
           sleep 3\n\
         done\n",
        pid = LINK_PID_FILE,
        stop = LINK_STOP_FILE,
        key = LINK_KEY_NAME,
        known = LINK_KNOWN_HOSTS,
    )
}

pub fn remote_link_dir(home: &str, instance_id: &AppInstanceId) -> HostPath {
    HostPath::from_posix(home)
        .join(REMOTE_LINKS_REL)
        .join(instance_id.as_str())
}

pub async fn ensure_resident_link(
    app_host: &dyn Host,
    bot_host: &dyn Host,
    spec: ResidentLinkSpec<'_>,
) -> Result<u16, AppFrameworkError> {
    if app_host.os() == Os::Windows {
        return windows::ensure(app_host, bot_host, &spec).await;
    }
    let mut created_dir = false;
    let mut wrote_auth = false;
    match ensure_resident_link_inner(app_host, bot_host, &spec, &mut created_dir, &mut wrote_auth)
        .await
    {
        Ok(port) => Ok(port),
        Err(e) => {
            if created_dir || wrote_auth {
                if let Err(cleanup) =
                    teardown_resident_link(app_host, Some(bot_host), &spec.instance.id).await
                {
                    return Err(AppFrameworkError::Host(format!(
                        "{e}；清理本次隧道失败：{cleanup}"
                    )));
                }
            }
            Err(e)
        }
    }
}

async fn ensure_resident_link_inner(
    app_host: &dyn Host,
    bot_host: &dyn Host,
    spec: &ResidentLinkSpec<'_>,
    created_dir: &mut bool,
    wrote_auth: &mut bool,
) -> Result<u16, AppFrameworkError> {
    match spec.forward {
        ResidentForward::ExposeApp { app_port: 0 } => {
            return Err(AppFrameworkError::Validation("应用实例端口无效".into()));
        }
        ResidentForward::ReachBot { bot_port: 0 } => {
            return Err(AppFrameworkError::Validation(
                "协议 Bot 的 WS 服务还没分配端口".into(),
            ));
        }
        ResidentForward::BridgeLoopback { target_port: 0, .. } => {
            return Err(AppFrameworkError::Validation(
                "Docker 桥接目标端口无效".into(),
            ));
        }
        _ => {}
    }
    if app_host.os() != Os::Linux {
        return Err(AppFrameworkError::Validation(
            "两台远端对接只支持 Linux 应用所在机".into(),
        ));
    }
    let mut dial = bot_host.ssh_dial_target().ok_or_else(|| {
        AppFrameworkError::Validation(
            "无法取得 Bot 所在机的 SSH 地址，请先验证该主机的 SSH 连接".into(),
        )
    })?;
    if matches!(spec.forward, ResidentForward::BridgeLoopback { .. }) {
        dial = bridge_dial(bot_host).await?;
    }
    if !app_host.command_exists("ssh").await || !app_host.command_exists("ssh-keygen").await {
        return Err(AppFrameworkError::Validation(
            "先在应用所在机安装 OpenSSH 客户端".into(),
        ));
    }

    let known = trusted_known_hosts(bot_host, &dial)?;
    let listen_host = match spec.forward {
        ResidentForward::ExposeApp { .. } => bot_host,
        ResidentForward::ReachBot { .. } => app_host,
        ResidentForward::BridgeLoopback { .. } => app_host,
    };
    let fwd = allocate_forward_port(listen_host, spec.reuse_port, spec.taken_ports).await?;

    let home_b = remote_home(app_host).await?;
    let link_dir = remote_link_dir(&home_b, &spec.instance.id);
    app_host.create_dir_all(&link_dir).await.map_err(host_err)?;
    *created_dir = true;
    app_host
        .write_file(&link_dir.join(LINK_KNOWN_HOSTS), known.as_bytes())
        .await
        .map_err(host_err)?;

    ensure_ed25519_key(app_host, &link_dir, &spec.instance.id).await?;
    let pub_bytes = app_host
        .read_file(&link_dir.join(LINK_KEY_PUB))
        .await
        .map_err(host_err)?;
    let pub_line = String::from_utf8_lossy(&pub_bytes);
    let auth_line =
        authorized_keys_line_for_host(bot_host, &pub_line, &spec.instance.id, spec.forward, fwd)
            .map_err(AppFrameworkError::Validation)?;

    upsert_bot_authorized_keys(bot_host, &spec.instance.id, &auth_line).await?;
    *wrote_auth = true;

    let script = render_run_sh(&ResidentLinkScriptInput {
        forward: spec.forward,
        listen_port: fwd,
        dial: &dial,
    });
    app_host
        .write_file(&link_dir.join(LINK_RUN_SH), script.as_bytes())
        .await
        .map_err(host_err)?;

    stop_resident_script(app_host, &link_dir).await?;
    start_resident_script(app_host, &link_dir).await?;
    wait_resident_ready(app_host, bot_host, &link_dir, &dial, spec.forward, fwd).await?;
    Ok(fwd)
}

pub async fn teardown_resident_link(
    app_host: &dyn Host,
    bot_host: Option<&dyn Host>,
    instance_id: &AppInstanceId,
) -> Result<(), AppFrameworkError> {
    if app_host.os() == Os::Windows {
        return windows::teardown(app_host, bot_host, instance_id).await;
    }
    let mut failures = Vec::new();
    match remote_home(app_host).await {
        Ok(home) => {
            let dir = remote_link_dir(&home, instance_id);
            if let Err(error) = stop_resident_script(app_host, &dir).await {
                failures.push(format!("停止进程：{error}"));
            }
            match app_host.exists(&dir).await {
                Ok(true) => {
                    if let Err(error) = app_host.remove_dir_all(&dir).await {
                        failures.push(format!("删除目录：{error}"));
                    }
                }
                Ok(false) => {}
                Err(error) => failures.push(format!("检查目录：{error}")),
            }
        }
        Err(error) => failures.push(format!("读取主目录：{error}")),
    }
    if let Some(bot) = bot_host {
        if let Err(error) = strip_bot_authorized_key(bot, instance_id).await {
            failures.push(format!("撤销授权：{error}"));
        }
    }
    if !failures.is_empty() {
        return Err(AppFrameworkError::Host(format!(
            "清理常驻隧道失败：{}",
            failures.join("；")
        )));
    }
    Ok(())
}

/// 认证完成后才会建立 ControlMaster socket；另查实际监听，不能只认重连脚本 PID。
pub(super) async fn bridge_ready(
    host: &dyn Host,
    dir: &HostPath,
    listen_host: Ipv4Addr,
    port: u16,
) -> Result<bool, AppFrameworkError> {
    let dial = bridge_dial(host).await?;
    let out = host
        .run_to_string(bridge_probe_command(dir, &dial, listen_host, port))
        .await
        .map_err(host_err)?;
    Ok(out.success())
}

async fn bridge_dial(host: &dyn Host) -> Result<SshDialTarget, AppFrameworkError> {
    let mut dial = host
        .ssh_dial_target()
        .ok_or_else(|| AppFrameworkError::Validation("Docker 桥接缺少宿主机 SSH 地址".into()))?;
    dial.host = "127.0.0.1".into();
    let candidates = if dial.port == 22 {
        vec![22]
    } else {
        vec![dial.port, 22]
    };
    for port in candidates {
        let connect = shell_single_quote(&format!("exec 3<>/dev/tcp/127.0.0.1/{port}"));
        let script = format!(
            "if command -v bash >/dev/null 2>&1; then bash -c {connect}; \
             elif command -v ss >/dev/null 2>&1; then \
             ss -H -lnt | awk '$4 ~ /:{port}$/ {{ found=1 }} END {{ exit !found }}'; \
             else exit 1; fi"
        );
        let out = host
            .run_to_string(
                HostCommand::new("sh")
                    .arg("-c")
                    .arg(script)
                    .timeout(Duration::from_secs(3)),
            )
            .await
            .map_err(host_err)?;
        if out.success() {
            dial.port = port;
            return Ok(dial);
        }
    }
    Err(AppFrameworkError::Host(
        "宿主机回环上的 SSH 端口不可访问（已检查档案端口和 22）".into(),
    ))
}

fn bridge_probe_command(
    dir: &HostPath,
    dial: &SshDialTarget,
    listen_host: Ipv4Addr,
    port: u16,
) -> HostCommand {
    let control = shell_single_quote(dir.join(LINK_CONTROL).as_posix());
    let peer = shell_single_quote(&format!("{}@{}", dial.username, dial.host));
    let endpoint = shell_single_quote(&format!("{listen_host}:{port}"));
    let connect = shell_single_quote(&format!("exec 3<>/dev/tcp/{listen_host}/{port}"));
    let script = format!(
        "ssh -S {control} -O check -p {} -- {peer} >/dev/null 2>&1 || exit 1\n\
         if command -v bash >/dev/null 2>&1; then\n\
           bash -c {connect}\n\
         elif command -v ss >/dev/null 2>&1; then\n\
           ss -H -lnt | awk -v endpoint={endpoint} '$4 == endpoint {{ found=1 }} END {{ exit !found }}'\n\
         else\n\
           exit 1\n\
         fi",
        dial.port
    );
    HostCommand::new("sh")
        .arg("-c")
        .arg(script)
        .timeout(Duration::from_secs(5))
}

async fn resident_transport_ready(
    app_host: &dyn Host,
    bot_host: &dyn Host,
    dir: &HostPath,
    dial: &SshDialTarget,
    forward: ResidentForward,
    port: u16,
) -> Result<bool, AppFrameworkError> {
    if let ResidentForward::ExposeApp { .. } = forward {
        let master = HostCommand::new("ssh")
            .arg("-S")
            .arg(dir.join(LINK_CONTROL).as_posix())
            .arg("-O")
            .arg("check")
            .arg("-p")
            .arg(dial.port.to_string())
            .arg("--")
            .arg(format!("{}@{}", dial.username, dial.host))
            .timeout(Duration::from_secs(5));
        if !app_host
            .run_to_string(master)
            .await
            .map_err(host_err)?
            .success()
        {
            return Ok(false);
        }
        let connect = shell_single_quote(&format!("exec 3<>/dev/tcp/127.0.0.1/{port}"));
        let probe = format!(
            "if command -v bash >/dev/null 2>&1; then bash -c {connect}; \
            elif command -v ss >/dev/null 2>&1; then \
            ss -H -lnt | awk '$4 ~ /:{port}$/ {{ found=1 }} END {{ exit !found }}'; \
            else exit 1; fi"
        );
        return Ok(bot_host
            .run_to_string(
                HostCommand::new("sh")
                    .arg("-c")
                    .arg(probe)
                    .timeout(Duration::from_secs(5)),
            )
            .await
            .map_err(host_err)?
            .success());
    }
    let listen_host = match forward {
        ResidentForward::BridgeLoopback { listen_host, .. } => listen_host,
        _ => Ipv4Addr::LOCALHOST,
    };
    Ok(app_host
        .run_to_string(bridge_probe_command(dir, dial, listen_host, port))
        .await
        .map_err(host_err)?
        .success())
}

async fn wait_resident_ready(
    app_host: &dyn Host,
    bot_host: &dyn Host,
    dir: &HostPath,
    dial: &SshDialTarget,
    forward: ResidentForward,
    port: u16,
) -> Result<(), AppFrameworkError> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    loop {
        if resident_transport_ready(app_host, bot_host, dir, dial, forward, port).await? {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(AppFrameworkError::Host(format!(
                "常驻 SSH 隧道未能完成认证并监听端口 {port}，请检查主机间 SSH 登录及转发权限"
            )));
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

pub async fn reconcile_resident_link(
    app_host: &dyn Host,
    bot_host: &dyn Host,
    instance: &AppInstance,
    forward: ResidentForward,
    forward_port: u16,
) -> Result<(), AppFrameworkError> {
    if app_host.os() == Os::Windows {
        return windows::reconcile(app_host, bot_host, instance, forward, forward_port).await;
    }
    let home = remote_home(app_host).await?;
    let dir = remote_link_dir(&home, &instance.id);
    let mut dial = bot_host
        .ssh_dial_target()
        .ok_or_else(|| AppFrameworkError::Validation("无法取得常驻隧道的 SSH 地址".into()))?;
    if matches!(forward, ResidentForward::BridgeLoopback { .. }) {
        dial = bridge_dial(bot_host).await?;
    }
    let expected = render_run_sh(&ResidentLinkScriptInput {
        forward,
        listen_port: forward_port,
        dial: &dial,
    });
    let same_target = match app_host.read_file(&dir.join(LINK_RUN_SH)).await {
        Ok(bytes) => bytes.as_ref() == expected.as_bytes(),
        Err(HostError::PathNotFound { .. }) => false,
        Err(error) => return Err(host_err(error)),
    };
    if same_target
        && resident_transport_ready(app_host, bot_host, &dir, &dial, forward, forward_port).await?
    {
        return Ok(());
    }
    ensure_resident_link(
        app_host,
        bot_host,
        ResidentLinkSpec {
            instance,
            reuse_port: Some(forward_port),
            taken_ports: &[],
            forward,
        },
    )
    .await?;
    Ok(())
}

pub(super) async fn remote_home(host: &dyn Host) -> Result<String, AppFrameworkError> {
    if host.os() == Os::Windows {
        return Ok(host.remote_windows_info().await.map_err(host_err)?.home_dir);
    }
    let out = host
        .run_to_string(
            HostCommand::new("sh")
                .arg("-c")
                .arg("printf '%s' \"$HOME\"")
                .timeout(Duration::from_secs(15)),
        )
        .await
        .map_err(host_err)?;
    let home = out.stdout.trim();
    if home.is_empty() || !home.starts_with('/') {
        return Err(AppFrameworkError::Host(
            "无法解析远端 $HOME，不能决定常驻隧道目录".into(),
        ));
    }
    Ok(home.to_string())
}

fn trusted_known_hosts(
    bot_host: &dyn Host,
    dial: &SshDialTarget,
) -> Result<String, AppFrameworkError> {
    let key = bot_host.trusted_ssh_host_key().ok_or_else(|| {
        AppFrameworkError::Validation(
            "Bot 所在机尚未通过 SSH 主机密钥验证，不能建立常驻隧道".into(),
        )
    })?;
    let mut pieces = key.split_whitespace();
    let kind = pieces
        .next()
        .ok_or_else(|| AppFrameworkError::Validation("SSH 主机公钥为空".into()))?;
    let blob = pieces
        .next()
        .ok_or_else(|| AppFrameworkError::Validation("SSH 主机公钥不完整".into()))?;
    if dial.host.is_empty()
        || dial
            .host
            .chars()
            .any(|c| c.is_control() || c.is_whitespace() || matches!(c, ',' | '|'))
        || pieces.next().is_some()
    {
        return Err(AppFrameworkError::Validation(
            "SSH 主机密钥或地址格式无效".into(),
        ));
    }
    let host = if dial.port == 22 {
        dial.host.clone()
    } else {
        format!("[{}]:{}", dial.host, dial.port)
    };
    Ok(format!("{host} {kind} {blob}\n"))
}

fn authorized_keys_line_for_host(
    bot_host: &dyn Host,
    public_key: &str,
    instance: &AppInstanceId,
    forward: ResidentForward,
    port: u16,
) -> Result<String, String> {
    let line = authorized_keys_line(public_key, instance, forward, port)?;
    Ok(if bot_host.os() == Os::Windows {
        line.replacen(
            "command=\"/bin/false\"",
            "command=\"cmd.exe /d /c exit 1\"",
            1,
        )
    } else {
        line
    })
}

async fn allocate_forward_port(
    bot_host: &dyn Host,
    reuse: Option<u16>,
    taken: &[u16],
) -> Result<u16, AppFrameworkError> {
    if let Some(port) = reuse {
        if port == 0 {
            return Err(AppFrameworkError::Validation("常驻隧道听口无效".into()));
        }
        return Ok(port);
    }
    let mut blocked = taken.to_vec();
    for _ in 0..48 {
        let port =
            allocate_listen_port(None, &blocked, false).map_err(AppFrameworkError::Validation)?;
        if !remote_loopback_busy(bot_host, port).await {
            return Ok(port);
        }
        blocked.push(port);
    }
    Err(AppFrameworkError::Validation(
        "分配不到空闲的常驻隧道听口".into(),
    ))
}

async fn remote_loopback_busy(host: &dyn Host, port: u16) -> bool {
    if host.os() == Os::Windows {
        return ncd_appframework::ports::remote_listening_ports(host)
            .await
            .map(|ports| ports.contains(&port))
            .unwrap_or(true);
    }
    let script = format!(
        "bash -c 'echo >/dev/tcp/127.0.0.1/{port}' >/dev/null 2>&1 \
         || (command -v ss >/dev/null 2>&1 && ss -lnt 2>/dev/null | grep -Eq ':{port}([[:space:]]|$)') \
         || (command -v netstat >/dev/null 2>&1 && netstat -lnt 2>/dev/null | grep -Eq ':{port}([[:space:]]|$)')"
    );
    match host
        .run_to_string(
            HostCommand::new("sh")
                .arg("-c")
                .arg(script)
                .timeout(Duration::from_secs(5)),
        )
        .await
    {
        Ok(out) => out.success(),
        Err(_) => false,
    }
}

async fn ensure_ed25519_key(
    app_host: &dyn Host,
    link_dir: &HostPath,
    instance_id: &AppInstanceId,
) -> Result<(), AppFrameworkError> {
    let key = link_dir.join(LINK_KEY_NAME);
    let pub_path = link_dir.join(LINK_KEY_PUB);
    let has_key = app_host.exists(&key).await.unwrap_or(false);
    let has_pub = app_host.exists(&pub_path).await.unwrap_or(false);
    if has_key && has_pub {
        app_host.secure_private_key(&key).await.map_err(host_err)?;
        return Ok(());
    }
    let out = app_host
        .run_to_string(
            HostCommand::new("ssh-keygen")
                .arg("-t")
                .arg("ed25519")
                .arg("-N")
                .arg("")
                .arg("-f")
                .arg(key.render_for(app_host.os()))
                .arg("-C")
                .arg(link_comment(instance_id))
                .arg("-q")
                .timeout(Duration::from_secs(20)),
        )
        .await
        .map_err(host_err)?;
    if !out.success() {
        return Err(AppFrameworkError::Host(format!(
            "在应用所在机生成对接钥匙失败: {}",
            out.stderr.trim()
        )));
    }
    app_host.secure_private_key(&key).await.map_err(host_err)?;
    Ok(())
}

async fn upsert_bot_authorized_keys(
    bot_host: &dyn Host,
    instance_id: &AppInstanceId,
    line: &str,
) -> Result<(), AppFrameworkError> {
    if bot_host.os() == Os::Windows {
        return bot_host
            .install_authorized_key(line, &link_comment(instance_id))
            .await
            .map_err(host_err);
    }
    let (path, existing) = read_authorized_keys(bot_host).await?;
    let next = upsert_authorized_key(&existing, line, instance_id);
    write_authorized_keys(bot_host, &path, &next).await
}

async fn strip_bot_authorized_key(
    bot_host: &dyn Host,
    instance_id: &AppInstanceId,
) -> Result<(), AppFrameworkError> {
    if bot_host.os() == Os::Windows {
        return bot_host
            .remove_authorized_key(&link_comment(instance_id))
            .await
            .map_err(host_err);
    }
    let (path, existing) = read_authorized_keys(bot_host).await?;
    let next = remove_authorized_key(&existing, instance_id);
    if next == existing {
        return Ok(());
    }
    write_authorized_keys(bot_host, &path, &next).await
}

async fn read_authorized_keys(
    bot_host: &dyn Host,
) -> Result<(HostPath, String), AppFrameworkError> {
    let home = remote_home(bot_host).await?;
    let ssh_dir = HostPath::from_posix(&home).join(".ssh");
    bot_host.create_dir_all(&ssh_dir).await.map_err(host_err)?;
    let _ = bot_host
        .run_to_string(
            HostCommand::new("chmod")
                .arg("700")
                .arg(ssh_dir.as_posix())
                .timeout(Duration::from_secs(10)),
        )
        .await;
    let path = ssh_dir.join("authorized_keys");
    let text = match bot_host.read_file(&path).await {
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(HostError::PathNotFound { .. }) => String::new(),
        Err(e) => return Err(host_err(e)),
    };
    Ok((path, text))
}

async fn write_authorized_keys(
    bot_host: &dyn Host,
    path: &HostPath,
    text: &str,
) -> Result<(), AppFrameworkError> {
    bot_host
        .write_file(path, text.as_bytes())
        .await
        .map_err(host_err)?;
    let _ = bot_host
        .run_to_string(
            HostCommand::new("chmod")
                .arg("600")
                .arg(path.as_posix())
                .timeout(Duration::from_secs(10)),
        )
        .await;
    Ok(())
}

fn stop_resident_command(link_dir: &HostPath) -> HostCommand {
    let dir = shell_single_quote(link_dir.as_posix());
    let script = format!(
        r#"stop={dir}/{stop}; pidf={dir}/{pid}; run={dir}/{run}
owns_pid() {{
  [ -r "/proc/$pid/cmdline" ] && tr '\000' '\n' < "/proc/$pid/cmdline" | awk -v expected="$run" '
    NR == 1 {{ if ($0 != "sh" && $0 != "/bin/sh") exit 1 }}
    NR == 2 {{ found = ($0 == expected); exit }}
    END {{ exit !found }}'
}}
touch "$stop" || exit 1
if [ -f "$pidf" ]; then
  pid=$(tr -dc '0-9' < "$pidf")
  if [ -n "$pid" ] && owns_pid; then
    kill "$pid" 2>/dev/null || true
    for i in 1 2 3 4 5 6 7 8 9 10; do owns_pid || break; sleep 0.3; done
    if owns_pid; then kill -9 "$pid" || exit 1; fi
  fi
fi
rm -f "$pidf" "$stop" {dir}/{control}
"#,
        stop = LINK_STOP_FILE,
        pid = LINK_PID_FILE,
        run = LINK_RUN_SH,
        control = LINK_CONTROL
    );
    HostCommand::new("sh")
        .arg("-c")
        .arg(script)
        .timeout(Duration::from_secs(15))
}

async fn stop_resident_script(
    app_host: &dyn Host,
    link_dir: &HostPath,
) -> Result<(), AppFrameworkError> {
    if !app_host.exists(link_dir).await.map_err(host_err)? {
        return Ok(());
    }
    let out = app_host
        .run_to_string(stop_resident_command(link_dir))
        .await
        .map_err(host_err)?;
    if !out.success() {
        return Err(AppFrameworkError::Host(format!(
            "停止常驻隧道失败：{}",
            out.stderr.trim()
        )));
    }
    Ok(())
}

async fn start_resident_script(
    app_host: &dyn Host,
    link_dir: &HostPath,
) -> Result<(), AppFrameworkError> {
    let dir = shell_single_quote(link_dir.as_posix());
    let script = format!(
        "rm -f {dir}/{stop}\n\
         chmod 700 {dir}/{run} {dir}/{key} 2>/dev/null || true\n\
         nohup setsid /bin/sh {dir}/{run} >> {dir}/{log} 2>&1 </dev/null &\n\
         sleep 1\n\
         if [ -f {dir}/{pid} ] && kill -0 \"$(tr -dc '0-9' < {dir}/{pid})\" 2>/dev/null; then\n\
           echo RUNNING\n\
         else\n\
           echo EXITED\n\
           tail -n 40 {dir}/{log} 2>/dev/null\n\
           exit 98\n\
         fi\n",
        stop = LINK_STOP_FILE,
        run = LINK_RUN_SH,
        key = LINK_KEY_NAME,
        log = LINK_LOG,
        pid = LINK_PID_FILE,
    );
    let out = app_host
        .run_to_string(
            HostCommand::new("sh")
                .arg("-c")
                .arg(script)
                .timeout(Duration::from_secs(20)),
        )
        .await
        .map_err(host_err)?;
    if !out.success() || !out.stdout.contains("RUNNING") {
        let detail = out.stderr.trim();
        let detail = if detail.is_empty() {
            out.stdout.trim()
        } else {
            detail
        };
        return Err(AppFrameworkError::Host(format!(
            "拉起常驻隧道失败: {detail}"
        )));
    }
    Ok(())
}

fn host_err(e: HostError) -> AppFrameworkError {
    AppFrameworkError::Host(e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ncd_host::SshDialTarget;

    fn id() -> AppInstanceId {
        AppInstanceId::new("k1")
    }

    const EXPOSE: ResidentForward = ResidentForward::ExposeApp { app_port: 32100 };

    #[test]
    fn authorized_keys_round_trip_replaces_same_instance_only() {
        let pub1 = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIold comment";
        let pub2 = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAInew leftover";
        let other = "ssh-ed25519 AAAAother user@box\n";
        let first = authorized_keys_line(pub1, &id(), EXPOSE, 21001).unwrap();
        let merged = upsert_authorized_key(other, &first, &id());
        assert!(merged.contains("permitlisten=\"127.0.0.1:21001\""));
        assert!(merged.contains("ncd-link:k1"));
        assert!(merged.contains("user@box"));
        let second = authorized_keys_line(pub2, &id(), EXPOSE, 21002).unwrap();
        let replaced = upsert_authorized_key(&merged, &second, &id());
        assert!(replaced.contains("21002"));
        assert!(!replaced.contains("21001"));
        assert!(replaced.contains("AAAAother"));
        assert_eq!(replaced.matches("ncd-link:k1").count(), 1);
        let stripped = remove_authorized_key(&replaced, &id());
        assert!(!stripped.contains("ncd-link:k1"));
        assert!(stripped.contains("user@box"));
    }

    #[test]
    fn two_instance_auth_lines_do_not_clobber() {
        let a = AppInstanceId::new("a");
        let b = AppInstanceId::new("b");
        let la = authorized_keys_line("ssh-ed25519 AAAa c", &a, EXPOSE, 20001).unwrap();
        let lb = authorized_keys_line("ssh-ed25519 AAAb c", &b, EXPOSE, 20002).unwrap();
        let both = upsert_authorized_key(&upsert_authorized_key("", &la, &a), &lb, &b);
        assert!(both.contains("ncd-link:a"));
        assert!(both.contains("ncd-link:b"));
        let only_b = remove_authorized_key(&both, &a);
        assert!(!only_b.contains("ncd-link:a"));
        assert!(only_b.contains("ncd-link:b"));
    }

    #[test]
    fn run_sh_uses_isolated_key_and_remote_forward() {
        let dial = SshDialTarget {
            host: "bot.example".into(),
            port: 2222,
            username: "alice".into(),
        };
        let script = render_run_sh(&ResidentLinkScriptInput {
            forward: EXPOSE,
            listen_port: 21001,
            dial: &dial,
        });
        assert!(script.contains("-R 127.0.0.1:21001:127.0.0.1:32100"));
        assert!(script.contains("IdentitiesOnly=yes"));
        assert!(script.contains("ExitOnForwardFailure=yes"));
        assert!(script.contains("ServerAliveInterval=30"));
        assert!(script.contains(r#"-o "UserKnownHostsFile=\"$KNOWN\"""#));
        assert!(script.contains("GlobalKnownHostsFile=/dev/null"));
        assert!(script.contains("-p 2222"));
        assert!(script.contains("'alice@bot.example'"));
        assert!(!script.contains("UserKnownHostsFile=~/.ssh"));
    }

    /// 正向（麦麦）：应用机上 `-L` 听口接到 Bot 机回环上的服务口；钥匙只许连那一个服务口
    #[test]
    fn forward_links_reach_the_bot_server_from_the_app_host() {
        let dial = SshDialTarget {
            host: "bot.example".into(),
            port: 22,
            username: "alice".into(),
        };
        let reach = ResidentForward::ReachBot { bot_port: 23456 };
        let script = render_run_sh(&ResidentLinkScriptInput {
            forward: reach,
            listen_port: 34567,
            dial: &dial,
        });
        assert!(
            script.contains("-N -L 127.0.0.1:34567:127.0.0.1:23456"),
            "{script}"
        );
        assert!(!script.contains("-R "));

        let line = authorized_keys_line("ssh-ed25519 AAAa c", &id(), reach, 34567).unwrap();
        assert!(line.contains("permitopen=\"127.0.0.1:23456\""), "{line}");
        assert!(line.contains("permitlisten=\"127.0.0.1:23456\""), "{line}");
        assert!(line.contains("command=\"/bin/false\"") && line.ends_with("ncd-link:k1"));
        assert!(
            authorized_keys_line(
                "ssh-ed25519 AAAa c",
                &id(),
                ResidentForward::ReachBot { bot_port: 0 },
                1
            )
            .is_err()
        );
    }

    #[test]
    fn pubkey_rejects_garbage() {
        assert!(pubkey_type_and_blob("").is_err());
        assert!(pubkey_type_and_blob("not-a-key").is_err());
        assert!(authorized_keys_line("ssh-ed25519 blob c", &id(), EXPOSE, 0).is_err());
    }

    #[test]
    fn docker_bridge_uses_local_forward_on_gateway_and_authenticated_control_probe() {
        let dial = SshDialTarget {
            host: "127.0.0.1".into(),
            port: 2222,
            username: "alice".into(),
        };
        let gateway = Ipv4Addr::new(172, 17, 0, 1);
        let script = render_run_sh(&ResidentLinkScriptInput {
            forward: ResidentForward::BridgeLoopback {
                listen_host: gateway,
                target_port: 28080,
            },
            listen_port: 28081,
            dial: &dial,
        });
        assert!(script.contains("-L 172.17.0.1:28081:127.0.0.1:28080"));
        assert!(script.contains("-M -S \"$DIR/control.sock\""));
        assert!(!script.contains("-R "));
        let probe = bridge_probe_command(
            &HostPath::from_posix("/home/alice/ncd/links/app-docker-bridge"),
            &dial,
            gateway,
            28081,
        );
        assert!(probe.args[1].contains("-O check"));
        assert!(probe.args[1].contains("/dev/tcp/172.17.0.1/28081"));
        assert!(probe.args[1].contains("ss -H -lnt"));
        assert!(
            !probe.args[1].contains("kill -0"),
            "脚本 PID 不能代表端口就绪"
        );
    }

    #[cfg(unix)]
    #[test]
    fn stop_ignores_a_stale_pid_reused_by_an_unrelated_process() {
        let dir = tempfile::tempdir().unwrap();
        let mut unrelated = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .unwrap();
        let pid_file = dir.path().join(LINK_PID_FILE);
        std::fs::write(&pid_file, unrelated.id().to_string()).unwrap();
        let command = stop_resident_command(&HostPath::from_posix(dir.path().to_str().unwrap()));
        let status = std::process::Command::new(&command.program)
            .args(&command.args)
            .status()
            .unwrap();
        let untouched = unrelated.try_wait().unwrap().is_none();
        let removed = !pid_file.exists();
        let _ = unrelated.kill();
        let _ = unrelated.wait();
        assert!(status.success());
        assert!(untouched, "主机重启后的旧 PID 不能用于终止无关进程");
        assert!(removed, "旧 PID 记录应清理，以便重新建立隧道");
    }
}
