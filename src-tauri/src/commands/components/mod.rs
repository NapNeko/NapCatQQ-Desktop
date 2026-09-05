//! Components 页 Tauri 命令薄壳层
//!
//! 策略/工厂在 ncd-runtime；系统包与 QQ 依赖 runner 在本目录子模块。
//! command 只做 host 解析、错误转 String、提交 deployment task。

mod progress;
pub mod qq_deps;
mod sys_pkg;

use std::sync::Arc;

use ncd_component::{
    Component, ComponentDetectResult, ComponentId, ComponentInfo, DetectOutcome, ProgressKind,
};
use ncd_deploy::{DeployPlan, StepKind};
use ncd_domain::{DeploymentTaskKind, NodeEnvironmentCandidate, NodeProbeResult};
use ncd_host::{Host, HostPath, Locality, local::LocalWindowsHost};
use ncd_runtime::{
    ComponentTaskSpec, DeploymentTaskRequest, DeploymentTaskRunResult, DomainEvent, EventBus,
    RemoteHostProbe, RemoteSelectedPaths, SnowLumaLinuxPackage, build_component_for_host,
    collect_component_runtime_prerequisites_for, component_action_cancellable, component_catalog,
    component_dedupe_key, component_task_resources, direct_runtime_dependency_ids_for,
    infer_snowluma_linux_package, probe_from_inventory, release::read_cached_release_snapshot,
};
use ncd_traits::ConfigStore;
use tauri::State;
use uuid::Uuid;

use crate::AppState;
use crate::commands::host_resolve::resolve_host_with_autoconnect;
use sys_pkg::submit_component_package_prerequisites;

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
    let component = build_component_for_host_from_state(
        component_id,
        &state,
        host.as_ref(),
        probe.home.as_deref(),
        probe.layout,
        selected.as_ref(),
        None,
    )
    .await?;
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
    let task_id = task_id
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| Uuid::new_v4().to_string());
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

    submit_component_action_with_prerequisites(
        component_id,
        &host_id,
        kind,
        task_id,
        host,
        &probe,
        selected.as_ref(),
        sl_pkg,
        &state,
    )
    .await
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

async fn submit_component_action_with_prerequisites(
    component_id: ComponentId,
    host_id: &str,
    kind: StepKind,
    task_id: String,
    host: Arc<dyn Host>,
    probe: &RemoteHostProbe,
    selected: Option<&RemoteSelectedPaths>,
    snowluma_linux_package: Option<SnowLumaLinuxPackage>,
    state: &AppState,
) -> Result<String, String> {
    let target_dedupe_key = component_dedupe_key(host_id, component_id, kind);
    if let Some(existing) = state
        .deployment_tasks
        .active_task_by_dedupe_key(&target_dedupe_key)
        .await
    {
        return Ok(existing);
    }

    let target = ComponentTaskSpec { component_id, kind };
    let prerequisite_specs = collect_component_runtime_prerequisites_for(
        target,
        host.os(),
        host.locality(),
        snowluma_linux_package,
    );
    let mut submitted: Vec<(ComponentTaskSpec, String)> =
        Vec::with_capacity(prerequisite_specs.len());

    for spec in prerequisite_specs {
        if component_prerequisite_is_installed(spec, &host, probe, selected, snowluma_linux_package, state).await? {
            tracing::info!(
                host_id,
                component = spec.component_id.as_str(),
                action = spec.kind.as_str(),
                "skip installed runtime prerequisite"
            );
            continue;
        }
        let depends_on = direct_runtime_dependency_ids_for(
            spec,
            host.os(),
            host.locality(),
            &submitted,
            snowluma_linux_package,
        );
        let submitted_task_id = submit_single_component_task(
            spec.component_id,
            host_id,
            spec.kind,
            None,
            depends_on,
            Arc::clone(&host),
            probe,
            selected,
            snowluma_linux_package,
            state,
        )
        .await?;
        submitted.push((spec, submitted_task_id));
    }

    let depends_on = direct_runtime_dependency_ids_for(
        target,
        host.os(),
        host.locality(),
        &submitted,
        snowluma_linux_package,
    );
    submit_single_component_task(
        component_id,
        host_id,
        kind,
        Some(task_id),
        depends_on,
        host,
        probe,
        selected,
        snowluma_linux_package,
        state,
    )
    .await
}

