// 复用 SL 控件辅助，子进程输入输出有边界且取消时终止。
use ncd_domain::SnowLumaQrLoginSnapshot;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{process::Stdio, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command,
};

const HELPER: &str =
    include_str!("../../../ncd-backend-snowluma/resources/login-helper/atspi-login.py");

pub async fn read_limited(path: &str, max: u64) -> Result<Vec<u8>, String> {
    let file = tokio::fs::File::open(path)
        .await
        .map_err(|_| "无法读取进程状态")?;
    let mut bytes = Vec::new();
    file.take(max + 1)
        .read_to_end(&mut bytes)
        .await
        .map_err(|_| "无法读取进程状态")?;
    if bytes.len() as u64 > max {
        return Err("进程状态过大".into());
    }
    Ok(bytes)
}

pub async fn generation(container: Option<&str>, pid: u32) -> Option<String> {
    if pid == 0 {
        return None;
    }
    let path = format!("/proc/{pid}/stat");
    let (namespace, bytes) = if let Some(container) = container {
        if container.starts_with('-') || container.is_empty() {
            return None;
        }
        let mut inspect = Command::new("docker");
        inspect.args([
            "inspect",
            "--format",
            "{{.Id}}:{{.State.StartedAt}}",
            container,
        ]);
        let id = String::from_utf8(run(inspect, &[], 256).await.ok()?).ok()?;
        let mut command = Command::new("docker");
        command.args(["exec", container, "cat", &path]);
        (id.trim().to_owned(), run(command, &[], 4096).await.ok()?)
    } else {
        ("native".into(), read_limited(&path, 4096).await.ok()?)
    };
    let stat = std::str::from_utf8(&bytes).ok()?;
    let start = stat.rsplit_once(')')?.1.split_whitespace().nth(19)?;
    if start.parse::<u64>().is_err() {
        return None;
    }
    Some(format!("{namespace}:{pid}:{start}"))
}

pub async fn docker_anchor_pid(container: &str) -> Option<u32> {
    if container.starts_with('-') || container.is_empty() {
        return None;
    }
    let mut command = Command::new("docker");
    command.args([
        "exec",
        container,
        "cat",
        "/ncd-login-assist/qq-process.json",
    ]);
    let bytes = run(command, &[], 4096).await.ok()?;
    let anchor: Value = serde_json::from_slice(&bytes).ok()?;
    let pid = u32::try_from(anchor.get("pid")?.as_u64()?).ok()?;
    let start = anchor.get("start_time")?.as_str()?;
    let generation = generation(Some(container), pid).await?;
    (generation.rsplit(':').next() == Some(start)).then_some(pid)
}

pub async fn run(mut command: Command, input: &[u8], max: u64) -> Result<Vec<u8>, String> {
    let (status, bytes) = run_status(&mut command, input, max).await?;
    if !status.success() {
        return Err("扫码辅助不可用".into());
    }
    Ok(bytes)
}

async fn run_status(
    command: &mut Command,
    input: &[u8],
    max: u64,
) -> Result<(std::process::ExitStatus, Vec<u8>), String> {
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let mut child = command.spawn().map_err(|_| "无法启动扫码辅助")?;
    let mut stdin = child.stdin.take().ok_or("辅助输入不可用")?;
    let stdout = child.stdout.take().ok_or("辅助输出不可用")?;
    let result = tokio::time::timeout(Duration::from_secs(8), async {
        stdin.write_all(input).await.map_err(|_| "辅助输入失败")?;
        drop(stdin);
        let mut bytes = Vec::new();
        stdout
            .take(max + 1)
            .read_to_end(&mut bytes)
            .await
            .map_err(|_| "辅助输出失败")?;
        if bytes.len() as u64 > max {
            return Err("辅助输出过大");
        }
        let status = child.wait().await.map_err(|_| "辅助进程失败")?;
        Ok::<_, &str>((status, bytes))
    })
    .await;
    match result {
        Ok(Ok(bytes)) => Ok(bytes),
        _ => {
            let _ = child.kill().await;
            let _ = child.wait().await;
            Err("扫码辅助不可用或已超时".into())
        }
    }
}

