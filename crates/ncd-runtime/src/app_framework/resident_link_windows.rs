//! Windows 跨主机对接：用户私钥留在应用机，ssh.exe 由代理按代数监督。
use super::*;
use ncd_domain::windows_remote::*;
use std::collections::BTreeMap;

const STATE_FILE: &str = "link.json";
#[derive(serde::Serialize, serde::Deserialize)]
struct SavedLink {
    v: u32,
    bot_host_id: String,
    port: u16,
    authorization: String,
    workload: WindowsWorkloadSpec,
}

fn workload_id(id: &AppInstanceId) -> String {
    format!("link-{}", id.as_str())
}
async fn directory(host: &dyn Host, id: &AppInstanceId) -> Result<HostPath, AppFrameworkError> {
    let info = host.remote_windows_info().await.map_err(host_err)?;
    let root = HostPath::from_windows(&info.user_root);
    if !root.is_absolute() {
        return Err(AppFrameworkError::Validation(
            "Windows 用户受管目录无效".into(),
        ));
    }
    Ok(root.join("links").join(id.as_str()))
}
async fn request(
    host: &dyn Host,
    action: WindowsAgentAction,
) -> Result<WindowsAgentResult, AppFrameworkError> {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let id = format!("link-{}-{nonce}", std::process::id());
    let response = host
        .windows_agent_request(WindowsAgentRequest {
            v: WINDOWS_AGENT_PROTOCOL,
            request_id: id.clone(),
            action,
        })
        .await
        .map_err(host_err)?;
    if response.v != WINDOWS_AGENT_PROTOCOL || response.request_id != id {
        return Err(AppFrameworkError::Runtime(
            "常驻隧道代理响应身份不匹配".into(),
        ));
    }
    Ok(response.result)
}
async fn status(
    host: &dyn Host,
    id: &AppInstanceId,
) -> Result<Option<WindowsWorkloadStatus>, AppFrameworkError> {
    match request(
        host,
        WindowsAgentAction::Status {
            instance_id: workload_id(id),
        },
    )
    .await?
    {
        WindowsAgentResult::Workload(status) => Ok(Some(status)),
        WindowsAgentResult::Error(error)
            if matches!(
                error.code.as_str(),
                "NotFound" | "WorkloadNotFound" | "not_found"
            ) =>
        {
            Ok(None)
        }
        WindowsAgentResult::Error(error) => Err(AppFrameworkError::Runtime(error.message)),
        _ => Err(AppFrameworkError::Runtime("常驻隧道状态响应无效".into())),
    }
}
async fn stop(host: &dyn Host, id: &AppInstanceId) -> Result<(), AppFrameworkError> {
    let Some(current) = status(host, id).await? else {
        return Ok(());
    };
    if matches!(
        current.state,
        WindowsWorkloadState::Stopped | WindowsWorkloadState::Failed
    ) {
        return Ok(());
    }
    stop_generation(host, id, &current.generation).await
}
async fn stop_generation(
    host: &dyn Host,
    id: &AppInstanceId,
    generation: &str,
) -> Result<(), AppFrameworkError> {
    match request(
        host,
        WindowsAgentAction::Stop {
            instance_id: workload_id(id),
            generation: generation.into(),
        },
    )
    .await?
    {
        WindowsAgentResult::Workload(_) => {}
        WindowsAgentResult::Error(error) => return Err(AppFrameworkError::Runtime(error.message)),
        _ => return Err(AppFrameworkError::Runtime("停止常驻隧道的响应无效".into())),
    }
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let Some(current) = status(host, id).await? else {
            return Ok(());
        };
        if current.generation != generation {
            return Err(AppFrameworkError::Runtime(
                "常驻隧道已更换代数，请刷新后重试".into(),
            ));
        }
        if matches!(
            current.state,
            WindowsWorkloadState::Stopped | WindowsWorkloadState::Failed
        ) {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(AppFrameworkError::Runtime(
                "常驻隧道仍在停止，请稍后重试".into(),
            ));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
async fn load(host: &dyn Host, dir: &HostPath) -> Result<Option<SavedLink>, AppFrameworkError> {
    let path = dir.join(STATE_FILE);
    if !host.exists(&path).await.map_err(host_err)? {
        return Ok(None);
    }
    let bytes = host.read_file(&path).await.map_err(host_err)?;
    let state: SavedLink = serde_json::from_slice(&bytes)
        .map_err(|e| AppFrameworkError::Runtime(format!("常驻隧道记录损坏：{e}")))?;
    if state.v != 1 {
        return Err(AppFrameworkError::Runtime("常驻隧道记录版本不兼容".into()));
    }
    Ok(Some(state))
}

fn command(
    ssh: String,
    dir: &HostPath,
    dial: &SshDialTarget,
    forward: ResidentForward,
    listen: u16,
    id: &AppInstanceId,
) -> WindowsWorkloadSpec {
    let tunnel = match forward {
        ResidentForward::ExposeApp { app_port } => {
            format!("127.0.0.1:{listen}:127.0.0.1:{app_port}")
        }
        ResidentForward::ReachBot { bot_port } => {
            format!("127.0.0.1:{listen}:127.0.0.1:{bot_port}")
        }
        ResidentForward::BridgeLoopback {
            listen_host,
            target_port,
        } => {
            format!("{listen_host}:{listen}:127.0.0.1:{target_port}")
        }
    };
    let mut args: Vec<String> = vec!["-i".into(), dir.join(LINK_KEY_NAME).render_for(Os::Windows)];
    for option in [
        "IdentitiesOnly=yes",
        "ExitOnForwardFailure=yes",
        "ServerAliveInterval=30",
        "ServerAliveCountMax=3",
        "ConnectTimeout=15",
        "StrictHostKeyChecking=yes",
        "GlobalKnownHostsFile=NUL",
        "BatchMode=yes",
    ] {
        args.extend(["-o".into(), option.into()]);
    }
    let known = dir
        .join(LINK_KNOWN_HOSTS)
        .render_for(Os::Windows)
        .replace('\\', "/");
    args.extend([
        "-o".into(),
        format!("UserKnownHostsFile=\"{known}\""),
        "-N".into(),
        match forward {
            ResidentForward::ExposeApp { .. } => "-R",
            ResidentForward::ReachBot { .. } | ResidentForward::BridgeLoopback { .. } => "-L",
        }
        .into(),
        tunnel,
        "-p".into(),
        dial.port.to_string(),
        "--".into(),
        format!("{}@{}", dial.username, dial.host),
    ]);
    WindowsWorkloadSpec {
        instance_id: workload_id(id),
        command: WindowsCommandSpec {
            program: ssh,
            args,
            working_dir: Some(dir.render_for(Os::Windows)),
            environment: BTreeMap::new(),
            stdin: None,
            elevated: false,
        },
        interactive: false,
        service_owned: false,
        session_id: None,
        restart_on_failure: true,
    }
}

async fn launch(
    host: &dyn Host,
    id: &AppInstanceId,
    workload: &WindowsWorkloadSpec,
) -> Result<String, AppFrameworkError> {
    let mut result = request(host, WindowsAgentAction::Launch(workload.clone())).await?;
    let generation = match &result {
        WindowsAgentResult::Workload(status) if status.instance_id == workload.instance_id => {
            status.generation.clone()
        }
        WindowsAgentResult::Error(error) => {
            return Err(AppFrameworkError::Runtime(error.message.clone()));
        }
        _ => return Err(AppFrameworkError::Runtime("启动常驻隧道响应无效".into())),
    };
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    let outcome = async {
        loop {
            match result {
                WindowsAgentResult::Workload(status)
                    if status.generation != generation
                        || status.instance_id != workload.instance_id =>
                {
                    return Err(AppFrameworkError::Runtime(
                        "常驻隧道已更换代数，请刷新后重试".into(),
                    ));
                }
                WindowsAgentResult::Workload(status)
                    if status.state == WindowsWorkloadState::Running && status.pid.is_some() =>
                {
                    return Ok(generation.clone());
                }
                WindowsAgentResult::Workload(status)
                    if status.state == WindowsWorkloadState::Starting
                        && tokio::time::Instant::now() < deadline => {}
                WindowsAgentResult::Workload(status) => {
                    return Err(AppFrameworkError::Runtime(status.error.unwrap_or_else(
                        || format!("常驻隧道未就绪（{:?}）", status.state),
                    )));
                }
                WindowsAgentResult::Error(error) => {
                    return Err(AppFrameworkError::Runtime(error.message));
                }
                _ => return Err(AppFrameworkError::Runtime("启动常驻隧道响应无效".into())),
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
            result = request(
                host,
                WindowsAgentAction::Status {
                    instance_id: workload.instance_id.clone(),
                },
            )
            .await?;
        }
    }
    .await;
    if let Err(error) = outcome {
        stop_generation(host, id, &generation)
            .await
            .map_err(|cleanup| {
                AppFrameworkError::Runtime(format!("{error}；取消本次隧道失败：{cleanup}"))
            })?;
        return Err(error);
    }
    outcome
}

pub(super) async fn ensure(
    app_host: &dyn Host,
    bot_host: &dyn Host,
    spec: &ResidentLinkSpec<'_>,
) -> Result<u16, AppFrameworkError> {
    if matches!(spec.forward, ResidentForward::BridgeLoopback { .. }) {
        return Err(AppFrameworkError::Validation(
            "Docker 网桥仅支持 Linux 宿主机".into(),
        ));
    }
    if matches!(
        spec.forward,
        ResidentForward::ExposeApp { app_port: 0 } | ResidentForward::ReachBot { bot_port: 0 }
    ) {
        return Err(AppFrameworkError::Validation(
            "常驻隧道目标端口不能为 0".into(),
        ));
    }
    let dial = bot_host
        .ssh_dial_target()
        .ok_or_else(|| AppFrameworkError::Validation("Bot 所在机没有 SSH 拨号身份".into()))?;
    let ssh = app_host
        .which("ssh.exe")
        .await
        .map_err(host_err)?
        .ok_or_else(|| {
            AppFrameworkError::Validation("请在 Windows 应用所在机安装 OpenSSH 客户端".into())
        })?;
    let dir = directory(app_host, &spec.instance.id).await?;
    let previous = load(app_host, &dir).await?;
    if previous
        .as_ref()
        .is_some_and(|old| old.bot_host_id != bot_host.id())
    {
        return Err(AppFrameworkError::Validation(
            "请先解除旧主机对接，再连接新的 Bot 主机".into(),
        ));
    }
    let known = trusted_known_hosts(bot_host, &dial)?;
    let listen_host = match spec.forward {
        ResidentForward::ExposeApp { .. } => bot_host,
        ResidentForward::ReachBot { .. } | ResidentForward::BridgeLoopback { .. } => app_host,
    };
    let port = allocate_forward_port(
        listen_host,
        spec.reuse_port
            .or_else(|| previous.as_ref().map(|old| old.port)),
        spec.taken_ports,
    )
    .await?;
    app_host.create_dir_all(&dir).await.map_err(host_err)?;
    ensure_ed25519_key(app_host, &dir, &spec.instance.id).await?;
    app_host
        .write_file(&dir.join(LINK_KNOWN_HOSTS), known.as_bytes())
        .await
        .map_err(host_err)?;
    let public = app_host
        .read_file(&dir.join(LINK_KEY_PUB))
        .await
        .map_err(host_err)?;
    let authorization = authorized_keys_line_for_host(
        bot_host,
        &String::from_utf8_lossy(&public),
        &spec.instance.id,
        spec.forward,
        port,
    )
    .map_err(AppFrameworkError::Validation)?;
    let workload = command(ssh, &dir, &dial, spec.forward, port, &spec.instance.id);
    let current = status(app_host, &spec.instance.id).await?;
    let unchanged = previous
        .as_ref()
        .is_some_and(|old| old.workload == workload && old.authorization == authorization);
    if unchanged && current.as_ref().is_some_and(|state| state.is_running()) {
        return Ok(port);
    }
    let previous_was_running = current.as_ref().is_some_and(|state| {
        state.is_running() || state.state == WindowsWorkloadState::WaitingForUserSession
    });
    if previous.is_none() && previous_was_running {
        return Err(AppFrameworkError::Runtime(
            "发现没有所属记录的常驻隧道，请先解除对接再重试".into(),
        ));
    }
    upsert_bot_authorized_keys(bot_host, &spec.instance.id, &authorization).await?;
    if previous_was_running {
        if let Some(current) = current {
            if let Err(error) =
                stop_generation(app_host, &spec.instance.id, &current.generation).await
            {
                if let Some(previous) = &previous {
                    upsert_bot_authorized_keys(
                        bot_host,
                        &spec.instance.id,
                        &previous.authorization,
                    )
                    .await
                    .map_err(|restore| {
                        AppFrameworkError::Runtime(format!("{error}；恢复原授权失败：{restore}"))
                    })?;
                }
                return Err(error);
            }
        }
    }
    let next = SavedLink {
        v: 1,
        bot_host_id: bot_host.id().into(),
        port,
        authorization,
        workload,
    };
    let mut launched_generation = None;
    let result = async {
        launched_generation = Some(launch(app_host, &spec.instance.id, &next.workload).await?);
        save(app_host, &dir, &next).await
    }
    .await;
    if let Err(error) = result {
        if let Some(generation) = launched_generation {
            stop_generation(app_host, &spec.instance.id, &generation)
                .await
                .map_err(|cleanup| {
                    AppFrameworkError::Runtime(format!("{error}；停止本次隧道失败：{cleanup}"))
                })?;
        }
        if let Some(previous) = previous {
            upsert_bot_authorized_keys(bot_host, &spec.instance.id, &previous.authorization)
                .await?;
            save(app_host, &dir, &previous).await?;
            if previous_was_running {
                launch(app_host, &spec.instance.id, &previous.workload)
                    .await
                    .map_err(|restore| {
                        AppFrameworkError::Runtime(format!("{error}；恢复原隧道失败：{restore}"))
                    })?;
            }
        } else {
            strip_bot_authorized_key(bot_host, &spec.instance.id).await?;
        }
        return Err(error);
    }
    Ok(port)
}

async fn save(host: &dyn Host, dir: &HostPath, link: &SavedLink) -> Result<(), AppFrameworkError> {
    let bytes = serde_json::to_vec(link).map_err(|e| AppFrameworkError::Runtime(e.to_string()))?;
    let staged = dir.join("link.next.json");
    host.write_file(&staged, &bytes).await.map_err(host_err)?;
    host.rename(&staged, &dir.join(STATE_FILE))
        .await
        .map_err(host_err)
}

pub(super) async fn teardown(
    app_host: &dyn Host,
    bot_host: Option<&dyn Host>,
    id: &AppInstanceId,
) -> Result<(), AppFrameworkError> {
    stop(app_host, id).await?;
    if let Some(bot_host) = bot_host {
        strip_bot_authorized_key(bot_host, id).await?;
    }
    let dir = directory(app_host, id).await?;
    if app_host.exists(&dir).await.map_err(host_err)? {
        app_host.remove_dir_all(&dir).await.map_err(host_err)?;
    }
    Ok(())
}

pub(super) async fn reconcile(
    app_host: &dyn Host,
    bot_host: &dyn Host,
    instance: &AppInstance,
    forward: ResidentForward,
    port: u16,
) -> Result<(), AppFrameworkError> {
    ensure(
        app_host,
        bot_host,
        &ResidentLinkSpec {
            instance,
            reuse_port: Some(port),
            taken_ports: &[],
            forward,
        },
    )
    .await
    .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tunnel_spec_never_exposes_a_public_listener_or_bypasses_host_key_checking() {
        let dial = SshDialTarget {
            host: "example.invalid".into(),
            port: 2222,
            username: "owner".into(),
        };
        let workload = command(
            r"C:\Windows\System32\OpenSSH\ssh.exe".into(),
            &HostPath::from_windows(
                r"C:\ProgramData\NapCatQQ Desktop\remote\users\owner\links\one",
            ),
            &dial,
            ResidentForward::ExposeApp { app_port: 3000 },
            4000,
            &AppInstanceId::new("one"),
        );
        assert!(
            workload
                .command
                .args
                .contains(&"127.0.0.1:4000:127.0.0.1:3000".into())
        );
        assert!(
            workload
                .command
                .args
                .contains(&"StrictHostKeyChecking=yes".into())
        );
        assert!(workload.command.args.contains(&"BatchMode=yes".into()));
        assert!(workload.command.args.contains(&r#"UserKnownHostsFile="C:/ProgramData/NapCatQQ Desktop/remote/users/owner/links/one/known_hosts""#.into()));
        assert!(!workload.service_owned);
        assert!(workload.restart_on_failure);
    }
}
