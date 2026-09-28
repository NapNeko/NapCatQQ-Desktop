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

use ncd_appframework::AppFrameworkRegistry;
use ncd_component::{
    ActionCtx, Component, ComponentId, DependencyPlan, DependencyTarget, ProgressEvent,
    ProgressKind, ProgressLogLevel, RequirementPhase, RuntimeReadiness, VersionReq,
};
use ncd_deploy::{DeployOutcome, DeployPlan, StepKind};
use ncd_domain::release_snapshot::ReleaseSnapshot;
use ncd_domain::{
    AppSettings, DeploymentTaskKind, DeploymentTaskResource, InstallDependenciesResult,
    RemoteSelectedPaths, SnowLumaLinuxPackage,
};
use ncd_host::{Host, Locality, Os};
use ncd_server::ServerManager;
use tokio::sync::{RwLock, oneshot};
use tokio::task::JoinHandle;
use tokio_util::sync::{CancellationToken, DropGuard};

use crate::components::action_policy::{
    RemoteHostProbe, RemoteLayout, component_action_cancellable,
    component_action_needs_runtime_closure, component_dedupe_key, component_needs_package_manager,
    component_target_label, component_task_resources, dependency_target_display_name,
};
use crate::components::active_tasks::ActiveTasks;
use crate::components::factory::{AppComponentHint, BuildComponentCtx, build_component_for_host};
use crate::components::graph::catalog_version_reqs_for;
use crate::components::resolver::{ResolveCtx, resolve_dependencies, resolve_runtime_readiness};
use crate::components::system_package::{
    qq_install_failure_message, run_qq_dependency_install_for_command, run_system_package_task,
    system_package_group, system_package_title,
};
use crate::deploy::tasks::{
    DeploymentTaskContext, DeploymentTaskManager, DeploymentTaskRequest, DeploymentTaskRunResult,
};
use crate::events::{BroadcastEventBus, DomainEvent, EventBus};
use crate::release::read_cached_release_snapshot;
use crate::remote::inventory::RemoteInventoryService;

