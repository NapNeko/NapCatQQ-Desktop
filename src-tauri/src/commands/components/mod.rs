//! Components 页 Tauri 命令薄壳层
//!
//! 依赖闭包 / 任务编排 / 构建输入都在 ncd-runtime::ComponentExecutor;这里只做 host 解析、
//! 转参数、错误转 String。

pub mod qq_deps;

use ncd_component::{
    ComponentDetectResult, ComponentId, ComponentInfo, DependencyPlan, DetectOutcome,
    RequirementPhase, RuntimeReadiness,
};
use ncd_deploy::StepKind;
use ncd_domain::{NodeEnvironmentCandidate, NodeProbeResult};
use ncd_host::{Host, HostPath, local::LocalWindowsHost};
use ncd_runtime::{
    ComponentActionRequest, ComponentBuildInputs, ComponentExecutor, RemoteHostProbe,
    RemoteSelectedPaths, SnowLumaLinuxPackage, component_catalog,
};
use tauri::State;

use crate::AppState;
use crate::commands::host_resolve::resolve_host_with_autoconnect;

#[tauri::command]
pub async fn list_components() -> Vec<ComponentInfo> {
    component_catalog()
}

#[tauri::command]
pub async fn detect_component(
    component_id: ComponentId,
    host_id: String,
    state: State<'_, AppState>,
) -> Result<ComponentDetectResult, String> {
    let host = resolve_host_with_autoconnect(&host_id, &state).await?;
    let component = state
        .components
        .inputs_for(&host_id, host.as_ref(), None)
        .await
        .build(component_id, host.as_ref())?;
    let host_ref: &dyn Host = host.as_ref();

    if component.check_target(host_ref).is_err() {
        return Ok(ComponentDetectResult {
            component_id,
            host_id,
            detected: None,
            unusable: None,
            supported: false,
        });
    }

    let (detected, unusable) = match component.detect_outcome(host_ref).await {
        Ok(DetectOutcome::Installed(v)) => (Some(v), None),
        Ok(DetectOutcome::Unusable(u)) => (None, Some(u)),
        Ok(DetectOutcome::NotInstalled) => (None, None),
        Err(err) => return Err(format!("detect failed: {err}")),
    };
    Ok(ComponentDetectResult {
        component_id,
        host_id,
        detected,
        unusable,
        supported: true,
    })
}

#[tauri::command]
pub async fn run_component_action(
    component_id: ComponentId,
    host_id: String,
    kind: StepKind,
    task_id: Option<String>,
    snowluma_linux_package: Option<SnowLumaLinuxPackage>,
    state: State<'_, AppState>,
) -> Result<String, String> {
    ensure_host_idle_for_component_mutation(&host_id, kind, &state).await?;
    let host = resolve_host_with_autoconnect(&host_id, &state).await?;
    // 没显式选包时:本机取设置里记的 / 按目录推断,远端由 factory 按库存推断
    let inputs = state
        .components
        .inputs_for(&host_id, host.as_ref(), snowluma_linux_package)
        .await;

    state
        .components
        .submit(ComponentActionRequest {
            component_id,
            host_id,
            kind,
            task_id: task_id.filter(|s| !s.trim().is_empty()),
            host,
            inputs,
        })
        .await
}

#[tauri::command]
pub async fn cancel_component_action(
    task_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.components.cancel(&task_id).await
}

/// 更新 / 卸载会动宿主树或框架目录;对应机器上仍有 Bot 在跑时直接拒绝
async fn ensure_host_idle_for_component_mutation(
    host_id: &str,
    kind: StepKind,
    state: &AppState,
) -> Result<(), String> {
    let action = match kind {
        StepKind::Update => "更新",
        StepKind::Uninstall => "卸载",
        _ => return Ok(()),
    };
    let active = state
        .bot_manager
        .count_active_bots_on_component_host(host_id)
        .await
        .map_err(|e| e.to_string())?;
    if active == 0 {
        return Ok(());
    }
    Err(format!(
        "该机器上仍有 {active} 个 Bot 处于启动中/运行中/停止中，请先全部停止后再{action}组件"
    ))
}

pub(crate) fn executor(state: &AppState) -> &ComponentExecutor {
    &state.components
}

/// 取(或探测并缓存)一台主机的 home + layout + 库存选中路径
pub(crate) async fn cached_host_probe(
    host_id: &str,
    host: &dyn Host,
    state: &AppState,
) -> (RemoteHostProbe, Option<RemoteSelectedPaths>) {
    state
        .components
        .inventory()
        .host_probe_or_default(host_id, host)
        .await
}