pub async fn process_absent(
    bot: &crate::config::NotifyBotTarget,
    target: Option<&crate::relogin_config::WatchLoginTarget>,
) -> Option<bool> {
    if bot.is_docker() {
        let container = bot.resolved_container_name();
        if container.is_empty() || container.starts_with('-') {
            return None;
        }
        let mut command = Command::new("docker");
        command.args(["inspect", "--format", "{{.State.Running}}", &container]);
        let bytes = run(command, &[], 128).await.ok()?;
        return match std::str::from_utf8(&bytes).ok()?.trim() {
            "true" => Some(false),
            "false" => Some(true),
            _ => None,
        };
    }
    if let Some(file) = target
        .and_then(|t| t.pid_file.as_deref())
        .or(bot.pid_file.as_deref())
    {
        if !tokio::fs::try_exists(file).await.ok()? {
            return Some(true);
        }
        let bytes = read_limited(file, 32).await.ok()?;
        let pid = std::str::from_utf8(&bytes)
            .ok()?
            .trim()
            .parse::<u32>()
            .ok()?;
        if pid == 0 {
            return None;
        }
        // 只有可读取的 procfs 才能把 PID 缺失视为停止，权限错误保持未知。
        if !tokio::fs::try_exists("/proc/self/stat").await.ok()? {
            return None;
        }
        return tokio::fs::try_exists(format!("/proc/{pid}/stat"))
            .await
            .ok()
            .map(|exists| !exists);
    }
    let pattern = bot.process_match.as_deref().filter(|p| !p.is_empty())?;
    let mut command = Command::new("pgrep");
    command.args(["-f", "--", pattern]);
    let (status, _) = run_status(&mut command, &[], 4096).await.ok()?;
    match status.code() {
        Some(0) => Some(false),
        Some(1) => Some(true),
        _ => None,
    }
}

pub async fn docker_port(container: &str, port: u16) -> Option<u16> {
    if container.starts_with('-') || container.is_empty() || port == 0 {
        return None;
    }
    let mut command = Command::new("docker");
    command.args([
        "inspect",
        "--format",
        &format!("{{{{(index (index .NetworkSettings.Ports \"{port}/tcp\") 0).HostPort}}}}"),
        container,
    ]);
    String::from_utf8(run(command, &[], 128).await.ok()?)
        .ok()?
        .trim()
        .parse()
        .ok()
}

fn helper(container: Option<&str>, uid: Option<u32>) -> Command {
    let mut command = if let Some(container) = container {
        let mut command = Command::new("docker");
        command.args(["exec", "-i"]);
        if let Some(uid) = uid {
            command.args(["--user", &uid.to_string()]);
        }
        command.args([container, "/usr/bin/python3"]);
        command
    } else {
        Command::new("python3")
    };
    command.args(["-c", HELPER]);
    command
}

pub async fn binding(container: Option<&str>, pid: u32, exe: &str) -> Result<Value, String> {
    if pid == 0 || !exe.starts_with('/') || container.is_some_and(|c| c.starts_with('-')) {
        return Err("QQ 进程绑定无效".into());
    }
    let uid = if let Some(container) = container {
        let mut cmd = Command::new("docker");
        cmd.args([
            "exec",
            container,
            "/usr/bin/stat",
            "-c",
            "%u",
            &format!("/proc/{pid}"),
        ]);
        Some(
            String::from_utf8(run(cmd, &[], 128).await?)
                .map_err(|_| "无效 UID")?
                .trim()
                .parse::<u32>()
                .map_err(|_| "无效 UID")?,
        )
    } else {
        None
    };
    let mut command = helper(container, uid);
    command.args(["--bind", "--pid", &pid.to_string(), "--exe", exe]);
    let bytes = run(command, &[], 8192).await?;
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| "QQ 绑定响应无效")?;
    let binding = value.get("binding").cloned().ok_or("缺少 QQ 绑定")?;
    if binding.get("pid").and_then(Value::as_u64) != Some(u64::from(pid))
        || binding.get("exe").and_then(Value::as_str) != Some(exe)
    {
        return Err("QQ 进程已改变".into());
    }
    Ok(binding)
}

pub async fn capture(
    container: Option<&str>,
    binding: &Value,
    refresh: bool,
) -> Result<SnowLumaQrLoginSnapshot, String> {
    let session = uuid::Uuid::new_v4().simple().to_string();
    let uid = binding
        .get("uid")
        .and_then(Value::as_u64)
        .and_then(|u| u32::try_from(u).ok())
        .ok_or("缺少 UID")?;
    let request = json!({"v":1,"session_id":session,"binding":binding,"action":if refresh {"refresh"} else {"snapshot"},"scanned":false});
    let input = serde_json::to_vec(&request).map_err(|_| "无法编码取码请求")?;
    let bytes = run(helper(container, Some(uid)), &input, 192 * 1024).await?;
    #[derive(Deserialize)]
    struct Reply {
        session_id: String,
        #[serde(flatten)]
        snapshot: SnowLumaQrLoginSnapshot,
    }
    let reply: Reply = serde_json::from_slice(&bytes).map_err(|_| "取码响应无效")?;
    if reply.session_id != session
        || reply.snapshot.v != 1
        || !reply.snapshot.observer_removed
        || reply.snapshot.qq_pid.map(u64::from) != binding.get("pid").and_then(Value::as_u64)
    {
        return Err("取码会话已改变".into());
    }
    Ok(reply.snapshot)
}
