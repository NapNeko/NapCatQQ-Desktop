// 短期 AT-SPI 请求绑定 QQ 进程；二维码只通过 SSH 私有管道返回。
use super::layout::{RemoteSnowLumaLayout, SnowLumaRemotePaths};
use ncd_domain::{BotId, SnowLumaQrLoginSnapshot, SnowLumaQrLoginState};
use ncd_host::{Host, HostCommand, HostPath, Os};
use ncd_traits::runtime_backend::BotBackendError;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, time::Duration};
use tokio_util::sync::CancellationToken;

const HELPER: &[u8] = include_bytes!("../../resources/login-helper/atspi-login.py");
pub(crate) const DESKTOP_SESSION_UNAVAILABLE: &str = "QQ 桌面会话已失效，请重新启动此 Bot";
#[derive(Clone, Serialize, Deserialize, PartialEq)]
struct ProcessBinding {
    pid: u32,
    start_time: String,
    exe: String,
    uid: u32,
    display: String,
    bus: String,
    xauthority: Option<String>,
    runtime_dir: Option<String>,
}
fn valid_binding(binding: &ProcessBinding, pid: u32) -> bool {
    let display = binding.display.strip_prefix(':').unwrap_or_default();
    let mut parts = display.split('.');
    let screen = parts.next().unwrap_or_default();
    let suffix = parts.next();
    binding.pid == pid
        && pid != 0
        && !binding.start_time.is_empty()
        && binding.start_time.bytes().all(|b| b.is_ascii_digit())
        && binding.exe.starts_with('/')
        && binding.display.len() <= 40
        && !screen.is_empty()
        && screen.bytes().all(|b| b.is_ascii_digit())
        && suffix.is_none_or(|value| !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit()))
        && parts.next().is_none()
        && binding.bus.starts_with("unix:")
        && binding.bus.len() <= 1024
}

fn parse_start_time(bytes: &[u8]) -> Option<String> {
    let stat = std::str::from_utf8(bytes).ok()?;
    let (_, fields) = stat.rsplit_once(')')?;
    let value = fields.split_whitespace().nth(19)?;
    (!value.is_empty() && value.bytes().all(|b| b.is_ascii_digit())).then(|| value.to_string())
}

pub(crate) async fn process_start_time(
    host: &dyn Host,
    pid: u32,
) -> Result<String, BotBackendError> {
    if pid == 0 {
        return Err(io_error());
    }
    let bytes = bounded_read(host, &format!("/proc/{pid}/stat"), 4096).await?;
    parse_start_time(&bytes).ok_or_else(io_error)
}
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct LoginSession {
    directory: String,
    session_id: String,
    python_bin: String,
    qq_exe: String,
    qq_pid: u32,
    expected_uin: String,
    binding: Option<ProcessBinding>,
    #[serde(skip)]
    cancel: CancellationToken,
}
pub(crate) struct PreparedLogin {
    pub session: LoginSession,
    pub environment: BTreeMap<String, String>,
}
#[derive(Deserialize)]
struct WireReply {
    session_id: String,
    #[serde(flatten)]
    snapshot: SnowLumaQrLoginSnapshot,
}
fn io_error() -> BotBackendError {
    BotBackendError::Io("远端扫码控件不可用，请打开 VNC".into())
}
fn directory(paths: &SnowLumaRemotePaths, bot_id: &BotId) -> Result<String, BotBackendError> {
    if bot_id.as_str().is_empty() || !bot_id.as_str().bytes().all(|b| b.is_ascii_digit()) {
        return Err(io_error());
    }
    Ok(format!(
        "{}/login-assist/{}",
        paths.runtime_dir,
        bot_id.as_str()
    ))
}
async fn bounded_read(host: &dyn Host, path: &str, max: u64) -> Result<Vec<u8>, BotBackendError> {
    let path = HostPath::from_posix(path);
    if host
        .file_size(&path)
        .await
        .map_err(|_| io_error())?
        .is_some_and(|size| size > max)
    {
        return Err(io_error());
    }
    let bytes = host.read_file(&path).await.map_err(|_| io_error())?;
    if bytes.len() as u64 > max {
        return Err(io_error());
    }
    Ok(bytes.to_vec())
}

