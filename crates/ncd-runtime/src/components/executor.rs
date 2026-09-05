//! 组件动作执行器:把 DependencyPlan 变成 deployment task 队列
//!
//! 从 L4 commands/components 下沉。一次动作会:
//! 1. resolve(root, Install) 探出闭包里每个节点的状态
//! 2. 没满足的组件节点 → EnsureInstalled 任务;主机命令 / 系统包节点 → 系统包任务
//! 3. depends_on 按图反推:谁要它,谁的任务就等它;root 任务最后提交
//! 4. dedupe_key 命中已在跑的直接复用,不重复排队

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ncd_component::{
    ActionCtx, Component, ComponentId, DependencyTarget, ProgressEvent, ProgressKind,
    ProgressLogLevel, RequirementPhase,
};
use ncd_deploy::{DeployOutcome, DeployPlan, StepKind};
use ncd_domain::release_snapshot::ReleaseSnapshot;
use ncd_domain::{
    AppSettings, DeploymentTaskKind, DeploymentTaskResource, InstallDependenciesResult,
    RemoteInventory, RemoteSelectedPaths, SnowLumaLinuxPackage,
};
use ncd_host::{Host, Locality};
use ncd_server::ServerManager;
use ncd_traits::ConfigStore;
use tokio::sync::{Mutex, RwLock, oneshot};
use tokio_util::sync::CancellationToken;

use crate::components::action_policy::{
    RemoteLayout, component_action_cancellable, component_action_needs_runtime_closure,
    component_dedupe_key, component_task_resources,
};
use crate::components::factory::{BuildComponentCtx, build_component_for_host};
use crate::components::resolver::{ResolveCtx, resolve_dependencies};
use crate::components::system_package::{
    qq_install_failure_message, run_qq_dependency_install_for_command, run_system_package_task,
    system_package_group, system_package_title,
};
use crate::config_store_impl::LocalConfigStore;
use crate::deploy::tasks::{
    DeploymentTaskContext, DeploymentTaskManager, DeploymentTaskRequest, DeploymentTaskRunResult,
};
use crate::events::{BroadcastEventBus, DomainEvent, EventBus};

/// 实例化组件所需的全部输入,owned,能跨 task 边界。L4 从 AppState 收集一次
#[derive(Debug, Clone)]
pub struct ComponentBuildInputs {
    pub data_root: PathBuf,
    pub remote_home: Option<String>,
    pub layout: RemoteLayout,
    pub snapshot: Option<ReleaseSnapshot>,
    pub local_snowluma_version: Option<String>,
    pub desktop_product_version: String,
    pub selected: Option<RemoteSelectedPaths>,
    /// 本次动作明确的 SnowLuma 包;None 交给 factory 按库存推断
    pub snowluma_linux_package: Option<SnowLumaLinuxPackage>,
    pub snowluma_node_path: Option<String>,
}

impl ComponentBuildInputs {
    pub fn build(&self, id: ComponentId, host: &dyn Host) -> Result<Arc<dyn Component>, String> {
        build_component_for_host(
            id,
            &BuildComponentCtx {
                data_root: &self.data_root,
                host,
                remote_home: self.remote_home.as_deref(),
                layout: self.layout,
                snapshot: self.snapshot.as_ref(),
                local_snowluma_version: self.local_snowluma_version.as_deref(),
                desktop_product_version: &self.desktop_product_version,
                selected: self.selected.as_ref(),
                snowluma_linux_package: self.snowluma_linux_package,
                snowluma_node_path: self.snowluma_node_path.as_deref(),
            },
        )
    }
}

pub struct ComponentActionRequest {
    pub component_id: ComponentId,
    pub host_id: String,
    pub kind: StepKind,
    /// 前端预生成的 task id;None 则随机
    pub task_id: Option<String>,
    pub host: Arc<dyn Host>,
    pub inputs: ComponentBuildInputs,
}

/// 组件页所有会排任务的动作都从这里走;AppState 持有一份
pub struct ComponentExecutor {
    deployment_tasks: DeploymentTaskManager,
    server_manager: Arc<ServerManager>,
    event_bus: BroadcastEventBus,
    /// task_id → ActionCtx 的取消令牌;cancel 命令用
    active_tasks: Arc<Mutex<HashMap<String, CancellationToken>>>,
    /// 动作完成后清掉该 host 的布局缓存(安装可能改变布局)
    host_probe_cache: Arc<Mutex<HashMap<String, RemoteInventory>>>,
    app_settings: Arc<RwLock<AppSettings>>,
    data_root: PathBuf,
}

