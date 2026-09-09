//! 按 host / 布局 / release 快照实例化 Component
//!
//! 从 Layer4 command 下沉。DesktopSelf 的产品版本由调用方传入（来自 tauri 构建注入）。

use std::path::Path;
use std::sync::Arc;

use ncd_appframework::{AppComponentSpec, AppFrameworkRegistry};
use ncd_component::{
    ncd_watch_asset_name, ncd_watch_release_download_url, ncd_watch_release_download_url_for_tag,
    Component, ComponentId, DesktopSelfComponent, NapCatComponent, NcdWatchComponent,
    NoVncComponent, NodeJsComponent, QQComponent, SnowLumaComponent, UV_DEFAULT_VERSION,
    UvComponent,
};
use ncd_domain::RemoteSelectedPaths;
use ncd_domain::SnowLumaLinuxPackage;
use ncd_domain::desktop_default_install_paths;
use ncd_domain::release_snapshot::ReleaseSnapshot;
use ncd_domain::require_qq_install_base;
use ncd_domain::snowluma_install_candidates;
use ncd_host::{Arch, Host, HostPath, Os};

use crate::components::action_policy::{
    RemoteLayout, asset_sha256, data_root_to_host_path, infer_snowluma_linux_package,
    is_bundled_snowluma_node, require_remote_home, snowluma_github_release_tag,
    snowluma_linux_release_asset, snowluma_windows_release_asset,
};
use crate::components::graph::catalog_version_reqs_for;

/// 实例化 Component 时的上下文（避免过长参数列表）。
pub struct BuildComponentCtx<'a> {
    pub data_root: &'a Path,
    pub host: &'a dyn Host,
    pub remote_home: Option<&'a str>,
    pub layout: RemoteLayout,
    pub snapshot: Option<&'a ReleaseSnapshot>,
    pub local_snowluma_version: Option<&'a str>,
    pub desktop_product_version: &'a str,
    /// 库存选中路径；缺字段时回落桌面默认路径
    pub selected: Option<&'a RemoteSelectedPaths>,
    /// 安装/更新时显式选择的 Linux 包；None 则按库存推断
    pub snowluma_linux_package: Option<SnowLumaLinuxPackage>,
    /// 本机 SnowLuma Node 覆盖路径；远端路径走 selected.node_bin
    pub snowluma_node_path: Option<&'a str>,
    /// 应用端组件（Karin 等）按实例安装，必须带实例目录 / 端口；协议组件忽略
    pub app_component: Option<&'a AppComponentHint>,
}

/// 应用端组件的实例级输入（组件页 catalog 不列应用端，只有应用端页面会填）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppComponentHint {
    pub instance_id: String,
    pub install_dir: HostPath,
    pub port: u16,
    pub npm_registry: Option<String>,
    pub install_renderer: bool,
    pub adopt_existing: bool,
}