pub(crate) async fn prepare(
    host: &dyn Host,
    paths: &SnowLumaRemotePaths,
    layout: &RemoteSnowLumaLayout,
    bot_id: &BotId,
    expected_uin: u64,
) -> Result<Option<PreparedLogin>, BotBackendError> {
    prepare_with_cancel(
        host,
        paths,
        layout,
        bot_id,
        expected_uin,
        CancellationToken::new(),
    )
    .await
}

async fn prepare_with_cancel(
    host: &dyn Host,
    paths: &SnowLumaRemotePaths,
    layout: &RemoteSnowLumaLayout,
    bot_id: &BotId,
    expected_uin: u64,
    cancel: CancellationToken,
) -> Result<Option<PreparedLogin>, BotBackendError> {
    if cancel.is_cancelled() {
        return Ok(None);
    }
    if host.os() != Os::Linux {
        return Ok(None);
    }
    let python_bin = host
        .which("python3")
        .await
        .map_err(|_| io_error())?
        .ok_or_else(io_error)?;
    let mut session = LoginSession {
        directory: directory(paths, bot_id)?,
        session_id: format!("{:032x}", rand::random::<u128>()),
        python_bin,
        qq_exe: layout.qq_bin.clone(),
        qq_pid: 0,
        expected_uin: expected_uin.to_string(),
        binding: None,
        cancel,
    };
    session.deploy(host).await?;
    #[derive(Deserialize)]
    struct Probe {
        available: bool,
    }
    let probe = host
        .run_to_string(
            session
                .command()
                .arg("--probe")
                .cancel_token(session.cancel.clone()),
        )
        .await
        .map_err(|_| io_error())?;
    if !probe.success()
        || !serde_json::from_str::<Probe>(&probe.stdout).is_ok_and(|value| value.available)
    {
        session.remove_assets(host).await;
        return Ok(None);
    }
    Ok(Some(PreparedLogin {
        session,
        environment: BTreeMap::from([
            ("QT_ACCESSIBILITY".into(), "1".into()),
            ("NO_AT_BRIDGE".into(), "0".into()),
        ]),
    }))
}