pub(crate) async fn build_inputs(
    state: &AppState,
    host: &dyn Host,
    probe: &RemoteHostProbe,
    selected: Option<RemoteSelectedPaths>,
    snowluma_linux_package: Option<SnowLumaLinuxPackage>,
) -> ComponentBuildInputs {
    state
        .components
        .build_inputs(host, probe, selected, snowluma_linux_package)
        .await
}

/// 测试用 AppState 的执行器;和生产一样共用传入的任务队列 / 主机档案 / 设置
#[cfg(test)]
pub(crate) fn test_components(
    root: &std::path::Path,
    bus: &ncd_runtime::BroadcastEventBus,
    deployment_tasks: &ncd_runtime::DeploymentTaskManager,
    server_manager: &std::sync::Arc<ncd_runtime::ServerManager>,
    app_settings: &std::sync::Arc<tokio::sync::RwLock<ncd_domain::AppSettings>>,
) -> std::sync::Arc<ComponentExecutor> {
    use std::sync::Arc;
    Arc::new(ComponentExecutor::new(
        ncd_runtime::components::ComponentExecutorDeps {
            deployment_tasks: deployment_tasks.clone(),
            server_manager: Arc::clone(server_manager),
            event_bus: bus.clone(),
            app_settings: Arc::clone(app_settings),
            data_root: root.to_path_buf(),
            local_snowluma_version: None,
            desktop_product_version: crate::desktop_update::product_version_str().to_string(),
            registry: Arc::new(ncd_runtime::AppFrameworkRegistry::with_builtin()),
        },
    ))
}

/// 组件在某台主机上按阶段的依赖状态(组件页「为什么不可用」/ 装前预览)
#[tauri::command]
pub async fn resolve_component_dependencies(
    component_id: ComponentId,
    host_id: String,
    phase: RequirementPhase,
    snowluma_linux_package: Option<SnowLumaLinuxPackage>,
    state: State<'_, AppState>,
) -> Result<DependencyPlan, String> {
    let host = resolve_host_with_autoconnect(&host_id, &state).await?;
    state
        .components
        .inputs_for(&host_id, host.as_ref(), snowluma_linux_package)
        .await
        .resolve(component_id, host.as_ref(), phase)
        .await
}

/// 组件自己 + Run 依赖是否都在(Bot 启动 / 保存门禁)。
/// SnowLuma 包类型不由前端传:本机读设置,远端读库存
#[tauri::command]
pub async fn resolve_runtime_readiness(
    component_id: ComponentId,
    host_id: String,
    state: State<'_, AppState>,
) -> Result<RuntimeReadiness, String> {
    let host = resolve_host_with_autoconnect(&host_id, &state).await?;
    state
        .components
        .runtime_readiness(component_id, &host_id, host.as_ref())
        .await
}

#[tauri::command]
pub async fn probe_local_node_candidates(
    state: State<'_, AppState>,
) -> Result<Vec<NodeEnvironmentCandidate>, String> {
    let host = LocalWindowsHost::new();
    let data_root = &state.data_root;
    let comp_node = HostPath::from_windows(
        data_root
            .join("components")
            .join("NodeJs")
            .join("node.exe")
            .to_str()
            .unwrap_or_default(),
    );
    let custom = {
        let settings = state.app_settings.read().await;
        settings.snowluma_node_path.clone()
    };
    // 版本约束来自本机上要用 Node 的组件声明,不在这里写死
    let accept = state
        .components
        .catalog_version_reqs_for(ComponentId::NodeJs, host.os(), host.locality());
    let candidates = ncd_component::nodejs::probe_local_system_nodes(
        &host,
        Some(&comp_node),
        custom.as_deref(),
        &accept,
    )
    .await;
    Ok(candidates)
}

#[tauri::command]
pub async fn probe_node_binary_version(
    path: String,
    state: State<'_, AppState>,
) -> Result<NodeProbeResult, String> {
    let host = LocalWindowsHost::new();
    let hp = HostPath::from_windows(path.trim());
    let accept = state
        .components
        .catalog_version_reqs_for(ComponentId::NodeJs, host.os(), host.locality());
    match ncd_component::nodejs::probe_node_raw_version(&host, &hp).await {
        Ok(Some(raw_ver)) => {
            let is_valid = ncd_component::VersionReq::all_match(&accept, &raw_ver);
            let error = (!is_valid)
                .then(|| ncd_component::nodejs::version_mismatch_reason(&raw_ver, &accept));
            Ok(NodeProbeResult {
                path,
                exists: true,
                version: Some(raw_ver),
                is_valid,
                error,
            })
        }
        Ok(None) => Ok(NodeProbeResult {
            path,
            exists: false,
            version: None,
            is_valid: false,
            error: Some("无法执行或未找到 node.exe".to_string()),
        }),
        Err(err) => Ok(NodeProbeResult {
            path,
            exists: false,
            version: None,
            is_valid: false,
            error: Some(format!("{err}")),
        }),
    }
}
