//! 按 host / 布局 / release 快照实例化 Component
//!
//! 从 Layer4 command 下沉。DesktopSelf 的产品版本由调用方传入（来自 tauri 构建注入）。

use std::path::Path;
use std::sync::Arc;

use ncd_component::{
    Component, ComponentId, DesktopSelfComponent, NapCatComponent, NcdWatchComponent,
    NoVncComponent, NodeJsComponent, QQComponent, SnowLumaComponent, ncd_watch_asset_name,
    ncd_watch_release_download_url, ncd_watch_release_download_url_for_tag,
};
use ncd_domain::RemoteSelectedPaths;
use ncd_domain::SnowLumaLinuxPackage;
use ncd_domain::release_snapshot::ReleaseSnapshot;
use ncd_host::{Arch, Host, HostPath, Os};

use crate::components::action_policy::{
    RemoteLayout, asset_sha256, data_root_to_host_path, infer_snowluma_linux_package,
    is_bundled_snowluma_node, require_remote_home, snowluma_github_release_tag,
    snowluma_linux_release_asset,
};

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
        if let Some(base) = ctx
            .selected
            .and_then(|s| s.qq_install_base.as_deref())
            .filter(|p| !p.is_empty())
        {
            return Ok(HostPath::from_posix(base));
        }
        Ok(match layout {
            RemoteLayout::System => HostPath::from_posix("/"),
            RemoteLayout::Rootless => {
                HostPath::from_posix(format!("{}/Napcat", require_remote_home(remote_home)?))
            }
        })
    };

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
                let mut comp = SnowLumaComponent::for_windows(install, tag.clone());
                if let Some(sha) = latest
                    .and_then(|info| asset_sha256(info, &format!("SnowLuma-{tag}-win-x64.zip")))
                {
                    comp = comp.with_sha256(sha);
                }
                Arc::new(comp)
            } else {
                let workspace = if let Some(ws) = ctx
                    .selected
                    .and_then(|s| s.snowluma_workspace.as_deref())
                    .filter(|p| !p.is_empty())
                {
                    HostPath::from_posix(ws)
                } else {
                    HostPath::from_posix(format!(
                        "{}/snowluma-remote/workspace",
                        require_remote_home(remote_home)?
                    ))
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
                let mut comp = SnowLumaComponent::new(workspace, url);
                if let Some(sha) = latest.and_then(|info| asset_sha256(info, &asset)) {
                    comp = comp.with_sha256(sha);
                }
                Arc::new(comp)
            }
        }
        ComponentId::Qq => {
            if ctx.host.os() == Os::Windows {
                let _unused = data_root_host.join("runtime").join("_qq_win_stub");
                Arc::new(QQComponent::default_v3_2_25(_unused))
            } else {
                Arc::new(QQComponent::default_v3_2_25(resolve_napcat_base()?))
            }
        }
        ComponentId::NodeJs => {
            let install_dir = node_install_dir(ctx.selected, remote_home)?;
            let mut comp = NodeJsComponent::new("22.13.0", install_dir);
            if let Some(bin) = nodejs_extra_detect_bin(ctx.selected) {
                comp = comp.with_extra_detect_bin(bin);
            }
            Arc::new(comp)
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
    };
    Ok(component)
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
    Ok(HostPath::from_posix(format!(
        "{}/snowluma-remote/workspace/node",
        require_remote_home(remote_home)?
    )))
}

#[cfg(test)]
mod selected_path_tests {
    use super::{node_install_dir, nodejs_extra_detect_bin};
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