async fn component_prerequisite_is_installed(
    spec: ComponentTaskSpec,
    host: &Arc<dyn Host>,
    probe: &RemoteHostProbe,
    selected: Option<&RemoteSelectedPaths>,
    snowluma_linux_package: Option<SnowLumaLinuxPackage>,
    state: &AppState,
) -> Result<bool, String> {
    if spec.kind != StepKind::EnsureInstalled {
        return Ok(false);
    }

    let component = build_component_for_host_from_state(
        spec.component_id,
        state,
        host.as_ref(),
        probe.home.as_deref(),
        probe.layout,
        selected,
        snowluma_linux_package,
    )
    .await?;

    if component.check_target(host.as_ref()).is_err() {
        return Ok(false);
    }

    match component.detect(host.as_ref()).await {
        Ok(Some(detected)) => {
            tracing::info!(
                component = spec.component_id.as_str(),
                version = %detected.version,
                source = %detected.source,
                "runtime prerequisite already installed"
            );
            Ok(true)
        }
        Ok(None) => Ok(false),
        Err(err) => {
            tracing::warn!(
                component = spec.component_id.as_str(),
                error = %err,
                "runtime prerequisite pre-detect failed; submitting prerequisite task"
            );
            Ok(false)
        }
    }
}

async fn submit_single_component_task(
    component_id: ComponentId,
    host_id: &str,
    kind: StepKind,
    requested_task_id: Option<String>,
    mut depends_on: Vec<String>,
    host: Arc<dyn Host>,
    probe: &RemoteHostProbe,
    selected: Option<&RemoteSelectedPaths>,
    snowluma_linux_package: Option<SnowLumaLinuxPackage>,
    state: &AppState,
) -> Result<String, String> {
    let component = build_component_for_host_from_state(
        component_id,
        state,
        host.as_ref(),
        probe.home.as_deref(),
        probe.layout,
        selected,
        snowluma_linux_package,
    )
    .await?;

    let plan = DeployPlan::builder()
        .step("single", kind, Arc::clone(&component))
        .build();
    plan.validate().map_err(|err| format!("{err}"))?;

    let host_id_owned = host_id.to_string();
    let server_id = host_id_owned.strip_prefix("remote:").map(str::to_string);
    let remote_long_install = server_id.is_some()
        && matches!(
            kind,
            StepKind::EnsureInstalled
                | StepKind::ForceInstall
                | StepKind::Update
                | StepKind::EnsureDependencies
        );

    let task_id = requested_task_id.unwrap_or_else(|| Uuid::new_v4().to_string());
    let host_probe_cache = Arc::clone(&state.host_probe_cache);
    let probe_cache_key = host_id_owned.clone();
    let event_bus = state.event_bus.clone();
    let active_tasks = Arc::clone(&state.active_tasks);
    let app_settings = Arc::clone(&state.app_settings);
    let data_root = state.data_root.clone();
    let server_manager = Arc::clone(&state.server_manager);
    let deployment_tasks = state.deployment_tasks.clone();
    let dedupe_key = Some(component_dedupe_key(&host_id_owned, component_id, kind));
    if let Some(existing) = deployment_tasks
        .active_task_by_dedupe_key(dedupe_key.as_deref().unwrap())
        .await
    {
        return Ok(existing);
    }
    let package_depends_on = submit_component_package_prerequisites(
        component_id,
        kind,
        &host_id_owned,
        Arc::clone(&host),
        state,
    )
    .await;
    depends_on.extend(package_depends_on);
    let resources = component_task_resources(
        component_id,
        &host_id_owned,
        kind,
        host.os(),
        host.locality(),
    );
    let cancellable = component_action_cancellable(component_id, kind, host.os(), host.locality());
    let title = format!("{} {}", component_id.as_str(), kind.as_str());
    let submitted_task_id = deployment_tasks
        .submit(DeploymentTaskRequest {
            task_id: task_id.clone(),
            kind: DeploymentTaskKind::ComponentAction {
                component_id: component_id.as_str().to_string(),
                action: kind.as_str().to_string(),
            },
            host_id: host_id_owned,
            title,
            resources,
            depends_on,
            dedupe_key,
            cancellable,
            runner: Box::new(move |task_ctx| {
                Box::pin(async move {
                    let (mut ctx, mut rx) = ncd_component::ActionCtx::new();
                    let cancel_token = ctx.cancel_token();
                    let task_id_for_runner = task_ctx.task_id().to_string();
                    active_tasks
                        .lock()
                        .await
                        .insert(task_id_for_runner.clone(), cancel_token.clone());

                    let task_cancel = task_ctx.cancel_token();
                    let action_cancel = cancel_token.clone();
                    tauri::async_runtime::spawn(async move {
                        task_cancel.cancelled().await;
                        action_cancel.cancel();
                    });

                    let event_bus_for_progress = event_bus.clone();
                    let task_ctx_for_progress = task_ctx.clone();
                    let progress_task_id = task_id_for_runner.clone();
                    tauri::async_runtime::spawn(async move {
                        while let Some(progress_event) = rx.recv().await {
                            task_ctx_for_progress
                                .push_progress(progress_event.clone())
                                .await;
                            event_bus_for_progress.publish(DomainEvent::component_action_progress(
                                progress_task_id.clone(),
                                progress_event,
                            ));
                        }
                    });

                    if cancel_token.is_cancelled() {
                        active_tasks.lock().await.remove(&task_id_for_runner);
                        let finished =
                            ncd_component::ProgressEvent::new(ProgressKind::Finished { ok: false });
                        task_ctx.push_progress(finished.clone()).await;
                        event_bus.publish(DomainEvent::component_action_progress(
                            task_id_for_runner.clone(),
                            finished,
                        ));
                        return DeploymentTaskRunResult::failed("任务已取消");
                    }

                    let outcome: Result<ncd_deploy::DeployOutcome, String> = if remote_long_install
                    {
                        let Some(id) = server_id.clone() else {
                            return DeploymentTaskRunResult::failed("missing remote server id");
                        };
                        server_manager
                            .with_isolated_connection(&id, move |iso_host| {
                                Box::pin(async move {
                                    plan.run(iso_host.as_ref(), &mut ctx)
                                        .await
                                        .map_err(|e| format!("{e}"))
                                })
                            })
                            .await
                    } else {
                        plan.run(host.as_ref(), &mut ctx)
                            .await
                            .map_err(|e| format!("{e}"))
                    };

                    if outcome.is_err() {
                        if let Some(ref id) = server_id {
                            server_manager.disconnect_cached_host(id).await;
                        }
                    }

                    active_tasks.lock().await.remove(&task_id_for_runner);
                    host_probe_cache.lock().await.remove(&probe_cache_key);

                    match outcome {
                        Ok(outcome) if outcome.ok => {
                            if component_id == ComponentId::SnowLuma
                                && host.locality() == Locality::Local
                                && matches!(kind, StepKind::EnsureInstalled | StepKind::ForceInstall | StepKind::Update | StepKind::Uninstall)
                            {
                                let next = if kind == StepKind::Uninstall {
                                    None
                                } else {
                                    snowluma_linux_package
                                };
                                if let Err(err) = persist_local_snowluma_package(
                                    &data_root,
                                    &app_settings,
                                    next,
                                )
                                .await
                                {
                                    tracing::error!(error = %err, "failed to persist SnowLuma package state");
                                    return DeploymentTaskRunResult::failed(format!(
                                        "组件操作已完成，但保存 SnowLuma 包类型失败: {err}"
                                    ));
                                }
                            }
                            DeploymentTaskRunResult::ok("组件操作完成")
                        }
                        Ok(outcome) => {
                            let err = outcome
                                .steps
                                .iter()
                                .find_map(|s| s.error.clone())
                                .unwrap_or_else(|| "组件操作失败".to_string());
                            DeploymentTaskRunResult::failed(err)
                        }
                        Err(err) => {
                            let progress_event =
                                ncd_component::ProgressEvent::new(ProgressKind::Log {
                                    level: ncd_component::ProgressLogLevel::Error,
                                    message: format!("plan failed: {err}"),
                                });
                            task_ctx.push_progress(progress_event.clone()).await;
                            event_bus.publish(DomainEvent::component_action_progress(
                                task_id_for_runner.clone(),
                                progress_event,
                            ));
                            let finished =
                                ncd_component::ProgressEvent::new(ProgressKind::Finished {
                                    ok: false,
                                });
                            task_ctx.push_progress(finished.clone()).await;
                            event_bus.publish(DomainEvent::component_action_progress(
                                task_id_for_runner.clone(),
                                finished,
                            ));
                            DeploymentTaskRunResult::failed(err)
                        }
                    }
                })
            }),
        })
        .await;

    Ok(submitted_task_id)
}

