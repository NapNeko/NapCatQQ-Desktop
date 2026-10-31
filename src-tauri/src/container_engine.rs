//! 容器引擎选择落在 Desktop 配置中，Host 的稳定 ID 同时用于本机与远端。
use crate::AppState;
use crate::commands::host_resolve::resolve_host_with_autoconnect;
use ncd_deploy::docker::engine::{self, ContainerEngine};
use ncd_traits::ConfigStore;
use std::{collections::HashMap, path::Path, sync::Mutex};
use tauri::State;
static WRITE_LOCK: Mutex<()> = Mutex::new(());

pub fn initialize(root: &Path) {
    let path = root.join("config/container-engines.json");
    if !path.exists() {
        return;
    }
    let store = ncd_runtime::LocalConfigStore::new(root);
    match store.read_json(&path).and_then(|value| {
        serde_json::from_value::<HashMap<String, ContainerEngine>>(value)
            .map_err(|e| ncd_domain::ConfigError::Json(e.to_string()))
    }) {
        Ok(value) => engine::replace_preferences(value),
        Err(error) => tracing::warn!(%error, "读取容器引擎选择失败"),
    }
}

#[tauri::command]
pub async fn container_engine_get(
    host_id: String,
    state: State<'_, AppState>,
) -> Result<ContainerEngine, String> {
    let host = resolve_host_with_autoconnect(&host_id, &state).await?;
    Ok(engine::engine_for(host.as_ref()))
}

#[tauri::command]
pub async fn container_engine_set(
    host_id: String,
    engine: ContainerEngine,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let host = resolve_host_with_autoconnect(&host_id, &state).await?;
    if host.os() != ncd_host::Os::Linux {
        return Err("容器引擎选择仅支持 Linux 主机".into());
    }
    if state
        .bot_manager
        .list_bot_configs()
        .await
        .map_err(|e| e.to_string())?
        .iter()
        .any(|c| {
            c.bot.deployment_type == ncd_domain::DeploymentType::Docker
                && match &c.bot.runtime_target {
                    ncd_domain::RuntimeTarget::Local => host_id == "local",
                    ncd_domain::RuntimeTarget::Server(id) => host_id == format!("remote:{id}"),
                }
        })
    {
        return Err("这台主机已有容器 Bot 配置，请先迁移或移除这些配置再切换引擎，避免运行中的 Bot 失去管理。".into());
    }
    {
        let _guard = WRITE_LOCK.lock().map_err(|e| e.to_string())?;
        let mut value = engine::preferences();
        value.insert(host.id().to_owned(), engine);
        let store = ncd_runtime::LocalConfigStore::new(&state.data_root);
        store
            .write_json_atomic(
                &state.data_root.join("config/container-engines.json"),
                &serde_json::to_value(&value).map_err(|e| e.to_string())?,
            )
            .map_err(|e| e.to_string())?;
        engine::replace_preferences(value);
    }
    if let Some(server_id) = host_id.strip_prefix("remote:") {
        state.components.inventory().invalidate(server_id).await;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn restores_per_host_engine_choices_from_disk() {
        let root = tempfile::tempdir().unwrap();
        let original = engine::preferences();
        let mut configured = original.clone();
        configured.insert("test-podman-host".into(), ContainerEngine::Podman);
        configured.insert("test-docker-host".into(), ContainerEngine::Docker);
        let store = ncd_runtime::LocalConfigStore::new(root.path());
        store
            .write_json_atomic(
                &root.path().join("config/container-engines.json"),
                &serde_json::to_value(&configured).unwrap(),
            )
            .unwrap();
        initialize(root.path());
        assert_eq!(
            engine::preferences().get("test-podman-host"),
            Some(&ContainerEngine::Podman)
        );
        assert_eq!(
            engine::preferences().get("test-docker-host"),
            Some(&ContainerEngine::Docker)
        );
        engine::replace_preferences(original);
    }
}