impl ComponentExecutor {
    pub fn new(
        deployment_tasks: DeploymentTaskManager,
        server_manager: Arc<ServerManager>,
        event_bus: BroadcastEventBus,
        active_tasks: Arc<Mutex<HashMap<String, CancellationToken>>>,
        host_probe_cache: Arc<Mutex<HashMap<String, RemoteInventory>>>,
        app_settings: Arc<RwLock<AppSettings>>,
        data_root: PathBuf,
    ) -> Self {
        Self {
            deployment_tasks,
            server_manager,
            event_bus,
            active_tasks,
            host_probe_cache,
            app_settings,
            data_root,
        }
    }

    /// 提交 root 动作及其未满足的前置;返回 root 的 task id
    pub async fn submit(&self, req: ComponentActionRequest) -> Result<String, String> {
        let ComponentActionRequest {
            component_id,
            host_id,
            kind,
            task_id,
            host,
            inputs,
        } = req;

        let dedupe = component_dedupe_key(&host_id, component_id, kind);
        if let Some(existing) = self.deployment_tasks.active_task_by_dedupe_key(&dedupe).await {
            return Ok(existing);
        }

        let root = inputs.build(component_id, host.as_ref())?;
        let mut submitted: HashMap<DependencyTarget, String> = HashMap::new();

        if component_action_needs_runtime_closure(kind) {
            let build = |id: ComponentId| inputs.build(id, host.as_ref());
            let plan = resolve_dependencies(
                root.as_ref(),
                &ResolveCtx {
                    host: host.as_ref(),
                    phase: RequirementPhase::Install,
                    build: &build,
                },
            )
            .await;

            for node in &plan.nodes {
                if !node.status.needs_action() {
                    tracing::info!(
                        host_id,
                        target = %node.target.label(),
                        status = ?node.status,
                        "skip satisfied prerequisite"
                    );
                    continue;
                }
                // 拓扑序保证本节点依赖的节点已处理完(要么跳过要么已排队)
                let depends_on: Vec<String> = match node.target.component_id() {
                    Some(id) => plan
                        .nodes
                        .iter()
                        .filter(|n| n.required_by.contains(&id))
                        .filter_map(|n| submitted.get(&n.target).cloned())
                        .collect(),
                    None => Vec::new(),
                };
                let task_id = match node.target.component_id() {
                    Some(id) => {
                        self.submit_component_task(
                            id,
                            &host_id,
                            StepKind::EnsureInstalled,
                            None,
                            depends_on,
                            Arc::clone(&host),
                            &inputs,
                        )
                        .await?
                    }
                    None => {
                        self.submit_system_package_task(
                            node.target.clone(),
                            &host_id,
                            Arc::clone(&host),
                        )
                        .await
                    }
                };
                submitted.insert(node.target.clone(), task_id);
            }
        }

        let depends_on: Vec<String> = root
            .requirements(host.os(), host.locality())
            .into_iter()
            .filter(|req| req.applies_to(RequirementPhase::Install))
            .filter_map(|req| submitted.get(&req.target()).cloned())
            .collect();

        self.submit_component_task(
            component_id,
            &host_id,
            kind,
            task_id,
            depends_on,
            host,
            &inputs,
        )
        .await
    }

    pub async fn cancel(&self, task_id: &str) -> Result<(), String> {
        let token = self.active_tasks.lock().await.get(task_id).cloned();
        if let Some(t) = token {
            t.cancel();
        }
        self.deployment_tasks.cancel(task_id).await
    }

