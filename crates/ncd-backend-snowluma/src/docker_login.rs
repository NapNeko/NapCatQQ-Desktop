// Docker 扫码只查询 QQ 的可访问性控件，启动包装保留供容器重启。
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ncd_deploy::docker::{DockerCli, DockerLoginOverlay};
use ncd_deploy::{DockerDeployment, bot_docker_container_candidates, docker_project_dir};
use ncd_domain::{
    BackendType, BotConfig, BotId, ContainerState, DockerFlavor, SnowLumaQrLoginSnapshot,
    SnowLumaQrLoginState,
};
use ncd_host::{Arch, Host, HostCommand, HostPath, Os, shell_single_quote};
use ncd_traits::runtime_backend::BotBackendError;
use serde::{Deserialize, Serialize};

const CONTAINER_ROOT: &str = "/ncd-login-assist";
const HELPER: &str = include_str!("../resources/login-helper/atspi-login.py");
const LEASE_SECONDS: u64 = 10 * 60;
const IMAGE_PROBE: &str = r#"
const fs=require('fs');
function read(p,max){if(fs.statSync(p).size>max)throw Error('size');return fs.readFileSync(p,'utf8');}
const supervisor=read('/etc/supervisord.conf',131072);
const startup=read('/root/start.sh',131072);
const account=read('/etc/passwd',131072).split('\n').find(line=>line.startsWith('snowluma:'))?.split(':');
const uid_remap=/usermod[^\n]*SNOWLUMA_UID[^\n]*snowluma/.test(startup)&&/groupmod[^\n]*SNOWLUMA_GID[^\n]*snowluma/.test(startup);
for(const file of ['/usr/bin/python3','/usr/bin/dbus-run-session','/usr/bin/dbus-send'])if(!fs.existsSync(file))throw Error('dependencies');
process.stdout.write(JSON.stringify({arch:process.arch,supervisor,qq_exe:fs.realpathSync('/usr/bin/qq'),uid_remap,uid:Number(account?.[2]),gid:Number(account?.[3])}));
"#;
const PROCESS_PROBE: &str = r#"
const fs=require('fs');
const [file,session,exe,uidText]=process.argv.slice(1);
if(fs.statSync(file).size>4096)throw Error('size');
const anchor=JSON.parse(fs.readFileSync(file,'utf8'));
if(anchor.session_id!==session||!Number.isInteger(anchor.pid)||anchor.pid<=0||!/^\d+$/.test(anchor.start_time))throw Error('scope');
const root='/proc/'+anchor.pid;
if(fs.realpathSync(root+'/exe')!==exe)throw Error('executable');
const uid=Number(fs.readFileSync(root+'/status','utf8').match(/^Uid:\s+(\d+)/m)?.[1]);
if(uid!==Number(uidText)||process.getuid()!==uid)throw Error('uid');
const stat=fs.readFileSync(root+'/stat','utf8');
const start=stat.slice(stat.lastIndexOf(')')+2).trim().split(/\s+/)[19];
if(start!==anchor.start_time)throw Error('pid reuse');
if(fs.readFileSync(root+'/cmdline','utf8').split('\0').some(arg=>arg.startsWith('--type=')))throw Error('renderer');
process.stdout.write(JSON.stringify({pid:anchor.pid,start_time:start}));
"#;

#[derive(Deserialize)]
struct ImageLayout {
    arch: String,
    supervisor: String,
    qq_exe: String,
    uid_remap: bool,
    uid: u32,
    gid: u32,
}

#[derive(Clone, Serialize, Deserialize)]
struct CaptureBinding {
    pid: u32,
    start_time: String,
    exe: String,
    uid: u32,
    display: String,
    bus: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    xauthority: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    runtime_dir: Option<String>,
}

#[derive(Deserialize)]
struct ProcessAnchor {
    pid: u32,
    start_time: String,
}
#[derive(Deserialize)]
struct ProbeReply {
    available: bool,
}
#[derive(Deserialize)]
struct BindReply {
    binding: CaptureBinding,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct DockerLoginSession {
    method: String,
    directory: String,
    session_id: String,
    launch_id: String,
    expected_uin: String,
    container: String,
    container_id: Option<String>,
    qq_exe: String,
    uid: u32,
    gid: u32,
    #[serde(default)]
    binding: Option<CaptureBinding>,
    #[serde(default)]
    lease_started_at: Option<u64>,
    #[serde(default)]
    scanned: bool,
    #[serde(default)]
    login_detected: bool,
}

#[derive(Serialize)]
struct CaptureRequest<'a> {
    v: u32,
    session_id: &'a str,
    action: &'a str,
    binding: &'a CaptureBinding,
    scanned: bool,
    cancel_path: String,
}

