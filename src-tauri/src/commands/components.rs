//! Components 页 Tauri 命令薄壳层。
//!
//! 暴露 4 个命令给前端：
//! - `list_components`：返回所有 6 个 ComponentInfo 元数据（顺序：Framework
//!   → RuntimeDep → SelfApp）。
//! - `detect_component`：在指定 host 上探测某 component 的安装版本。
//! - `run_component_action`：把单 step DeployPlan 跑起来，进度走
//!   `DomainEvent::ComponentActionProgress`，立即返回 task_id。
//! - `cancel_component_action`：用 task_id 找到 cancel token 并 cancel。
//!
//! 所有错误都用 `format!("{}", err)` 转 String，不向前端泄漏 ActionError /
//! DeployError 的 enum 结构。

use std::sync::Arc;

use ncd_component::{
    Component, ComponentDetectResult, ComponentId, ComponentInfo, DesktopSelfComponent,
    LinuxQQComponent, NapCatComponent, NoVncComponent, NodeJsComponent, ProgressKind,
    SnowLumaComponent,
};
use ncd_deploy::{DeployPlan, StepKind};
use ncd_host::{Host, HostPath};
use ncd_runtime::{release::read_cached_release_snapshot, DomainEvent, EventBus, ReleaseInfo};
use tauri::State;
use uuid::Uuid;

use crate::AppState;

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
    let host = resolve_host(&host_id)?;
    let component = build_component_for_host(component_id, &state, host.as_ref());
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
    state: State<'_, AppState>,
) -> Result<String, String> {
    let host = resolve_host(&host_id)?;
    let component = build_component_for_host(component_id, &state, host.as_ref());

    let plan = DeployPlan::builder()
        .step("single", kind, Arc::clone(&component))
        .build();
    plan.validate().map_err(|err| format!("{err}"))?;

    let (mut ctx, mut rx) = ncd_component::ActionCtx::new();
    let cancel_token = ctx.cancel_token();
    let task_id = Uuid::new_v4().to_string();

    state
        .active_tasks
        .lock()
        .await
        .insert(task_id.clone(), cancel_token.clone());

    let event_bus = state.event_bus.clone();
    let active_tasks = Arc::clone(&state.active_tasks);

    // 进度转发：rx → DomainEvent::ComponentActionProgress
    let event_task_id = task_id.clone();
    let event_bus_for_progress = event_bus.clone();
    tauri::async_runtime::spawn(async move {
        while let Some(progress_event) = rx.recv().await {
            event_bus_for_progress.publish(DomainEvent::component_action_progress(
                event_task_id.clone(),
                progress_event,
            ));
        }
    });

    // plan 执行：完成 / 失败 / 取消都走同一回收路径
    let task_id_for_runner = task_id.clone();
    tauri::async_runtime::spawn(async move {
        let outcome = plan.run(host.as_ref(), &mut ctx).await;
        // 任意 case 都要清理 active_tasks 注册条目，避免长期内存泄漏。
        active_tasks.lock().await.remove(&task_id_for_runner);

        // plan.run 内部会 emit Finished 事件，但若它本身返回 Err
        // （比如 InvalidPlan / RollbackFailed），forward 通道就拿不到。
        // 这种情况下补发一个 Finished{ok:false} + 一条 Log 描述错误，
        // 保证前端 ActionProgressView 一定能终结。
        if let Err(err) = outcome {
            let progress_event = ncd_component::ProgressEvent::new(ProgressKind::Log {
                level: ncd_component::ProgressLogLevel::Error,
                message: format!("plan failed: {err}"),
            });
            event_bus.publish(DomainEvent::component_action_progress(
                task_id_for_runner.clone(),
                progress_event,
            ));
            let finished = ncd_component::ProgressEvent::new(ProgressKind::Finished { ok: false });
            event_bus.publish(DomainEvent::component_action_progress(
                task_id_for_runner.clone(),
                finished,
            ));
        }
    });

    Ok(task_id)
}

#[tauri::command]
pub async fn cancel_component_action(
    task_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let token = state.active_tasks.lock().await.get(&task_id).cloned();
    match token {
        Some(t) => {
            t.cancel();
            Ok(())
        }
        None => Err(format!("task not found: {task_id}")),
    }
}

