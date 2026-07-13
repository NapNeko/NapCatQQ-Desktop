//! Components 页 Tauri 命令薄壳层
//!
//! 策略/工厂在 ncd-runtime；系统包与 QQ 依赖 runner 在本目录子模块。
//! command 只做 host 解析、错误转 String、提交 deployment task。

mod progress;
pub mod qq_deps;
mod sys_pkg;

use std::sync::Arc;

use ncd_component::{Component, ComponentDetectResult, ComponentId, ComponentInfo, ProgressKind};
use ncd_deploy::{DeployPlan, StepKind};
use ncd_domain::DeploymentTaskKind;
use ncd_host::Host;
use ncd_runtime::{
    ComponentTaskSpec, DeploymentTaskRequest, DeploymentTaskRunResult, DomainEvent, EventBus,
    RemoteHostProbe, build_component_for_host, collect_component_runtime_prerequisites,
    component_action_cancellable, component_catalog, component_dedupe_key,
    component_task_resources, direct_runtime_dependency_ids, parse_remote_host_probe_stdout,
    release::read_cached_release_snapshot,
};
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
    let probe = cached_host_probe(&host_id, host.as_ref(), &state).await;
    let component = build_component_for_host_from_state(
        component_id,
        &state,
        host.as_ref(),
        probe.home.as_deref(),
        probe.layout,
    )?;
    let host_ref: &dyn Host = host.as_ref();

    if component.check_target(host_ref).is_err() {
        return Ok(ComponentDetectResult {
            component_id,
            host_id,
            detected: None,
            supported: false,
        });
    }

    match component.detect(host_ref).await {
        Ok(detected) => Ok(ComponentDetectResult {
            component_id,
            host_id,
            detected,
            supported: true,
        }),
        Err(err) => Err(format!("detect failed: {err}")),
    }
}

#[tauri::command]
pub async fn run_component_action(
    component_id: ComponentId,
    host_id: String,
    kind: StepKind,
    task_id: Option<String>,
    state: State<'_, AppState>,
) -> Result<String, String> {
    ensure_host_idle_for_component_mutation(&host_id, kind, &state).await?;
    let host = resolve_host_with_autoconnect(&host_id, &state).await?;
    let probe = cached_host_probe(&host_id, host.as_ref(), &state).await;
    let task_id = task_id
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    submit_component_action_with_prerequisites(
        component_id,
        &host_id,
        kind,
        task_id,
        host,
        &probe,
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
    let prerequisite_specs =
        collect_component_runtime_prerequisites(target, host.os(), host.locality());
    let mut submitted: Vec<(ComponentTaskSpec, String)> =
        Vec::with_capacity(prerequisite_specs.len());

    for spec in prerequisite_specs {
        if component_prerequisite_is_installed(spec, &host, probe, state).await? {
            tracing::info!(
                host_id,
                component = spec.component_id.as_str(),
                action = spec.kind.as_str(),
                "skip installed runtime prerequisite"
            );
            continue;
        }
        let depends_on =
            direct_runtime_dependency_ids(spec, host.os(), host.locality(), &submitted);
        let submitted_task_id = submit_single_component_task(
            spec.component_id,
            host_id,
            spec.kind,
            None,
            depends_on,
            Arc::clone(&host),
            probe,
            state,
        )
        .await?;
        submitted.push((spec, submitted_task_id));
    }

    let depends_on = direct_runtime_dependency_ids(target, host.os(), host.locality(), &submitted);
    submit_single_component_task(
        component_id,
        host_id,
        kind,
        Some(task_id),
        depends_on,
        host,
        probe,
        state,
    )
    .await
}

async fn component_prerequisite_is_installed(
    spec: ComponentTaskSpec,
    host: &Arc<dyn Host>,
    probe: &RemoteHostProbe,
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
    )?;

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
    state: &AppState,
) -> Result<String, String> {
    let component = build_component_for_host_from_state(
        component_id,
        state,
        host.as_ref(),
        probe.home.as_deref(),
        probe.layout,
    )?;

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
                        Ok(outcome) if outcome.ok => DeploymentTaskRunResult::ok("组件操作完成"),
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

/// 取(或探测并缓存)一台主机的 home + layout
pub(crate) async fn cached_host_probe(
    host_id: &str,
    host: &dyn Host,
    state: &AppState,
) -> RemoteHostProbe {
    if !host_id.starts_with("remote:") {
        return RemoteHostProbe::local_default();
    }
    if let Some(cached) = state.host_probe_cache.lock().await.get(host_id) {
        return cached.clone();
    }
    let probe = probe_remote_host(host).await;
    state
        .host_probe_cache
        .lock()
        .await
        .insert(host_id.to_string(), probe.clone());
    probe
}

async fn probe_remote_host(host: &dyn Host) -> RemoteHostProbe {
    let script = "echo \"$HOME\"; \
         test -e /opt/QQ/resources/app/app_launcher/napcat/napcat.mjs && echo 1 || echo 0";
    let cmd = ncd_host::HostCommand::new("sh").arg("-c").arg(script);
    let out = match host.run_to_string(cmd).await {
        Ok(out) if out.success() => out,
        _ => return RemoteHostProbe::local_default(),
    };
    parse_remote_host_probe_stdout(&out.stdout)
}

fn build_component_for_host_from_state(
    id: ComponentId,
    state: &AppState,
    host: &dyn Host,
    remote_home: Option<&str>,
    layout: ncd_runtime::RemoteLayout,
) -> Result<Arc<dyn Component>, String> {
    let snapshot = read_cached_release_snapshot(&state.data_root);
    let desktop_ver = crate::desktop_update::product_version_str();
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
        },
    )
}
