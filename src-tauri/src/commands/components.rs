//! Components 页 Tauri 命令薄壳层
//!
//! 暴露 4 个命令给前端:
//! - list_components:返回所有 6 个 ComponentInfo 元数据(顺序:Framework
//!   → RuntimeDep → SelfApp)
//! - detect_component:在指定 host 上探测某 component 的安装版本
//! - run_component_action:把单 step DeployPlan 跑起来,进度走
//!   DomainEvent::ComponentActionProgress,立即返回 task_id
//! - cancel_component_action:用 task_id 找到 cancel token 并 cancel
//!
//! 所有错误都用 format!("{}", err) 转 String,不向前端泄漏 ActionError /
//! DeployError 的 enum 结构

use std::sync::Arc;
use std::time::Duration;

use ncd_component::{
    Component, ComponentDetectResult, ComponentId, ComponentInfo, DesktopSelfComponent,
    NapCatComponent, NoVncComponent, NodeJsComponent, ProgressEvent, ProgressKind,
    ProgressLogLevel, QQComponent, SnowLumaComponent,
};
use ncd_deploy::{DeployPlan, StepKind};
use ncd_domain::release_snapshot::ReleaseInfo;
use ncd_domain::{
    DeploymentTaskKind, DeploymentTaskResource, InstallDependenciesResult, QqDependencyReport,
};
use ncd_host::{Host, HostCommand, HostPath, Locality, Os};
use ncd_runtime::{
    DeploymentTaskContext, DeploymentTaskRequest, DeploymentTaskRunResult, DomainEvent, EventBus,
    release::read_cached_release_snapshot,
};
use tauri::State;
use tokio::sync::oneshot;
use uuid::Uuid;

use crate::AppState;
use crate::commands::host_resolve::resolve_host_with_autoconnect;

#[tauri::command]
pub async fn list_components() -> Vec<ComponentInfo> {
    catalog()
}

