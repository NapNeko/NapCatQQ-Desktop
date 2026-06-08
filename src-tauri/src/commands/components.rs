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
    NapCatComponent, NoVncComponent, NodeJsComponent, ProgressKind, QQComponent,
    SnowLumaComponent,
};
use ncd_deploy::{DeployPlan, StepKind};
use ncd_host::{Host, HostPath};
use ncd_runtime::{release::read_cached_release_snapshot, DomainEvent, EventBus, ReleaseInfo};
use tauri::State;
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
    state: State<'_, AppState>,
) -> Result<String, String> {
    let host = resolve_host_with_autoconnect(&host_id, &state).await?;
    let probe = cached_host_probe(&host_id, host.as_ref(), &state).await;
    let component = build_component_for_host(
        component_id,
        &state,
        host.as_ref(),
        probe.home.as_deref(),
        probe.layout,
    )?;

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
    // 安装 / 卸载会改变远端布局（如新建 $HOME/Napcat），动作结束后失效该主机
    // 的布局缓存，下次 detect 重新探一次拿到最新布局。
    let host_probe_cache = Arc::clone(&state.host_probe_cache);
    let probe_cache_key = host_id.clone();

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
        // 布局可能已变，丢弃该主机缓存。
        host_probe_cache.lock().await.remove(&probe_cache_key);

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
        QQComponent::info(),
        NoVncComponent::info(),
        DesktopSelfComponent::info(),
    ]
}

/// 远端 NapCat / QQ 的安装布局：system 是官方 NapCat-Installer 风格
/// （/opt/QQ，需要 sudo），rootless 是 NapCat-TUI-CLI 风格（$HOME/Napcat，
/// 不需要 sudo，本工程默认）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteLayout {
    /// 系统安装：/opt/QQ/...，install_base_dir = "/"
    System,
    /// 用户安装：$HOME/Napcat/opt/QQ/...，install_base_dir = "$HOME/Napcat"
    Rootless,
}

/// 一台远端主机的布局探测结果：$HOME + NapCat 安装布局。
/// 本机（host_id="local"）这两项都没意义，用默认值（home=None / Rootless）。
#[derive(Debug, Clone)]
pub struct RemoteHostProbe {
    pub home: Option<String>,
    pub layout: RemoteLayout,
}

impl RemoteHostProbe {
    /// 本机 / 探测失败时的默认值。
    fn local_default() -> Self {
        Self {
            home: None,
            layout: RemoteLayout::Rootless,
        }
    }
}

