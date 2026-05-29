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
    let host = resolve_host_with_autoconnect(&host_id, &state).await?;
    let remote_home = probe_remote_home_if_needed(&host_id, host.as_ref()).await;
    let layout = probe_napcat_layout(&host_id, host.as_ref(), remote_home.as_deref()).await;
    let component = build_component_for_host(
        component_id,
        &state,
        host.as_ref(),
        remote_home.as_deref(),
        layout,
    );
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
    let host = resolve_host_with_autoconnect(&host_id, &state).await?;
    let remote_home = probe_remote_home_if_needed(&host_id, host.as_ref()).await;
    let layout = probe_napcat_layout(&host_id, host.as_ref(), remote_home.as_deref()).await;
    let component = build_component_for_host(
        component_id,
        &state,
        host.as_ref(),
        remote_home.as_deref(),
        layout,
    );

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

/// 仅在远端 host 上探测 $HOME（轻量级，失败回退 None 让组件使用绝对路径默认值）。
async fn probe_remote_home_if_needed(host_id: &str, host: &dyn Host) -> Option<String> {
    if !host_id.starts_with("remote:") {
        return None;
    }
    let cmd = ncd_host::HostCommand::new("sh").arg("-c").arg("echo $HOME");
    match host.run_to_string(cmd).await {
        Ok(out) if out.success() => {
            let home = out.stdout.trim().to_string();
            if home.is_empty() { None } else { Some(home) }
        }
        _ => None,
    }
}

/// 远端 NapCat / LinuxQQ 的安装布局：system 是官方 NapCat-Installer 风格
/// （/opt/QQ，需要 sudo），rootless 是 NapCat-TUI-CLI 风格（$HOME/Napcat，
/// 不需要 sudo，本工程默认）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RemoteLayout {
    /// 系统安装：/opt/QQ/...，install_base_dir = "/"
    System,
    /// 用户安装：$HOME/Napcat/opt/QQ/...，install_base_dir = "$HOME/Napcat"
    Rootless,
}