#[tauri::command]
pub async fn detect_component(
    component_id: ComponentId,
    host_id: String,
    state: State<'_, AppState>,
) -> Result<ComponentDetectResult, String> {
    let host = resolve_host_with_autoconnect(&host_id, &state).await?;
    let probe = cached_host_probe(&host_id, host.as_ref(), &state).await;
    let component = build_component_for_host(
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
        let depends_on = direct_runtime_dependency_ids(
            spec,
            host.os(),
            host.locality(),
            &submitted,
        );
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

    let component = build_component_for_host(
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
    let component = build_component_for_host(
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
            StepKind::EnsureInstalled | StepKind::ForceInstall | StepKind::EnsureDependencies
        );

    let task_id = requested_task_id.unwrap_or_else(|| Uuid::new_v4().to_string());
    // 安装 / 卸载会改变远端布局(如新建 $HOME/Napcat),动作结束后失效该主机
    // 的布局缓存,下次 detect 重新探一次拿到最新布局
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
    let resources =
        component_task_resources(component_id, &host_id_owned, kind, host.os(), host.locality());
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ComponentTaskSpec {
    component_id: ComponentId,
    kind: StepKind,
}

fn component_dedupe_key(host_id: &str, component_id: ComponentId, kind: StepKind) -> String {
    format!(
        "component:{}:{}:{}",
        host_id,
        component_id.as_str(),
        kind.as_str()
    )
}

fn component_action_needs_runtime_closure(kind: StepKind) -> bool {
    matches!(
        kind,
        StepKind::EnsureInstalled | StepKind::ForceInstall | StepKind::Update
    )
}

fn component_runtime_prerequisites(
    component_id: ComponentId,
    kind: StepKind,
    host_os: Os,
    host_locality: Locality,
) -> Vec<ComponentTaskSpec> {
    if !component_action_needs_runtime_closure(kind) {
        return Vec::new();
    }

    let ensure = |component_id| ComponentTaskSpec {
        component_id,
        kind: StepKind::EnsureInstalled,
    };

    match component_id {
        ComponentId::NapCat => match host_os {
            Os::Windows | Os::Linux => vec![ensure(ComponentId::Qq)],
            _ => Vec::new(),
        },
        ComponentId::SnowLuma => match host_os {
            Os::Windows => vec![ensure(ComponentId::Qq)],
            Os::Linux => {
                let mut deps = vec![ensure(ComponentId::NodeJs), ensure(ComponentId::Qq)];
                if host_locality == Locality::Remote {
                    deps.push(ensure(ComponentId::NoVnc));
                }
                deps
            }
            _ => Vec::new(),
        },
        _ => Vec::new(),
    }
}

fn collect_component_runtime_prerequisites(
    target: ComponentTaskSpec,
    host_os: Os,
    host_locality: Locality,
) -> Vec<ComponentTaskSpec> {
    let mut seen = Vec::new();
    let mut ordered = Vec::new();
    collect_component_runtime_prerequisites_inner(
        target,
        host_os,
        host_locality,
        &mut seen,
        &mut ordered,
    );
    ordered
}

fn collect_component_runtime_prerequisites_inner(
    target: ComponentTaskSpec,
    host_os: Os,
    host_locality: Locality,
    seen: &mut Vec<ComponentTaskSpec>,
    ordered: &mut Vec<ComponentTaskSpec>,
) {
    for dep in component_runtime_prerequisites(
        target.component_id,
        target.kind,
        host_os,
        host_locality,
    ) {
        if seen.contains(&dep) {
            continue;
        }
        seen.push(dep);
        collect_component_runtime_prerequisites_inner(dep, host_os, host_locality, seen, ordered);
        ordered.push(dep);
    }
}

fn direct_runtime_dependency_ids(
    target: ComponentTaskSpec,
    host_os: Os,
    host_locality: Locality,
    submitted: &[(ComponentTaskSpec, String)],
) -> Vec<String> {
    component_runtime_prerequisites(target.component_id, target.kind, host_os, host_locality)
        .into_iter()
        .filter_map(|dep| {
            submitted
                .iter()
                .find(|(spec, _)| *spec == dep)
                .map(|(_, task_id)| task_id.clone())
        })
        .collect()
}

fn component_task_resources(
    component_id: ComponentId,
    host_id: &str,
    kind: StepKind,
    host_os: Os,
    host_locality: Locality,
) -> Vec<DeploymentTaskResource> {
    let mut resources = Vec::new();
    if !matches!(kind, StepKind::Verify) {
        resources.push(DeploymentTaskResource::InstallTarget {
            host_id: host_id.to_string(),
            target: component_id.as_str().to_string(),
        });
    }
    if component_needs_download_slot(component_id, kind) {
        resources.push(DeploymentTaskResource::GlobalDownloadSlot);
    }
    if component_needs_package_manager(component_id, kind, host_os, host_locality) {
        resources.push(DeploymentTaskResource::PackageManager {
            host_id: host_id.to_string(),
        });
    }
    resources
}

fn component_needs_download_slot(component_id: ComponentId, kind: StepKind) -> bool {
    matches!(
        kind,
        StepKind::EnsureInstalled | StepKind::ForceInstall | StepKind::Update
    ) && matches!(
        component_id,
        ComponentId::NapCat | ComponentId::SnowLuma | ComponentId::NodeJs | ComponentId::Qq
    )
}

fn component_needs_package_manager(
    component_id: ComponentId,
    kind: StepKind,
    host_os: Os,
    _host_locality: Locality,
) -> bool {
    if host_os != Os::Linux {
        return false;
    }
    match component_id {
        ComponentId::NoVnc => matches!(
            kind,
            StepKind::EnsureInstalled | StepKind::ForceInstall | StepKind::Uninstall
        ),
        ComponentId::Qq => kind == StepKind::EnsureDependencies,
        _ => false,
    }
}

fn component_action_cancellable(
    component_id: ComponentId,
    kind: StepKind,
    host_os: Os,
    host_locality: Locality,
) -> bool {
    !component_needs_package_manager(component_id, kind, host_os, host_locality)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SystemPackagePrerequisite {
    ArchiveTool {
        command: &'static str,
        package: &'static str,
    },
    QqDependencies,
}

impl SystemPackagePrerequisite {
    fn package_group(&self) -> String {
        match self {
            Self::ArchiveTool { command, .. } => format!("archive_tool:{command}"),
            Self::QqDependencies => "qq_dependencies".to_string(),
        }
    }

    fn title(&self) -> String {
        match self {
            Self::ArchiveTool { command, .. } => format!("准备系统工具 {command}"),
            Self::QqDependencies => "安装 QQ 系统依赖".to_string(),
        }
    }
}

fn component_package_prerequisites(
    component_id: ComponentId,
    kind: StepKind,
    host_os: Os,
) -> Vec<SystemPackagePrerequisite> {
    if host_os != Os::Linux
        || !matches!(
            kind,
            StepKind::EnsureInstalled | StepKind::ForceInstall | StepKind::Update
        )
    {
        return Vec::new();
    }

    match component_id {
        ComponentId::NapCat => vec![SystemPackagePrerequisite::ArchiveTool {
            command: "unzip",
            package: "unzip",
        }],
        ComponentId::NodeJs | ComponentId::SnowLuma => {
            vec![SystemPackagePrerequisite::ArchiveTool {
                command: "tar",
                package: "tar",
            }]
        }
        ComponentId::Qq => vec![SystemPackagePrerequisite::QqDependencies],
        _ => Vec::new(),
    }
}

async fn submit_component_package_prerequisites(
    component_id: ComponentId,
    kind: StepKind,
    host_id: &str,
    host: Arc<dyn Host>,
    state: &AppState,
) -> Vec<String> {
    let tasks = component_package_prerequisites(component_id, kind, host.os());
    let mut submitted_ids = Vec::with_capacity(tasks.len());
    for task in tasks {
        if system_package_prerequisite_is_satisfied(&task, host.as_ref()).await {
            tracing::info!(
                host_id,
                package_group = %task.package_group(),
                "skip satisfied system package prerequisite"
            );
            continue;
        }
        let id = submit_system_package_task(task, host_id, Arc::clone(&host), state).await;
        submitted_ids.push(id);
    }
    submitted_ids
}

async fn system_package_prerequisite_is_satisfied(
    task: &SystemPackagePrerequisite,
    host: &dyn Host,
) -> bool {
    match task {
        SystemPackagePrerequisite::ArchiveTool { command, .. } => host.command_exists(command).await,
        SystemPackagePrerequisite::QqDependencies => {
            let manifest = ncd_component::qq_deps::qq_qqnt_dependencies_v3_2_25();
            let detector = ncd_component::qq_deps::QqDependencyDetector::new(manifest);
            match detector.detect(host, None).await {
                Ok(report) => report.missing.is_empty(),
                Err(err) => {
                    tracing::warn!(
                        error = %err,
                        "QQ dependency pre-detect failed; submitting system package task"
                    );
                    false
                }
            }
        }
    }
}

async fn submit_system_package_task(
    task: SystemPackagePrerequisite,
    host_id: &str,
    host: Arc<dyn Host>,
    state: &AppState,
) -> String {
    let package_group = task.package_group();
    let task_id = Uuid::new_v4().to_string();
    let server_id = host_id.strip_prefix("remote:").map(str::to_string);
    let server_manager = Arc::clone(&state.server_manager);
    let deployment_tasks = state.deployment_tasks.clone();
    let host_id_owned = host_id.to_string();
    let title = task.title();
    let resources = vec![
        DeploymentTaskResource::PackageManager {
            host_id: host_id_owned.clone(),
        },
        DeploymentTaskResource::InstallTarget {
            host_id: host_id_owned.clone(),
            target: format!("system_package:{package_group}"),
        },
    ];
    deployment_tasks
        .submit(DeploymentTaskRequest {
            task_id: task_id.clone(),
            kind: DeploymentTaskKind::SystemPackage {
                package_group: package_group.clone(),
            },
            host_id: host_id_owned.clone(),
            title,
            resources,
            depends_on: vec![],
            dedupe_key: Some(format!("system-package:{host_id}:{package_group}")),
            cancellable: false,
            runner: Box::new(move |task_ctx| {
                Box::pin(async move {
                    if let Some(id) = server_id {
                        let task_for_run = task.clone();
                        server_manager
                            .with_isolated_connection(&id, move |iso_host| {
                                let task_ctx = task_ctx.clone();
                                Box::pin(async move {
                                    Ok(run_system_package_task(
                                        task_for_run,
                                        iso_host.as_ref(),
                                        task_ctx,
                                    )
                                    .await)
                                })
                            })
                            .await
                            .unwrap_or_else(DeploymentTaskRunResult::failed)
                    } else {
                        run_system_package_task(task, host.as_ref(), task_ctx).await
                    }
                })
            }),
        })
        .await
}

async fn run_system_package_task(
    task: SystemPackagePrerequisite,
    host: &dyn Host,
    task_ctx: DeploymentTaskContext,
) -> DeploymentTaskRunResult {
    match task {
        SystemPackagePrerequisite::ArchiveTool { command, package } => {
            ensure_archive_tool_task(command, package, host, task_ctx).await
        }
        SystemPackagePrerequisite::QqDependencies => {
            install_qq_dependencies_task(host, Vec::new(), None, task_ctx).await
        }
    }
}

async fn push_task_progress(task_ctx: &DeploymentTaskContext, kind: ProgressKind) {
    task_ctx.push_progress(ProgressEvent::new(kind)).await;
}

async fn ensure_archive_tool_task(
    command: &str,
    package: &str,
    host: &dyn Host,
    task_ctx: DeploymentTaskContext,
) -> DeploymentTaskRunResult {
    push_task_progress(&task_ctx, ProgressKind::Started { total_steps: 1 }).await;
    push_task_progress(
        &task_ctx,
        ProgressKind::StepBegin {
            step: 1,
            message: format!("检查系统工具 {command}"),
        },
    )
    .await;

    if host.command_exists(command).await {
        push_task_progress(
            &task_ctx,
            ProgressKind::Log {
                level: ProgressLogLevel::Info,
                message: format!("{command} 已可用"),
            },
        )
        .await;
        push_task_progress(&task_ctx, ProgressKind::StepEnd { step: 1, ok: true }).await;
        push_task_progress(&task_ctx, ProgressKind::Finished { ok: true }).await;
        return DeploymentTaskRunResult::ok(format!("{command} 已就绪"));
    }

    let Some(pm) = SystemPackageManager::detect(host).await else {
        let msg = format!("远端缺少 {command} 且未识别到包管理器，请手动安装 {package} 后重试");
        push_task_progress(
            &task_ctx,
            ProgressKind::Log {
                level: ProgressLogLevel::Error,
                message: msg.clone(),
            },
        )
        .await;
        push_task_progress(&task_ctx, ProgressKind::StepEnd { step: 1, ok: false }).await;
        push_task_progress(&task_ctx, ProgressKind::Finished { ok: false }).await;
        return DeploymentTaskRunResult::failed(msg);
    };

    let access = ncd_host::remote::probe_sudo(host).await;
    let has_password = host.has_elevation_password().await;
    let elevation_ok = matches!(
        access,
        ncd_host::remote::SudoAccess::RootAlready | ncd_host::remote::SudoAccess::Passwordless
    ) || has_password;
    if !elevation_ok {
        let msg = format!(
            "安装 {package} 需要 sudo 密码，请在远端主机配置中保存 sudo 密码，或手动执行 sudo {} 后重试",
            pm.install_hint(package)
        );
        push_task_progress(
            &task_ctx,
            ProgressKind::Log {
                level: ProgressLogLevel::Error,
                message: msg.clone(),
            },
        )
        .await;
        push_task_progress(&task_ctx, ProgressKind::StepEnd { step: 1, ok: false }).await;
        push_task_progress(&task_ctx, ProgressKind::Finished { ok: false }).await;
        return DeploymentTaskRunResult::failed(msg);
    }

    let install_line = pm.install_command(package);
    push_task_progress(
        &task_ctx,
        ProgressKind::Log {
            level: ProgressLogLevel::Info,
            message: format!("通过 {} 安装 {package}", pm.binary()),
        },
    )
    .await;
    let out = host
        .run_to_string(
            HostCommand::new("sh")
                .arg("-c")
                .arg(&install_line)
                .elevated()
                .timeout(Duration::from_secs(180)),
        )
        .await;
    match out {
        Ok(out) if out.success() && host.command_exists(command).await => {
            push_task_progress(&task_ctx, ProgressKind::StepEnd { step: 1, ok: true }).await;
            push_task_progress(&task_ctx, ProgressKind::Finished { ok: true }).await;
            DeploymentTaskRunResult::ok(format!("{command} 已安装"))
        }
        Ok(out) => {
            let msg = format!(
                "安装 {package} 后仍无法找到 {command}: exit={:?} stderr={}",
                out.exit_code,
                out.stderr.trim()
            );
            push_task_progress(
                &task_ctx,
                ProgressKind::Log {
                    level: ProgressLogLevel::Error,
                    message: msg.clone(),
                },
            )
            .await;
            push_task_progress(&task_ctx, ProgressKind::StepEnd { step: 1, ok: false }).await;
            push_task_progress(&task_ctx, ProgressKind::Finished { ok: false }).await;
            DeploymentTaskRunResult::failed(msg)
        }
        Err(err) => {
            let msg = format!("安装 {package} 失败: {err}");
            push_task_progress(
                &task_ctx,
                ProgressKind::Log {
                    level: ProgressLogLevel::Error,
                    message: msg.clone(),
                },
            )
            .await;
            push_task_progress(&task_ctx, ProgressKind::StepEnd { step: 1, ok: false }).await;
            push_task_progress(&task_ctx, ProgressKind::Finished { ok: false }).await;
            DeploymentTaskRunResult::failed(msg)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SystemPackageManager {
    Apt,
    Dnf,
    Yum,
    Apk,
    Pacman,
}

impl SystemPackageManager {
    const ALL: &'static [Self] = &[Self::Apt, Self::Dnf, Self::Yum, Self::Apk, Self::Pacman];

    async fn detect(host: &dyn Host) -> Option<Self> {
        for pm in Self::ALL {
            if host.command_exists(pm.binary()).await {
                return Some(*pm);
            }
        }
        None
    }

    fn binary(self) -> &'static str {
        match self {
            Self::Apt => "apt-get",
            Self::Dnf => "dnf",
            Self::Yum => "yum",
            Self::Apk => "apk",
            Self::Pacman => "pacman",
        }
    }

    fn install_command(self, package: &str) -> String {
        match self {
            Self::Apt => format!("apt-get update && apt-get install -y {package}"),
            Self::Dnf => format!("dnf install -y {package}"),
            Self::Yum => format!("yum install -y {package}"),
            Self::Apk => format!("apk add --no-cache {package}"),
            Self::Pacman => format!("pacman -Sy --noconfirm {package}"),
        }
    }

    fn install_hint(self, package: &str) -> String {
        match self {
            Self::Apt => format!("apt-get install -y {package}"),
            Self::Dnf => format!("dnf install -y {package}"),
            Self::Yum => format!("yum install -y {package}"),
            Self::Apk => format!("apk add {package}"),
            Self::Pacman => format!("pacman -S {package}"),
        }
    }
}

/// 6 个组件元数据按 Framework → RuntimeDep → SelfApp 顺序返回
fn catalog() -> Vec<ComponentInfo> {
    vec![
        NapCatComponent::info(),
        SnowLumaComponent::info(),
        NodeJsComponent::info(),
        QQComponent::info(),
        NoVncComponent::info(),
        DesktopSelfComponent::info(),
    ]
}

/// 远端 NapCat / QQ 的安装布局:system 是官方 NapCat-Installer 风格
/// (/opt/QQ,需要 sudo),rootless 是 NapCat-TUI-CLI 风格($HOME/Napcat,
/// 不需要 sudo,本工程默认)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteLayout {
    /// 系统安装:/opt/QQ/...,install_base_dir = "/"
    System,
    /// 用户安装:$HOME/Napcat/opt/QQ/...,install_base_dir = "$HOME/Napcat"
    Rootless,
}

/// 一台远端主机的布局探测结果:$HOME + NapCat 安装布局
/// 本机(host_id="local")这两项都没意义,用默认值(home=None / Rootless)
#[derive(Debug, Clone)]
pub struct RemoteHostProbe {
    pub home: Option<String>,
    pub layout: RemoteLayout,
}

impl RemoteHostProbe {
    /// 本机 / 探测失败时的默认值
    fn local_default() -> Self {
        Self {
            home: None,
            layout: RemoteLayout::Rootless,
        }
    }
}

/// 取(或探测并缓存)一台主机的 home + layout
///
/// 同一台远端在一次 UI 会话里 home / layout 是稳定的,5 个组件并发 detect 时
/// 没必要各探一遍缓存命中直接返回;未命中走单次合并探测,结果写缓存安装 /
/// 卸载动作结束后由 run_component_action 清掉对应条目(布局可能变)
async fn cached_host_probe(host_id: &str, host: &dyn Host, state: &AppState) -> RemoteHostProbe {
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

/// 一条 shell 命令同时拿 $HOME 和 system 布局标记,省掉原来"1 次 echo + 最多 2
/// 次 SFTP stat"分多趟的往返输出两行:HOME,system 标记存在与否
/// (test -e ... && echo 1 || echo 0)system 不存在时一律按 rootless 处理,
/// 所以不必再单独探 rootless 标记
async fn probe_remote_host(host: &dyn Host) -> RemoteHostProbe {
    let script = "echo \"$HOME\"; \
         test -e /opt/QQ/resources/app/app_launcher/napcat/napcat.mjs && echo 1 || echo 0";
    let cmd = ncd_host::HostCommand::new("sh").arg("-c").arg(script);
    let out = match host.run_to_string(cmd).await {
        Ok(out) if out.success() => out,
        _ => return RemoteHostProbe::local_default(),
    };

    let mut lines = out.stdout.lines();
    let home = lines
        .next()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let system_exists = lines.next().map(str::trim) == Some("1");

    // system(/opt/QQ)优先;否则一律 Rootless($HOME/Napcat,含从零安装)
    let layout = if system_exists {
        RemoteLayout::System
    } else {
        RemoteLayout::Rootless
    };

    RemoteHostProbe { home, layout }
}

/// 把 component_id 实例化成具体 Component
///
/// NapCat / SnowLuma 在 Windows 本机走"扁平 zip 部署"分支(legacy 同款),
/// 安装目录从 state.data_root 派生(对齐 bootstrap::resolve_data_root,
/// 红线 §4.1)其余组件保持 Linux 默认假设 —— Components 页 v1 只在
/// Windows 本机和 Linux 远端两条路径上验证过,中间 case 留作后续工单
fn build_component_for_host(
    id: ComponentId,
    state: &AppState,
    host: &dyn Host,
    remote_home: Option<&str>,
    layout: RemoteLayout,
) -> Result<Arc<dyn Component>, String> {
    let data_root_host = data_root_to_host_path(&state.data_root, host.os());
    // 读 release 缓存反查 SHA256:缓存缺失 / 无 digest 时退化到"无 hash"分支,
    // 让安装链路走原有路径(race 仍尝试切 mirror,但失去内容级保护)
    // 缓存由前端 useReleases hook 在启动/轮询时通过 get_release_snapshot 维护
    let snapshot = read_cached_release_snapshot(&state.data_root);

    // 远端 NapCat / QQ 共用 install_base_dir:layout 决定 / 还是 $HOME/Napcat
    // Rootless 但探不到 $HOME 时 fail-fast(不回退 /root):路径落盘红线,宁可报错也不
    // 把组件装到错误目录(/root 多半无权限或污染 root 家目录)惰性求值——只有真正
    // 用到 base 的组件(NapCat/QQ)才校验,home 无关组件(NoVnc/DesktopSelf)不受影响
    let resolve_napcat_base = || -> Result<HostPath, String> {
        Ok(match layout {
            RemoteLayout::System => HostPath::from_posix("/"),
            RemoteLayout::Rootless => {
                HostPath::from_posix(format!("{}/Napcat", require_remote_home(remote_home)?))
            }
        })
    };

    let component: Arc<dyn Component> = match id {
        ComponentId::NapCat => {
            if host.os() == ncd_host::Os::Windows {
                // legacy PathFunc.napcat_path = data_path/runtime/NapCatQQ
                let install = data_root_host.join("runtime").join("NapCatQQ");
                let mut comp = NapCatComponent::for_windows(install);
                if let Some(sha) = snapshot
                    .as_ref()
                    .and_then(|s| s.napcat_latest.as_ref())
                    .and_then(|info| asset_sha256(info, "NapCat.Shell.zip"))
                {
                    comp = comp.with_sha256(sha);
                }
                Arc::new(comp)
            } else {
                // 远端 Linux:layout 决定 base_dir + 是否需要 sudo
                // System 走 /opt/QQ 必须 sudo(对齐官方 NapCat-Installer.py);
                // Rootless 走 $HOME/Napcat 不需要 sudo(对齐 NapCat-TUI-CLI)
                Arc::new(
                    NapCatComponent::new(resolve_napcat_base()?)
                        .with_sudo(matches!(layout, RemoteLayout::System)),
                )
            }
        }
        ComponentId::SnowLuma => {
            if host.os() == ncd_host::Os::Windows {
                // legacy PathFunc.snowluma_path = data_path/runtime/SnowLuma;
                // tag 来源优先级:release 缓存的 latest tag → 已装版本 fallback
                // → 空串(install 阶段会拒绝)已装版本不能直接拿来拼装 URL,
                // 因为它是当前安装的旧版,需要装的是 latest(这是 EOCD 调查
                // 顺带发现的二次 bug:之前永远拿旧 tag 拼 URL)
                let install = data_root_host.join("runtime").join("SnowLuma");
                let latest = snapshot.as_ref().and_then(|s| s.snowluma_latest.as_ref());
                let tag = snowluma_github_release_tag(
                    latest,
                    state.snapshot.local_versions.snowluma.as_deref(),
                );
                if tag.is_empty() {
                    return Err(
                        "无法确定 SnowLuma 发布版本（GitHub 版本快照与本机已装版本均不可用）。\
                         请确认能访问 GitHub 并在概览等待版本检查完成后再安装。"
                            .to_string(),
                    );
                }
                let mut comp = SnowLumaComponent::for_windows(install, tag.clone());
                if let Some(sha) = latest
                    .and_then(|info| asset_sha256(info, &format!("SnowLuma-{tag}-win-x64.zip")))
                {
                    comp = comp.with_sha256(sha);
                }
                Arc::new(comp)
            } else {
                // 对齐 legacy SnowLumaRemotePaths:装到 $HOME/snowluma-remote/workspace
                // SnowLumaComponent::new 内部把 workspace 推出 snowluma 子目录
                let workspace = HostPath::from_posix(format!(
                    "{}/snowluma-remote/workspace",
                    require_remote_home(remote_home)?
                ));
                // 不能写死 latest/download/SnowLuma-linux-x64-lite.tar.gz:真实资产名带
                // 版本号(SnowLuma-v1.9.3-linux-x64-lite.tar.gz),无版本号的 URL 404,
                // 镜像代理把 404 页当 200 转发,下载器没 hash 拦就把 HTML 当 tar.gz 上传,
                // 远端 tar 解压报 "not in gzip format"和 Windows 分支一样从 release 快照
                // 拿 tag 拼对 URL + 反查 sha256,既修 404 又补上内容校验(双保险)
                let latest = snapshot.as_ref().and_then(|s| s.snowluma_latest.as_ref());
                let tag = snowluma_github_release_tag(
                    latest,
                    state.snapshot.local_versions.snowluma.as_deref(),
                );
                if tag.is_empty() {
                    return Err(
                        "无法确定 SnowLuma 发布版本（GitHub 版本快照与本机已装版本均不可用）。\
                         请确认能访问 GitHub 并在概览等待版本检查完成后再安装。"
                            .to_string(),
                    );
                }
                let asset = format!("SnowLuma-{tag}-linux-x64-lite.tar.gz");
                let url =
                    format!("https://github.com/SnowLuma/SnowLuma/releases/download/{tag}/{asset}");
                let mut comp = SnowLumaComponent::new(workspace, url);
                if let Some(sha) = latest.and_then(|info| asset_sha256(info, &asset)) {
                    comp = comp.with_sha256(sha);
                }
                Arc::new(comp)
            }
        }
        ComponentId::Qq => {
            if host.os() == ncd_host::Os::Windows {
                // 本机 Windows:detect/install 走注册表 + pcConfig,不用 Napcat
                // 远端 layoutinstall_base_dir 仅 Linux 解包路径会读,这里占位即可
                let _unused = data_root_host.join("runtime").join("_qq_win_stub");
                Arc::new(QQComponent::default_v3_2_25(_unused))
            } else {
                // 远端 / Linux 本地 QQ 跟随 NapCat layout(Rootless → $HOME/Napcat/opt/QQ)
                Arc::new(QQComponent::default_v3_2_25(resolve_napcat_base()?))
            }
        }
        ComponentId::NodeJs => {
            // SnowLuma 才需要 Node.js;装到 SnowLuma workspace 下
            let install_dir = HostPath::from_posix(format!(
                "{}/snowluma-remote/workspace/node",
                require_remote_home(remote_home)?
            ));
            Arc::new(NodeJsComponent::new("22.12.0", install_dir))
        }
        ComponentId::NoVnc => Arc::new(NoVncComponent::new()),
        ComponentId::DesktopSelf => {
            Arc::new(DesktopSelfComponent::from_env().unwrap_or_else(|_| {
                DesktopSelfComponent::new(
                    env!("CARGO_PKG_VERSION"),
                    HostPath::from_posix("NapCatQQ-Desktop"),
                )
            }))
        }
    };
    Ok(component)
}

/// 远端 Rootless 安装必须有可信 $HOME探不到就 fail-fast,不回退 /root——避免把
/// NapCat / QQ / SnowLuma / Node.js 装到错误目录(/root 通常无权限或污染 root 家目录)
fn require_remote_home(remote_home: Option<&str>) -> Result<&str, String> {
    remote_home.ok_or_else(|| {
        "无法探测远端 $HOME,已拒绝回退到 /root 安装(避免组件落到错误目录)。\
         请确认远端 SSH 用户有正常的家目录后重试。"
            .to_string()
    })
}

/// GitHub SnowLuma release 路径段必须带 v 前缀;package.json 的 version 常是 1.9.5
fn normalize_github_release_tag(raw: &str) -> String {
    let t = raw.trim();
    if t.is_empty() {
        return String::new();
    }
    if t.starts_with('v') || t.starts_with('V') {
        t.to_string()
    } else {
        format!("v{t}")
    }
}

/// 远端/本机 SnowLuma 安装 URL 用的 tag:优先 release 快照,否则本机已装版本(仅作兜底)
fn snowluma_github_release_tag(
    latest: Option<&ReleaseInfo>,
    local_version: Option<&str>,
) -> String {
    if let Some(info) = latest {
        if !info.tag.is_empty() {
            return normalize_github_release_tag(&info.tag);
        }
        if !info.version.is_empty() {
            return normalize_github_release_tag(&info.version);
        }
    }
    local_version
        .map(normalize_github_release_tag)
        .unwrap_or_default()
}

/// 在 ReleaseInfo 的 assets 里按文件名反查 sha256,命中且非空才返回
fn asset_sha256(info: &ReleaseInfo, name: &str) -> Option<String> {
    info.assets
        .iter()
        .find(|a| a.name == name)
        .map(|a| a.sha256.clone())
        .filter(|s| !s.is_empty())
}

/// 把 std::path::PathBuf(AppState.data_root)转成 HostPath,按 host 当前
/// 平台决定字符串风格data_root 由 bootstrap::resolve_data_root 决定,
/// 不会自己再次推断 —— 严格遵守路径落盘红线
fn data_root_to_host_path(data_root: &std::path::Path, os: ncd_host::Os) -> HostPath {
    let s = data_root.to_string_lossy();
    match os {
        ncd_host::Os::Windows => HostPath::from_windows(&s),
        // Linux / Mac:data_root 在新工程里只在 Windows ProgramData 域使用,
        // 真用 LinuxLocalHost 时再决定;当前直接当作 POSIX 字符串透传
        _ => HostPath::from_posix(s.into_owned()),
    }
}

async fn install_qq_dependencies_task(
    host: &dyn Host,
    packages: Vec<String>,
    sudo_password: Option<String>,
    task_ctx: DeploymentTaskContext,
) -> DeploymentTaskRunResult {
    if host.os() != Os::Linux {
        return DeploymentTaskRunResult::failed(
            "QQ dependencies installation is only supported on Linux",
        );
    }

    push_task_progress(&task_ctx, ProgressKind::Started { total_steps: 1 }).await;
    push_task_progress(
        &task_ctx,
        ProgressKind::StepBegin {
            step: 1,
            message: "安装 QQ 系统依赖".into(),
        },
    )
    .await;

    let packages = if packages.is_empty() {
        push_task_progress(
            &task_ctx,
            ProgressKind::Log {
                level: ProgressLogLevel::Info,
                message: "检测 QQ 系统依赖".into(),
            },
        )
        .await;
        let manifest = ncd_component::qq_deps::qq_qqnt_dependencies_v3_2_25();
        let detector = ncd_component::qq_deps::QqDependencyDetector::new(manifest);
        match detector.detect(host, None).await {
            Ok(report) => report.missing.into_iter().map(|p| p.name).collect(),
            Err(err) => {
                let msg = format!("检测 QQ 系统依赖失败: {err}");
                push_task_progress(
                    &task_ctx,
                    ProgressKind::Log {
                        level: ProgressLogLevel::Error,
                        message: msg.clone(),
                    },
                )
                .await;
                push_task_progress(&task_ctx, ProgressKind::StepEnd { step: 1, ok: false }).await;
                push_task_progress(&task_ctx, ProgressKind::Finished { ok: false }).await;
                return DeploymentTaskRunResult::failed(msg);
            }
        }
    } else {
        packages
    };

    if packages.is_empty() {
        push_task_progress(
            &task_ctx,
            ProgressKind::Log {
                level: ProgressLogLevel::Info,
                message: "QQ 系统依赖已满足".into(),
            },
        )
        .await;
        push_task_progress(&task_ctx, ProgressKind::StepEnd { step: 1, ok: true }).await;
        push_task_progress(&task_ctx, ProgressKind::Finished { ok: true }).await;
        return DeploymentTaskRunResult::ok("QQ 系统依赖已满足");
    }

    push_task_progress(
        &task_ctx,
        ProgressKind::Log {
            level: ProgressLogLevel::Info,
            message: format!(
                "缺失 {} 个 QQ 系统依赖: {}",
                packages.len(),
                packages.join(", ")
            ),
        },
    )
    .await;

    let (mut ctx, mut rx) = ncd_component::ActionCtx::new();
    let task_ctx_for_progress = task_ctx.clone();
    tauri::async_runtime::spawn(async move {
        while let Some(progress_event) = rx.recv().await {
            task_ctx_for_progress.push_progress(progress_event).await;
        }
    });

    let installer = ncd_component::qq_deps::QqDependencyInstaller;
    let result = installer
        .install(host, packages, sudo_password.as_deref(), &mut ctx)
        .await;

    match result {
        Ok(result) if result.elevation_required => {
            let msg = "安装 QQ 系统依赖需要 sudo 密码，请在远端主机配置中保存 sudo 密码后重试"
                .to_string();
            push_task_progress(
                &task_ctx,
                ProgressKind::Log {
                    level: ProgressLogLevel::Error,
                    message: msg.clone(),
                },
            )
            .await;
            push_task_progress(&task_ctx, ProgressKind::StepEnd { step: 1, ok: false }).await;
            push_task_progress(&task_ctx, ProgressKind::Finished { ok: false }).await;
            DeploymentTaskRunResult::failed(msg)
        }
        Ok(result) if result.success => {
            push_task_progress(
                &task_ctx,
                ProgressKind::Log {
                    level: ProgressLogLevel::Info,
                    message: format!("QQ 系统依赖安装成功: {}", result.installed.join(", ")),
                },
            )
            .await;
            push_task_progress(&task_ctx, ProgressKind::StepEnd { step: 1, ok: true }).await;
            push_task_progress(&task_ctx, ProgressKind::Finished { ok: true }).await;
            DeploymentTaskRunResult::ok(format!("已安装 {} 个 QQ 系统依赖", result.installed.len()))
        }
        Ok(result) => {
            let failed_list = result
                .failed
                .iter()
                .map(|f| format!("{}: {}", f.name, f.reason))
                .collect::<Vec<_>>()
                .join(", ");
            let msg = if failed_list.is_empty() {
                "QQ 系统依赖安装失败".to_string()
            } else {
                format!("部分 QQ 系统依赖安装失败: {failed_list}")
            };
            push_task_progress(
                &task_ctx,
                ProgressKind::Log {
                    level: ProgressLogLevel::Error,
                    message: msg.clone(),
                },
            )
            .await;
            push_task_progress(&task_ctx, ProgressKind::StepEnd { step: 1, ok: false }).await;
            push_task_progress(&task_ctx, ProgressKind::Finished { ok: false }).await;
            DeploymentTaskRunResult::failed(msg)
        }
        Err(err) => {
            let msg = format!("安装 QQ 系统依赖失败: {err}");
            push_task_progress(
                &task_ctx,
                ProgressKind::Log {
                    level: ProgressLogLevel::Error,
                    message: msg.clone(),
                },
            )
            .await;
            push_task_progress(&task_ctx, ProgressKind::StepEnd { step: 1, ok: false }).await;
            push_task_progress(&task_ctx, ProgressKind::Finished { ok: false }).await;
            DeploymentTaskRunResult::failed(msg)
        }
    }
}

async fn run_qq_dependency_install_for_command(
    host: &dyn Host,
    packages: Vec<String>,
    sudo_password: Option<String>,
    task_ctx: DeploymentTaskContext,
) -> Result<InstallDependenciesResult, String> {
    push_task_progress(&task_ctx, ProgressKind::Started { total_steps: 1 }).await;
    push_task_progress(
        &task_ctx,
        ProgressKind::StepBegin {
            step: 1,
            message: "安装 QQ 系统依赖".into(),
        },
    )
    .await;
    push_task_progress(
        &task_ctx,
        ProgressKind::Log {
            level: ProgressLogLevel::Info,
            message: if packages.is_empty() {
                "安装 QQ 系统依赖: 未指定包列表".into()
            } else {
                format!("安装 QQ 系统依赖: {}", packages.join(", "))
            },
        },
    )
    .await;

    let (mut ctx, mut rx) = ncd_component::ActionCtx::new();
    let task_ctx_for_progress = task_ctx.clone();
    tauri::async_runtime::spawn(async move {
        while let Some(progress_event) = rx.recv().await {
            task_ctx_for_progress.push_progress(progress_event).await;
        }
    });

    let installer = ncd_component::qq_deps::QqDependencyInstaller;
    let result = installer
        .install(host, packages, sudo_password.as_deref(), &mut ctx)
        .await
        .map_err(|e| e.to_string())?;

    let ok = result.success && !result.elevation_required;
    if ok {
        push_task_progress(
            &task_ctx,
            ProgressKind::Log {
                level: ProgressLogLevel::Info,
                message: format!("QQ 系统依赖安装成功: {}", result.installed.join(", ")),
            },
        )
        .await;
    }
    push_task_progress(&task_ctx, ProgressKind::StepEnd { step: 1, ok }).await;
    push_task_progress(&task_ctx, ProgressKind::Finished { ok }).await;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn component_spec(component_id: ComponentId, kind: StepKind) -> ComponentTaskSpec {
        ComponentTaskSpec { component_id, kind }
    }

    /// catalog 顺序 + 元素数量必须稳定:前端按数组顺序渲染卡片
    #[test]
    fn normalize_github_release_tag_adds_v_prefix() {
        assert_eq!(normalize_github_release_tag("1.9.5"), "v1.9.5");
        assert_eq!(normalize_github_release_tag("v1.9.5"), "v1.9.5");
        assert_eq!(normalize_github_release_tag(""), "");
    }

    #[test]
    fn catalog_returns_six_items_in_expected_order() {
        let list = catalog();
        let ids: Vec<ComponentId> = list.iter().map(|info| info.id).collect();
        assert_eq!(
            ids,
            vec![
                ComponentId::NapCat,
                ComponentId::SnowLuma,
                ComponentId::NodeJs,
                ComponentId::Qq,
                ComponentId::NoVnc,
                ComponentId::DesktopSelf,
            ]
        );
    }

    #[test]
    fn component_runtime_prerequisites_match_native_runtime_chains() {
        assert_eq!(
            component_runtime_prerequisites(
                ComponentId::NapCat,
                StepKind::EnsureInstalled,
                Os::Windows,
                Locality::Local,
            ),
            vec![component_spec(ComponentId::Qq, StepKind::EnsureInstalled)]
        );
        assert_eq!(
            component_runtime_prerequisites(
                ComponentId::SnowLuma,
                StepKind::EnsureInstalled,
                Os::Windows,
                Locality::Local,
            ),
            vec![component_spec(ComponentId::Qq, StepKind::EnsureInstalled)]
        );
        assert_eq!(
            component_runtime_prerequisites(
                ComponentId::NapCat,
                StepKind::EnsureInstalled,
                Os::Linux,
                Locality::Remote,
            ),
            vec![component_spec(ComponentId::Qq, StepKind::EnsureInstalled)]
        );
        assert_eq!(
            component_runtime_prerequisites(
                ComponentId::SnowLuma,
                StepKind::EnsureInstalled,
                Os::Linux,
                Locality::Remote,
            ),
            vec![
                component_spec(ComponentId::NodeJs, StepKind::EnsureInstalled),
                component_spec(ComponentId::Qq, StepKind::EnsureInstalled),
                component_spec(ComponentId::NoVnc, StepKind::EnsureInstalled),
            ]
        );
    }

    #[test]
    fn component_runtime_prerequisites_only_apply_to_install_like_actions() {
        for kind in [StepKind::Verify, StepKind::Uninstall, StepKind::EnsureDependencies] {
            assert!(
                component_runtime_prerequisites(
                    ComponentId::SnowLuma,
                    kind,
                    Os::Linux,
                    Locality::Remote,
                )
                .is_empty(),
                "{kind:?} must not auto-submit runtime prerequisites"
            );
        }
    }

    #[test]
    fn force_install_keeps_runtime_prerequisites_as_ensure_installed() {
        assert_eq!(
            component_runtime_prerequisites(
                ComponentId::SnowLuma,
                StepKind::ForceInstall,
                Os::Linux,
                Locality::Remote,
            ),
            vec![
                component_spec(ComponentId::NodeJs, StepKind::EnsureInstalled),
                component_spec(ComponentId::Qq, StepKind::EnsureInstalled),
                component_spec(ComponentId::NoVnc, StepKind::EnsureInstalled),
            ]
        );
    }

    #[test]
    fn update_keeps_runtime_prerequisites_as_ensure_installed() {
        assert_eq!(
            component_runtime_prerequisites(
                ComponentId::SnowLuma,
                StepKind::Update,
                Os::Linux,
                Locality::Remote,
            ),
            vec![
                component_spec(ComponentId::NodeJs, StepKind::EnsureInstalled),
                component_spec(ComponentId::Qq, StepKind::EnsureInstalled),
                component_spec(ComponentId::NoVnc, StepKind::EnsureInstalled),
            ]
        );
        assert_eq!(
            component_runtime_prerequisites(
                ComponentId::NapCat,
                StepKind::Update,
                Os::Windows,
                Locality::Local,
            ),
            vec![component_spec(ComponentId::Qq, StepKind::EnsureInstalled)]
        );
    }

    #[test]
    fn collected_snowluma_remote_prerequisites_are_deduped_in_dependency_order() {
        let chain = collect_component_runtime_prerequisites(
            component_spec(ComponentId::SnowLuma, StepKind::EnsureInstalled),
            Os::Linux,
            Locality::Remote,
        );

        assert_eq!(
            chain,
            vec![
                component_spec(ComponentId::NodeJs, StepKind::EnsureInstalled),
                component_spec(ComponentId::Qq, StepKind::EnsureInstalled),
                component_spec(ComponentId::NoVnc, StepKind::EnsureInstalled),
            ]
        );
    }

    #[test]
    fn direct_runtime_dependency_ids_return_only_direct_component_tasks() {
        let submitted = vec![
            (
                component_spec(ComponentId::NodeJs, StepKind::EnsureInstalled),
                "node-task".to_string(),
            ),
            (
                component_spec(ComponentId::Qq, StepKind::EnsureInstalled),
                "qq-task".to_string(),
            ),
            (
                component_spec(ComponentId::NoVnc, StepKind::EnsureInstalled),
                "novnc-task".to_string(),
            ),
        ];

        let ids = direct_runtime_dependency_ids(
            component_spec(ComponentId::SnowLuma, StepKind::EnsureInstalled),
            Os::Linux,
            Locality::Remote,
            &submitted,
        );

        assert_eq!(ids, vec!["node-task", "qq-task", "novnc-task"]);
    }

    #[test]
    fn linux_archive_component_actions_create_visible_package_prerequisites() {
        assert_eq!(
            component_package_prerequisites(
                ComponentId::NapCat,
                StepKind::EnsureInstalled,
                Os::Linux
            ),
            vec![SystemPackagePrerequisite::ArchiveTool {
                command: "unzip",
                package: "unzip",
            }]
        );
        assert_eq!(
            component_package_prerequisites(ComponentId::NodeJs, StepKind::Update, Os::Linux),
            vec![SystemPackagePrerequisite::ArchiveTool {
                command: "tar",
                package: "tar",
            }]
        );
        assert!(
            component_package_prerequisites(
                ComponentId::NapCat,
                StepKind::EnsureInstalled,
                Os::Windows
            )
            .is_empty()
        );
        assert!(
            component_package_prerequisites(ComponentId::NapCat, StepKind::Verify, Os::Linux)
                .is_empty()
        );
    }

    #[test]
    fn linux_qq_install_creates_dependency_prerequisite() {
        assert_eq!(
            component_package_prerequisites(ComponentId::Qq, StepKind::ForceInstall, Os::Linux),
            vec![SystemPackagePrerequisite::QqDependencies]
        );
    }

    #[test]
    fn component_package_manager_resources_cover_direct_pkg_commands_only() {
        let resources = component_task_resources(
            ComponentId::NoVnc,
            "remote:a",
            StepKind::Uninstall,
            Os::Linux,
            Locality::Remote,
        );
        assert!(resources.contains(&DeploymentTaskResource::PackageManager {
            host_id: "remote:a".to_string(),
        }));

        let resources = component_task_resources(
            ComponentId::NapCat,
            "remote:a",
            StepKind::EnsureInstalled,
            Os::Linux,
            Locality::Remote,
        );
        assert!(
            !resources.contains(&DeploymentTaskResource::PackageManager {
                host_id: "remote:a".to_string(),
            })
        );

        let resources = component_task_resources(
            ComponentId::Qq,
            "remote:a",
            StepKind::EnsureDependencies,
            Os::Linux,
            Locality::Remote,
        );
        assert!(resources.contains(&DeploymentTaskResource::PackageManager {
            host_id: "remote:a".to_string(),
        }));
    }

    #[test]
    fn component_cancellable_matches_safe_runtime_stop_support() {
        assert!(!component_action_cancellable(
            ComponentId::NoVnc,
            StepKind::EnsureInstalled,
            Os::Linux,
            Locality::Remote,
        ));
        assert!(!component_action_cancellable(
            ComponentId::Qq,
            StepKind::EnsureDependencies,
            Os::Linux,
            Locality::Remote,
        ));
        assert!(component_action_cancellable(
            ComponentId::NapCat,
            StepKind::EnsureInstalled,
            Os::Linux,
            Locality::Remote,
        ));
        assert!(component_action_cancellable(
            ComponentId::Qq,
            StepKind::EnsureInstalled,
            Os::Linux,
            Locality::Remote,
        ));
    }

    /// catalog 中每个 ComponentInfo 的 supported_targets 必须与对应
    /// Component trait 的 supported_targets 完全一致;防止两边漂移
    #[test]
    fn catalog_supported_targets_match_components() {
        let pairs: Vec<(ComponentInfo, Arc<dyn Component>)> = vec![
            (
                NapCatComponent::info(),
                Arc::new(NapCatComponent::new(HostPath::from_posix("/x"))),
            ),
            (
                SnowLumaComponent::info(),
                Arc::new(SnowLumaComponent::new(
                    HostPath::from_posix("/x"),
                    "https://example.com/x.tar.gz",
                )),
            ),
            (
                NodeJsComponent::info(),
                Arc::new(NodeJsComponent::new("22.12.0", HostPath::from_posix("/x"))),
            ),
            (
                QQComponent::info(),
                Arc::new(QQComponent::default_v3_2_25(HostPath::from_posix("/x"))),
            ),
            (NoVncComponent::info(), Arc::new(NoVncComponent::new())),
        ];
        for (info, component) in pairs {
            let from_trait: Vec<(ncd_host::Os, ncd_host::Locality)> =
                component.supported_targets().to_vec();
            let from_info: Vec<(ncd_host::Os, ncd_host::Locality)> = info
                .supported_targets
                .iter()
                .map(|st| (st.os, st.locality))
                .collect();
            assert_eq!(
                from_info, from_trait,
                "ComponentInfo::supported_targets diverged from Component::supported_targets for {:?}",
                info.id
            );
        }
    }

    #[test]
    fn list_components_returns_six_items() {
        // tauri::command 内部就是调 catalog(),本测试直接验等价
        let result = catalog();
        assert_eq!(result.len(), 6);
    }
}

/// 检测 QQ 系统依赖(仅 Linux 远端)
#[tauri::command]
pub async fn detect_qq_dependencies(
    host_id: String,
    state: State<'_, AppState>,
) -> Result<QqDependencyReport, String> {
    let host = resolve_host_with_autoconnect(&host_id, &state).await?;

    if host.os() != ncd_host::Os::Linux {
        return Err("QQ dependencies check is only supported on Linux".to_string());
    }

    let manifest = ncd_component::qq_deps::qq_qqnt_dependencies_v3_2_25();
    let detector = ncd_component::qq_deps::QqDependencyDetector::new(manifest);

    let report = detector
        .detect(host.as_ref(), None)
        .await
        .map_err(|e| e.to_string())?;

    Ok(report)
}

/// 安装 QQ 系统依赖(仅 Linux 远端)
///
/// sudo_password: 前端弹框收集到的 sudo 密码None 时后端自动从 keyring 找该
/// 服务器的缓存密码两边都没有且远端确实需要密码时,返回 elevation_required=true
/// 让前端弹框
#[tauri::command]
pub async fn install_qq_dependencies(
    host_id: String,
    packages: Vec<String>,
    sudo_password: Option<String>,
    state: State<'_, AppState>,
) -> Result<InstallDependenciesResult, String> {
    let host = resolve_host_with_autoconnect(&host_id, &state).await?;

    if host.os() != ncd_host::Os::Linux {
        return Err("QQ dependencies installation is only supported on Linux".to_string());
    }

    // 有效密码:用户显式输入优先,fallback 到 keyring 缓存的 sudo 密码
    let effective_password = sudo_password.clone().or_else(|| {
        host_id
            .strip_prefix("remote:")
            .and_then(|id| state.server_manager.sudo_password(id))
    });

    tracing::info!(
        "[install_qq_dependencies] host={}, sudo_password={}, effective_password={}",
        host_id,
        sudo_password
            .as_ref()
            .map(|_| "<provided>")
            .unwrap_or("<none>"),
        effective_password
            .as_ref()
            .map(|_| "<resolved>")
            .unwrap_or("<none>")
    );

    let (tx, rx) = oneshot::channel::<Result<InstallDependenciesResult, String>>();
    let task_id = Uuid::new_v4().to_string();
    let requested_task_id = task_id.clone();
    let server_id = host_id.strip_prefix("remote:").map(str::to_string);
    let server_manager = Arc::clone(&state.server_manager);
    let local_host = if server_id.is_none() {
        Some(host)
    } else {
        None
    };
    let submitted = state
        .deployment_tasks
        .submit(DeploymentTaskRequest {
            task_id: task_id.clone(),
            kind: DeploymentTaskKind::SystemPackage {
                package_group: "qq_dependencies".to_string(),
            },
            host_id: host_id.clone(),
            title: "安装 QQ 系统依赖".to_string(),
            resources: vec![
                DeploymentTaskResource::PackageManager {
                    host_id: host_id.clone(),
                },
                DeploymentTaskResource::InstallTarget {
                    host_id: host_id.clone(),
                    target: "system_package:qq_dependencies".to_string(),
                },
            ],
            depends_on: vec![],
            dedupe_key: Some(format!("system-package:{host_id}:qq_dependencies")),
            cancellable: false,
            runner: Box::new(move |task_ctx| {
                Box::pin(async move {
                    let result = if let Some(id) = server_id {
                        let packages = packages.clone();
                        let effective_password = effective_password.clone();
                        server_manager
                            .with_isolated_connection(&id, move |iso_host| {
                                let task_ctx = task_ctx.clone();
                                Box::pin(async move {
                                    run_qq_dependency_install_for_command(
                                        iso_host.as_ref(),
                                        packages,
                                        effective_password,
                                        task_ctx,
                                    )
                                    .await
                                })
                            })
                            .await
                    } else if let Some(host) = local_host {
                        run_qq_dependency_install_for_command(
                            host.as_ref(),
                            packages,
                            effective_password,
                            task_ctx,
                        )
                        .await
                    } else {
                        Err("无法解析 QQ 依赖安装目标主机".to_string())
                    };

                    let run_result = match &result {
                        Ok(result) if result.success => {
                            DeploymentTaskRunResult::ok("QQ 系统依赖已就绪")
                        }
                        Ok(result) if result.elevation_required => {
                            DeploymentTaskRunResult::failed("安装 QQ 系统依赖需要 sudo 密码")
                        }
                        Ok(result) => {
                            let failed = result
                                .failed
                                .iter()
                                .map(|f| format!("{}: {}", f.name, f.reason))
                                .collect::<Vec<_>>()
                                .join(", ");
                            DeploymentTaskRunResult::failed(if failed.is_empty() {
                                "QQ 系统依赖安装失败".to_string()
                            } else {
                                format!("部分 QQ 系统依赖安装失败: {failed}")
                            })
                        }
                        Err(err) => DeploymentTaskRunResult::failed(err.clone()),
                    };
                    let _ = tx.send(result);
                    run_result
                })
            }),
        })
        .await;

    if submitted != requested_task_id {
        return Err("该主机已有 QQ 系统依赖安装任务在队列中".to_string());
    }

    rx.await
        .map_err(|_| "QQ 系统依赖安装任务异常结束".to_string())?
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
