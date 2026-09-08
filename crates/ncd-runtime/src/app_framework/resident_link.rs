//! 两台远端对接的主机常驻 ssh -R:授权行与 run.sh 正文,编排走 Host 命令
//!
//! Desktop 不握这条隧道;钥匙只在应用机生成,公钥进 Bot 机 authorized_keys

use std::time::Duration;

use ncd_domain::{AppInstance, AppInstanceId};
use ncd_host::{Host, HostCommand, HostError, HostPath, Os, SshDialTarget};
use ncd_traits::AppFrameworkError;

use super::listen_port::allocate_listen_port;

pub const REMOTE_LINKS_REL: &str = "ncd/links";
pub const LINK_PID_FILE: &str = ".ncd-link.pid";
pub const LINK_STOP_FILE: &str = ".ncd-link.stop";
pub const LINK_KEY_NAME: &str = "id_ed25519";
pub const LINK_KEY_PUB: &str = "id_ed25519.pub";
pub const LINK_KNOWN_HOSTS: &str = "known_hosts";
pub const LINK_RUN_SH: &str = "run.sh";
pub const LINK_LOG: &str = "run.log";

const COMMENT_PREFIX: &str = "ncd-link:";
const KEYSCAN_TIMEOUT: Duration = Duration::from_secs(20);

pub struct ResidentLinkSpec<'a> {
    pub instance: &'a AppInstance,
    pub reuse_port: Option<u16>,
    pub taken_ports: &'a [u16],
}

pub fn link_comment(instance_id: &AppInstanceId) -> String {
    format!("{COMMENT_PREFIX}{}", instance_id.as_str())
}

pub fn pubkey_type_and_blob(pubkey_line: &str) -> Result<(String, String), String> {
    let line = pubkey_line.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    let mut parts = line.split_whitespace();
    let kind = parts.next().ok_or("公钥为空")?.to_string();
    let blob = parts.next().ok_or("公钥缺少 key 本体")?.to_string();
    if !(kind.starts_with("ssh-") || kind.starts_with("ecdsa-") || kind.contains("sk-")) {
        return Err(format!("不支持的公钥类型: {kind}"));
    }
    Ok((kind, blob))
}

pub fn authorized_keys_line(
    pubkey_line: &str,
    instance_id: &AppInstanceId,
    listen_port: u16,
) -> Result<String, String> {
    if listen_port == 0 {
        return Err("常驻隧道听口无效".into());
    }
    let (kind, blob) = pubkey_type_and_blob(pubkey_line)?;
    Ok(format!(
        "restrict,port-forwarding,permitlisten=\"127.0.0.1:{listen_port}\",command=\"/bin/false\" {kind} {blob} {}",
        link_comment(instance_id)
    ))
}

fn line_belongs_to_instance(line: &str, instance_id: &AppInstanceId) -> bool {
    let needle = link_comment(instance_id);
    line.split_whitespace().any(|w| w == needle)
}

pub fn upsert_authorized_key(existing: &str, new_line: &str, instance_id: &AppInstanceId) -> String {
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
    pub app_port: u16,
    pub forward_port: u16,
    pub dial: &'a SshDialTarget,
}