impl LoginSession {
    pub fn pid(&self) -> u32 {
        self.qq_pid
    }
    pub fn start_time(&self) -> Option<&str> {
        self.binding
            .as_ref()
            .map(|binding| binding.start_time.as_str())
    }
    pub async fn attach_running(
        host: &dyn Host,
        paths: &SnowLumaRemotePaths,
        layout: &RemoteSnowLumaLayout,
        bot_id: &BotId,
        pid: u32,
        start_time: &str,
        cancel: CancellationToken,
    ) -> Result<Option<Self>, BotBackendError> {
        if cancel.is_cancelled() || pid == 0 {
            return Ok(None);
        }
        if let Ok(Some(mut saved)) =
            Self::recover_with_cancel(host, paths, bot_id, pid, cancel.clone()).await
        {
            if cancel.is_cancelled() {
                return Ok(None);
            }
            if saved.start_time() != Some(start_time) {
                return Ok(None);
            }
            saved.renew_with_cancel(host, cancel.clone()).await?;
            return Ok((!cancel.is_cancelled()).then_some(saved));
        }
        if cancel.is_cancelled() {
            return Ok(None);
        }
        let expected_uin = bot_id.as_str().parse::<u64>().map_err(|_| io_error())?;
        let Some(prepared) =
            prepare_with_cancel(host, paths, layout, bot_id, expected_uin, cancel.clone()).await?
        else {
            return Ok(None);
        };
        if cancel.is_cancelled() {
            return Ok(None);
        }
        let mut session = prepared.session;
        session.cancel = cancel.clone();
        session.bind_matching(host, pid, Some(start_time)).await?;
        Ok((!cancel.is_cancelled() && session.start_time() == Some(start_time)).then_some(session))
    }
    fn command(&self) -> HostCommand {
        HostCommand::new(&self.python_bin)
            .arg("-B")
            .arg(format!("{}/atspi-login.py", self.directory))
            .timeout(Duration::from_secs(8))
    }
    async fn deploy(&mut self, host: &dyn Host) -> Result<(), BotBackendError> {
        if self.cancel.is_cancelled() {
            return Err(io_error());
        }
        host.create_dir_all(&HostPath::from_posix(&self.directory))
            .await
            .map_err(|_| io_error())?;
        let out = host
            .run_to_string(
                HostCommand::new("chmod")
                    .args(["700", "--", &self.directory])
                    .timeout(Duration::from_secs(3))
                    .cancel_token(self.cancel.clone()),
            )
            .await
            .map_err(|_| io_error())?;
        if !out.success() || self.cancel.is_cancelled() {
            return Err(io_error());
        }
        let path = format!("{}/atspi-login.py", self.directory);
        host.write_file(&HostPath::from_posix(&path), HELPER)
            .await
            .map_err(|_| io_error())?;
        let out = host
            .run_to_string(
                HostCommand::new("chmod")
                    .args(["600", "--", &path])
                    .timeout(Duration::from_secs(3))
                    .cancel_token(self.cancel.clone()),
            )
            .await
            .map_err(|_| io_error())?;
        if !out.success() {
            return Err(io_error());
        }
        Ok(())
    }
    async fn inspect(&self, host: &dyn Host, pid: u32) -> Result<ProcessBinding, BotBackendError> {
        if pid == 0 {
            return Err(io_error());
        }
        #[derive(Deserialize)]
        struct Bound {
            binding: ProcessBinding,
        }
        let out = host
            .run_to_string(
                self.command()
                    .args(["--bind", "--pid", &pid.to_string(), "--exe", &self.qq_exe])
                    .cancel_token(self.cancel.clone()),
            )
            .await
            .map_err(|_| io_error())?;
        if out.stdout.len() > 4096 {
            return Err(io_error());
        }
        if !out.success() {
            #[derive(Deserialize)]
            struct Failure {
                error: String,
            }
            return Err(
                if serde_json::from_str::<Failure>(&out.stdout)
                    .is_ok_and(|failure| failure.error == "DesktopSessionUnavailable")
                {
                    BotBackendError::Io(DESKTOP_SESSION_UNAVAILABLE.into())
                } else {
                    io_error()
                },
            );
        }
        let bound: Bound = serde_json::from_str(&out.stdout).map_err(|_| io_error())?;
        if !valid_binding(&bound.binding, pid) {
            return Err(io_error());
        }
        Ok(bound.binding)
    }
    pub async fn bind(&mut self, host: &dyn Host, pid: u32) -> Result<(), BotBackendError> {
        self.bind_matching(host, pid, None).await
    }
    async fn bind_matching(
        &mut self,
        host: &dyn Host,
        pid: u32,
        expected_start_time: Option<&str>,
    ) -> Result<(), BotBackendError> {
        let binding = self.inspect(host, pid).await?;
        if self.cancel.is_cancelled()
            || expected_start_time.is_some_and(|expected| binding.start_time != expected)
        {
            return Err(io_error());
        }
        self.binding = Some(binding);
        self.qq_pid = pid;
        self.save(host).await
    }
    async fn save(&self, host: &dyn Host) -> Result<(), BotBackendError> {
        if self.cancel.is_cancelled() {
            return Err(io_error());
        }
        let path = format!("{}/session.json", self.directory);
        let bytes = serde_json::to_vec(self).map_err(|_| io_error())?;
        host.write_file(&HostPath::from_posix(&path), &bytes)
            .await
            .map_err(|_| io_error())?;
        let out = host
            .run_to_string(
                HostCommand::new("chmod")
                    .args(["600", "--", &path])
                    .timeout(Duration::from_secs(3))
                    .cancel_token(self.cancel.clone()),
            )
            .await
            .map_err(|_| io_error())?;
        if !out.success() {
            return Err(io_error());
        }
        Ok(())
    }
    pub async fn renew_with_cancel(
        &mut self,
        host: &dyn Host,
        cancel: CancellationToken,
    ) -> Result<(), BotBackendError> {
        if cancel.is_cancelled() {
            return Err(io_error());
        }
        let old = self.binding.clone().ok_or_else(io_error)?;
        self.cancel = cancel;
        self.deploy(host).await?;
        if self.inspect(host, old.pid).await? != old {
            return Err(io_error());
        }
        self.session_id = format!("{:032x}", rand::random::<u128>());
        self.save(host).await
    }
    pub async fn recover(
        host: &dyn Host,
        paths: &SnowLumaRemotePaths,
        bot_id: &BotId,
        pid: u32,
        _node_bin: &str,
    ) -> Result<Option<Self>, BotBackendError> {
        Self::recover_with_cancel(host, paths, bot_id, pid, CancellationToken::new()).await
    }
    async fn recover_with_cancel(
        host: &dyn Host,
        paths: &SnowLumaRemotePaths,
        bot_id: &BotId,
        pid: u32,
        cancel: CancellationToken,
    ) -> Result<Option<Self>, BotBackendError> {
        if cancel.is_cancelled() {
            return Ok(None);
        }
        let root = directory(paths, bot_id)?;
        let path = format!("{root}/session.json");
        if !host
            .exists(&HostPath::from_posix(&path))
            .await
            .unwrap_or(false)
        {
            return Ok(None);
        }
        let bytes = bounded_read(host, &path, 8192).await?;
        let Ok(mut session) = serde_json::from_slice::<Self>(&bytes) else {
            return Ok(None);
        };
        session.cancel = cancel;
        if session.directory != root
            || session.qq_pid != pid
            || session.expected_uin != bot_id.as_str()
            || session.session_id.len() != 32
            || !session.session_id.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Ok(None);
        }
        let old = session.binding.clone().ok_or_else(io_error)?;
        session.deploy(host).await?;
        if session.inspect(host, pid).await? != old {
            return Ok(None);
        }
        Ok(Some(session))
    }
    fn decode(&self, bytes: &[u8]) -> Result<SnowLumaQrLoginSnapshot, BotBackendError> {
        let wire: WireReply = serde_json::from_slice(bytes).map_err(|_| io_error())?;
        if wire.session_id != self.session_id
            || wire.snapshot.qq_pid != Some(self.qq_pid)
            || wire.snapshot.v != 1
            || !wire.snapshot.observer_removed
            || wire
                .snapshot
                .qrcode_payload
                .as_ref()
                .is_some_and(|s| s.len() > 128 * 1024 || !s.starts_with("data:image/png;base64,"))
        {
            return Err(io_error());
        }
        let mut snapshot = wire.snapshot;
        // AT-SPI 主界面只阻止继续取图，账号及成功状态由 SL owner 判断。
        if snapshot.state == SnowLumaQrLoginState::LoggedIn {
            snapshot.state = SnowLumaQrLoginState::LoginDetected;
        }
        if snapshot
            .uin
            .as_ref()
            .is_some_and(|uin| uin != &self.expected_uin)
        {
            snapshot.state = SnowLumaQrLoginState::AccountMismatch;
            snapshot.message = Some("扫码账号与此 Bot 不一致".into());
        }
        if snapshot.state.is_terminal()
            || matches!(
                snapshot.state,
                SnowLumaQrLoginState::Scanned | SnowLumaQrLoginState::Expired
            )
        {
            snapshot.qrcode_payload = None;
        }
        Ok(snapshot)
    }
    pub async fn request(
        &self,
        host: &dyn Host,
        action: &str,
    ) -> Result<SnowLumaQrLoginSnapshot, BotBackendError> {
        self.request_with_cancel(host, action, self.cancel.clone(), false)
            .await
    }
    pub async fn request_with_cancel(
        &self,
        host: &dyn Host,
        action: &str,
        cancel: CancellationToken,
        scanned: bool,
    ) -> Result<SnowLumaQrLoginSnapshot, BotBackendError> {
        if !matches!(action, "snapshot" | "refresh" | "close") {
            return Err(io_error());
        }
        if action == "close" {
            self.cancel.cancel();
            let mut result = SnowLumaQrLoginSnapshot::unavailable("扫码已关闭");
            result.state = SnowLumaQrLoginState::Cancelled;
            result.qq_pid = Some(self.qq_pid);
            return Ok(result);
        }
        let binding = self.binding.as_ref().ok_or_else(io_error)?;
        let input=serde_json::to_vec(&serde_json::json!({"v":1,"session_id":self.session_id,"action":action,"binding":binding,"scanned":scanned})).map_err(|_|io_error())?;
        match host
            .run_to_string(self.command().stdin(input).cancel_token(cancel))
            .await
        {
            Ok(out) if out.success() && out.stdout.len() <= 192 * 1024 => {
                self.decode(out.stdout.as_bytes())
            }
            Ok(_) => {
                let mut result = SnowLumaQrLoginSnapshot::unavailable("扫码控件不可用，请使用 VNC");
                result.qq_pid = Some(self.qq_pid);
                Ok(result)
            }
            _ => {
                let mut result =
                    SnowLumaQrLoginSnapshot::unavailable("扫码请求未确认结束，请使用 VNC");
                result.state = SnowLumaQrLoginState::CleanupFailed;
                result.observer_removed = false;
                result.qq_pid = Some(self.qq_pid);
                Ok(result)
            }
        }
    }
    pub async fn remove_assets(&self, host: &dyn Host) {
        self.cancel.cancel();
        let _ = host
            .remove_file(&HostPath::from_posix(format!(
                "{}/atspi-login.py",
                self.directory
            )))
            .await;
    }
    pub async fn remove_all(&self, host: &dyn Host) {
        self.cancel.cancel();
        let _ = host
            .remove_dir_all(&HostPath::from_posix(&self.directory))
            .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use bytes::Bytes;
    use ncd_host::{
        Arch, ArchiveKind, CommandOutput, DirEntry, HostError, HostProcess, HostShell, Locality,
        ShellKind,
    };
    use std::{collections::HashMap, path::Path, sync::Mutex};

    struct TestShell;
    impl HostShell for TestShell {
        fn kind(&self) -> ShellKind {
            ShellKind::Bash
        }
        fn escape(&self, arg: &str) -> String {
            ncd_host::shell_single_quote(arg)
        }
        fn line_separator(&self) -> &'static str {
            "\n"
        }
    }
    static TEST_SHELL: TestShell = TestShell;

    struct BindingHost {
        files: Mutex<HashMap<String, Vec<u8>>>,
        commands: Mutex<Vec<HostCommand>>,
        birth: &'static str,
        bind_error: Option<&'static str>,
        cancel_on_bind: Option<CancellationToken>,
    }
    impl BindingHost {
        fn new() -> Self {
            Self {
                files: Mutex::new(HashMap::new()),
                commands: Mutex::new(Vec::new()),
                birth: "881923",
                bind_error: None,
                cancel_on_bind: None,
            }
        }
    }
    #[async_trait]
    impl Host for BindingHost {
        fn os(&self) -> Os {
            Os::Linux
        }
        fn arch(&self) -> Arch {
            Arch::X86_64
        }
        fn locality(&self) -> Locality {
            Locality::Remote
        }
        fn id(&self) -> &str {
            "bound-login-test"
        }
        fn shell(&self) -> &dyn HostShell {
            &TEST_SHELL
        }
        async fn read_file(&self, path: &HostPath) -> Result<Bytes, HostError> {
            self.files
                .lock()
                .unwrap()
                .get(path.as_posix())
                .cloned()
                .map(Bytes::from)
                .ok_or_else(|| HostError::PathNotFound { path: path.clone() })
        }
        async fn write_file(&self, path: &HostPath, bytes: &[u8]) -> Result<(), HostError> {
            self.files
                .lock()
                .unwrap()
                .insert(path.as_posix().to_string(), bytes.to_vec());
            Ok(())
        }
        async fn list_dir(&self, _: &HostPath) -> Result<Vec<DirEntry>, HostError> {
            Err(HostError::Unsupported { operation: "test" })
        }
        async fn create_dir_all(&self, _: &HostPath) -> Result<(), HostError> {
            Ok(())
        }
        async fn remove_file(&self, path: &HostPath) -> Result<(), HostError> {
            self.files.lock().unwrap().remove(path.as_posix());
            Ok(())
        }
        async fn remove_dir_all(&self, _: &HostPath) -> Result<(), HostError> {
            Ok(())
        }
        async fn exists(&self, path: &HostPath) -> Result<bool, HostError> {
            Ok(self.files.lock().unwrap().contains_key(path.as_posix()))
        }
        async fn upload(&self, _: &Path, _: &HostPath) -> Result<(), HostError> {
            Err(HostError::Unsupported { operation: "test" })
        }
        async fn download(&self, _: &HostPath, _: &Path) -> Result<(), HostError> {
            Err(HostError::Unsupported { operation: "test" })
        }
        async fn extract_archive(
            &self,
            _: &HostPath,
            _: &HostPath,
            _: ArchiveKind,
        ) -> Result<(), HostError> {
            Err(HostError::Unsupported { operation: "test" })
        }
        async fn spawn(&self, _: HostCommand) -> Result<Box<dyn HostProcess>, HostError> {
            panic!("session recovery must not launch QQ")
        }
        async fn which(&self, name: &str) -> Result<Option<String>, HostError> {
            assert_eq!(name, "python3");
            Ok(Some("/usr/bin/python3".into()))
        }
        async fn run_to_string(&self, command: HostCommand) -> Result<CommandOutput, HostError> {
            let (code, stdout) = if command.program == "chmod" {
                (0, String::new())
            } else if command.args.iter().any(|arg| arg == "--probe") {
                (0, "{\"available\":true}".into())
            } else if command.args.iter().any(|arg| arg == "--bind") {
                if let Some(cancel) = &self.cancel_on_bind {
                    cancel.cancel();
                }
                match self.bind_error {
                    Some(error) => (1, serde_json::json!({ "error": error }).to_string()),
                    None => (
                        0,
                        serde_json::json!({ "binding": {
                            "pid":19, "start_time":self.birth, "exe":"/opt/QQ/qq", "uid":1000,
                            "display":":0", "bus":"unix:path=/run/user/1000/bus",
                            "xauthority":null, "runtime_dir":null
                        }})
                        .to_string(),
                    ),
                }
            } else {
                panic!("recovery must not sample or invoke QQ controls")
            };
            self.commands.lock().unwrap().push(command);
            Ok(CommandOutput {
                exit_code: Some(code),
                stdout,
                stderr: String::new(),
            })
        }
    }
    fn layout() -> RemoteSnowLumaLayout {
        RemoteSnowLumaLayout {
            home: "/home/u".into(),
            paths: SnowLumaRemotePaths::from_remote_home("/home/u"),
            node_bin: "/opt/snowluma/node".into(),
            qq_bin: "/opt/QQ/qq".into(),
            qq_install_base: "/".into(),
        }
    }

    fn session() -> LoginSession {
        LoginSession {
            directory: "/tmp/private".into(),
            session_id: "session".into(),
            python_bin: "python3".into(),
            qq_exe: "/opt/QQ/qq".into(),
            qq_pid: 19,
            expected_uin: "123456".into(),
            binding: None,
            cancel: CancellationToken::new(),
        }
    }
    #[test]
    fn reply_must_match_session_and_pid() {
        assert!(session().decode(br#"{"session_id":"wrong","v":1,"state":"waiting_for_qr","qq_pid":19,"qrcode_payload":null,"uin":null,"observer_removed":true,"message":null}"#).is_err());
        assert!(session().decode(br#"{"session_id":"session","v":1,"state":"waiting_for_qr","qq_pid":20,"qrcode_payload":null,"uin":null,"observer_removed":true,"message":null}"#).is_err());
    }
    #[test]
    fn scan_clears_image_and_main_does_not_prove_login() {
        let reply=br#"{"session_id":"session","v":1,"state":"scanned","qq_pid":19,"qrcode_payload":"data:image/png;base64,YQ==","uin":null,"observer_removed":true,"message":null}"#;
        assert!(session().decode(reply).unwrap().qrcode_payload.is_none());
        let reply=br#"{"session_id":"session","v":1,"state":"logged_in","qq_pid":19,"qrcode_payload":null,"uin":null,"observer_removed":true,"message":null}"#;
        assert_eq!(
            session().decode(reply).unwrap().state,
            SnowLumaQrLoginState::LoginDetected
        );
    }
    #[test]
    fn proc_stat_uses_birth_field_even_with_parentheses_in_process_name() {
        let mut fields = vec!["S"; 20];
        fields[19] = "881923";
        let stat = format!("19 (QQ (renderer)) {}", fields.join(" "));
        assert_eq!(parse_start_time(stat.as_bytes()).as_deref(), Some("881923"));
        assert!(parse_start_time(b"19 (QQ) S").is_none());
        fields[19] = "invalid";
        let stat = format!("19 (QQ) {}", fields.join(" "));
        assert!(parse_start_time(stat.as_bytes()).is_none());
    }

    #[test]
    fn binding_requires_exact_pid_local_display_and_private_unix_bus() {
        let mut binding = ProcessBinding {
            pid: 19,
            start_time: "881923".into(),
            exe: "/opt/QQ/qq".into(),
            uid: 1000,
            display: ":0.0".into(),
            bus: "unix:path=/run/user/1000/bus".into(),
            xauthority: None,
            runtime_dir: None,
        };
        assert!(valid_binding(&binding, 19));
        assert!(!valid_binding(&binding, 20));
        binding.bus = "tcp:host=example.test".into();
        assert!(!valid_binding(&binding, 19));
        binding.bus = "unix:path=/run/user/1000/bus".into();
        for display in ["localhost:0", ":", ":0.x", ":0.0.0"] {
            binding.display = display.into();
            assert!(!valid_binding(&binding, 19));
        }
    }

    #[tokio::test]
    async fn running_qq_can_bind_without_saved_metadata_then_recover_with_fresh_nonce() {
        let host = BindingHost::new();
        let layout = layout();
        let bot_id = BotId::from("123456");
        let first = LoginSession::attach_running(
            &host,
            &layout.paths,
            &layout,
            &bot_id,
            19,
            "881923",
            CancellationToken::new(),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(first.start_time(), Some("881923"));
        assert_eq!(first.pid(), 19);
        let second = LoginSession::attach_running(
            &host,
            &layout.paths,
            &layout,
            &bot_id,
            19,
            "881923",
            CancellationToken::new(),
        )
        .await
        .unwrap()
        .unwrap();
        assert_ne!(first.session_id, second.session_id);
        assert!(
            host.commands
                .lock()
                .unwrap()
                .iter()
                .all(|command| command.stdin.is_none())
        );
    }

    #[tokio::test]
    async fn reused_pid_or_cancelled_bind_cannot_save_a_session() {
        let layout = layout();
        let bot_id = BotId::from("123456");
        let host = BindingHost::new();
        assert!(
            LoginSession::attach_running(
                &host,
                &layout.paths,
                &layout,
                &bot_id,
                19,
                "different-birth",
                CancellationToken::new(),
            )
            .await
            .is_err()
        );
        assert!(
            !host
                .files
                .lock()
                .unwrap()
                .keys()
                .any(|path| path.ends_with("session.json"))
        );

        let cancel = CancellationToken::new();
        let mut host = BindingHost::new();
        host.cancel_on_bind = Some(cancel.clone());
        assert!(
            LoginSession::attach_running(
                &host,
                &layout.paths,
                &layout,
                &bot_id,
                19,
                "881923",
                cancel,
            )
            .await
            .is_err()
        );
        assert!(
            !host
                .files
                .lock()
                .unwrap()
                .keys()
                .any(|path| path.ends_with("session.json"))
        );
    }

    #[tokio::test]
    async fn pre_cancelled_request_never_deploys_and_dead_desktop_bus_has_specific_fallback() {
        let layout = layout();
        let bot_id = BotId::from("123456");
        let host = BindingHost::new();
        let cancel = CancellationToken::new();
        cancel.cancel();
        assert!(
            LoginSession::attach_running(
                &host,
                &layout.paths,
                &layout,
                &bot_id,
                19,
                "881923",
                cancel,
            )
            .await
            .unwrap()
            .is_none()
        );
        assert!(host.files.lock().unwrap().is_empty());
        assert!(host.commands.lock().unwrap().is_empty());

        let mut host = BindingHost::new();
        host.bind_error = Some("DesktopSessionUnavailable");
        let result = LoginSession::attach_running(
            &host,
            &layout.paths,
            &layout,
            &bot_id,
            19,
            "881923",
            CancellationToken::new(),
        )
        .await;
        assert!(
            matches!(result, Err(BotBackendError::Io(message)) if message == DESKTOP_SESSION_UNAVAILABLE)
        );
    }
}