/// 把 component_id 实例化成具体 Component
pub fn build_component_for_host(
    id: ComponentId,
    ctx: &BuildComponentCtx<'_>,
) -> Result<Arc<dyn Component>, String> {
    let data_root_host = data_root_to_host_path(ctx.data_root, ctx.host.os());
    let layout = ctx.layout;
    let remote_home = ctx.remote_home;
    let snapshot = ctx.snapshot;
    let local_snowluma_version = ctx.local_snowluma_version;

    let resolve_napcat_base = || -> Result<HostPath, String> {
        if let Some(sel) = ctx.selected {
            if let Ok(base) = require_qq_install_base(sel) {
                return Ok(HostPath::from_posix(base));
            }
        }
        let home = require_remote_home(remote_home)?;
        let defaults = desktop_default_install_paths(&home)?;
        defaults
            .qq_install_base
            .map(HostPath::from_posix)
            .ok_or_else(|| format!("无法派生默认 QQ 安装根（home={home}）"))
    };

    if id.is_app_framework() {
        return build_app_framework_component(id, ctx, &data_root_host, remote_home);
    }

    let component: Arc<dyn Component> = match id {
        ComponentId::NapCat => {
            if ctx.host.os() == Os::Windows {
                let install = data_root_host.join("components").join("NapCatQQ");
                let mut comp = NapCatComponent::for_windows(install);
                if let Some(sha) = snapshot
                    .and_then(|s| s.napcat_latest.as_ref())
                    .and_then(|info| asset_sha256(info, "NapCat.Shell.zip"))
                {
                    comp = comp.with_sha256(sha);
                }
                Arc::new(comp)
            } else {
                Arc::new(
                    NapCatComponent::new(resolve_napcat_base()?)
                        .with_sudo(matches!(layout, RemoteLayout::System)),
                )
            }
        }
        ComponentId::SnowLuma => {
            if ctx.host.os() == Os::Windows {
                let install = data_root_host.join("components").join("SnowLuma");
                let latest = snapshot.and_then(|s| s.snowluma_latest.as_ref());
                let tag = snowluma_github_release_tag(latest, local_snowluma_version);
                if tag.is_empty() {
                    return Err(
                        "无法确定 SnowLuma 发布版本（GitHub 版本快照与本机已装版本均不可用）。\
                         请确认能访问 GitHub 并在概览等待版本检查完成后再安装。"
                            .to_string(),
                    );
                }
                let package = ctx
                    .snowluma_linux_package
                    .unwrap_or_else(|| infer_snowluma_linux_package(ctx.selected));
                let asset = snowluma_windows_release_asset(&tag, package);
                let mut comp = SnowLumaComponent::for_windows_with_package(install, tag.clone(), package);
                if let Some(sha) = latest
                    .and_then(|info| asset_sha256(info, &asset))
                {
                    comp = comp.with_sha256(sha);
                }
                Arc::new(comp)
            } else {
                let workspace =
                    if let Some(ws) = ctx
                        .selected
                        .and_then(|s| s.snowluma_workspace.as_deref())
                        .filter(|p| !p.is_empty())
                    {
                        HostPath::from_posix(ws)
                    } else {
                        let home = require_remote_home(remote_home)?;
                        let defaults = desktop_default_install_paths(&home)?;
                        HostPath::from_posix(defaults.snowluma_workspace.ok_or_else(|| {
                            format!("无法派生默认 SnowLuma workspace（home={home}）")
                        })?)
                    };
                let latest = snapshot.and_then(|s| s.snowluma_latest.as_ref());
                let tag = snowluma_github_release_tag(latest, local_snowluma_version);
                if tag.is_empty() {
                    return Err(
                        "无法确定 SnowLuma 发布版本（GitHub 版本快照与本机已装版本均不可用）。\
                         请确认能访问 GitHub 并在概览等待版本检查完成后再安装。"
                            .to_string(),
                    );
                }
                let package = ctx
                    .snowluma_linux_package
                    .unwrap_or_else(|| infer_snowluma_linux_package(ctx.selected));
                let asset = snowluma_linux_release_asset(&tag, ctx.host.arch(), package);
                let url =
                    format!("https://github.com/SnowLuma/SnowLuma/releases/download/{tag}/{asset}");
                let mut comp = SnowLumaComponent::new(workspace, url).with_package(package);
                if let Some(dir) = ctx
                    .selected
                    .and_then(|s| s.snowluma_dir.as_deref())
                    .filter(|p| !p.is_empty())
                {
                    comp = comp.with_snowluma_dir(HostPath::from_posix(dir));
                }
                for dir in snowluma_linux_extra_detect_dirs(ctx.selected, remote_home) {
                    comp = comp.with_extra_detect_dir(dir);
                }
                if let Some(sha) = latest.and_then(|info| asset_sha256(info, &asset)) {
                    comp = comp.with_sha256(sha);
                }
                Arc::new(comp)
            }
        }
        ComponentId::Qq => {
            if ctx.host.os() == Os::Windows {
                // 安装包缓存目录:稳定文件名 QQNT-{version}.exe,失败保留可复用。
                // install_base_dir 在 Windows 上只是占位(detect 走注册表),
                // 真正生效的是 with_tmp_dir 指到的 cache
                let cache = data_root_host.join("runtime").join("cache").join("qq");
                Arc::new(QQComponent::default_v3_2_25(cache.clone()).with_tmp_dir(cache))
            } else {
                Arc::new(QQComponent::default_v3_2_25(resolve_napcat_base()?))
            }
        }
        ComponentId::NodeJs => {
            // Node 自己不知道谁要它;版本约束来自这台主机上所有可能依赖 Node 的组件
            let accept =
                catalog_version_reqs_for(ComponentId::NodeJs, ctx.host.os(), ctx.host.locality());
            if ctx.host.os() == Os::Windows {
                let install_dir = data_root_host.join("components").join("NodeJs");
                let mut comp =
                    NodeJsComponent::new("22.13.0", install_dir).with_version_reqs(accept);
                if let Some(bin) = nodejs_extra_detect_bin(ctx.selected) {
                    comp = comp.with_extra_detect_bin(bin);
                }
                if let Some(path) = ctx.snowluma_node_path.filter(|p| !p.trim().is_empty()) {
                    comp = comp.with_extra_detect_bin(HostPath::from_windows(path.trim()));
                }
                Arc::new(comp)
            } else {
                let install_dir = node_install_dir(ctx.selected, remote_home)?;
                let mut comp =
                    NodeJsComponent::new("22.13.0", install_dir).with_version_reqs(accept);
                if let Some(bin) = nodejs_extra_detect_bin(ctx.selected) {
                    comp = comp.with_extra_detect_bin(bin);
                }
                Arc::new(comp)
            }
        }
        ComponentId::NoVnc => Arc::new(NoVncComponent::new()),
        ComponentId::NcdWatch => {
            let mut comp = NcdWatchComponent::new(remote_home.map(|s| s.to_string()));
            if let Some(root) = ctx
                .selected
                .and_then(|s| s.ncd_watch_root.as_deref())
                .filter(|p| !p.is_empty())
            {
                comp = comp.with_install_root(HostPath::from_posix(root));
            }
            if let Some(info) = snapshot.and_then(|s| s.ncd_watch_latest.as_ref()) {
                let tag = if info.tag.trim().is_empty() {
                    format!(
                        "watch-v{}",
                        info.version.trim().trim_start_matches(['v', 'V'])
                    )
                } else {
                    info.tag.clone()
                };
                comp = comp
                    .with_release_tag(tag.clone())
                    .with_version_label(info.version.clone());
                if ctx.host.arch() == Arch::X86_64 {
                    if let Some(asset) = ncd_watch_asset_name(&tag, Arch::X86_64) {
                        if let Some(sha) = asset_sha256(info, &asset) {
                            comp = comp.with_sha256(sha);
                        }
                    }
                }
                if let Some(url) = ncd_watch_release_download_url_for_tag(&tag, ctx.host.arch()) {
                    comp = comp.with_download_url(url);
                }
            } else if let Some(url) = ncd_watch_release_download_url(ctx.host.arch()) {
                comp = comp.with_download_url(url);
            }
            Arc::new(comp)
        }
        ComponentId::DesktopSelf => {
            let ver = ctx.desktop_product_version;
            Arc::new(DesktopSelfComponent::from_env(ver).unwrap_or_else(|_| {
                DesktopSelfComponent::new(ver, HostPath::from_posix("NapCatQQ-Desktop"))
            }))
        }
        ComponentId::Uv => {
            let install_dir = if ctx.host.os() == Os::Windows {
                data_root_host.join("components").join("Uv")
            } else {
                let home = require_remote_home(remote_home)?;
                UvComponent::default_remote_install_dir(home)
            };
            Arc::new(UvComponent::new(UV_DEFAULT_VERSION, install_dir))
        }
        _ => {
            return Err(format!("组件工厂未覆盖: {}", id.as_str()));
        }
    };
    Ok(component)
}