pub fn render_run_sh(input: &ResidentLinkScriptInput<'_>) -> String {
    let user_host = shell_quote(&format!("{}@{}", input.dial.username, input.dial.host));
    let ssh_port = input.dial.port;
    let fwd = input.forward_port;
    let app = input.app_port;
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
           ssh -i \"$KEY\" \\\n\
             -o IdentitiesOnly=yes \\\n\
             -o ExitOnForwardFailure=yes \\\n\
             -o ServerAliveInterval=30 \\\n\
             -o ServerAliveCountMax=3 \\\n\
             -o StrictHostKeyChecking=yes \\\n\
             -o UserKnownHostsFile=\"$KNOWN\" \\\n\
             -o GlobalKnownHostsFile=/dev/null \\\n\
             -o BatchMode=yes \\\n\
             -N -R 127.0.0.1:{fwd}:127.0.0.1:{app} \\\n\
             -p {ssh_port} {user_host} &\n\
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

fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\"'\"'"))
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
    let mut created_dir = false;
    let mut wrote_auth = false;
    match ensure_resident_link_inner(app_host, bot_host, &spec, &mut created_dir, &mut wrote_auth)
        .await
    {
        Ok(port) => Ok(port),
        Err(e) => {
            if created_dir || wrote_auth {
                let _ = teardown_resident_link(app_host, Some(bot_host), &spec.instance.id).await;
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
    if spec.instance.port == 0 {
        return Err(AppFrameworkError::Validation("应用实例端口无效".into()));
    }
    if app_host.os() != Os::Linux {
        return Err(AppFrameworkError::Validation(
            "两台远端对接只支持 Linux 应用所在机".into(),
        ));
    }
    let dial = bot_host.ssh_dial_target().ok_or_else(|| {
        AppFrameworkError::Validation(
            "无法取得 Bot 所在机的 SSH 地址，两台远端对接需要 Bot 机是远端 Linux".into(),
        )
    })?;
    if !app_host.command_exists("ssh").await || !app_host.command_exists("ssh-keygen").await {
        return Err(AppFrameworkError::Validation(
            "先在应用所在机安装 OpenSSH 客户端".into(),
        ));
    }

    let known = ssh_keyscan(app_host, &dial).await?;
    let fwd = allocate_forward_port(bot_host, spec.reuse_port, spec.taken_ports).await?;

    let home_b = remote_home(app_host).await?;
    let link_dir = remote_link_dir(&home_b, &spec.instance.id);
    app_host
        .create_dir_all(&link_dir)
        .await
        .map_err(host_err)?;
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
    let auth_line = authorized_keys_line(&pub_line, &spec.instance.id, fwd)
        .map_err(AppFrameworkError::Validation)?;

    upsert_bot_authorized_keys(bot_host, &spec.instance.id, &auth_line).await?;
    *wrote_auth = true;

    let script = render_run_sh(&ResidentLinkScriptInput {
        app_port: spec.instance.port,
        forward_port: fwd,
        dial: &dial,
    });
    app_host
        .write_file(&link_dir.join(LINK_RUN_SH), script.as_bytes())
        .await
        .map_err(host_err)?;

    stop_resident_script(app_host, &link_dir).await?;
    start_resident_script(app_host, &link_dir).await?;
    Ok(fwd)
}

pub async fn teardown_resident_link(
    app_host: &dyn Host,
    bot_host: Option<&dyn Host>,
    instance_id: &AppInstanceId,
) -> Result<(), AppFrameworkError> {
    if let Ok(home) = remote_home(app_host).await {
        let dir = remote_link_dir(&home, instance_id);
        let _ = stop_resident_script(app_host, &dir).await;
        if app_host.exists(&dir).await.unwrap_or(false) {
            let _ = app_host.remove_dir_all(&dir).await;
        }
    }
    if let Some(bot) = bot_host {
        let _ = strip_bot_authorized_key(bot, instance_id).await;
    }
    Ok(())
}

pub async fn reconcile_resident_link(
    app_host: &dyn Host,
    bot_host: &dyn Host,
    instance: &AppInstance,
    forward_port: u16,
) -> Result<(), AppFrameworkError> {
    let home = remote_home(app_host).await?;
    let dir = remote_link_dir(&home, &instance.id);
    if resident_pid_alive(app_host, &dir).await {
        return Ok(());
    }
    ensure_resident_link(
        app_host,
        bot_host,
        ResidentLinkSpec {
            instance,
            reuse_port: Some(forward_port),
            taken_ports: &[],
        },
    )
    .await?;
    Ok(())
}

async fn remote_home(host: &dyn Host) -> Result<String, AppFrameworkError> {
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

async fn ssh_keyscan(app_host: &dyn Host, dial: &SshDialTarget) -> Result<String, AppFrameworkError> {
    let out = app_host
        .run_to_string(
            HostCommand::new("ssh-keyscan")
                .arg("-T")
                .arg("8")
                .arg("-p")
                .arg(dial.port.to_string())
                .arg(&dial.host)
                .timeout(KEYSCAN_TIMEOUT),
        )
        .await
        .map_err(|_| unreachable_ssh(dial))?;
    let keys: String = out
        .stdout
        .lines()
        .filter(|l| {
            let t = l.trim();
            !t.is_empty() && !t.starts_with('#')
        })
        .collect::<Vec<_>>()
        .join("\n");
    if keys.is_empty() || !out.success() {
        return Err(unreachable_ssh(dial));
    }
    let mut known = keys;
    if !known.ends_with('\n') {
        known.push('\n');
    }
    Ok(known)
}

fn unreachable_ssh(_dial: &SshDialTarget) -> AppFrameworkError {
    AppFrameworkError::Validation(
        "应用所在机无法连上 Bot 所在机的 SSH，请在 Bot 机防火墙或安全组开放「应用机 → Bot 机」的 SSH 端口"
            .into(),
    )
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
        let port = allocate_listen_port(None, &blocked, false)
            .map_err(AppFrameworkError::Validation)?;
        if !remote_loopback_busy(bot_host, port).await {
            return Ok(port);
        }
        blocked.push(port);
    }
    Err(AppFrameworkError::Validation(
        "无法在 Bot 所在机分配空闲的常驻隧道听口".into(),
    ))
}

async fn remote_loopback_busy(host: &dyn Host, port: u16) -> bool {
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
                .arg(key.as_posix())
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
    Ok(())
}

async fn upsert_bot_authorized_keys(
    bot_host: &dyn Host,
    instance_id: &AppInstanceId,
    line: &str,
) -> Result<(), AppFrameworkError> {
    let (path, existing) = read_authorized_keys(bot_host).await?;
    let next = upsert_authorized_key(&existing, line, instance_id);
    write_authorized_keys(bot_host, &path, &next).await
}

async fn strip_bot_authorized_key(
    bot_host: &dyn Host,
    instance_id: &AppInstanceId,
) -> Result<(), AppFrameworkError> {
    let (path, existing) = match read_authorized_keys(bot_host).await {
        Ok(v) => v,
        Err(_) => return Ok(()),
    };
    let next = remove_authorized_key(&existing, instance_id);
    if next == existing {
        return Ok(());
    }
    write_authorized_keys(bot_host, &path, &next).await
}

async fn read_authorized_keys(bot_host: &dyn Host) -> Result<(HostPath, String), AppFrameworkError> {
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

async fn stop_resident_script(
    app_host: &dyn Host,
    link_dir: &HostPath,
) -> Result<(), AppFrameworkError> {
    if !app_host.exists(link_dir).await.unwrap_or(false) {
        return Ok(());
    }
    let dir = shell_quote(link_dir.as_posix());
    let script = format!(
        "stop={dir}/{stop}; pidf={dir}/{pid};\n\
         touch \"$stop\" 2>/dev/null || true\n\
         if [ -f \"$pidf\" ]; then\n\
           pid=$(tr -dc '0-9' < \"$pidf\")\n\
           if [ -n \"$pid\" ]; then\n\
             kill \"$pid\" 2>/dev/null || true\n\
             for i in 1 2 3 4 5 6 7 8 9 10; do kill -0 \"$pid\" 2>/dev/null || break; sleep 0.3; done\n\
             kill -9 \"$pid\" 2>/dev/null || true\n\
           fi\n\
         fi\n\
         rm -f \"$pidf\" \"$stop\"\n",
        stop = LINK_STOP_FILE,
        pid = LINK_PID_FILE,
    );
    let _ = app_host
        .run_to_string(
            HostCommand::new("sh")
                .arg("-c")
                .arg(script)
                .timeout(Duration::from_secs(15)),
        )
        .await;
    Ok(())
}

async fn start_resident_script(
    app_host: &dyn Host,
    link_dir: &HostPath,
) -> Result<(), AppFrameworkError> {
    let dir = shell_quote(link_dir.as_posix());
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
        return Err(AppFrameworkError::Host(format!("拉起常驻隧道失败: {detail}")));
    }
    Ok(())
}

async fn resident_pid_alive(app_host: &dyn Host, link_dir: &HostPath) -> bool {
    let dir = shell_quote(link_dir.as_posix());
    let script = format!(
        "pidf={dir}/{pid};\n\
         [ -f \"$pidf\" ] || exit 1\n\
         pid=$(tr -dc '0-9' < \"$pidf\")\n\
         [ -n \"$pid\" ] && kill -0 \"$pid\" 2>/dev/null\n",
        pid = LINK_PID_FILE,
    );
    matches!(
        app_host
            .run_to_string(
                HostCommand::new("sh")
                    .arg("-c")
                    .arg(script)
                    .timeout(Duration::from_secs(8)),
            )
            .await,
        Ok(out) if out.success()
    )
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

    #[test]
    fn authorized_keys_round_trip_replaces_same_instance_only() {
        let pub1 = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIold comment";
        let pub2 = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAInew leftover";
        let other = "ssh-ed25519 AAAAother user@box\n";
        let first = authorized_keys_line(pub1, &id(), 21001).unwrap();
        let merged = upsert_authorized_key(other, &first, &id());
        assert!(merged.contains("permitlisten=\"127.0.0.1:21001\""));
        assert!(merged.contains("ncd-link:k1"));
        assert!(merged.contains("user@box"));
        let second = authorized_keys_line(pub2, &id(), 21002).unwrap();
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
        let la = authorized_keys_line("ssh-ed25519 AAAa c", &a, 20001).unwrap();
        let lb = authorized_keys_line("ssh-ed25519 AAAb c", &b, 20002).unwrap();
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
            app_port: 32100,
            forward_port: 21001,
            dial: &dial,
        });
        assert!(script.contains("-R 127.0.0.1:21001:127.0.0.1:32100"));
        assert!(script.contains("IdentitiesOnly=yes"));
        assert!(script.contains("ExitOnForwardFailure=yes"));
        assert!(script.contains("ServerAliveInterval=30"));
        assert!(script.contains("UserKnownHostsFile=\"$KNOWN\""));
        assert!(script.contains("GlobalKnownHostsFile=/dev/null"));
        assert!(script.contains("-p 2222"));
        assert!(script.contains("'alice@bot.example'"));
        assert!(!script.contains("UserKnownHostsFile=~/.ssh"));
    }

    #[test]
    fn pubkey_rejects_garbage() {
        assert!(pubkey_type_and_blob("").is_err());
        assert!(pubkey_type_and_blob("not-a-key").is_err());
        assert!(authorized_keys_line("ssh-ed25519 blob c", &id(), 0).is_err());
    }

    #[test]
    fn shell_quote_escapes_single_quotes() {
        assert_eq!(shell_quote("it's"), "'it'\"'\"'s'");
    }
}
