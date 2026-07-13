//! 隧道槽位与 loopback 健康探测

use std::sync::Arc;
use std::time::Duration;

use ncd_deploy::parse_napcat_webui_line;
use ncd_domain::ids::BotId;
use ncd_host::remote::{TunnelHandle, TunnelSpec};
use ncd_host::{Host, HostCommand, HostError};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;

pub type WebuiUnreachableHook = Arc<dyn Fn(BotId) + Send + Sync>;

const TUNNEL_HEALTH_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Default)]
pub(crate) struct TunnelSlot {
    pub(crate) handle: Option<TunnelHandle>,
    /// Desktop 本机可达口
    pub(crate) local_port: Option<u16>,
    /// 隧道远端目标口(Bot 真实 WebUI);None 表示无隧道
    pub(crate) remote_port: Option<u16>,
}

impl TunnelSlot {
    pub(crate) fn clear(&mut self) {
        self.handle = None;
        self.local_port = None;
        self.remote_port = None;
    }

    pub(crate) fn set_open(&mut self, handle: TunnelHandle, remote_port: u16) {
        self.local_port = Some(handle.local_port());
        self.remote_port = Some(remote_port);
        self.handle = Some(handle);
    }

    pub(crate) fn take_handle(&mut self) -> Option<TunnelHandle> {
        self.local_port = None;
        self.remote_port = None;
        self.handle.take()
    }
}


pub(crate) fn notify_unreachable(hook: &Option<WebuiUnreachableHook>, bot_id: &BotId) {
    if let Some(h) = hook {
        h(bot_id.clone());
    }
}

pub(crate) fn scan_latest_webui(bytes: &[u8]) -> Option<(u16, String)> {
    let text = String::from_utf8_lossy(bytes);
    let mut latest = None;
    for line in text.lines() {
        if let Some(pair) = parse_napcat_webui_line(line) {
            latest = Some(pair);
        }
    }
    latest
}

/// 远端 127.0.0.1:port 是否有人听;避免 bind 了本地口但 direct-tcpip ConnectFailed
pub(crate) async fn remote_loopback_listening(host: &dyn Host, remote_port: u16) -> bool {
    let script = format!(
        "bash -c 'echo >/dev/tcp/127.0.0.1/{remote_port}' 2>/dev/null \
         || (command -v ss >/dev/null 2>&1 && ss -lnt 2>/dev/null | grep -Eq ':{remote_port}([[:space:]]|$)') \
         || (command -v netstat >/dev/null 2>&1 && netstat -lnt 2>/dev/null | grep -Eq ':{remote_port}([[:space:]]|$)')"
    );
    let cmd = HostCommand::new("sh")
        .arg("-c")
        .arg(script)
        .timeout(Duration::from_secs(5));
    match host.run_to_string(cmd).await {
        Ok(out) => out.success(),
        Err(_) => false,
    }
}

/// 对本机转发口做极短探测:能 connect 且至少读到 1 字节才算通
pub(crate) async fn local_forward_healthy(local_port: u16) -> bool {
    let connect = timeout(
        TUNNEL_HEALTH_TIMEOUT,
        TcpStream::connect(("127.0.0.1", local_port)),
    )
    .await;
    let Ok(Ok(mut stream)) = connect else {
        return false;
    };
    let req = b"GET / HTTP/1.0\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n";
    if timeout(TUNNEL_HEALTH_TIMEOUT, stream.write_all(req))
        .await
        .ok()
        .and_then(|r| r.ok())
        .is_none()
    {
        return false;
    }
    let mut buf = [0u8; 16];
    matches!(
        timeout(TUNNEL_HEALTH_TIMEOUT, stream.read(&mut buf)).await,
        Ok(Ok(n)) if n > 0
    )
}

pub(crate) async fn ensure_loopback_tunnel(
    host: &dyn Host,
    remote_port: u16,
) -> Result<TunnelHandle, HostError> {
    if !remote_loopback_listening(host, remote_port).await {
        // 目标口无人听,不是 SSH session 断线;勿用 remote_disconnected 以免误触发 host 自愈
        return Err(HostError::InvalidArgument {
            reason: format!("remote 127.0.0.1:{remote_port} not listening"),
        });
    }
    open_loopback_tunnel(host, remote_port).await
}

pub(crate) async fn open_loopback_tunnel(
    host: &dyn Host,
    remote_port: u16,
) -> Result<TunnelHandle, HostError> {
    let spec = TunnelSpec {
        local_host: "127.0.0.1".to_string(),
        local_port: 0,
        remote_host: "127.0.0.1".to_string(),
        remote_port,
    };
    host.open_tunnel(spec).await
}

