//! Docker 页 Tauri 命令薄壳层。
//!
//! 暴露给前端的命令:
//! - docker_probe:探测某主机的 docker 状态
//! - docker_install:缺 docker 时帮装(Linux 脚本 / Windows 引导)
//! - docker_list_containers:列已有容器
//! - docker_container_action:对单个容器 start/stop/restart/remove
//! - docker_logs:取容器最近日志
//! - docker_deploy:一键部署 NapCat/SnowLuma(生成凭据 + compose up + 回读地址)
//! - docker_compose_down:停并清理一个 compose 部署
//!
//! 所有命令走 host_resolve 选主机(local 或 remote:<id>),错误统一转 String。
//! 业务编排尽量薄;真正的 docker 操作在 ncd_deploy::docker::DockerCli。

use ncd_deploy::docker::{install_docker, render_compose, DockerCli};
use ncd_domain::{
    ContainerAction, ContainerInfo, DeployedContainer, DockerDeploySpec, DockerFlavor, DockerStatus,
};
use ncd_host::{Host, HostCommand, HostPath};
use tauri::State;

use crate::AppState;
use crate::commands::host_resolve::{host_display_address, resolve_host_with_autoconnect};

#[tauri::command]
pub async fn docker_probe(
    host_id: String,
    state: State<'_, AppState>,
) -> Result<DockerStatus, String> {
    let host = resolve_host_with_autoconnect(&host_id, &state).await?;
    Ok(DockerCli::new(host.as_ref()).probe().await)
}

#[tauri::command]
pub async fn docker_install(
    host_id: String,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let host = resolve_host_with_autoconnect(&host_id, &state).await?;
    let outcome = install_docker(host.as_ref())
        .await
        .map_err(|e| format!("docker 安装失败: {e}"))?;
    // 把结果塌缩成一句给前端 toast 的人话。
    use ncd_deploy::docker::DockerInstallOutcome::*;
    let msg = match outcome {
        AlreadyInstalled { version } => format!("docker 已就绪（{version}）"),
        Installed => "docker 安装完成".to_string(),
        ManualRequired { reason, download_url } => match download_url {
            Some(url) => format!("需要手动安装: {reason}（下载: {url}）"),
            None => format!("需要手动安装: {reason}"),
        },
    };
    Ok(msg)
}

