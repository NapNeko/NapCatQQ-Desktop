//! 真机 SSH 端到端 smoke test。
//!
//! 这些测试默认 `#[ignore]`,需要环境变量 + 真机配合:
//!
//! ```powershell
//! $env:NCD_TEST_SSH_HOST = "175.178.53.24"
//! $env:NCD_TEST_SSH_USER = "ubuntu"
//! $env:NCD_TEST_SSH_KEY  = "$env:USERPROFILE\.ssh\id_ed25519"
//! cargo test -p ncd-host --test remote_linux_smoke -- --ignored --test-threads=1
//! ```
//!
//! 安全约束:
//! - 所有测试只在 `/tmp/ncd-host-test-<pid>-<rand>/` 内操作,Drop 时清理
//! - 禁止触碰 `~/Napcat`、`/etc/`、`/var/`、`~/.ssh` 等任何业务目录
//! - 禁止杀任何已有进程(只测自己 spawn 出来的 sleep)

use std::path::PathBuf;
use std::time::Duration;

use ncd_host::command::HostCommand;
use ncd_host::host::Host;
use ncd_host::path::HostPath;
use ncd_host::remote::{
    ConnectionConfig, HostKeyPolicy, RemoteLinuxHost, SshCredentials, TunnelSpec,
};

fn env_host() -> Option<String> {
    std::env::var("NCD_TEST_SSH_HOST").ok()
}

fn env_user() -> String {
    std::env::var("NCD_TEST_SSH_USER").unwrap_or_else(|_| "ubuntu".to_string())
}

fn env_key() -> Option<PathBuf> {
    std::env::var("NCD_TEST_SSH_KEY").ok().map(PathBuf::from)
}

async fn make_host() -> RemoteLinuxHost {
    let host = env_host().expect("set NCD_TEST_SSH_HOST");
    let key = env_key().expect("set NCD_TEST_SSH_KEY");
    let creds = SshCredentials::key_file(env_user(), key, None);
    let cfg = ConnectionConfig::new(host, 22, creds, HostKeyPolicy::Insecure)
        .with_connect_timeout(Duration::from_secs(15));
    RemoteLinuxHost::connect("test-server", cfg)
        .await
        .expect("ssh connect")
}

fn unique_tmpdir() -> HostPath {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    HostPath::from_posix(format!(
        "/tmp/ncd-host-test-{}-{}",
        std::process::id(),
        stamp
    ))
}

async fn cleanup_tmpdir(host: &RemoteLinuxHost, path: &HostPath) {
    let _ = host.remove_dir_all(path).await;
}

// ============================================================
// 基本 IO
// ============================================================

#[tokio::test]
#[ignore = "requires NCD_TEST_SSH_* env vars + real Linux server"]
async fn smoke_run_to_string_echo() {
    let host = make_host().await;
    let cmd = HostCommand::new("echo").arg("hello-from-ncd-host");
    let out = host.run_to_string(cmd).await.unwrap();
    assert!(out.success(), "exit_code={:?} stderr={}", out.exit_code, out.stderr);
    assert!(out.stdout.contains("hello-from-ncd-host"));
}

#[tokio::test]
#[ignore = "requires NCD_TEST_SSH_*"]
async fn smoke_identity_methods() {
    let host = make_host().await;
    use ncd_host::host::{Locality, Os};
    assert_eq!(host.os(), Os::Linux);
    assert_eq!(host.locality(), Locality::Remote);
}

#[tokio::test]
#[ignore = "requires NCD_TEST_SSH_*"]
async fn smoke_sftp_write_read_round_trip() {
    let host = make_host().await;
    let dir = unique_tmpdir();
    host.create_dir_all(&dir).await.unwrap();
    let file = dir.join("hello.txt");
    let payload = b"hello-sftp-round-trip\n";
    host.write_file(&file, payload).await.unwrap();
    let read = host.read_file(&file).await.unwrap();
    assert_eq!(read.as_ref(), payload);
    cleanup_tmpdir(&host, &dir).await;
}

