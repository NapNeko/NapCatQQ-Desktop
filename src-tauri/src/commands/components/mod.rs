//! Components 页 Tauri 命令薄壳层
//!
//! 依赖闭包 / 任务编排在 ncd-runtime::ComponentExecutor;这里只做 host 解析、
//! 从 AppState 收集构建输入、错误转 String。

pub mod qq_deps;

use std::sync::Arc;

use ncd_component::{ComponentDetectResult, ComponentId, ComponentInfo, DetectOutcome};
use ncd_deploy::StepKind;
use ncd_domain::{NodeEnvironmentCandidate, NodeProbeResult};
use ncd_host::{Host, HostPath, Locality, local::LocalWindowsHost};
use ncd_runtime::{
    ComponentActionRequest, ComponentBuildInputs, ComponentExecutor, RemoteHostProbe,
    RemoteSelectedPaths, SnowLumaLinuxPackage, component_catalog, infer_local_snowluma_package,
    infer_snowluma_linux_package, probe_from_inventory, release::read_cached_release_snapshot,
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
    let (probe, selected) = cached_host_probe(&host_id, host.as_ref(), &state).await;
    let component = build_inputs(&state, host.as_ref(), &probe, selected, None)
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
    let (probe, selected) = cached_host_probe(&host_id, host.as_ref(), &state).await;
    let sl_pkg = if component_id == ComponentId::SnowLuma {
        let persisted_local = if host.locality() == Locality::Local {
            state.app_settings.read().await.snowluma_package
        } else {
            None
        };
        Some(
            snowluma_linux_package
                .or(persisted_local)
                .unwrap_or_else(|| {
                    if host.locality() == Locality::Local {
                        infer_local_snowluma_package(&state.data_root)
                    } else {
                        infer_snowluma_linux_package(selected.as_ref())
                    }
                }),
        )
    } else {
        None
    };
    let inputs = build_inputs(&state, host.as_ref(), &probe, selected, sl_pkg).await;

    executor(&state)
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
    executor(&state).cancel(&task_id).await
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

pub(crate) fn executor(state: &AppState) -> ComponentExecutor {
    ComponentExecutor::new(
        state.deployment_tasks.clone(),
        Arc::clone(&state.server_manager),
        state.event_bus.clone(),
        Arc::clone(&state.active_tasks),
        Arc::clone(&state.host_probe_cache),
        Arc::clone(&state.app_settings),
        state.data_root.clone(),
    )
}

/// 取(或探测并缓存)一台主机的 home + layout + 库存选中路径
pub(crate) async fn cached_host_probe(
    host_id: &str,
    host: &dyn Host,
    state: &AppState,
) -> (RemoteHostProbe, Option<RemoteSelectedPaths>) {
    if !host_id.starts_with("remote:") {
        return (RemoteHostProbe::local_default(), None);
    }
    let server_id = host_id.trim_start_matches("remote:");
    match crate::commands::servers::ensure_remote_inventory(server_id, host, state, false).await {
        Ok(inv) => {
            let selected = inv.selected.clone();
            (probe_from_inventory(&inv), Some(selected))
        }
        Err(_) => (RemoteHostProbe::local_default(), None),
    }
}

/// 从 AppState 收集一次构建输入;本机 SnowLuma 的包类型与 Node 覆盖路径来自设置
pub(crate) async fn build_inputs(
    state: &AppState,
    host: &dyn Host,
    probe: &RemoteHostProbe,
    selected: Option<RemoteSelectedPaths>,
    snowluma_linux_package: Option<SnowLumaLinuxPackage>,
) -> ComponentBuildInputs {
    let local = host.locality() == Locality::Local;
    let (persisted_package, snowluma_node_path) = if local {
        let settings = state.app_settings.read().await;
        (settings.snowluma_package, settings.snowluma_node_path.clone())
    } else {
        (None, None)
    };
    let snowluma_linux_package = snowluma_linux_package.or_else(|| {
        local.then(|| {
            persisted_package.unwrap_or_else(|| infer_local_snowluma_package(&state.data_root))
        })
    });
    ComponentBuildInputs {
        data_root: state.data_root.clone(),
        remote_home: probe.home.clone(),
        layout: probe.layout,
        snapshot: read_cached_release_snapshot(&state.data_root),
        local_snowluma_version: state.snapshot.local_versions.snowluma.clone(),
        desktop_product_version: crate::desktop_update::product_version_str().to_string(),
        selected,
        snowluma_linux_package,
        snowluma_node_path,
    }
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
    let accept = ncd_runtime::catalog_version_reqs_for(ComponentId::NodeJs, host.os(), host.locality());
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
pub async fn probe_node_binary_version(path: String) -> Result<NodeProbeResult, String> {
    let host = LocalWindowsHost::new();
    let hp = HostPath::from_windows(path.trim());
    let accept = ncd_runtime::catalog_version_reqs_for(ComponentId::NodeJs, host.os(), host.locality());
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