fn build_app_framework_component(
    id: ComponentId,
    ctx: &BuildComponentCtx<'_>,
    data_root_host: &HostPath,
    remote_home: Option<&str>,
) -> Result<Arc<dyn Component>, String> {
    let hint = ctx.app_component.ok_or_else(|| {
        "应用端组件按实例安装，请从「应用端」页面操作".to_string()
    })?;
    let adapter = AppFrameworkRegistry::with_builtin()
        .by_component_id(id.as_str())
        .ok_or_else(|| format!("应用端框架未注册: {}", id.as_str()))?;
    // 工具链取值口径与其他远端组件一致：用户 path_overrides 优先，其次桌面端管理的
    // 组件落点（NodeJs / Uv 组件同一目录），都没有再由框架组件回退到实例标记 / PATH。
    // 两条都算出来交给适配器，Node 系只用 node_bin，Python 系只用 uv_bin。
    let node_bin = if ctx.host.os() == Os::Windows {
        Some(NodeJsComponent::node_binary_path_for_os(
            &data_root_host.join("components").join("NodeJs"),
            Os::Windows,
        ))
    } else {
        nodejs_extra_detect_bin(ctx.selected).or_else(|| {
            node_install_dir(ctx.selected, remote_home)
                .ok()
                .map(|dir| NodeJsComponent::node_binary_path_for_os(&dir, ctx.host.os()))
        })
    };
    let uv_bin = if ctx.host.os() == Os::Windows {
        Some(UvComponent::uv_binary_path_for_os(
            &data_root_host.join("components").join("Uv"),
            Os::Windows,
        ))
    } else {
        remote_home.map(|home| {
            UvComponent::uv_binary_path_for_os(
                &UvComponent::default_remote_install_dir(home),
                ctx.host.os(),
            )
        })
    };
    Ok(adapter.component(&AppComponentSpec {
        install_dir: hint.install_dir.clone(),
        port: hint.port,
        node_bin,
        uv_bin,
        npm_registry: hint.npm_registry.clone(),
        install_renderer: hint.install_renderer,
        adopt_existing: hint.adopt_existing,
    }))
}