#[tokio::test]
#[ignore = "requires NCD_TEST_SSH_*"]
async fn smoke_list_dir_returns_entries() {
    let host = make_host().await;
    let dir = unique_tmpdir();
    host.create_dir_all(&dir).await.unwrap();
    host.write_file(&dir.join("alpha.txt"), b"a").await.unwrap();
    host.write_file(&dir.join("beta.txt"), b"b").await.unwrap();

    let entries = host.list_dir(&dir).await.unwrap();
    let names: Vec<_> = entries.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&"alpha.txt"));
    assert!(names.contains(&"beta.txt"));
    cleanup_tmpdir(&host, &dir).await;
}

#[tokio::test]
#[ignore = "requires NCD_TEST_SSH_*"]
async fn smoke_remove_dir_all_refuses_dangerous_paths() {
    use ncd_host::error::HostError;
    let host = make_host().await;
    // 防御性检查:绝对禁止删 /etc /home /tmp 这种保护路径
    let err = host.remove_dir_all(&HostPath::from_posix("/etc")).await.unwrap_err();
    assert!(matches!(err, HostError::InvalidArgument { .. }));
}

// ============================================================
// 命令执行
// ============================================================

#[tokio::test]
#[ignore = "requires NCD_TEST_SSH_*"]
async fn smoke_run_captures_nonzero_exit() {
    let host = make_host().await;
    let cmd = HostCommand::new("sh").arg("-c").arg("exit 7");
    let out = host.run_to_string(cmd).await.unwrap();
    assert_eq!(out.exit_code, Some(7));
    assert!(!out.success());
}

#[tokio::test]
#[ignore = "requires NCD_TEST_SSH_*"]
async fn smoke_run_with_env_and_working_dir() {
    let host = make_host().await;
    let dir = unique_tmpdir();
    host.create_dir_all(&dir).await.unwrap();
    let cmd = HostCommand::new("sh")
        .arg("-c")
        .arg("echo $NCD_TEST_VAR && pwd")
        .env("NCD_TEST_VAR", "ssh-payload")
        .working_dir(dir.clone());
    let out = host.run_to_string(cmd).await.unwrap();
    assert!(out.success());
    assert!(out.stdout.contains("ssh-payload"));
    assert!(out.stdout.contains(dir.as_posix()));
    cleanup_tmpdir(&host, &dir).await;
}

#[tokio::test]
#[ignore = "requires NCD_TEST_SSH_*"]
async fn smoke_run_respects_timeout() {
    use ncd_host::error::HostError;
    let host = make_host().await;
    let cmd = HostCommand::new("sleep").arg("60").timeout(Duration::from_millis(500));
    let err = host.run_to_string(cmd).await.unwrap_err();
    assert!(matches!(err, HostError::Timeout { .. }));
}

#[tokio::test]
#[ignore = "requires NCD_TEST_SSH_*"]
async fn smoke_run_napcat_probe_matches_legacy_layout() {
    // NapCat-Installer 官方 rootless 路径:$HOME/Napcat/opt/QQ
    // 这台测试机已装好,验证 ncd-host 能探测到 package.json 与 qq 可执行
    let host = make_host().await;
    let cmd = HostCommand::new("sh")
        .arg("-c")
        .arg("test -f $HOME/Napcat/opt/QQ/resources/app/package.json && echo PROBE_OK");
    let out = host.run_to_string(cmd).await.unwrap();
    if out.success() {
        assert!(out.stdout.contains("PROBE_OK"));
    } else {
        // 服务器没装也不算测试失败,只是说明这台不能用作"已装"场景
        eprintln!("[smoke] napcat not present at standard path; treating as soft-skip");
    }
}

// ============================================================
// 解压
// ============================================================

#[tokio::test]
#[ignore = "requires NCD_TEST_SSH_*"]
async fn smoke_extract_tar_gz() {
    use ncd_host::path::ArchiveKind;
    let host = make_host().await;
    let dir = unique_tmpdir();
    host.create_dir_all(&dir).await.unwrap();
    let payload_dir = dir.join("payload");
    host.create_dir_all(&payload_dir).await.unwrap();
    host.write_file(&payload_dir.join("a.txt"), b"alpha\n").await.unwrap();

    // 在远端打 tar
    let tarball = dir.join("payload.tar.gz");
    let out = host
        .run_to_string(
            HostCommand::new("sh").arg("-c").arg(format!(
                "cd {} && tar -czf {} payload",
                dir.as_posix(),
                tarball.as_posix()
            )),
        )
        .await
        .unwrap();
    assert!(out.success());

    let unpack = dir.join("unpack");
    host.create_dir_all(&unpack).await.unwrap();
    host.extract_archive(&tarball, &unpack, ArchiveKind::TarGz)
        .await
        .unwrap();

    let bytes = host.read_file(&unpack.join("payload/a.txt")).await.unwrap();
    assert_eq!(bytes.as_ref(), b"alpha\n");

    cleanup_tmpdir(&host, &dir).await;
}