/// 取（或探测并缓存）一台主机的 home + layout。
///
/// 同一台远端在一次 UI 会话里 home / layout 是稳定的，5 个组件并发 detect 时
/// 没必要各探一遍。缓存命中直接返回；未命中走单次合并探测，结果写缓存。安装 /
/// 卸载动作结束后由 run_component_action 清掉对应条目（布局可能变）。
async fn cached_host_probe(
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

/// 一条 shell 命令同时拿 $HOME 和 system 布局标记，省掉原来"1 次 echo + 最多 2
/// 次 SFTP stat"分多趟的往返。输出两行：HOME、system 标记存在与否
/// （`test -e ... && echo 1 || echo 0`）。system 不存在时一律按 rootless 处理，
/// 所以不必再单独探 rootless 标记。
async fn probe_remote_host(host: &dyn Host) -> RemoteHostProbe {
    let script = "echo \"$HOME\"; \
         test -e /opt/QQ/resources/app/app_launcher/napcat/napcat.mjs && echo 1 || echo 0";
    let cmd = ncd_host::HostCommand::new("sh").arg("-c").arg(script);
    let out = match host.run_to_string(cmd).await {
        Ok(out) if out.success() => out,
        _ => return RemoteHostProbe::local_default(),
    };

    let mut lines = out.stdout.lines();
    let home = lines.next().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
    let system_exists = lines.next().map(str::trim) == Some("1");

    // system（/opt/QQ）优先；否则一律 Rootless（$HOME/Napcat，含从零安装）。
    let layout = if system_exists {
        RemoteLayout::System
    } else {
        RemoteLayout::Rootless
    };

    RemoteHostProbe { home, layout }
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
) -> Result<Arc<dyn Component>, String> {
    let data_root_host = data_root_to_host_path(&state.data_root, host.os());
    // 读 release 缓存反查 SHA256：缓存缺失 / 无 digest 时退化到"无 hash"分支，
    // 让安装链路走原有路径（race 仍尝试切 mirror，但失去内容级保护）。
    // 缓存由前端 useReleases hook 在启动/轮询时通过 get_release_snapshot 维护。
    let snapshot = read_cached_release_snapshot(&state.data_root);

    // 远端 NapCat / QQ 共用 install_base_dir：layout 决定 / 还是 $HOME/Napcat。
    // Rootless 但探不到 $HOME 时 fail-fast(不回退 /root):路径落盘红线,宁可报错也不
    // 把组件装到错误目录(/root 多半无权限或污染 root 家目录)。惰性求值——只有真正
    // 用到 base 的组件(NapCat/QQ)才校验,home 无关组件(NoVnc/DesktopSelf)不受影响。
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
                    NapCatComponent::new(resolve_napcat_base()?)
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
                let workspace = HostPath::from_posix(format!(
                    "{}/snowluma-remote/workspace",
                    require_remote_home(remote_home)?
                ));
                // 不能写死 latest/download/SnowLuma-linux-x64-lite.tar.gz:真实资产名带
                // 版本号(SnowLuma-v1.9.3-linux-x64-lite.tar.gz),无版本号的 URL 404,
                // 镜像代理把 404 页当 200 转发,下载器没 hash 拦就把 HTML 当 tar.gz 上传,
                // 远端 tar 解压报 "not in gzip format"。和 Windows 分支一样从 release 快照
                // 拿 tag 拼对 URL + 反查 sha256,既修 404 又补上内容校验(双保险)。
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
                let asset = format!("SnowLuma-{tag}-linux-x64-lite.tar.gz");
                let url = format!(
                    "https://github.com/SnowLuma/SnowLuma/releases/download/{tag}/{asset}"
                );
                let mut comp = SnowLumaComponent::new(workspace, url);
                if let Some(sha) = latest.and_then(|info| asset_sha256(info, &asset)) {
                    comp = comp.with_sha256(sha);
                }
                Arc::new(comp)
            }
        }
        ComponentId::Qq => {
            if host.os() == ncd_host::Os::Windows {
                // 本机 Windows：detect/install 走注册表 + pcConfig，不用 Napcat
                // 远端 layout。install_base_dir 仅 Linux 解包路径会读，这里占位即可。
                let _unused = data_root_host.join("runtime").join("_qq_win_stub");
                Arc::new(QQComponent::default_v3_2_25(_unused))
            } else {
                // 远端 / Linux 本地 QQ 跟随 NapCat layout（Rootless → $HOME/Napcat/opt/QQ）。
                Arc::new(QQComponent::default_v3_2_25(resolve_napcat_base()?))
            }
        }
        ComponentId::NodeJs => {
            // SnowLuma 才需要 Node.js；装到 SnowLuma workspace 下。
            let install_dir = HostPath::from_posix(format!(
                "{}/snowluma-remote/workspace/node",
                require_remote_home(remote_home)?
            ));
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
    };
    Ok(component)
}

/// 远端 Rootless 安装必须有可信 $HOME。探不到就 fail-fast,不回退 /root——避免把
/// NapCat / QQ / SnowLuma / Node.js 装到错误目录(/root 通常无权限或污染 root 家目录)。
fn require_remote_home(remote_home: Option<&str>) -> Result<&str, String> {
    remote_home.ok_or_else(|| {
        "无法探测远端 $HOME,已拒绝回退到 /root 安装(避免组件落到错误目录)。\
         请确认远端 SSH 用户有正常的家目录后重试。"
            .to_string()
    })
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
                ComponentId::Qq,
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
            (QQComponent::info(), Arc::new(QQComponent::default_v3_2_25(HostPath::from_posix("/x")))),
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