/// 远端 NapCat 安装布局自动探测：
///
/// 1. 先看 /opt/QQ/resources/app/app_launcher/napcat/napcat.mjs（system，
///    用户已用 NapCat-Installer.py 装过）
/// 2. 再看 $HOME/Napcat/opt/QQ/resources/app/app_launcher/napcat/napcat.mjs
///    （rootless，用户已用 NapCat-TUI-CLI 装过）
/// 3. 都没有 → 默认 Rootless（从零安装走 TUI 风格，不需要 sudo，简单）
async fn probe_napcat_layout(
    host_id: &str,
    host: &dyn Host,
    home: Option<&str>,
) -> RemoteLayout {
    if !host_id.starts_with("remote:") {
        return RemoteLayout::Rootless;
    }
    // 尽量用 host.exists 而不是 shell test，SFTP 一次 stat 比 SSH 起 shell 快。
    let system_mjs = ncd_host::HostPath::from_posix(
        "/opt/QQ/resources/app/app_launcher/napcat/napcat.mjs",
    );
    if matches!(host.exists(&system_mjs).await, Ok(true)) {
        return RemoteLayout::System;
    }
    if let Some(h) = home {
        let rootless_mjs = ncd_host::HostPath::from_posix(format!(
            "{h}/Napcat/opt/QQ/resources/app/app_launcher/napcat/napcat.mjs"
        ));
        if matches!(host.exists(&rootless_mjs).await, Ok(true)) {
            return RemoteLayout::Rootless;
        }
    }
    RemoteLayout::Rootless
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
    remote_home: Option<&str>,
    layout: RemoteLayout,
) -> Arc<dyn Component> {
    let data_root_host = data_root_to_host_path(&state.data_root, host.os());
    // 读 release 缓存反查 SHA256：缓存缺失 / 无 digest 时退化到"无 hash"分支，
    // 让安装链路走原有路径（race 仍尝试切 mirror，但失去内容级保护）。
    // 缓存由前端 useReleases hook 在启动/轮询时通过 get_release_snapshot 维护。
    let snapshot = read_cached_release_snapshot(&state.data_root);

    // 远端 NapCat / LinuxQQ 共用 install_base_dir：layout 决定 / 还是 $HOME/Napcat。
    let napcat_base = match layout {
        RemoteLayout::System => HostPath::from_posix("/"),
        RemoteLayout::Rootless => match remote_home {
            Some(h) => HostPath::from_posix(format!("{h}/Napcat")),
            None => HostPath::from_posix("/root/Napcat"),
        },
    };

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
                // 远端 Linux：layout 决定 base_dir + 是否需要 sudo。
                // System 走 /opt/QQ 必须 sudo（对齐官方 NapCat-Installer.py）；
                // Rootless 走 $HOME/Napcat 不需要 sudo（对齐 NapCat-TUI-CLI）。
                Arc::new(
                    NapCatComponent::new(napcat_base.clone())
                        .with_sudo(matches!(layout, RemoteLayout::System)),
                )
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
                // 对齐 legacy SnowLumaRemotePaths：装到 $HOME/snowluma-remote/workspace。
                // SnowLumaComponent::new 内部把 workspace 推出 snowluma 子目录。
                let workspace = match remote_home {
                    Some(home) => HostPath::from_posix(format!("{home}/snowluma-remote/workspace")),
                    None => HostPath::from_posix("/root/snowluma-remote/workspace"),
                };
                Arc::new(SnowLumaComponent::new(
                    workspace,
                    "https://github.com/SnowLuma/SnowLuma/releases/latest/download/SnowLuma-linux-x64-lite.tar.gz",
                ))
            }
        }
        ComponentId::LinuxQq => {
            // 远端 LinuxQQ 跟随 NapCat 的 layout：
            //   System：base="/"，QQ 装到 /opt/QQ（系统包）—— 但当前实装走
            //     dpkg-deb -x 解包不会调系统包管理器，没法在 /opt 创目录，
            //     用户应该用官方 deb/rpm 自己装，detect 能识别。
            //   Rootless：base="$HOME/Napcat"，QQ 装到 $HOME/Napcat/opt/QQ。
            Arc::new(LinuxQQComponent::default_v3_2_25(napcat_base.clone()))
        }
        ComponentId::NodeJs => {
            // SnowLuma 才需要 Node.js；装到 SnowLuma workspace 下。
            let install_dir = match remote_home {
                Some(home) => HostPath::from_posix(format!("{home}/snowluma-remote/workspace/node")),
                None => HostPath::from_posix("/root/snowluma-remote/workspace/node"),
            };
            Arc::new(NodeJsComponent::new("20.10.0", install_dir))
        }
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
/// - `"remote:<server_id>"`：远端 SSH，从 ServerManager 取已建立的连接
///
/// 远端连接前置：用户必须先在远端页测试连接成功（ServerManager.test_connection
/// 会建立 SSH 并缓存到 hosts map）；否则 resolve_host 返回 "尚未连接" 错误。
async fn resolve_host(host_id: &str, state: &AppState) -> Result<Arc<dyn Host>, String> {
    if host_id == "local" {
        return local_host();
    }
    if let Some(server_id) = host_id.strip_prefix("remote:") {
        return state
            .server_manager
            .get_host(server_id)
            .await
            .ok_or_else(|| {
                format!("远端主机 {server_id} 尚未连接，请先在远端页测试连接")
            });
    }
    Err(format!("unknown host_id: {host_id}"))
}

/// resolve_host 的"自动连接"包装：远端 host 缓存命中直接用；不命中尝试调
/// `ServerManager.test_connection(server_id, None)` 用 keyring 缓存的凭据建立
/// SSH 连接。专给 detect_component / run_component_action 用——用户进组件页
/// 时不需要先去远端页点测试。
///
/// 失败时返回原始错误，让前端在那一行 host status 显示"未连接 + 原因"。
async fn resolve_host_with_autoconnect(
    host_id: &str,
    state: &AppState,
) -> Result<Arc<dyn Host>, String> {
    if host_id == "local" {
        return local_host();
    }
    let Some(server_id) = host_id.strip_prefix("remote:") else {
        return Err(format!("unknown host_id: {host_id}"));
    };

    if let Some(host) = state.server_manager.get_host(server_id).await {
        return Ok(host);
    }

    // 缓存未命中——尝试用 keyring 缓存凭据自动连一次。
    match state.server_manager.test_connection(server_id, None).await {
        Ok(report) if report.success => state
            .server_manager
            .get_host(server_id)
            .await
            .ok_or_else(|| format!("auto-connect 成功但缓存为空: {server_id}（不应发生）")),
        Ok(report) => {
            let err = report.error.clone().unwrap_or_else(|| "未知错误".into());
            Err(format!("自动连接失败: {err}（请去远端页手动测试连接）"))
        }
        Err(err) => Err(format!(
            "自动连接被拒绝: {err}（凭据可能未保存，请去远端页手动测试）"
        )),
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
}