    /// 用户显式「安装 QQ 依赖」:排一个系统包任务并等它跑完,把结果原样带回。
    /// 没传 sudo 密码时用远端主机配置里记住的那份
    pub async fn install_qq_dependencies(
        &self,
        host_id: &str,
        host: Arc<dyn Host>,
        packages: Vec<String>,
        sudo_password: Option<String>,
    ) -> Result<InstallDependenciesResult, String> {
        let (tx, rx) = oneshot::channel::<Result<InstallDependenciesResult, String>>();
        let task_id = uuid_v4();
        let server_id = host_id.strip_prefix("remote:").map(str::to_string);
        let sudo_password = sudo_password.or_else(|| {
            server_id
                .as_deref()
                .and_then(|id| self.server_manager.sudo_password(id))
        });
        let server_manager = Arc::clone(&self.server_manager);
        let local_host = if server_id.is_none() { Some(host) } else { None };
        let group = "qq_dependencies";
        let submitted = self
            .deployment_tasks
            .submit(DeploymentTaskRequest {
                task_id: task_id.clone(),
                kind: DeploymentTaskKind::SystemPackage {
                    package_group: group.to_string(),
                },
                host_id: host_id.to_string(),
                title: "安装 QQ 系统依赖".to_string(),
                resources: system_package_resources(host_id, group),
                depends_on: vec![],
                dedupe_key: Some(system_package_dedupe_key(host_id, group)),
                cancellable: false,
                runner: Box::new(move |task_ctx| {
                    Box::pin(async move {
                        let result = if let Some(id) = server_id {
                            server_manager
                                .with_isolated_connection(&id, move |iso_host| {
                                    Box::pin(async move {
                                        run_qq_dependency_install_for_command(
                                            iso_host.as_ref(),
                                            packages,
                                            sudo_password,
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
                                sudo_password,
                                task_ctx,
                            )
                            .await
                        } else {
                            Err("无法解析 QQ 依赖安装目标主机".to_string())
                        };

                        let run_result = match &result {
                            Ok(r) if r.success => DeploymentTaskRunResult::ok("QQ 系统依赖已就绪"),
                            Ok(r) if r.elevation_required => {
                                DeploymentTaskRunResult::failed("安装 QQ 系统依赖需要 sudo 密码")
                            }
                            Ok(r) => DeploymentTaskRunResult::failed(qq_install_failure_message(r)),
                            Err(err) => DeploymentTaskRunResult::failed(err.clone()),
                        };
                        let _ = tx.send(result);
                        run_result
                    })
                }),
            })
            .await;

        if submitted != task_id {
            return Err("该主机已有 QQ 系统依赖安装任务在队列中".to_string());
        }
        rx.await
            .map_err(|_| "QQ 系统依赖安装任务异常结束".to_string())?
    }

    async fn submit_system_package_task(
        &self,
        target: DependencyTarget,
        host_id: &str,
        host: Arc<dyn Host>,
    ) -> String {
        let group = system_package_group(&target).unwrap_or_else(|| target.label());
        let title = system_package_title(&target);
        let server_id = host_id.strip_prefix("remote:").map(str::to_string);
        let server_manager = Arc::clone(&self.server_manager);
        self.deployment_tasks
            .submit(DeploymentTaskRequest {
                task_id: uuid_v4(),
                kind: DeploymentTaskKind::SystemPackage {
                    package_group: group.clone(),
                },
                host_id: host_id.to_string(),
                title,
                resources: system_package_resources(host_id, &group),
                depends_on: vec![],
                dedupe_key: Some(system_package_dedupe_key(host_id, &group)),
                cancellable: false,
                runner: Box::new(move |task_ctx| {
                    Box::pin(async move {
                        if let Some(id) = server_id {
                            let target = target.clone();
                            server_manager
                                .with_isolated_connection(&id, move |iso_host| {
                                    Box::pin(async move {
                                        Ok(run_system_package_task(
                                            target,
                                            iso_host.as_ref(),
                                            task_ctx,
                                        )
                                        .await)
                                    })
                                })
                                .await
                                .unwrap_or_else(DeploymentTaskRunResult::failed)
                        } else {
                            run_system_package_task(target, host.as_ref(), task_ctx).await
                        }
                    })
                }),
            })
            .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn submit_component_task(
        &self,
        component_id: ComponentId,
        host_id: &str,
        kind: StepKind,
        requested_task_id: Option<String>,
        depends_on: Vec<String>,
        host: Arc<dyn Host>,
        inputs: &ComponentBuildInputs,
    ) -> Result<String, String> {
        let dedupe_key = component_dedupe_key(host_id, component_id, kind);
        if let Some(existing) = self
            .deployment_tasks
            .active_task_by_dedupe_key(&dedupe_key)
            .await
        {
            return Ok(existing);
        }

        let component = inputs.build(component_id, host.as_ref())?;
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

        let task_id = requested_task_id.unwrap_or_else(uuid_v4);
        let resources = component_task_resources(
            component_id,
            &host_id_owned,
            kind,
            host.os(),
            host.locality(),
        );
        let cancellable =
            component_action_cancellable(component_id, kind, host.os(), host.locality());
        let title = format!("{} {}", component_id.as_str(), kind.as_str());

        let runner = ComponentTaskRunner {
            component_id,
            kind,
            plan,
            host,
            server_id,
            remote_long_install,
            snowluma_linux_package: inputs.snowluma_linux_package,
            host_probe_cache: Arc::clone(&self.host_probe_cache),
            probe_cache_key: host_id_owned.clone(),
            event_bus: self.event_bus.clone(),
            active_tasks: Arc::clone(&self.active_tasks),
            app_settings: Arc::clone(&self.app_settings),
            data_root: self.data_root.clone(),
            server_manager: Arc::clone(&self.server_manager),
        };

        let submitted = self
            .deployment_tasks
            .submit(DeploymentTaskRequest {
                task_id,
                kind: DeploymentTaskKind::ComponentAction {
                    component_id: component_id.as_str().to_string(),
                    action: kind.as_str().to_string(),
                },
                host_id: host_id_owned,
                title,
                resources,
                depends_on,
                dedupe_key: Some(dedupe_key),
                cancellable,
                runner: Box::new(move |task_ctx| Box::pin(runner.run(task_ctx))),
            })
            .await;
        Ok(submitted)
    }
}

/// 单个组件任务 runner 需要的全部状态(owned,进 task 闭包)
struct ComponentTaskRunner {
    component_id: ComponentId,
    kind: StepKind,
    plan: DeployPlan,
    host: Arc<dyn Host>,
    server_id: Option<String>,
    /// 远端长安装走独立 SSH 连接,不占共享会话
    remote_long_install: bool,
    snowluma_linux_package: Option<SnowLumaLinuxPackage>,
    host_probe_cache: Arc<Mutex<HashMap<String, RemoteInventory>>>,
    probe_cache_key: String,
    event_bus: BroadcastEventBus,
    active_tasks: Arc<Mutex<HashMap<String, CancellationToken>>>,
    app_settings: Arc<RwLock<AppSettings>>,
    data_root: PathBuf,
    server_manager: Arc<ServerManager>,
}

impl ComponentTaskRunner {
    async fn run(mut self, task_ctx: DeploymentTaskContext) -> DeploymentTaskRunResult {
        let (mut ctx, mut rx) = ActionCtx::new();
        let cancel_token = ctx.cancel_token();
        let task_id = task_ctx.task_id().to_string();
        self.active_tasks
            .lock()
            .await
            .insert(task_id.clone(), cancel_token.clone());

        // 队列取消 → ActionCtx 取消
        let task_cancel = task_ctx.cancel_token();
        let action_cancel = cancel_token.clone();
        tokio::spawn(async move {
            task_cancel.cancelled().await;
            action_cancel.cancel();
        });

        // ActionCtx 进度 → task 记录 + 领域事件
        let event_bus = self.event_bus.clone();
        let progress_ctx = task_ctx.clone();
        let progress_task_id = task_id.clone();
        tokio::spawn(async move {
            while let Some(event) = rx.recv().await {
                progress_ctx.push_progress(event.clone()).await;
                event_bus.publish(DomainEvent::component_action_progress(
                    progress_task_id.clone(),
                    event,
                ));
            }
        });

        if cancel_token.is_cancelled() {
            self.active_tasks.lock().await.remove(&task_id);
            self.emit(&task_ctx, &task_id, ProgressKind::Finished { ok: false })
                .await;
            return DeploymentTaskRunResult::failed("任务已取消");
        }

        // plan 进 'static 闭包要 owned;取出来换个空的占位
        let plan = std::mem::replace(&mut self.plan, DeployPlan::builder().build());
        let outcome: Result<DeployOutcome, String> = if self.remote_long_install {
            let Some(id) = self.server_id.clone() else {
                return DeploymentTaskRunResult::failed("missing remote server id");
            };
            self.server_manager
                .with_isolated_connection(&id, move |iso_host| {
                    Box::pin(async move {
                        plan.run(iso_host.as_ref(), &mut ctx)
                            .await
                            .map_err(|e| format!("{e}"))
                    })
                })
                .await
        } else {
            plan.run(self.host.as_ref(), &mut ctx)
                .await
                .map_err(|e| format!("{e}"))
        };

        if outcome.is_err() {
            if let Some(ref id) = self.server_id {
                self.server_manager.disconnect_cached_host(id).await;
            }
        }

        self.active_tasks.lock().await.remove(&task_id);
        self.host_probe_cache
            .lock()
            .await
            .remove(&self.probe_cache_key);

        match outcome {
            Ok(outcome) if outcome.ok => {
                if self.component_id == ComponentId::SnowLuma
                    && self.host.locality() == Locality::Local
                    && matches!(
                        self.kind,
                        StepKind::EnsureInstalled
                            | StepKind::ForceInstall
                            | StepKind::Update
                            | StepKind::Uninstall
                    )
                {
                    let next = if self.kind == StepKind::Uninstall {
                        None
                    } else {
                        self.snowluma_linux_package
                    };
                    if let Err(err) =
                        persist_local_snowluma_package(&self.data_root, &self.app_settings, next)
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
                self.emit(
                    &task_ctx,
                    &task_id,
                    ProgressKind::Log {
                        level: ProgressLogLevel::Error,
                        message: format!("plan failed: {err}"),
                    },
                )
                .await;
                self.emit(&task_ctx, &task_id, ProgressKind::Finished { ok: false })
                    .await;
                DeploymentTaskRunResult::failed(err)
            }
        }
    }

    async fn emit(&self, task_ctx: &DeploymentTaskContext, task_id: &str, kind: ProgressKind) {
        let event = ProgressEvent::new(kind);
        task_ctx.push_progress(event.clone()).await;
        self.event_bus
            .publish(DomainEvent::component_action_progress(task_id.to_string(), event));
    }
}

/// 本机 SnowLuma 装完 / 卸完后把包类型写回 app-settings.json
async fn persist_local_snowluma_package(
    data_root: &Path,
    app_settings: &Arc<RwLock<AppSettings>>,
    package: Option<SnowLumaLinuxPackage>,
) -> Result<(), String> {
    let mut settings = app_settings.read().await.clone();
    settings.snowluma_package = package;
    let store = LocalConfigStore::new(data_root);
    let path = store.config_dir().join("app-settings.json");
    let payload = serde_json::to_value(&settings).map_err(|e| e.to_string())?;
    store
        .write_json_atomic(&path, &payload)
        .map_err(|e| e.to_string())?;
    *app_settings.write().await = settings;
    Ok(())
}

/// 本机 SnowLuma 目录里有 node.exe 就是完整包;没装按完整包(不会平白拉 Node)
pub fn infer_local_snowluma_package(data_root: &Path) -> SnowLumaLinuxPackage {
    let install_dir = data_root.join("components").join("SnowLuma");
    if install_dir.is_dir() && !install_dir.join("node.exe").is_file() {
        SnowLumaLinuxPackage::Lite
    } else {
        SnowLumaLinuxPackage::Full
    }
}

fn system_package_resources(host_id: &str, group: &str) -> Vec<DeploymentTaskResource> {
    vec![
        DeploymentTaskResource::PackageManager {
            host_id: host_id.to_string(),
        },
        DeploymentTaskResource::InstallTarget {
            host_id: host_id.to_string(),
            target: format!("system_package:{group}"),
        },
    ]
}

fn system_package_dedupe_key(host_id: &str, group: &str) -> String {
    format!("system-package:{host_id}:{group}")
}

fn uuid_v4() -> String {
    uuid::Uuid::new_v4().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infer_local_package_defaults_to_full_when_not_installed() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(
            infer_local_snowluma_package(tmp.path()),
            SnowLumaLinuxPackage::Full
        );
        let dir = tmp.path().join("components").join("SnowLuma");
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(
            infer_local_snowluma_package(tmp.path()),
            SnowLumaLinuxPackage::Lite
        );
        std::fs::write(dir.join("node.exe"), b"x").unwrap();
        assert_eq!(
            infer_local_snowluma_package(tmp.path()),
            SnowLumaLinuxPackage::Full
        );
    }
}
