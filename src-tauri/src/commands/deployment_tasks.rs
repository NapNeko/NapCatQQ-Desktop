use ncd_domain::DeploymentTaskList;
use tauri::State;

use crate::AppState;

#[tauri::command]
pub async fn list_deployment_tasks(
    state: State<'_, AppState>,
) -> Result<DeploymentTaskList, String> {
    Ok(state.deployment_tasks.list().await)
}

#[tauri::command]
pub async fn cancel_deployment_task(
    task_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.deployment_tasks.cancel(&task_id).await
}
