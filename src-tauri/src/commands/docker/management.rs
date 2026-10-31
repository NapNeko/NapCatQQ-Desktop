//! Docker 详情与网络、数据卷管理命令。

use ncd_deploy::docker::DockerCli;
use ncd_domain::docker::ContainerEngine;
use ncd_domain::docker::{
    DockerContainerDetails, DockerContainerStats, DockerNetworkInfo, DockerResourceKind,
    DockerVolumeInfo,
};
use tauri::State;

use crate::{AppState, commands::host_resolve::resolve_host_with_autoconnect};

#[tauri::command]
pub async fn docker_container_details(
    host_id: String,
    id: String,
    engine: Option<ContainerEngine>,
    state: State<'_, AppState>,
) -> Result<DockerContainerDetails, String> {
    let host = resolve_host_with_autoconnect(&host_id, &state).await?;
    let cli = DockerCli::for_engine(host.as_ref(), engine);
    cli.ensure_daemon_ready().await.map_err(|e| e.to_string())?;
    cli.container_details(&id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn docker_container_stats(
    host_id: String,
    id: String,
    engine: Option<ContainerEngine>,
    state: State<'_, AppState>,
) -> Result<DockerContainerStats, String> {
    let host = resolve_host_with_autoconnect(&host_id, &state).await?;
    let cli = DockerCli::for_engine(host.as_ref(), engine);
    cli.ensure_daemon_ready().await.map_err(|e| e.to_string())?;
    cli.container_stats(&id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn docker_list_networks(
    host_id: String,
    engine: Option<ContainerEngine>,
    state: State<'_, AppState>,
) -> Result<Vec<DockerNetworkInfo>, String> {
    let host = resolve_host_with_autoconnect(&host_id, &state).await?;
    let cli = DockerCli::for_engine(host.as_ref(), engine);
    cli.ensure_daemon_ready().await.map_err(|e| e.to_string())?;
    cli.list_networks().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn docker_list_volumes(
    host_id: String,
    engine: Option<ContainerEngine>,
    state: State<'_, AppState>,
) -> Result<Vec<DockerVolumeInfo>, String> {
    let host = resolve_host_with_autoconnect(&host_id, &state).await?;
    let cli = DockerCli::for_engine(host.as_ref(), engine);
    cli.ensure_daemon_ready().await.map_err(|e| e.to_string())?;
    cli.list_volumes().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn docker_remove_resource(
    host_id: String,
    kind: DockerResourceKind,
    id: String,
    engine: Option<ContainerEngine>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let host = resolve_host_with_autoconnect(&host_id, &state).await?;
    let cli = DockerCli::for_engine(host.as_ref(), engine);
    cli.ensure_daemon_ready().await.map_err(|e| e.to_string())?;
    cli.remove_resource(kind, &id)
        .await
        .map_err(|e| e.to_string())
}