async fn persist_local_snowluma_package(
    data_root: &std::path::Path,
    app_settings: &Arc<tokio::sync::RwLock<ncd_domain::AppSettings>>,
    package: Option<SnowLumaLinuxPackage>,
) -> Result<(), String> {
    let mut settings = app_settings.read().await.clone();
    settings.snowluma_package = package;
    let store = ncd_runtime::LocalConfigStore::new(data_root);
    let path = store.config_dir().join("app-settings.json");
    let payload = serde_json::to_value(&settings).map_err(|e| e.to_string())?;
    store.write_json_atomic(&path, &payload).map_err(|e| e.to_string())?;
    *app_settings.write().await = settings;
    Ok(())
}

#[tauri::command]
pub async fn cancel_component_action(
    task_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let token = state.active_tasks.lock().await.get(&task_id).cloned();
    if let Some(t) = token {
        t.cancel();
    }
    state.deployment_tasks.cancel(&task_id).await
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

async fn build_component_for_host_from_state(
    id: ComponentId,
    state: &AppState,
    host: &dyn Host,
    remote_home: Option<&str>,
    layout: ncd_runtime::RemoteLayout,
    selected: Option<&RemoteSelectedPaths>,
    snowluma_linux_package: Option<SnowLumaLinuxPackage>,
) -> Result<Arc<dyn Component>, String> {
    let snapshot = read_cached_release_snapshot(&state.data_root);
    let desktop_ver = crate::desktop_update::product_version_str();
    let effective_package = if snowluma_linux_package.is_some() {
        snowluma_linux_package
    } else if host.locality() == Locality::Local && id == ComponentId::SnowLuma {
        Some(
            state
                .app_settings
                .read()
                .await
                .snowluma_package
                .unwrap_or_else(|| infer_local_snowluma_package(&state.data_root)),
        )
    } else {
        None
    };
    let snowluma_node_path = if host.locality() == Locality::Local {
        state.app_settings.read().await.snowluma_node_path.clone()
    } else {
        None
    };
    build_component_for_host(
        id,
        &ncd_runtime::BuildComponentCtx {
            data_root: &state.data_root,
            host,
            remote_home,
            layout,
            snapshot: snapshot.as_ref(),
            local_snowluma_version: state.snapshot.local_versions.snowluma.as_deref(),
            desktop_product_version: desktop_ver,
            selected,
            snowluma_linux_package: effective_package,
            snowluma_node_path: snowluma_node_path.as_deref(),
        },
    )
}

pub(crate) fn infer_local_snowluma_package(data_root: &std::path::Path) -> SnowLumaLinuxPackage {
    let install_dir = data_root.join("components").join("SnowLuma");
    if !install_dir.is_dir() {
        return SnowLumaLinuxPackage::Full;
    }
    let bundled_node = install_dir
        .join("node.exe");
    if bundled_node.is_file() {
        SnowLumaLinuxPackage::Full
    } else {
        SnowLumaLinuxPackage::Lite
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