/// 实例化组件所需的全部输入,owned,能跨 task 边界。由 ComponentExecutor::build_inputs 收集
#[derive(Clone)]
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
    /// 应用端实例（Karin 等）的目录 / 端口；组件页动作为 None
    pub app_component: Option<AppComponentHint>,
    /// 与 AppManager 同一份注册表,应用端组件的适配器从这里取
    pub registry: Arc<AppFrameworkRegistry>,
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
                app_component: self.app_component.as_ref(),
                registry: &self.registry,
            },
        )
    }

    /// 任务去重 / 资源的实例作用域：只有应用端组件带
    fn scope_for(&self, id: ComponentId) -> Option<&str> {
        if id.is_app_framework() {
            self.app_component.as_ref().map(|h| h.instance_id.as_str())
        } else {
            None
        }
    }

    /// root 在 host 上按 phase 的依赖状态;root 自己构建失败才 Err
    pub async fn resolve(
        &self,
        root: ComponentId,
        host: &dyn Host,
        phase: RequirementPhase,
    ) -> Result<DependencyPlan, String> {
        let root = self.build(root, host)?;
        let build = |id: ComponentId| self.build(id, host);
        Ok(resolve_dependencies(
            root.as_ref(),
            &ResolveCtx {
                host,
                phase,
                build: &build,
                registry: &self.registry,
            },
        )
        .await)
    }

    /// root 现在能不能跑(root 自己 + Run 依赖)
    pub async fn readiness(
        &self,
        root: ComponentId,
        host: &dyn Host,
    ) -> Result<RuntimeReadiness, String> {
        let root = self.build(root, host)?;
        let build = |id: ComponentId| self.build(id, host);
        Ok(resolve_runtime_readiness(
            root.as_ref(),
            &ResolveCtx {
                host,
                phase: RequirementPhase::Run,
                build: &build,
                registry: &self.registry,
            },
        )
        .await)
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

/// ComponentExecutor 的构造参数;启动时凑齐一次
pub struct ComponentExecutorDeps {
    pub deployment_tasks: DeploymentTaskManager,
    pub server_manager: Arc<ServerManager>,
    pub event_bus: BroadcastEventBus,
    /// 本机 SnowLuma 的包类型 / Node 覆盖路径从这里读,装卸完回写包类型
    pub app_settings: Arc<RwLock<AppSettings>>,
    pub data_root: PathBuf,
    /// 启动时读到的本机 SnowLuma 版本;GitHub 版本快照拿不到时靠它定 release tag
    pub local_snowluma_version: Option<String>,
    /// Desktop 产品版本(构建时注入),DesktopSelf 组件用
    pub desktop_product_version: String,
    /// 和 AppManager 共用的应用端注册表
    pub registry: Arc<AppFrameworkRegistry>,
}

/// 组件页所有会排任务的动作都从这里走;启动时建一份,AppState 与 Bot 启动预检共用
pub struct ComponentExecutor {
    deployment_tasks: DeploymentTaskManager,
    server_manager: Arc<ServerManager>,
    event_bus: BroadcastEventBus,
    active_tasks: ActiveTasks,
    /// 动作完成后清掉该主机的库存副本(安装可能改变布局)
    inventory: Arc<RemoteInventoryService>,
    app_settings: Arc<RwLock<AppSettings>>,
    data_root: PathBuf,
    local_snowluma_version: Option<String>,
    desktop_product_version: String,
    registry: Arc<AppFrameworkRegistry>,
}

impl ComponentExecutor {
    pub fn new(deps: ComponentExecutorDeps) -> Self {
        let ComponentExecutorDeps {
            deployment_tasks,
            server_manager,
            event_bus,
            app_settings,
            data_root,
            local_snowluma_version,
            desktop_product_version,
            registry,
        } = deps;
        Self {
            deployment_tasks,
            inventory: Arc::new(RemoteInventoryService::new(Arc::clone(&server_manager))),
            server_manager,
            event_bus,
            active_tasks: ActiveTasks::default(),
            app_settings,
            data_root,
            local_snowluma_version,
            desktop_product_version,
            registry,
        }
    }

    /// 依赖图里所有会用到 `target` 的组件(含已注册应用端)对它的版本约束
    pub fn catalog_version_reqs_for(
        &self,
        target: ComponentId,
        os: Os,
        locality: Locality,
    ) -> Vec<VersionReq> {
        catalog_version_reqs_for(&self.registry, target, os, locality)
    }

    pub fn inventory(&self) -> &RemoteInventoryService {
        &self.inventory
    }

    /// 给 Bot 启动路由共用同一份库存副本
    pub fn shared_inventory(&self) -> Arc<RemoteInventoryService> {
        Arc::clone(&self.inventory)
    }

    pub fn active_tasks(&self) -> &ActiveTasks {
        &self.active_tasks
    }

    /// 在已探好的布局上收集构建输入;本机 SnowLuma 的包类型与 Node 覆盖路径来自设置
    pub async fn build_inputs(
        &self,
        host: &dyn Host,
        probe: &RemoteHostProbe,
        selected: Option<RemoteSelectedPaths>,
        snowluma_linux_package: Option<SnowLumaLinuxPackage>,
    ) -> ComponentBuildInputs {
        let local = host.locality() == Locality::Local;
        let (persisted_package, snowluma_node_path) = if local {
            let settings = self.app_settings.read().await;
            (
                settings.snowluma_package,
                local_snowluma_node_path(&settings).map(str::to_string),
            )
        } else {
            (None, None)
        };
        let snowluma_linux_package = snowluma_linux_package.or_else(|| {
            local.then(|| {
                persisted_package.unwrap_or_else(|| infer_local_snowluma_package(&self.data_root))
            })
        });
        ComponentBuildInputs {
            data_root: self.data_root.clone(),
            remote_home: probe.home.clone(),
            layout: probe.layout,
            snapshot: read_cached_release_snapshot(&self.data_root),
            local_snowluma_version: self.local_snowluma_version.clone(),
            desktop_product_version: self.desktop_product_version.clone(),
            selected,
            snowluma_linux_package,
            snowluma_node_path,
            app_component: None,
            registry: Arc::clone(&self.registry),
        }
    }

    /// 取(或探测)主机库存再收集构建输入;远端探不到时按本机默认布局构建
    pub async fn inputs_for(
        &self,
        host_id: &str,
        host: &dyn Host,
        snowluma_linux_package: Option<SnowLumaLinuxPackage>,
    ) -> ComponentBuildInputs {
        let (probe, selected) = self.inventory.host_probe_or_default(host_id, host).await;
        self.build_inputs(host, &probe, selected, snowluma_linux_package)
            .await
    }

    /// root 现在能不能跑(root 自己 + Run 依赖);组件页门禁和 Bot 启动预检共用。
    /// 远端库存探不到直接报错:按猜的布局算出的「缺什么」对不上那台机器
    pub async fn runtime_readiness(
        &self,
        root: ComponentId,
        host_id: &str,
        host: &dyn Host,
    ) -> Result<RuntimeReadiness, String> {
        let (probe, selected) = self.inventory.host_probe(host_id, host).await?;
        self.build_inputs(host, &probe, selected, None)
            .await
            .readiness(root, host)
            .await
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

        let dedupe =
            component_dedupe_key(&host_id, component_id, kind, inputs.scope_for(component_id));
        if let Some(existing) = self
            .deployment_tasks
            .active_task_by_dedupe_key(&dedupe)
            .await
        {
            return Ok(existing);
        }

        let root = inputs.build(component_id, host.as_ref())?;
        let mut submitted: HashMap<DependencyTarget, String> = HashMap::new();

        if component_action_needs_runtime_closure(kind) {
            let plan = inputs
                .resolve(component_id, host.as_ref(), RequirementPhase::Install)
                .await?;

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

        // root 直接依赖里被排了任务的:既是 depends_on,也是给 UI 看的「在等谁」
        let waiting_on: Vec<(DependencyTarget, String)> = root
            .requirements(host.os(), host.locality())
            .into_iter()
            .filter(|req| req.applies_to(RequirementPhase::Install))
            .filter_map(|req| {
                let target = req.target();
                submitted.get(&target).cloned().map(|id| (target, id))
            })
            .collect();
        let depends_on: Vec<String> = waiting_on.iter().map(|(_, id)| id.clone()).collect();
        let queue_note = queue_note(
            &waiting_on,
            component_needs_package_manager(component_id, kind, host.os(), host.locality()),
        );

        let requested = task_id.clone();
        let submitted_id = self
            .submit_component_task(
                component_id,
                &host_id,
                kind,
                task_id,
                depends_on,
                host,
                &inputs,
            )
            .await?;

        // dedupe 命中已有任务时不补写,免得往别人的日志里塞排队提示
        let fresh = requested.as_deref().is_none_or(|t| t == submitted_id);
        if let (true, Some(message)) = (fresh, queue_note) {
            self.event_bus
                .publish(DomainEvent::component_action_progress(
                    submitted_id.clone(),
                    ProgressEvent::new(ProgressKind::Log {
                        level: ProgressLogLevel::Info,
                        message,
                    }),
                ));
        }
        Ok(submitted_id)
    }

    /// 队列里的任务只按队列的规矩取消:在跑的不可取消任务由队列拒绝,可取消的经
    /// forward_cancel 转给 ActionCtx。先直接掐 ActionCtx 的话,拒绝了也已经停下。
    /// 队列不认识的只有 Desktop 自更新,它只登记在活跃表里
    pub async fn cancel(&self, task_id: &str) -> Result<(), String> {
        if self.deployment_tasks.status_of(task_id).await.is_some() {
            return self.deployment_tasks.cancel(task_id).await;
        }
        if self.active_tasks.cancel(task_id) {
            Ok(())
        } else {
            Err("任务不存在或已结束".to_string())
        }
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
        let local_host = if server_id.is_none() {
            Some(host)
        } else {
            None
        };
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
        let scope = inputs.scope_for(component_id);
        let dedupe_key = component_dedupe_key(host_id, component_id, kind, scope);
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
            scope,
        );
        let cancellable =
            component_action_cancellable(component_id, kind, host.os(), host.locality());
        let title = format!(
            "{} {}",
            component_target_label(component_id, scope),
            kind.as_str()
        );

        let runner = ComponentTaskRunner {
            component_id,
            kind,
            plan,
            host,
            server_id,
            remote_long_install,
            snowluma_linux_package: inputs.snowluma_linux_package,
            inventory: Arc::clone(&self.inventory),
            event_bus: self.event_bus.clone(),
            active_tasks: self.active_tasks.clone(),
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
    inventory: Arc<RemoteInventoryService>,
    event_bus: BroadcastEventBus,
    active_tasks: ActiveTasks,
    app_settings: Arc<RwLock<AppSettings>>,
    data_root: PathBuf,
    server_manager: Arc<ServerManager>,
}

impl ComponentTaskRunner {
    async fn run(mut self, task_ctx: DeploymentTaskContext) -> DeploymentTaskRunResult {
        let (mut ctx, mut rx) = ActionCtx::new();
        let cancel_token = ctx.cancel_token();
        let task_id = task_ctx.task_id().to_string();
        // 从哪条路返回都会摘掉,取消命令不会拿到已结束任务的令牌
        let registered = self
            .active_tasks
            .register(task_id.clone(), cancel_token.clone());
        let (_stop_cancel_forward, _) =
            forward_cancel(task_ctx.cancel_token(), cancel_token.clone());

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
            drop(registered);
            self.emit(&task_ctx, &task_id, ProgressKind::Finished { ok: false })
                .await;
            return DeploymentTaskRunResult::failed("任务已取消");
        }

        // plan 进 'static 闭包要 owned;取出来换个空的占位
        let plan = std::mem::replace(&mut self.plan, DeployPlan::builder().build());
        let isolated_server = self.server_id.clone().filter(|_| self.remote_long_install);
        let outcome: Result<DeployOutcome, String> = match isolated_server {
            Some(id) => {
                self.server_manager
                    .with_isolated_connection(&id, move |iso_host| {
                        Box::pin(async move {
                            plan.run(iso_host.as_ref(), &mut ctx)
                                .await
                                .map_err(|e| format!("{e}"))
                        })
                    })
                    .await
            }
            None => plan
                .run(self.host.as_ref(), &mut ctx)
                .await
                .map_err(|e| format!("{e}")),
        };

        if outcome.is_err() {
            if let Some(ref id) = self.server_id {
                self.server_manager.disconnect_cached_host(id).await;
            }
        }

        drop(registered);
        if let Some(ref id) = self.server_id {
            self.inventory.invalidate(id).await;
        }

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
            .publish(DomainEvent::component_action_progress(
                task_id.to_string(),
                event,
            ));
    }
}

/// 本机 SnowLuma 装完 / 卸完后把包类型写回 app-settings.json
async fn persist_local_snowluma_package(
    data_root: &Path,
    app_settings: &Arc<RwLock<AppSettings>>,
    package: Option<SnowLumaLinuxPackage>,
) -> Result<(), String> {
    crate::desktop::update_app_settings(data_root, app_settings, |s| {
        s.snowluma_package = package;
    })
    .await
    .map(|_| ())
}

/// 设置里本机 SnowLuma 的自定义 Node 路径;只填了空白当没设。
/// 启动、保存设置、组件探测都按这一个口径读
pub fn local_snowluma_node_path(settings: &AppSettings) -> Option<&str> {
    settings
        .snowluma_node_path
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty())
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

/// 队列取消转给 ActionCtx。guard 一丢转发就收尾:正常跑完的任务
/// 不会留下一个永远等着取消信号的 watcher
fn forward_cancel(from: CancellationToken, to: CancellationToken) -> (DropGuard, JoinHandle<()>) {
    let done = CancellationToken::new();
    let stop = done.clone();
    let handle = tokio::spawn(async move {
        tokio::select! {
            () = from.cancelled() => to.cancel(),
            () = stop.cancelled() => {}
        }
    });
    (done.drop_guard(), handle)
}

/// root 任务刚入队、还没开跑时给 UI 的一句话;没什么可等就 None。
/// 前置任务优先于包管理器锁:有前置时锁的等待藏在前置之后,不值得同时说
fn queue_note(
    waiting_on: &[(DependencyTarget, String)],
    needs_package_manager: bool,
) -> Option<String> {
    if !waiting_on.is_empty() {
        let labels: Vec<String> = waiting_on
            .iter()
            .map(|(t, _)| dependency_target_display_name(t))
            .collect();
        return Some(format!("等待前置:{}", labels.join("、")));
    }
    needs_package_manager.then(|| "排队等待包管理器空闲".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_note_prefers_prerequisites_over_package_manager() {
        assert_eq!(queue_note(&[], false), None);
        assert_eq!(
            queue_note(&[], true).as_deref(),
            Some("排队等待包管理器空闲")
        );
        let waiting = vec![
            (
                DependencyTarget::Component {
                    id: ComponentId::Qq,
                },
                "t1".to_string(),
            ),
            (
                DependencyTarget::HostCommand {
                    command: "tar".to_string(),
                    package: "tar".to_string(),
                },
                "t2".to_string(),
            ),
        ];
        assert_eq!(
            queue_note(&waiting, true).as_deref(),
            Some("等待前置:QQ、tar")
        );
    }

    #[tokio::test]
    async fn cancel_forward_passes_cancel_and_ends_with_its_guard() {
        let from = CancellationToken::new();
        let to = CancellationToken::new();
        let (_guard, handle) = forward_cancel(from.clone(), to.clone());
        from.cancel();
        handle.await.unwrap();
        assert!(to.is_cancelled());

        // 没人取消:guard 一丢 watcher 就退出,目标令牌不受影响
        let from = CancellationToken::new();
        let to = CancellationToken::new();
        let (guard, handle) = forward_cancel(from, to.clone());
        drop(guard);
        tokio::time::timeout(std::time::Duration::from_secs(1), handle)
            .await
            .expect("watcher should end once its guard is dropped")
            .unwrap();
        assert!(!to.is_cancelled());
    }

    fn test_executor(root: &Path) -> ComponentExecutor {
        let bus = BroadcastEventBus::default();
        ComponentExecutor::new(ComponentExecutorDeps {
            deployment_tasks: DeploymentTaskManager::new(bus.clone()),
            server_manager: Arc::new(ServerManager::new(
                root,
                Arc::new(ncd_server::InMemoryCredentialStore::default()),
            )),
            event_bus: bus,
            app_settings: Arc::new(RwLock::new(AppSettings::default())),
            data_root: root.to_path_buf(),
            local_snowluma_version: None,
            desktop_product_version: "0.0.0".into(),
            registry: Arc::new(AppFrameworkRegistry::new()),
        })
    }

    /// 排一个和 ComponentTaskRunner 一样登记令牌、接上队列转发的任务,
    /// 跑起来后一直等到令牌被取消;返回它的 ActionCtx 令牌
    async fn submit_waiting_task(
        executor: &ComponentExecutor,
        task_id: &str,
        cancellable: bool,
    ) -> CancellationToken {
        let (started_tx, started_rx) = oneshot::channel();
        let active = executor.active_tasks().clone();
        executor
            .deployment_tasks
            .submit(DeploymentTaskRequest {
                task_id: task_id.to_string(),
                kind: DeploymentTaskKind::ComponentAction {
                    component_id: "qq".into(),
                    action: "ensure_dependencies".into(),
                },
                host_id: "remote:s1".into(),
                title: "t".into(),
                resources: Vec::new(),
                depends_on: Vec::new(),
                dedupe_key: None,
                cancellable,
                runner: Box::new(move |task_ctx| {
                    Box::pin(async move {
                        let token = CancellationToken::new();
                        let _registered = active.register(task_ctx.task_id(), token.clone());
                        let (_forward, _) = forward_cancel(task_ctx.cancel_token(), token.clone());
                        let _ = started_tx.send(token.clone());
                        token.cancelled().await;
                        DeploymentTaskRunResult::failed("任务已取消")
                    })
                }),
            })
            .await;
        started_rx.await.expect("task should start")
    }

    #[tokio::test]
    async fn cancel_refused_by_queue_leaves_running_action_alone() {
        let tmp = tempfile::tempdir().unwrap();
        let executor = test_executor(tmp.path());
        let token = submit_waiting_task(&executor, "pkg", false).await;

        let err = executor.cancel("pkg").await.unwrap_err();

        assert!(err.contains("不能安全强制停止"));
        assert!(!token.is_cancelled());
    }

    #[tokio::test]
    async fn cancel_running_cancellable_task_goes_through_queue() {
        let tmp = tempfile::tempdir().unwrap();
        let executor = test_executor(tmp.path());
        let token = submit_waiting_task(&executor, "dl", true).await;

        executor.cancel("dl").await.unwrap();

        tokio::time::timeout(std::time::Duration::from_secs(1), token.cancelled())
            .await
            .expect("queue cancel should reach the action token");
    }

    #[tokio::test]
    async fn cancel_outside_queue_uses_active_table() {
        let tmp = tempfile::tempdir().unwrap();
        let executor = test_executor(tmp.path());
        let token = CancellationToken::new();
        let _registered = executor.active_tasks().register("desktop", token.clone());

        executor.cancel("desktop").await.unwrap();
        assert!(token.is_cancelled());
        assert!(executor.cancel("gone").await.is_err());
    }

    #[test]
    fn blank_node_path_setting_means_unset() {
        let mut settings = AppSettings::default();
        assert_eq!(local_snowluma_node_path(&settings), None);
        settings.snowluma_node_path = Some("   ".into());
        assert_eq!(local_snowluma_node_path(&settings), None);
        settings.snowluma_node_path = Some("  D:\\node\\node.exe ".into());
        assert_eq!(
            local_snowluma_node_path(&settings),
            Some("D:\\node\\node.exe")
        );
    }

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