#[derive(Deserialize)]
struct WireReply {
    session_id: String,
    #[serde(flatten)]
    snapshot: SnowLumaQrLoginSnapshot,
}

fn unavailable() -> BotBackendError {
    BotBackendError::Io("Docker 扫码通道不可用，请打开 VNC".into())
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

async fn session_root(host: &dyn Host, bot_id: &BotId) -> Result<String, BotBackendError> {
    if host.os() != Os::Linux
        || bot_id.as_str().is_empty()
        || !bot_id.as_str().bytes().all(|b| b.is_ascii_digit())
    {
        return Err(unavailable());
    }
    let project = docker_project_dir(host, &format!("slbot-{}", bot_id.as_str()))
        .await
        .map_err(|_| unavailable())?;
    if !project.starts_with('/')
        || project.contains(':')
        || project.contains('\n')
        || project.split('/').any(|part| part == "..")
    {
        return Err(unavailable());
    }
    Ok(format!("{project}/ncd-login"))
}

fn active_container(state: &ContainerState) -> bool {
    matches!(
        state,
        ContainerState::Running | ContainerState::Paused | ContainerState::Restarting
    )
}

fn patch_supervisor(source: &str) -> Result<String, BotBackendError> {
    let mut in_qq = false;
    let mut replacements = 0;
    let mut qq_user = None;
    let mut result = String::new();
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_qq = trimmed == "[program:qq]";
        }
        if in_qq && trimmed.starts_with("user=") {
            if qq_user.is_some() {
                return Err(unavailable());
            }
            qq_user = Some(trimmed.trim_start_matches("user=").trim().to_string());
        }
        if in_qq && trimmed.starts_with("command=") {
            let command = trimmed.trim_start_matches("command=").trim();
            if !command.starts_with("qq ") && !command.starts_with("/usr/bin/qq ") {
                return Err(unavailable());
            }
            result.push_str("command=/bin/sh /ncd-login-launch.sh ");
            result.push_str(command);
            replacements += 1;
        } else {
            result.push_str(line);
        }
        result.push('\n');
    }
    if replacements != 1 || qq_user.as_deref() != Some("snowluma") {
        return Err(unavailable());
    }
    Ok(result)
}

fn launcher(session_id: &str) -> String {
    let child = format!(
        "umask 077\nstart_time=$(awk '{{print $22}}' /proc/$$/stat)\nprintf '{{\"session_id\":\"{session_id}\",\"pid\":%s,\"start_time\":\"%s\"}}\\n' \"$$\" \"$start_time\" > {CONTAINER_ROOT}/qq-process.json\ndbus-send --session --print-reply --reply-timeout=1500 --dest=org.a11y.Bus /org/a11y/bus org.freedesktop.DBus.Properties.Set string:org.a11y.Status string:IsEnabled variant:boolean:true >/dev/null 2>&1 || :\nexec \"$@\" --force-renderer-accessibility\n"
    );
    format!(
        "#!/bin/sh\nset -eu\nexport QT_ACCESSIBILITY=1\nexport NO_AT_BRIDGE=0\nrm -f -- {CONTAINER_ROOT}/qq-process.json\nif /usr/bin/dbus-run-session -- /bin/sh -c {} ncd-qq \"$@\"; then\n  exit 0\nelse\n  status=$?\n  if [ -s {CONTAINER_ROOT}/qq-process.json ]; then exit \"$status\"; fi\n  exec \"$@\"\nfi\n",
        shell_single_quote(&child)
    )
}