/// 6 个组件元数据按 Framework → RuntimeDep → SelfApp 顺序返回。
fn catalog() -> Vec<ComponentInfo> {
    vec![
        NapCatComponent::info(),
        SnowLumaComponent::info(),
        NodeJsComponent::info(),
        LinuxQQComponent::info(),
        NoVncComponent::info(),
        DesktopSelfComponent::info(),
    ]
}

/// 把 component_id 实例化成具体 Component。
///
/// NapCat / SnowLuma 在 Windows 本机走"扁平 zip 部署"分支(legacy 同款),
/// 安装目录从 `state.data_root` 派生(对齐 `bootstrap::resolve_data_root`,
/// 红线 §4.1)。其余组件保持 Linux 默认假设 —— Components 页 v1 只在
/// Windows 本机和 Linux 远端两条路径上验证过,中间 case 留作后续工单。
fn build_component_for_host(
    id: ComponentId,
    state: &AppState,
    host: &dyn Host,
) -> Arc<dyn Component> {
    let data_root_host = data_root_to_host_path(&state.data_root, host.os());
    // 读 release 缓存反查 SHA256：缓存缺失 / 无 digest 时退化到"无 hash"分支，
    // 让安装链路走原有路径（race 仍尝试切 mirror，但失去内容级保护）。
    // 缓存由前端 useReleases hook 在启动/轮询时通过 get_release_snapshot 维护。
    let snapshot = read_cached_release_snapshot(&state.data_root);
    match id {
        ComponentId::NapCat => {
            if host.os() == ncd_host::Os::Windows {
                // legacy `PathFunc.napcat_path = data_path/runtime/NapCatQQ`。
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
                Arc::new(NapCatComponent::new(HostPath::from_posix("/home/napcat/Napcat")))
            }
        }
        ComponentId::SnowLuma => {
            if host.os() == ncd_host::Os::Windows {
                // legacy `PathFunc.snowluma_path = data_path/runtime/SnowLuma`;
                // tag 来源优先级：release 缓存的 latest tag → 已装版本 fallback
                // → 空串（install 阶段会拒绝）。已装版本不能直接拿来拼装 URL，
                // 因为它是当前安装的旧版，需要装的是 latest（这是 EOCD 调查
                // 顺带发现的二次 bug：之前永远拿旧 tag 拼 URL）。
                let install = data_root_host.join("runtime").join("SnowLuma");
                let latest = snapshot
                    .as_ref()
                    .and_then(|s| s.snowluma_latest.as_ref());
                let tag = latest
                    .map(|info| {
                        if !info.tag.is_empty() {
                            info.tag.clone()
                        } else {
                            format!("v{}", info.version)
                        }
                    })
                    .or_else(|| state.snapshot.local_versions.snowluma.clone())
                    .unwrap_or_default();
                let mut comp = SnowLumaComponent::for_windows(install, tag.clone());
                if let Some(sha) = latest.and_then(|info| {
                    asset_sha256(info, &format!("SnowLuma-{tag}-win-x64.zip"))
                }) {
                    comp = comp.with_sha256(sha);
                }
                Arc::new(comp)
            } else {
                Arc::new(SnowLumaComponent::new(
                    HostPath::from_posix("/home/napcat/Napcat/snowluma-workspace"),
                    "https://github.com/SnowLuma/SnowLuma/releases/latest/download/SnowLuma-linux-x64-lite.tar.gz",
                ))
            }
        }
        ComponentId::LinuxQq => Arc::new(LinuxQQComponent::default_v3_2_25(
            HostPath::from_posix("/home/napcat/Napcat"),
        )),
        ComponentId::NodeJs => Arc::new(NodeJsComponent::new(
            "20.10.0",
            HostPath::from_posix("/home/napcat/Napcat/usr/node"),
        )),
        ComponentId::NoVnc => Arc::new(NoVncComponent::new()),
        ComponentId::DesktopSelf => Arc::new(
            DesktopSelfComponent::from_env()
                .unwrap_or_else(|_| {
                    DesktopSelfComponent::new(
                        env!("CARGO_PKG_VERSION"),
                        HostPath::from_posix("ncd-tauri"),
                    )
                }),
        ),
    }
}