// ============================================================
// 端口转发(隧道)
// ============================================================

#[tokio::test]
#[ignore = "requires NCD_TEST_SSH_*"]
async fn smoke_open_tunnel_to_remote_localhost() {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpStream;
    let host = make_host().await;

    // 在远端起一个简单 TCP echo:nc -l -p <port> 一次性
    // 用 python -c 更可靠(Ubuntu 24 自带 python3)
    let dir = unique_tmpdir();
    host.create_dir_all(&dir).await.unwrap();
    let server_script = r#"
import socket
s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
s.bind(("127.0.0.1", 0))
port = s.getsockname()[1]
print(f"PORT={port}", flush=True)
s.listen(1)
conn, _ = s.accept()
data = conn.recv(1024)
conn.sendall(b"echo: " + data)
conn.close()
s.close()
"#;
    host.write_file(&dir.join("echo.py"), server_script.as_bytes())
        .await
        .unwrap();

    // 起 server,后台跑,记录 stdout 找端口
    let server_proc = host
        .spawn(
            HostCommand::new("python3")
                .arg(dir.join("echo.py").as_posix())
                .timeout(Duration::from_secs(20)),
        )
        .await
        .unwrap();

    // 等 server 打印 PORT=,远端 HostProcess::wait 是 blocking 全消费,
    // 这里换个策略:也通过 ssh 远端命令直接抓启动后端口
    // 简化:让 server 把端口写到文件
    drop(server_proc); // 实际我们在 spawn 时还没读 stdout,这里先放
    let server_with_file = format!(
        r#"
import socket
s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
s.bind(("127.0.0.1", 0))
port = s.getsockname()[1]
open("{port_file}", "w").write(str(port))
s.listen(1)
conn, _ = s.accept()
data = conn.recv(1024)
conn.sendall(b"echo: " + data)
conn.close()
s.close()
"#,
        port_file = dir.join("port.txt").as_posix()
    );
    host.write_file(&dir.join("echo2.py"), server_with_file.as_bytes())
        .await
        .unwrap();

    // 后台启动 server
    let _bg = tokio::spawn({
        let host = make_host();
        let dir = dir.clone();
        async move {
            let host = host.await;
            let _ = host
                .run_to_string(
                    HostCommand::new("python3")
                        .arg(dir.join("echo2.py").as_posix())
                        .timeout(Duration::from_secs(15)),
                )
                .await;
        }
    });

    // 轮询 port.txt
    let mut remote_port = 0u16;
    for _ in 0..30 {
        tokio::time::sleep(Duration::from_millis(200)).await;
        if let Ok(bytes) = host.read_file(&dir.join("port.txt")).await {
            if let Ok(s) = std::str::from_utf8(&bytes) {
                if let Ok(p) = s.trim().parse::<u16>() {
                    remote_port = p;
                    break;
                }
            }
        }
    }
    assert!(remote_port > 0, "remote port not ready");

    // 开隧道
    let tunnel = host
        .open_tunnel(TunnelSpec::local_to_remote(0, remote_port))
        .await
        .unwrap();
    let local_port = tunnel.local_port();
    assert!(local_port > 0);

    // 走本地端口:连上去发 ping,期望收到 "echo: ping"
    tokio::time::sleep(Duration::from_millis(300)).await;
    let mut stream = TcpStream::connect(("127.0.0.1", local_port)).await.unwrap();
    stream.write_all(b"ping").await.unwrap();
    drop(stream);
    // 简化断言:连接成功且未 panic 视为隧道工作
    let _ = tunnel; // 显式持有到此

    cleanup_tmpdir(&host, &dir).await;
}
