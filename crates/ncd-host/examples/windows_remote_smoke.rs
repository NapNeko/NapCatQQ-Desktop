//! 用显式凭据和固定主机公钥验证真实 Windows Host，仅操作随机临时目录。
use std::{error::Error, net::IpAddr, path::PathBuf, sync::Arc, time::Duration};

use base64::{Engine, engine::general_purpose::STANDARD};
use ncd_host::remote::{
    ConnectionConfig, HostKeyPolicy, KnownHostsStore, SshCredentials, TunnelSpec,
    windows::connect_remote_host_with_platform,
};
use ncd_host::{Host, HostCommand, HostPath, Os};
use tokio::{
    io::AsyncReadExt,
    net::{TcpListener, TcpSocket, TcpStream},
};

type Result<T> = std::result::Result<T, Box<dyn Error + Send + Sync>>;

fn required(name: &str) -> Result<String> {
    std::env::var(name).map_err(|_| format!("missing {name}").into())
}

fn powershell(script: &str) -> HostCommand {
    let bytes: Vec<_> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    HostCommand::new("powershell.exe")
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-EncodedCommand",
        ])
        .arg(STANDARD.encode(bytes))
        .timeout(Duration::from_secs(30))
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    run().await
}

async fn run() -> Result<()> {
    let target = required("NCD_SMOKE_HOST")?;
    let port = std::env::var("NCD_SMOKE_PORT")
        .unwrap_or_else(|_| "22".into())
        .parse()?;
    let username = required("NCD_SMOKE_USER")?;
    let key = PathBuf::from(required("NCD_SMOKE_KEY")?);
    let server_key = required("NCD_SMOKE_SERVER_KEY")?;
    let key_kind =
        std::env::var("NCD_SMOKE_SERVER_KEY_KIND").unwrap_or_else(|_| "ssh-ed25519".into());
    let local = tempfile::tempdir()?;
    let (connect_host, connect_port, relay) =
        if let Ok(source) = std::env::var("NCD_SMOKE_BIND_IPV6") {
            let source: IpAddr = source.parse()?;
            let destination = (target.parse::<IpAddr>()?, port).into();
            let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
            let local_port = listener.local_addr()?.port();
            let relay = tokio::spawn(async move {
                let (mut incoming, _) = listener.accept().await?;
                let socket = TcpSocket::new_v6()?;
                socket.bind((source, 0).into())?;
                let mut outgoing = socket.connect(destination).await?;
                tokio::io::copy_bidirectional(&mut incoming, &mut outgoing).await?;
                Ok::<_, std::io::Error>(())
            });
            ("127.0.0.1".to_owned(), local_port, Some(relay))
        } else {
            (target, port, None)
        };
    let trust = local.path().join("known_hosts");
    KnownHostsStore::new(&trust)
        .append(&connect_host, connect_port, &key_kind, &server_key)
        .await?;
    let config = ConnectionConfig::new(
        connect_host,
        connect_port,
        SshCredentials::key_file(username, key, None),
        HostKeyPolicy::strict(&trust),
    )
    .with_connect_timeout(Duration::from_secs(15));
    let host =
        connect_remote_host_with_platform("windows-live-smoke", config, Some("windows")).await?;
    if host.os() != Os::Windows {
        return Err("target is not Windows".into());
    }
    let info = host.remote_windows_info().await?;
    println!(
        "PASS platform: {} / {} / {}",
        info.os_version, info.architecture, info.username
    );
    println!(
        "PASS target directories: {} / {}",
        info.managed_root, info.user_root
    );

    let nonce = local
        .path()
        .file_name()
        .ok_or("missing temp directory name")?
        .to_string_lossy();
    let remote = HostPath::from_windows(&info.temp_dir).join(format!("NcdRemoteSmoke-{nonce}"));
    host.create_dir_all(&remote).await?;
    println!("TEMP {}", remote.render_for(Os::Windows));
    let outcome = tokio::time::timeout(Duration::from_secs(180), check(&host, &remote))
        .await
        .map_err(|_| "remote checks exceeded 180 seconds")
        .map_err(Into::<Box<dyn Error + Send + Sync>>::into)
        .and_then(|result| result);
    let cleanup = tokio::time::timeout(Duration::from_secs(10), host.remove_dir_all(&remote))
        .await
        .unwrap_or(Err(ncd_host::HostError::Timeout {
            operation: "smoke_cleanup",
        }));
    if cleanup.is_ok() {
        println!("PASS cleanup: {}", remote.render_for(Os::Windows));
    } else if let Err(error) = &cleanup {
        eprintln!("CLEANUP FAILED {}: {error}", remote.render_for(Os::Windows));
    }
    outcome?;
    cleanup?;
    drop(host);
    if let Some(relay) = relay {
        relay.abort();
    }
    Ok(())
}

