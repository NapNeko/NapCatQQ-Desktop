//! QQ 系统依赖 detect / install 命令薄壳;runner 在 ncd-runtime::components::system_package

use ncd_component::qq_deps::{QqDependencyDetector, qq_qqnt_dependencies_v3_2_25};
use ncd_domain::{InstallDependenciesResult, QqDependencyReport};
use ncd_host::Os;
use tauri::State;

use super::executor;
use crate::AppState;
use crate::commands::host_resolve::resolve_host_with_autoconnect;

/// 检测 QQ 系统依赖(仅 Linux 远端)
#[tauri::command]
pub async fn detect_qq_dependencies(
    host_id: String,
    state: State<'_, AppState>,
) -> Result<QqDependencyReport, String> {
    let host = resolve_host_with_autoconnect(&host_id, &state).await?;
    if host.os() != Os::Linux {
        return Err("QQ dependencies check is only supported on Linux".to_string());
    }
    QqDependencyDetector::new(qq_qqnt_dependencies_v3_2_25())
        .detect(host.as_ref(), None)
        .await
        .map_err(|e| e.to_string())
}

/// 安装 QQ 系统依赖(仅 Linux 远端)
#[tauri::command]
pub async fn install_qq_dependencies(
    host_id: String,
    packages: Vec<String>,
    sudo_password: Option<String>,
    state: State<'_, AppState>,
) -> Result<InstallDependenciesResult, String> {
    let host = resolve_host_with_autoconnect(&host_id, &state).await?;
    if host.os() != Os::Linux {
        return Err("QQ dependencies installation is only supported on Linux".to_string());
    }
    executor(&state)
        .install_qq_dependencies(&host_id, host, packages, sudo_password)
        .await
}

/// 记住远端服务器的 sudo 密码(用于提权操作)
#[tauri::command]
pub async fn remember_sudo_password(
    server_id: String,
    password: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state
        .server_manager
        .remember_sudo_password(&server_id, &password)?;
    Ok(())
}
