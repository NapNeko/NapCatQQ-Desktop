// 短期 AT-SPI 请求绑定 QQ 进程；二维码只通过 SSH 私有管道返回。
use super::layout::{RemoteSnowLumaLayout, SnowLumaRemotePaths};
use ncd_domain::{BotId, SnowLumaQrLoginSnapshot, SnowLumaQrLoginState};
use ncd_host::{Host, HostCommand, HostPath, Os};
use ncd_traits::runtime_backend::BotBackendError;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, time::Duration};
use tokio_util::sync::CancellationToken;

const HELPER: &[u8] = include_bytes!("../../resources/login-helper/atspi-login.py");
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
        cancel: CancellationToken::new(),
    };
    session.deploy(host).await?;
    #[derive(Deserialize)]
    struct Probe {
        available: bool,
    }
    let probe = host
        .run_to_string(session.command().arg("--probe"))
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
    fn command(&self) -> HostCommand {
        HostCommand::new(&self.python_bin)
            .arg("-B")
            .arg(format!("{}/atspi-login.py", self.directory))
            .timeout(Duration::from_secs(8))
    }
    async fn deploy(&mut self, host: &dyn Host) -> Result<(), BotBackendError> {
        host.create_dir_all(&HostPath::from_posix(&self.directory))
            .await
            .map_err(|_| io_error())?;
        let out = host
            .run_to_string(HostCommand::new("chmod").args(["700", "--", &self.directory]))
            .await
            .map_err(|_| io_error())?;
        if !out.success() {
            return Err(io_error());
        }
        let path = format!("{}/atspi-login.py", self.directory);
        host.write_file(&HostPath::from_posix(&path), HELPER)
            .await
            .map_err(|_| io_error())?;
        let out = host
            .run_to_string(HostCommand::new("chmod").args(["600", "--", &path]))
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
            .run_to_string(self.command().args([
                "--bind",
                "--pid",
                &pid.to_string(),
                "--exe",
                &self.qq_exe,
            ]))
            .await
            .map_err(|_| io_error())?;
        if !out.success() || out.stdout.len() > 4096 {
            return Err(io_error());
        }
        let bound: Bound = serde_json::from_str(&out.stdout).map_err(|_| io_error())?;
        if bound.binding.pid != pid
            || bound.binding.start_time.is_empty()
            || !bound.binding.start_time.bytes().all(|b| b.is_ascii_digit())
        {
            return Err(io_error());
        }
        Ok(bound.binding)
    }
    pub async fn bind(&mut self, host: &dyn Host, pid: u32) -> Result<(), BotBackendError> {
        self.binding = Some(self.inspect(host, pid).await?);
        self.qq_pid = pid;
        self.save(host).await
    }
    async fn save(&self, host: &dyn Host) -> Result<(), BotBackendError> {
        let path = format!("{}/session.json", self.directory);
        let bytes = serde_json::to_vec(self).map_err(|_| io_error())?;
        host.write_file(&HostPath::from_posix(&path), &bytes)
            .await
            .map_err(|_| io_error())?;
        let out = host
            .run_to_string(HostCommand::new("chmod").args(["600", "--", &path]))
            .await
            .map_err(|_| io_error())?;
        if !out.success() {
            return Err(io_error());
        }
        Ok(())
    }
    pub async fn renew(&mut self, host: &dyn Host) -> Result<(), BotBackendError> {
        let old = self.binding.clone().ok_or_else(io_error)?;
        self.deploy(host).await?;
        if self.inspect(host, old.pid).await? != old {
            return Err(io_error());
        }
        self.session_id = format!("{:032x}", rand::random::<u128>());
        self.cancel = CancellationToken::new();
        self.save(host).await
    }
    pub async fn recover(
        host: &dyn Host,
        paths: &SnowLumaRemotePaths,
        bot_id: &BotId,
        pid: u32,
        _node_bin: &str,
    ) -> Result<Option<Self>, BotBackendError> {
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
}
