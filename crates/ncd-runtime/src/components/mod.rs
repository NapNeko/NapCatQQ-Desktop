//! 组件安装策略 / 工厂 / 依赖图与解析 / 动作执行器 / 包管理锁。

pub mod action_policy;
pub mod executor;
pub mod factory;
pub mod graph;
pub mod package_lock;
pub mod resolver;
pub mod system_package;

pub use action_policy::{
    RemoteHostProbe, RemoteLayout, asset_sha256, component_action_cancellable,
    component_action_needs_runtime_closure, component_catalog, component_dedupe_key,
    component_needs_download_slot, component_needs_package_manager, component_task_resources,
    data_root_to_host_path, infer_snowluma_linux_package, normalize_github_release_tag,
    parse_remote_host_probe_stdout, require_remote_home, snowluma_github_release_tag,
    snowluma_linux_release_asset, snowluma_windows_release_asset,
};
pub use executor::{
    ComponentActionRequest, ComponentBuildInputs, ComponentExecutor, infer_local_snowluma_package,
};
pub use factory::{BuildComponentCtx, build_component_for_host};
pub use graph::{
    ClosureNode, GRAPH_COMPONENT_IDS, catalog_version_reqs_for, graph_component,
    render_dependency_graph, requirement_closure,
};
pub use resolver::{ComponentBuilder, ResolveCtx, resolve_dependencies};
