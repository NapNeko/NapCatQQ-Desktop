//! 组件安装策略 / 工厂 / 依赖图与解析 / 包管理锁。

pub mod action_policy;
pub mod factory;
pub mod graph;
pub mod package_lock;
pub mod resolver;

pub use action_policy::{
    ComponentTaskSpec, RemoteHostProbe, RemoteLayout, SystemPackagePrerequisite, asset_sha256,
    collect_component_runtime_prerequisites, collect_component_runtime_prerequisites_for,
    component_action_cancellable, component_action_needs_runtime_closure, component_catalog,
    component_dedupe_key, component_needs_download_slot, component_needs_package_manager,
    component_package_prerequisites, component_runtime_prerequisites, component_task_resources,
    data_root_to_host_path, direct_runtime_dependency_ids, direct_runtime_dependency_ids_for,
    infer_snowluma_linux_package, normalize_github_release_tag, parse_remote_host_probe_stdout,
    require_remote_home, snowluma_github_release_tag, snowluma_linux_release_asset,
    snowluma_windows_release_asset,
};
pub use factory::{BuildComponentCtx, build_component_for_host};
pub use graph::{
    ClosureNode, GRAPH_COMPONENT_IDS, catalog_version_reqs_for, graph_component,
    render_dependency_graph, requirement_closure,
};
pub use resolver::{ComponentBuilder, ResolveCtx, resolve_dependencies};