/// Linux 远端 detect 在默认 workspace 之外再看这些目录（不扫盘）。
fn snowluma_linux_extra_detect_dirs(
    selected: Option<&RemoteSelectedPaths>,
    remote_home: Option<&str>,
) -> Vec<HostPath> {
    let mut dirs = Vec::new();
    if let Some(home) = remote_home.filter(|s| !s.is_empty()) {
        for dir in snowluma_install_candidates(home) {
            dirs.push(HostPath::from_posix(dir));
        }
    } else {
        dirs.push(HostPath::from_posix(ncd_domain::SYSTEM_SNOWLUMA_DIR));
    }
    if let Some(dir) = selected
        .and_then(|s| s.snowluma_dir.as_deref())
        .filter(|p| !p.is_empty())
    {
        dirs.push(HostPath::from_posix(dir));
    }
    dirs
}

/// 可复用 Node 的额外探测点：用户覆盖 / 便携安装。不含 SnowLuma 完整包自带的 `{dir}/node`。
fn nodejs_extra_detect_bin(selected: Option<&RemoteSelectedPaths>) -> Option<HostPath> {
    let sel = selected?;
    let bin = sel.node_bin.as_deref().filter(|s| !s.is_empty())?;
    if is_bundled_snowluma_node(bin, sel.snowluma_dir.as_deref()) {
        return None;
    }
    Some(HostPath::from_posix(bin))
}

fn node_install_dir(
    selected: Option<&RemoteSelectedPaths>,
    remote_home: Option<&str>,
) -> Result<HostPath, String> {
    if let Some(bin) = selected.and_then(|s| s.node_bin.as_deref()) {
        let n = bin.replace('\\', "/");
        if n.contains("/snowluma-remote/") || n.contains("/Napcat/usr/node/") {
            if let Some(dir) = n.strip_suffix("/bin/node") {
                return Ok(HostPath::from_posix(dir));
            }
        }
    }
    let home = require_remote_home(remote_home)?;
    let defaults = desktop_default_install_paths(&home)?;
    let ws = defaults
        .snowluma_workspace
        .ok_or_else(|| format!("无法派生默认 Node 安装目录（home={home}）"))?;
    Ok(HostPath::from_posix(ncd_domain::join_under(&ws, "node")))
}

#[cfg(test)]
mod selected_path_tests {
    use super::{node_install_dir, nodejs_extra_detect_bin, snowluma_linux_extra_detect_dirs};
    use ncd_domain::RemoteSelectedPaths;

    #[test]
    fn node_install_dir_uses_portable_prefix() {
        let selected = RemoteSelectedPaths {
            home: "/home/u".into(),
            node_bin: Some("/home/u/snowluma-remote/workspace/node/bin/node".into()),
            needs_sudo: false,
            ..RemoteSelectedPaths::default()
        };
        let dir = node_install_dir(Some(&selected), Some("/home/u")).unwrap();
        assert_eq!(dir.as_posix(), "/home/u/snowluma-remote/workspace/node");
    }

    #[test]
    fn node_install_dir_ignores_system_path_node() {
        let selected = RemoteSelectedPaths {
            home: "/home/u".into(),
            node_bin: Some("/usr/bin/node".into()),
            needs_sudo: false,
            ..RemoteSelectedPaths::default()
        };
        let dir = node_install_dir(Some(&selected), Some("/home/u")).unwrap();
        assert_eq!(dir.as_posix(), "/home/u/snowluma-remote/workspace/node");
    }

    #[test]
    fn snowluma_extra_detect_includes_opt_and_home_layouts() {
        let dirs = snowluma_linux_extra_detect_dirs(None, Some("/root"));
        let posix: Vec<_> = dirs.iter().map(|p| p.as_posix().to_string()).collect();
        assert!(posix.contains(&"/opt/snowluma".to_string()));
        assert!(posix.contains(&"/root/snowluma-remote/workspace/snowluma".to_string()));
        assert!(posix.contains(&"/root/Napcat/snowluma-workspace/snowluma".to_string()));
    }

    #[test]
    fn extra_detect_skips_snowluma_bundled_node() {
        let full = RemoteSelectedPaths {
            home: "/root".into(),
            snowluma_dir: Some("/opt/snowluma".into()),
            node_bin: Some("/opt/snowluma/node".into()),
            needs_sudo: false,
            ..RemoteSelectedPaths::default()
        };
        assert!(nodejs_extra_detect_bin(Some(&full)).is_none());
    }

    #[test]
    fn extra_detect_keeps_portable_node() {
        let lite = RemoteSelectedPaths {
            home: "/home/u".into(),
            snowluma_dir: Some("/home/u/snowluma-remote/workspace/snowluma".into()),
            node_bin: Some("/home/u/snowluma-remote/workspace/node/bin/node".into()),
            needs_sudo: false,
            ..RemoteSelectedPaths::default()
        };
        let bin = nodejs_extra_detect_bin(Some(&lite)).unwrap();
        assert_eq!(
            bin.as_posix(),
            "/home/u/snowluma-remote/workspace/node/bin/node"
        );
    }
}