/// 在 ReleaseInfo 的 assets 里按文件名反查 sha256，命中且非空才返回。
fn asset_sha256(info: &ReleaseInfo, name: &str) -> Option<String> {
    info.assets
        .iter()
        .find(|a| a.name == name)
        .map(|a| a.sha256.clone())
        .filter(|s| !s.is_empty())
}

/// 把 std::path::PathBuf(`AppState.data_root`)转成 HostPath,按 host 当前
/// 平台决定字符串风格。data_root 由 `bootstrap::resolve_data_root` 决定,
/// 不会自己再次推断 —— 严格遵守路径落盘红线。
fn data_root_to_host_path(data_root: &std::path::Path, os: ncd_host::Os) -> HostPath {
    let s = data_root.to_string_lossy();
    match os {
        ncd_host::Os::Windows => HostPath::from_windows(&s),
        // Linux / Mac:data_root 在新工程里只在 Windows ProgramData 域使用,
        // 真用 LinuxLocalHost 时再决定;当前直接当作 POSIX 字符串透传。
        _ => HostPath::from_posix(s.into_owned()),
    }
}

/// host_id 字符串约定：
/// - `"local"`：本机 Host
/// - `"remote:<remote_id>"`：远端 SSH（v1 还未接入，先返回 Err）
///
/// 返回 `Arc<dyn Host>` 因为 `LocalWindowsHost` 只在 Windows 编译；
/// non-Windows 下当前没有本机 Host 实装，返回 Unsupported 错误。
fn resolve_host(host_id: &str) -> Result<Arc<dyn Host>, String> {
    match host_id {
        "local" => local_host(),
        other if other.starts_with("remote:") => {
            // 占位：远端 host 注册表 v1 暂未接入。
            Err("remote host registry not implemented".to_string())
        }
        _ => Err(format!("unknown host_id: {host_id}")),
    }
}

#[cfg(windows)]
fn local_host() -> Result<Arc<dyn Host>, String> {
    Ok(Arc::new(ncd_host::local::LocalWindowsHost::new()))
}

#[cfg(not(windows))]
fn local_host() -> Result<Arc<dyn Host>, String> {
    Err("local host on non-Windows targets is not yet implemented".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// catalog 顺序 + 元素数量必须稳定：前端按数组顺序渲染卡片。
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
                ComponentId::LinuxQq,
                ComponentId::NoVnc,
                ComponentId::DesktopSelf,
            ]
        );
    }

    /// catalog 中每个 ComponentInfo 的 supported_targets 必须与对应
    /// Component trait 的 supported_targets 完全一致；防止两边漂移。
    #[test]
    fn catalog_supported_targets_match_components() {
        let pairs: Vec<(ComponentInfo, Arc<dyn Component>)> = vec![
            (NapCatComponent::info(), Arc::new(NapCatComponent::new(HostPath::from_posix("/x")))),
            (
                SnowLumaComponent::info(),
                Arc::new(SnowLumaComponent::new(HostPath::from_posix("/x"), "https://example.com/x.tar.gz")),
            ),
            (NodeJsComponent::info(), Arc::new(NodeJsComponent::new("20.10.0", HostPath::from_posix("/x")))),
            (LinuxQQComponent::info(), Arc::new(LinuxQQComponent::default_v3_2_25(HostPath::from_posix("/x")))),
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
        // tauri::command 内部就是调 catalog()，本测试直接验等价。
        let result = catalog();
        assert_eq!(result.len(), 6);
    }

    #[tokio::test]
    async fn resolve_host_rejects_remote_with_clear_error() {
        match resolve_host("remote:production") {
            Err(err) => assert!(
                err.contains("remote host registry not implemented"),
                "unexpected error: {err}"
            ),
            Ok(_) => panic!("expected Err for remote host_id"),
        }
    }

    #[tokio::test]
    async fn resolve_host_rejects_unknown_id() {
        match resolve_host("does-not-exist") {
            Err(err) => assert!(err.contains("unknown host_id"), "unexpected error: {err}"),
            Ok(_) => panic!("expected Err for unknown host_id"),
        }
    }
}