#[tauri::command]
pub async fn docker_list_containers(
    host_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<ContainerInfo>, String> {
    let host = resolve_host_with_autoconnect(&host_id, &state).await?;
    DockerCli::new(host.as_ref())
        .list_containers()
        .await
        .map_err(|e| format!("列容器失败: {e}"))
}

#[tauri::command]
pub async fn docker_container_action(
    host_id: String,
    name: String,
    action: ContainerAction,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let host = resolve_host_with_autoconnect(&host_id, &state).await?;
    let cli = DockerCli::new(host.as_ref());
    let result = match action {
        ContainerAction::Start => cli.lifecycle("start", &name).await,
        ContainerAction::Stop => cli.lifecycle("stop", &name).await,
        ContainerAction::Restart => cli.lifecycle("restart", &name).await,
        ContainerAction::Remove => cli.remove(&name).await,
    };
    result.map_err(|e| format!("{} 失败: {e}", action.as_str()))
}

#[tauri::command]
pub async fn docker_logs(
    host_id: String,
    name: String,
    tail: u32,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let host = resolve_host_with_autoconnect(&host_id, &state).await?;
    DockerCli::new(host.as_ref())
        .logs(&name, tail.min(2000))
        .await
        .map_err(|e| format!("取日志失败: {e}"))
}

#[tauri::command]
pub async fn docker_deploy(
    host_id: String,
    spec: DockerDeploySpec,
    state: State<'_, AppState>,
) -> Result<DeployedContainer, String> {
    spec.validate().map_err(|e| format!("部署参数非法: {e}"))?;
    let host = resolve_host_with_autoconnect(&host_id, &state).await?;
    let host_ref: &dyn Host = host.as_ref();

    let cli = DockerCli::new(host_ref);
    let status = cli.probe().await;
    if !status.ready_to_deploy() {
        return Err(format!(
            "docker 未就绪（installed={} daemon={} compose={}），请先安装/启动 docker",
            status.installed, status.daemon_running, status.compose_available
        ));
    }

    // 凭据:NapCat 当 WEBUI_TOKEN,SnowLuma 当 VNC_PASSWD。用 uuid v4 去掉横线
    // 当随机串,够强且无需额外依赖。
    let secret = uuid::Uuid::new_v4().simple().to_string();

    // compose project 目录:每个容器一个独立子目录,放它的 docker-compose.yml。
    // 本机走 data_root/docker/<name>;远端走 $HOME/.napcat-docker/<name>。
    let project_dir = resolve_project_dir(&host_id, host_ref, &state, &spec.container_name).await?;
    host.create_dir_all(&HostPath::from_posix(&project_dir))
        .await
        .map_err(|e| format!("创建部署目录失败: {e}"))?;

    // 渲染并写 compose 文件。uid/gid 在远端 Linux 用 1000 兜底(普通用户),
    // 本机 Windows 用 0(Docker Desktop 不在意属主)。
    let (uid, gid) = default_uid_gid(host_ref);
    let yaml = render_compose(&spec, &secret, uid, gid);
    let compose_path = HostPath::from_posix(format!("{project_dir}/docker-compose.yml"));
    host.write_file(&compose_path, yaml.as_bytes())
        .await
        .map_err(|e| format!("写 compose 文件失败: {e}"))?;

    // 拉镜像 + 起容器。compose_up 自带 --pull missing,这里显式 pull 让首次
    // 部署的网络耗时更可预期(失败信息也更清楚)。
    cli.pull(spec.flavor.default_image())
        .await
        .map_err(|e| format!("拉取镜像失败: {e}"))?;
    cli.compose_up(&project_dir)
        .await
        .map_err(|e| format!("启动容器失败: {e}"))?;

    // 回读:拼 WebUI / noVNC 地址,SnowLuma 的密码尝试从日志 grep。
    let address = host_display_address(&host_id, &state).await;
    Ok(build_deployed(&spec, &secret, &address, host_ref).await)
}

#[tauri::command]
pub async fn docker_compose_down(
    host_id: String,
    name: String,
    remove_volumes: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let host = resolve_host_with_autoconnect(&host_id, &state).await?;
    let host_ref: &dyn Host = host.as_ref();
    let project_dir = resolve_project_dir(&host_id, host_ref, &state, &name).await?;
    DockerCli::new(host_ref)
        .compose_down(&project_dir, remove_volumes)
        .await
        .map_err(|e| format!("停止部署失败: {e}"))
}

/// 解析 compose project 目录(放 docker-compose.yml 的地方)。
/// 本机:<data_root>/docker/<name>(POSIX 化路径,LocalWindowsHost 内部转盘符)。
/// 远端:<$HOME>/.napcat-docker/<name>;探不到 $HOME 时退回 /root 下。
async fn resolve_project_dir(
    host_id: &str,
    host: &dyn Host,
    state: &AppState,
    name: &str,
) -> Result<String, String> {
    if host_id == "local" {
        let base = state.data_root.join("docker").join(name);
        // HostPath::from_windows 把 C:\... 规范成 /c/...,LocalWindowsHost 能还原。
        return Ok(HostPath::from_windows(&base.to_string_lossy()).as_posix().to_string());
    }
    // 远端:探 $HOME。
    let home = probe_remote_home(host).await.unwrap_or_else(|| "/root".to_string());
    Ok(format!("{home}/.napcat-docker/{name}"))
}

/// 远端探 $HOME。失败返回 None,调用方兜底 /root。
async fn probe_remote_home(host: &dyn Host) -> Option<String> {
    let cmd = HostCommand::new("sh").arg("-c").arg("echo $HOME");
    match host.run_to_string(cmd).await {
        Ok(out) if out.success() => {
            let h = out.stdout.trim().to_string();
            if h.is_empty() { None } else { Some(h) }
        }
        _ => None,
    }
}

/// 默认文件属主。远端 Linux 普通用户一般是 1000;本机 Windows Docker Desktop
/// 不在意,给 0。
fn default_uid_gid(host: &dyn Host) -> (u32, u32) {
    match host.os() {
        ncd_host::Os::Linux => (1000, 1000),
        _ => (0, 0),
    }
}

/// 拼部署结果。NapCat 的 WebUI token 就是我们设的 secret;SnowLuma 的 WebUI
/// 密码由容器首启随机生成并打日志,这里尝试 grep 一次拿到(拿不到留 None,
/// 前端提示去看 noVNC / docker logs)。
async fn build_deployed(
    spec: &DockerDeploySpec,
    secret: &str,
    address: &str,
    host: &dyn Host,
) -> DeployedContainer {
    // 找某个容器端口在宿主机上映射到哪个端口,拿来拼 URL。找不到用容器端口兜底。
    let host_port = |container_port: u16| -> u16 {
        spec.ports
            .iter()
            .find(|p| p.container == container_port)
            .map(|p| p.host)
            .unwrap_or(container_port)
    };

    match spec.flavor {
        DockerFlavor::NapCat => {
            let webui = host_port(6099);
            DeployedContainer {
                name: spec.container_name.clone(),
                flavor: DockerFlavor::NapCat,
                webui_url: format!("http://{address}:{webui}/webui"),
                novnc_url: None,
                // NapCat 的 token 就是我们设进 WEBUI_TOKEN 的值。
                webui_secret: Some(secret.to_string()),
            }
        }
        DockerFlavor::SnowLuma => {
            let webui = host_port(5099);
            let novnc = host_port(6081);
            let password = grep_snowluma_password(host, &spec.container_name).await;
            DeployedContainer {
                name: spec.container_name.clone(),
                flavor: DockerFlavor::SnowLuma,
                webui_url: format!("http://{address}:{webui}/"),
                novnc_url: Some(format!("http://{address}:{novnc}/")),
                webui_secret: password,
            }
        }
    }
}

/// 从 SnowLuma 容器日志里 grep 首启随机密码。容器刚起来日志可能还没出密码行,
/// 拿不到返回 None,不阻塞部署成功。
async fn grep_snowluma_password(host: &dyn Host, container: &str) -> Option<String> {
    let cli = DockerCli::new(host);
    let logs = cli.logs(container, 400).await.ok()?;
    for line in logs.lines() {
        // SnowLuma 镜像打的是 "临时密码: xxxx" 或 "initial credentials: user=admin password=xxxx"。
        if let Some(idx) = line.find("临时密码:") {
            let tail = line[idx + "临时密码:".len()..].trim();
            let pw = tail.split_whitespace().next().unwrap_or("").trim();
            if !pw.is_empty() {
                return Some(pw.to_string());
            }
        }
        if let Some(idx) = line.find("password=") {
            let tail = line[idx + "password=".len()..].trim();
            let pw = tail.split_whitespace().next().unwrap_or("").trim();
            if !pw.is_empty() {
                return Some(pw.to_string());
            }
        }
    }
    None
}
