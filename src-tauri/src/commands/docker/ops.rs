//! Docker 探测 / 列表 / 生命周期 / compose down

use ncd_deploy::docker::DockerCli;
use ncd_domain::{ContainerAction, ContainerInfo, DockerFlavor, DockerStatus, ImageInfo};
use ncd_host::{Host, HostCommand, HostPath};
use tauri::State;
use tracing::error;

use crate::AppState;
use crate::commands::host_resolve::resolve_host_with_autoconnect;

#[tauri::command]
pub async fn docker_probe(
    host_id: String,
    state: State<'_, AppState>,
) -> Result<DockerStatus, String> {
    let host = resolve_host_with_autoconnect(&host_id, &state).await?;
    Ok(DockerCli::new(host.as_ref()).probe().await)
}

#[tauri::command]
pub async fn docker_list_containers(
    host_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<ContainerInfo>, String> {
    let host = resolve_host_with_autoconnect(&host_id, &state).await?;
    let cli = DockerCli::new(host.as_ref());
    // 先 probe 定夺提权:远端没进 docker 组时裸 docker 会 permission denied,
    // ensure_daemon_ready 探一次让后续命令一致地走 sudo
    cli.ensure_daemon_ready()
        .await
        .map_err(|e| format!("Docker 未就绪: {e}"))?;
    cli.list_containers()
        .await
        .map_err(|e| format!("列容器失败: {e}"))
}

#[tauri::command]
pub async fn docker_list_images(
    host_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<ImageInfo>, String> {
    let host = resolve_host_with_autoconnect(&host_id, &state).await?;
    let cli = DockerCli::new(host.as_ref());
    cli.ensure_daemon_ready()
        .await
        .map_err(|e| format!("Docker 未就绪: {e}"))?;
    cli.list_images()
        .await
        .map_err(|e| format!("列镜像失败: {e}"))
}

#[tauri::command]
pub async fn docker_remove_image(
    host_id: String,
    image_ref: String,
    force: Option<bool>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let host = resolve_host_with_autoconnect(&host_id, &state).await?;
    let cli = DockerCli::new(host.as_ref());
    cli.ensure_daemon_ready()
        .await
        .map_err(|e| format!("Docker 未就绪: {e}"))?;
    let force = force.unwrap_or(false);
    cli.remove_image(image_ref.trim(), force)
        .await
        .map_err(|e| {
            error!(
                target: "ncd_tauri::docker",
                host_id = %host_id,
                image_ref = %image_ref,
                force,
                err = %e,
                "Docker 删除镜像失败"
            );
            format!("删除镜像失败: {e}")
        })
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
    cli.ensure_daemon_ready()
        .await
        .map_err(|e| format!("Docker 未就绪: {e}"))?;
    let result = match action {
        ContainerAction::Start => cli.lifecycle("start", &name).await,
        ContainerAction::Stop => cli.lifecycle("stop", &name).await,
        ContainerAction::Restart => cli.lifecycle("restart", &name).await,
        ContainerAction::Remove => cli.remove(&name).await,
    };
    result.map_err(|e| {
        error!(
            target: "ncd_tauri::docker",
            host_id = %host_id,
            container = %name,
            action = action.as_str(),
            err = %e,
            "Docker 容器操作失败"
        );
        format!("{} 失败: {e}", action.as_str())
    })
}

#[tauri::command]
pub async fn docker_logs(
    host_id: String,
    name: String,
    tail: u32,
    state: State<'_, AppState>,
) -> Result<String, String> {
    let host = resolve_host_with_autoconnect(&host_id, &state).await?;
    let cli = DockerCli::new(host.as_ref());
    cli.ensure_daemon_ready()
        .await
        .map_err(|e| format!("Docker 未就绪: {e}"))?;
    cli.logs(&name, tail.min(2000))
        .await
        .map_err(|e| format!("取日志失败: {e}"))
}

#[tauri::command]
pub async fn docker_image_ready_for_flavor(
    host_id: String,
    flavor: DockerFlavor,
    state: State<'_, AppState>,
) -> Result<bool, String> {
    let host = resolve_host_with_autoconnect(&host_id, &state).await?;
    let cli = DockerCli::new(host.as_ref());
    cli.ensure_daemon_ready()
        .await
        .map_err(|e| format!("Docker 未就绪: {e}"))?;
    let image = flavor.default_image();
    cli.image_exists(image)
        .await
        .map_err(|e| format!("探测镜像失败: {e}"))
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
    let cli = DockerCli::new(host_ref);
    // compose down 要用 compose 插件,走 ensure_ready(probe 定夺提权 + 要求 compose)
    cli.ensure_ready()
        .await
        .map_err(|e| format!("Docker 未就绪: {e}"))?;
    cli.compose_down(&project_dir, remove_volumes)
        .await
        .map_err(|e| {
            error!(
                target: "ncd_tauri::docker",
                host_id = %host_id,
                name = %name,
                err = %e,
                "Docker compose down 失败"
            );
            format!("停止部署失败: {e}")
        })
}

/// 解析 compose project 目录(放 docker-compose.yml 的地方)
/// 本机:<data_root>/docker/<name>(POSIX 化路径,LocalWindowsHost 内部转盘符)
/// 远端:<$HOME>/.napcat-docker/<name>;探不到 $HOME 时返回错误(不回退 /root)
async fn resolve_project_dir(
    host_id: &str,
    host: &dyn Host,
    state: &AppState,
    name: &str,
) -> Result<String, String> {
    if host_id == "local" {
        let base = state.data_root.join("docker").join(name);
        // HostPath::from_windows 把 C:\... 规范成 /c/...,LocalWindowsHost 能还原
        return Ok(HostPath::from_windows(&base.to_string_lossy())
            .as_posix()
            .to_string());
    }
    // 远端:探 $HOME探不到就 fail-fast,不回退 /root——路径落盘红线:宁可报错让
    // 用户处理,也不把生产数据静默落到错误目录(/root 通常无权限,或污染 root 家目录)
    let home = probe_remote_home(host).await.ok_or_else(|| {
        "无法探测远端 $HOME,已拒绝回退到 /root 部署 Docker(避免把数据落到错误目录)。\
         请确认远端 SSH 用户有正常的家目录后重试。"
            .to_string()
    })?;
    Ok(format!("{home}/.napcat-docker/{name}"))
}

/// 远端探 $HOME失败返回 None;调用方须 fail-fast,不得回退 /root
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