async fn check(host: &Arc<dyn Host>, remote: &HostPath) -> Result<()> {
    let data: Vec<u8> = (0..=255).cycle().take(4096).collect();
    let path = remote.join("路径 空格 ' 测试.bin");
    host.write_file(&path, &data).await?;
    if host.read_file(&path).await? != data || host.list_dir(remote).await?.is_empty() {
        return Err("SFTP binary round-trip or directory listing failed".into());
    }
    println!("PASS SFTP: binary round-trip and Unicode/space/apostrophe path");

    let command = powershell("$ProgressPreference='SilentlyContinue';$m=New-Object IO.MemoryStream;[Console]::OpenStandardInput().CopyTo($m);[Console]::Write([Convert]::ToBase64String($m.ToArray()))")
        .stdin(data.clone());
    let status = host.spawn(command).await?.wait().await?;
    if !status.success() || status.stdout != STANDARD.encode(&data) || !status.stderr.is_empty() {
        return Err(format!(
            "command round-trip failed: exit={:?}, stderr={}",
            status.exit_code, status.stderr
        )
        .into());
    }
    println!("PASS command: all byte values preserved on stdin and clean encoded stdout");

    let lines = Arc::new(std::sync::Mutex::new(Vec::new()));
    let captured = Arc::clone(&lines);
    let result = host.run_streaming(
        powershell("$ProgressPreference='SilentlyContinue';[Console]::WriteLine('NCD_STDOUT_OK');[Console]::Error.WriteLine('NCD_STDERR_OK')"),
        Box::new(move |source, line| captured.lock().unwrap().push((source, line))),
    ).await?;
    let seen = lines.lock().unwrap();
    if !result.success()
        || !seen.iter().any(|(source, line)| {
            *source == ncd_host::StreamSource::Stdout && line == "NCD_STDOUT_OK"
        })
        || !seen.iter().any(|(source, line)| {
            *source == ncd_host::StreamSource::Stderr && line == "NCD_STDERR_OK"
        })
    {
        return Err("streaming stdout/stderr separation failed".into());
    }
    drop(seen);
    println!("PASS streaming: stdout/stderr line callbacks");

    let result = host.run_to_string(powershell("$ProgressPreference='SilentlyContinue';[Console]::OutputEncoding=[Text.UTF8Encoding]::new($false);[Console]::Write($env:NCD_SMOKE_VALUE)")
        .working_dir(remote.clone())
        .env("NCD_SMOKE_VALUE", "中文 空格 ' $() ; literal")).await?;
    if !result.success() || result.stdout != "中文 空格 ' $() ; literal" {
        return Err(format!("literal environment check failed: {}", result.stderr).into());
    }
    println!("PASS command environment: literal Unicode and metacharacters");

    let tunnel = host.open_tunnel(TunnelSpec::local_to_remote(0, 22)).await?;
    let mut connection = TcpStream::connect(("127.0.0.1", tunnel.local_port())).await?;
    let mut greeting = [0; 256];
    let size =
        tokio::time::timeout(Duration::from_secs(10), connection.read(&mut greeting)).await??;
    if !greeting[..size].starts_with(b"SSH-2.0-") {
        return Err("SSH tunnel returned an unexpected greeting".into());
    }
    drop(connection);
    tunnel.close();
    drop(tunnel);
    println!("PASS tunnel: remote loopback SSH banner");

    if let Some(binary) = std::env::var_os("NCD_SMOKE_AGENT_EXE") {
        let executable = remote.join("ncd-agent.exe");
        host.upload(&PathBuf::from(binary), &executable).await?;
        let version = host
            .run_to_string(
                HostCommand::new(executable.render_for(Os::Windows))
                    .arg("--version")
                    .timeout(Duration::from_secs(20)),
            )
            .await?;
        if !version.success() {
            return Err(format!("Agent version failed: {}", version.stderr).into());
        }
        println!("PASS Agent executable: {}", version.stdout.trim());
        let result = host
            .run_to_string(
                HostCommand::new(executable.render_for(Os::Windows))
                    .arg("probe")
                    .timeout(Duration::from_secs(30)),
            )
            .await?;
        if !result.success() {
            return Err(format!("Agent probe failed: {}", result.stderr).into());
        }
        let observed: ncd_domain::windows_remote::RemoteWindowsInfo =
            serde_json::from_str(&result.stdout)?;
        let expected = host.remote_windows_info().await?;
        if observed.user_sid != expected.user_sid
            || observed.managed_root != expected.managed_root
            || observed.user_root != expected.user_root
        {
            return Err("Agent and Host disagree on target identity or directories".into());
        }
        println!(
            "PASS Agent probe: SID and machine/user roots match Host probe; service installed={}",
            observed.agent.is_some()
        );
    }
    Ok(())
}