impl DockerLoginSession {
    pub async fn prepare(
        host: &dyn Host,
        config: &BotConfig,
    ) -> Result<Option<DockerLoginOverlay>, BotBackendError> {
        if host.os() != Os::Linux
            || host.arch() != Arch::X86_64
            || config.bot.backend_type != BackendType::SnowLuma
        {
            return Ok(None);
        }
        let bot_id = BotId::new(config.bot.qq_id.to_string());
        if config.bot.qq_id == 0 {
            return Err(unavailable());
        }
        let cli = DockerCli::new(host);
        cli.ensure_daemon_ready().await.map_err(|_| unavailable())?;
        let candidates = bot_docker_container_candidates(BackendType::SnowLuma, config.bot.qq_id);
        if cli
            .list_containers()
            .await
            .map_err(|_| unavailable())?
            .iter()
            .any(|c| candidates.contains(&c.name) && active_container(&c.state))
        {
            return Ok(None);
        }
        let image = DockerFlavor::SnowLuma.default_image();
        if !cli.image_exists(image).await.map_err(|_| unavailable())? {
            return Ok(None);
        }
        let run_probe = |entry: &str, source: &str, arguments: &[&str]| {
            cli.command()
                .args([
                    "run",
                    "--rm",
                    "--pull",
                    "never",
                    "--network",
                    "none",
                    "--read-only",
                    "--cap-add",
                    "SYS_PTRACE",
                    "--entrypoint",
                    entry,
                    image,
                ])
                .arg(if entry.ends_with("node") { "-e" } else { "-c" })
                .arg(source)
                .args(arguments.iter().copied())
                .timeout(Duration::from_secs(20))
        };
        let out = host
            .run_to_string(run_probe("/usr/local/bin/node", IMAGE_PROBE, &[]))
            .await
            .map_err(|_| unavailable())?;
        if !out.success() || out.stdout.len() > 192 * 1024 {
            return Err(unavailable());
        }
        let layout: ImageLayout = serde_json::from_str(&out.stdout).map_err(|_| unavailable())?;
        if layout.arch != "x64"
            || !layout.uid_remap
            || layout.uid == 0
            || layout.gid == 0
            || !layout.qq_exe.starts_with("/opt/QQ/")
        {
            return Ok(None);
        }
        let dependency = host
            .run_to_string(run_probe("/usr/bin/python3", HELPER, &["--probe"]))
            .await
            .map_err(|_| unavailable())?;
        if !dependency.success()
            || dependency.stdout.len() > 4096
            || !serde_json::from_str::<ProbeReply>(&dependency.stdout)
                .map_err(|_| unavailable())?
                .available
        {
            return Ok(None);
        }
        let supervisor = patch_supervisor(&layout.supervisor)?;
        let root = session_root(host, &bot_id).await?;
        let assets = format!("{root}/assets");
        host.create_dir_all(&HostPath::from_posix(&root))
            .await
            .map_err(|_| unavailable())?;
        let cleanup = host
            .run_to_string(
                HostCommand::new("rm")
                    .args(["-rf", "--", &assets])
                    .elevated(),
            )
            .await
            .map_err(|_| unavailable())?;
        if !cleanup.success() {
            return Err(unavailable());
        }
        host.create_dir_all(&HostPath::from_posix(&assets))
            .await
            .map_err(|_| unavailable())?;
        let (uid, gid) = DockerDeployment::container_uid_gid(host);
        let session = Self {
            method: "atspi".into(),
            directory: root.clone(),
            session_id: format!("{:032x}", rand::random::<u128>()),
            launch_id: format!("{:032x}", rand::random::<u128>()),
            expected_uin: bot_id.as_str().into(),
            container: DockerDeployment::container_name(config),
            container_id: None,
            qq_exe: layout.qq_exe,
            uid,
            gid,
            binding: None,
            lease_started_at: None,
            scanned: false,
            login_detected: false,
        };
        host.write_file(
            &HostPath::from_posix(format!("{root}/launch.sh")),
            launcher(&session.launch_id).as_bytes(),
        )
        .await
        .map_err(|_| unavailable())?;
        host.write_file(
            &HostPath::from_posix(format!("{root}/supervisord.conf")),
            supervisor.as_bytes(),
        )
        .await
        .map_err(|_| unavailable())?;
        session.save(host).await?;
        for command in [
            HostCommand::new("chmod").args(["700", "--", &root, &assets]),
            HostCommand::new("chmod").args(["600", "--", &format!("{root}/session.json")]),
            HostCommand::new("chmod").args([
                "644",
                "--",
                &format!("{root}/launch.sh"),
                &format!("{root}/supervisord.conf"),
            ]),
            HostCommand::new("chown")
                .args(["-R", &format!("{uid}:{gid}"), "--", &assets])
                .elevated(),
        ] {
            if !host
                .run_to_string(command)
                .await
                .map_err(|_| unavailable())?
                .success()
            {
                return Err(unavailable());
            }
        }
        Ok(Some(DockerLoginOverlay {
            assets_volume: format!("{assets}:{CONTAINER_ROOT}"),
            launcher_volume: format!("{root}/launch.sh:/ncd-login-launch.sh:ro"),
            supervisor_volume: format!("{root}/supervisord.conf:/etc/supervisord.conf:ro"),
        }))
    }

