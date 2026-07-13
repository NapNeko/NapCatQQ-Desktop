//! 组件安装策略 / 工厂 / 包管理锁。

pub mod action_policy;
pub mod factory;
pub mod package_lock;

pub use action_policy::{
    ComponentTaskSpec, RemoteHostProbe, RemoteLayout, SystemPackagePrerequisite, asset_sha256,
    collect_component_runtime_prerequisites, component_action_cancellable,
    component_action_needs_runtime_closure, component_catalog, component_dedupe_key,
    component_needs_download_slot, component_needs_package_manager, component_package_prerequisites,
    component_runtime_prerequisites, component_task_resources, data_root_to_host_path,
    direct_runtime_dependency_ids, normalize_github_release_tag, parse_remote_host_probe_stdout,
    require_remote_home, snowluma_github_release_tag,
};
pub use factory::build_component_for_host;