    async fn save(&self, host: &dyn Host) -> Result<(), BotBackendError> {
        host.write_file(
            &HostPath::from_posix(format!("{}/session.json", self.directory)),
            &serde_json::to_vec(self).map_err(|_| unavailable())?,
        )
        .await
        .map_err(|_| unavailable())
    }

    async fn recover(host: &dyn Host, bot_id: &BotId) -> Result<Option<Self>, BotBackendError> {
        let root = session_root(host, bot_id).await?;
        let file = HostPath::from_posix(format!("{root}/session.json"));
        if !host.exists(&file).await.map_err(|_| unavailable())? {
            return Ok(None);
        }
        if host
            .file_size(&file)
            .await
            .map_err(|_| unavailable())?
            .is_some_and(|n| n > 8192)
        {
            return Err(unavailable());
        }
        let bytes = host.read_file(&file).await.map_err(|_| unavailable())?;
        if bytes.len() > 8192 {
            return Err(unavailable());
        }
        let session: Self = match serde_json::from_slice(&bytes) {
            Ok(session) => session,
            // 旧入口观察器会话不接管，运行中的旧容器仍保留 VNC。
            Err(_) => return Ok(None),
        };
        let (uid, gid) = DockerDeployment::container_uid_gid(host);
        if session.method != "atspi"
            || session.directory != root
            || session.expected_uin != bot_id.as_str()
            || session.container != format!("slbot-{}", bot_id.as_str())
            || session.session_id.len() != 32
            || !session.session_id.bytes().all(|b| b.is_ascii_hexdigit())
            || !session.qq_exe.starts_with("/opt/QQ/")
            || session.uid != uid
            || session.gid != gid
        {
            return Err(unavailable());
        }
        if session.launch_id.len() != 32
            || !session.launch_id.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(unavailable());
        }
        Ok(Some(session))
    }

    async fn container_id(&mut self, cli: &DockerCli<'_>) -> Result<String, BotBackendError> {
        let container = cli
            .list_containers()
            .await
            .map_err(|_| unavailable())?
            .into_iter()
            .find(|c| c.name == self.container && c.state == ContainerState::Running)
            .ok_or_else(unavailable)?;
        if self
            .container_id
            .as_ref()
            .is_some_and(|id| id != &container.id)
        {
            return Err(unavailable());
        }
        self.container_id = Some(container.id.clone());
        Ok(container.id)
    }

    async fn anchor(
        &mut self,
        host: &dyn Host,
        cli: &DockerCli<'_>,
    ) -> Result<(String, ProcessAnchor), BotBackendError> {
        let id = self.container_id(cli).await?;
        let out = host
            .run_to_string(
                cli.command()
                    .args([
                        "exec",
                        "--user",
                        &format!("{}:{}", self.uid, self.gid),
                        &id,
                        "/usr/local/bin/node",
                        "-e",
                        PROCESS_PROBE,
                        &format!("{CONTAINER_ROOT}/qq-process.json"),
                        &self.launch_id,
                        &self.qq_exe,
                        &self.uid.to_string(),
                    ])
                    .timeout(Duration::from_secs(5)),
            )
            .await
            .map_err(|_| unavailable())?;
        if !out.success() || out.stdout.len() > 4096 {
            return Err(unavailable());
        }
        let anchor: ProcessAnchor = serde_json::from_str(&out.stdout).map_err(|_| unavailable())?;
        if anchor.pid == 0
            || anchor.start_time.is_empty()
            || !anchor.start_time.bytes().all(|b| b.is_ascii_digit())
            || self
                .binding
                .as_ref()
                .is_some_and(|b| b.pid != anchor.pid || b.start_time != anchor.start_time)
        {
            return Err(unavailable());
        }
        Ok((id, anchor))
    }

    // 只核验启动锚点，不访问可访问性树或采样图像。
    pub async fn bound_pid(
        host: &dyn Host,
        bot_id: &BotId,
    ) -> Result<Option<u32>, BotBackendError> {
        let Some(mut session) = Self::recover(host, bot_id).await? else {
            return Ok(None);
        };
        let cli = DockerCli::new(host);
        cli.ensure_daemon_ready().await.map_err(|_| unavailable())?;
        let (_, anchor) = session.anchor(host, &cli).await?;
        Ok(Some(anchor.pid))
    }

    async fn bind(
        &mut self,
        host: &dyn Host,
        cli: &DockerCli<'_>,
        id: &str,
        anchor: &ProcessAnchor,
    ) -> Result<(), BotBackendError> {
        let out = host
            .run_to_string(
                cli.command()
                    .args([
                        "exec",
                        "--user",
                        &format!("{}:{}", self.uid, self.gid),
                        id,
                        "/usr/bin/python3",
                        "-c",
                        HELPER,
                        "--bind",
                        "--pid",
                        &anchor.pid.to_string(),
                        "--exe",
                        &self.qq_exe,
                    ])
                    .timeout(Duration::from_secs(6)),
            )
            .await
            .map_err(|_| unavailable())?;
        if !out.success() || out.stdout.len() > 8192 {
            return Err(unavailable());
        }
        let binding = serde_json::from_str::<BindReply>(&out.stdout)
            .map_err(|_| unavailable())?
            .binding;
        if binding.pid != anchor.pid
            || binding.start_time != anchor.start_time
            || binding.exe != self.qq_exe
            || binding.uid != self.uid
            || !binding.display.starts_with(':')
            || !binding.bus.starts_with("unix:")
        {
            return Err(unavailable());
        }
        self.binding = Some(binding);
        Ok(())
    }

    fn decode(&mut self, bytes: &[u8]) -> Result<SnowLumaQrLoginSnapshot, BotBackendError> {
        let wire: WireReply = serde_json::from_slice(bytes).map_err(|_| unavailable())?;
        if wire.session_id != self.session_id
            || wire.snapshot.v != 1
            || wire.snapshot.qq_pid != self.binding.as_ref().map(|b| b.pid)
            || !wire.snapshot.observer_removed
            || wire
                .snapshot
                .qrcode_payload
                .as_ref()
                .is_some_and(|p| p.len() > 128 * 1024 || !p.starts_with("data:image/png;base64,"))
        {
            return Err(unavailable());
        }
        let mut snapshot = wire.snapshot;
        if snapshot
            .uin
            .as_ref()
            .is_some_and(|uin| uin != &self.expected_uin)
        {
            snapshot.state = SnowLumaQrLoginState::AccountMismatch;
            snapshot.message = Some("扫码账号与此 Bot 不一致".into());
        }
        if snapshot.state == SnowLumaQrLoginState::Scanned {
            self.scanned = true;
        }
        if matches!(
            snapshot.state,
            SnowLumaQrLoginState::LoginDetected | SnowLumaQrLoginState::LoggedIn
        ) {
            self.login_detected = true;
            snapshot.state = SnowLumaQrLoginState::LoginDetected;
        }
        if self.scanned
            || self.login_detected
            || snapshot.state.is_terminal()
            || snapshot.state == SnowLumaQrLoginState::Expired
        {
            snapshot.qrcode_payload = None;
        }
        Ok(snapshot)
    }

    pub async fn snapshot(
        host: &dyn Host,
        bot_id: &BotId,
        refresh: bool,
    ) -> Result<SnowLumaQrLoginSnapshot, BotBackendError> {
        let Some(mut session) = Self::recover(host, bot_id).await? else {
            return Ok(SnowLumaQrLoginSnapshot::unavailable(
                "此容器未准备可访问性扫码，请使用 VNC",
            ));
        };
        // 一轮采集复用已判定的 Docker 权限，避免慢 SSH 上重复探测占满请求期限。
        let cli = DockerCli::new(host);
        cli.ensure_daemon_ready().await.map_err(|_| unavailable())?;
        let (id, anchor) = session.anchor(host, &cli).await?;
        if session
            .lease_started_at
            .is_some_and(|started| now().saturating_sub(started) >= LEASE_SECONDS)
        {
            Self::close(host, bot_id).await?;
            let mut snapshot =
                SnowLumaQrLoginSnapshot::unavailable("扫码等待已结束，请在 VNC 中继续");
            snapshot.state = SnowLumaQrLoginState::Cancelled;
            snapshot.qq_pid = Some(anchor.pid);
            return Ok(snapshot);
        }
        if session.scanned || session.login_detected {
            let mut snapshot = SnowLumaQrLoginSnapshot::unavailable(if session.login_detected {
                "正在确认 SL 登录状态"
            } else {
                "已扫码，请在手机上确认"
            });
            snapshot.state = if session.login_detected {
                SnowLumaQrLoginState::LoginDetected
            } else {
                SnowLumaQrLoginState::Scanned
            };
            snapshot.qq_pid = Some(anchor.pid);
            return Ok(snapshot);
        }
        session.bind(host, &cli, &id, &anchor).await?;
        if session.lease_started_at.is_none() {
            session.lease_started_at = Some(now());
            session.session_id = format!("{:032x}", rand::random::<u128>());
            let out = host
                .run_to_string(
                    HostCommand::new("rm")
                        .args([
                            "-f",
                            "--",
                            &format!("{}/assets/cancelled", session.directory),
                        ])
                        .elevated(),
                )
                .await
                .map_err(|_| unavailable())?;
            if !out.success() {
                return Err(unavailable());
            }
        }
        session.save(host).await?;
        let request = CaptureRequest {
            v: 1,
            session_id: &session.session_id,
            action: if refresh { "refresh" } else { "snapshot" },
            binding: session.binding.as_ref().ok_or_else(unavailable)?,
            scanned: session.scanned,
            cancel_path: format!("{CONTAINER_ROOT}/cancelled"),
        };
        let out = host
            .run_to_string(
                cli.command()
                    .args([
                        "exec",
                        "--interactive",
                        "--user",
                        &format!("{}:{}", session.uid, session.gid),
                        &id,
                        "/usr/bin/python3",
                        "-c",
                        HELPER,
                    ])
                    .stdin(serde_json::to_vec(&request).map_err(|_| unavailable())?)
                    .timeout(Duration::from_secs(8)),
            )
            .await
            .map_err(|_| unavailable())?;
        if !out.success() || out.stdout.len() > 192 * 1024 {
            return Err(unavailable());
        }
        // 采样后再次核验 PID 与启动时间，拒绝进程替换后的迟到图片。
        session.anchor(host, &cli).await?;
        let snapshot = session.decode(out.stdout.as_bytes())?;
        session.save(host).await?;
        Ok(snapshot)
    }

    pub async fn close(host: &dyn Host, bot_id: &BotId) -> Result<(), BotBackendError> {
        let Some(mut session) = Self::recover(host, bot_id).await? else {
            return Ok(());
        };
        let out = host
            .run_to_string(
                HostCommand::new("touch")
                    .args(["--", &format!("{}/assets/cancelled", session.directory)])
                    .elevated(),
            )
            .await
            .map_err(|_| unavailable())?;
        if !out.success() {
            return Err(unavailable());
        }
        Self::clean_query_assets(host, &session.directory).await?;
        session.lease_started_at = None;
        session.scanned = false;
        session.login_detected = false;
        session.binding = None;
        session.session_id = format!("{:032x}", rand::random::<u128>());
        session.save(host).await
    }

    pub async fn abort_prepare(host: &dyn Host, bot_id: &BotId) -> Result<(), BotBackendError> {
        let root = session_root(host, bot_id).await?;
        if !host
            .exists(&HostPath::from_posix(&root))
            .await
            .map_err(|_| unavailable())?
        {
            return Ok(());
        }
        let cli = DockerCli::new(host);
        cli.ensure_daemon_ready().await.map_err(|_| unavailable())?;
        let candidates = bot_docker_container_candidates(BackendType::SnowLuma, bot_id.as_str());
        if cli
            .list_containers()
            .await
            .map_err(|_| unavailable())?
            .iter()
            .any(|c| candidates.contains(&c.name) && active_container(&c.state))
        {
            return Err(unavailable());
        }
        Self::clean_query_assets(host, &root).await?;
        let index = HostPath::from_posix(format!("{root}/session.json"));
        if host.exists(&index).await.map_err(|_| unavailable())? {
            host.remove_file(&index).await.map_err(|_| unavailable())?;
        }
        Ok(())
    }

    async fn clean_query_assets(host: &dyn Host, directory: &str) -> Result<(), BotBackendError> {
        // 只删固定旧查询资产，保留挂载目录与 QQ 启动锚点。
        let mut command = HostCommand::new("rm").args(["-f", "--"]).elevated();
        for name in [
            "libentry-overlay.so",
            "observer-core.cjs",
            "observer.cjs",
            "client.cjs",
            "package-overlay.json",
            "control.sock",
            "receipt.json",
            "receipt.json.tmp",
        ] {
            command = command.arg(format!("{directory}/assets/{name}"));
        }
        if !host
            .run_to_string(command)
            .await
            .map_err(|_| unavailable())?
            .success()
        {
            return Err(unavailable());
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "docker_login_tests.rs"]
mod request_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supervisor_overlay_only_changes_qq_and_requires_its_user() {
        let source = "[program:qq]\ncommand=qq --no-sandbox %(ENV_SNOWLUMA_QQ_FLAGS)s\nuser=snowluma\n[program:snowluma]\ncommand=/usr/local/bin/node /app/runtime/index.mjs\n";
        let patched = patch_supervisor(source).unwrap();
        assert!(patched.contains("command=/bin/sh /ncd-login-launch.sh qq --no-sandbox"));
        assert!(patched.contains("command=/usr/local/bin/node /app/runtime/index.mjs"));
        assert_eq!(patched.matches("/ncd-login-launch.sh").count(), 1);
        assert!(
            patch_supervisor("[program:qq]\ncommand=custom-wrapper qq\nuser=snowluma\n").is_err()
        );
        assert!(patch_supervisor("[program:qq]\ncommand=qq a\nuser=root\n").is_err());
    }

    #[test]
    fn launcher_is_restartable_without_query_helper_files() {
        let script = launcher("0123456789abcdef0123456789abcdef");
        assert!(script.contains("/usr/bin/dbus-run-session"));
        assert!(script.contains("--force-renderer-accessibility"));
        assert!(script.contains("qq-process.json"));
        assert!(!script.contains("LD_PRELOAD"));
        assert!(!script.contains("package-overlay"));
        assert!(!script.contains("observer.cjs"));
        assert!(!script.contains("python"));
    }

    #[test]
    fn replies_bind_session_pid_and_clear_terminal_or_scanned_qr() {
        let mut session = DockerLoginSession {
            method: "atspi".into(),
            directory: "/home/u/private".into(),
            session_id: "abc".into(),
            launch_id: "launch".into(),
            expected_uin: "123".into(),
            container: "slbot-123".into(),
            container_id: None,
            qq_exe: "/opt/QQ/qq".into(),
            uid: 1000,
            gid: 1000,
            binding: Some(CaptureBinding {
                pid: 24,
                start_time: "100".into(),
                exe: "/opt/QQ/qq".into(),
                uid: 1000,
                display: ":1".into(),
                bus: "unix:path=/tmp/bus".into(),
                xauthority: None,
                runtime_dir: None,
            }),
            lease_started_at: None,
            scanned: false,
            login_detected: false,
        };
        let login = br#"{"session_id":"abc","v":1,"state":"logged_in","qq_pid":24,"qrcode_payload":"data:image/png;base64,secret","uin":"123","observer_removed":true,"message":null}"#;
        assert!(session.decode(login).unwrap().qrcode_payload.is_none());
        let wrong_pid = br#"{"session_id":"abc","v":1,"state":"waiting_for_qr","qq_pid":25,"qrcode_payload":null,"uin":null,"observer_removed":true,"message":null}"#;
        assert!(session.decode(wrong_pid).is_err());
        let active = br#"{"session_id":"abc","v":1,"state":"waiting_for_qr","qq_pid":24,"qrcode_payload":null,"uin":null,"observer_removed":false,"message":null}"#;
        assert!(session.decode(active).is_err());
        let expired = br#"{"session_id":"abc","v":1,"state":"expired","qq_pid":24,"qrcode_payload":"data:image/png;base64,secret","uin":null,"observer_removed":true,"message":null}"#;
        assert!(session.decode(expired).unwrap().qrcode_payload.is_none());
        let scanned = br#"{"session_id":"abc","v":1,"state":"scanned","qq_pid":24,"qrcode_payload":"data:image/png;base64,secret","uin":null,"observer_removed":true,"message":null}"#;
        assert!(session.decode(scanned).unwrap().qrcode_payload.is_none());
        assert!(session.scanned);
    }
}
