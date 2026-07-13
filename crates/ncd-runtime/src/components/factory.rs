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
use ncd_domain::release_snapshot::ReleaseSnapshot;
use ncd_host::{Arch, Host, HostPath, Os};

use crate::component_action_policy::{
    RemoteLayout, asset_sha256, data_root_to_host_path, require_remote_home,
    snowluma_github_release_tag,
};

/// 把 component_id 实例化成具体 Component
pub fn build_component_for_host(
    id: ComponentId,
    data_root: &Path,
    host: &dyn Host,
    remote_home: Option<&str>,
    layout: RemoteLayout,
    snapshot: Option<&ReleaseSnapshot>,
    local_snowluma_version: Option<&str>,
    desktop_product_version: &str,
) -> Result<Arc<dyn Component>, String> {
    let data_root_host = data_root_to_host_path(data_root, host.os());

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
            if host.os() == Os::Windows {
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
            if host.os() == Os::Windows {
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
                let workspace = HostPath::from_posix(format!(
                    "{}/snowluma-remote/workspace",
                    require_remote_home(remote_home)?
                ));
                let latest = snapshot.and_then(|s| s.snowluma_latest.as_ref());
                let tag = snowluma_github_release_tag(latest, local_snowluma_version);
                if tag.is_empty() {
                    return Err(
                        "无法确定 SnowLuma 发布版本（GitHub 版本快照与本机已装版本均不可用）。\
                         请确认能访问 GitHub 并在概览等待版本检查完成后再安装。"
                            .to_string(),
                    );
                }
                let asset = format!("SnowLuma-{tag}-linux-x64-lite.tar.gz");
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
            if host.os() == Os::Windows {
                let _unused = data_root_host.join("runtime").join("_qq_win_stub");
                Arc::new(QQComponent::default_v3_2_25(_unused))
            } else {
                Arc::new(QQComponent::default_v3_2_25(resolve_napcat_base()?))
            }
        }
        ComponentId::NodeJs => {
            let install_dir = HostPath::from_posix(format!(
                "{}/snowluma-remote/workspace/node",
                require_remote_home(remote_home)?
            ));
            Arc::new(NodeJsComponent::new("22.12.0", install_dir))
        }
        ComponentId::NoVnc => Arc::new(NoVncComponent::new()),
        ComponentId::NcdWatch => {
            let mut comp = NcdWatchComponent::new(remote_home.map(|s| s.to_string()));
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
                if host.arch() == Arch::X86_64 {
                    if let Some(asset) = ncd_watch_asset_name(&tag, Arch::X86_64) {
                        if let Some(sha) = asset_sha256(info, &asset) {
                            comp = comp.with_sha256(sha);
                        }
                    }
                }
                if let Some(url) = ncd_watch_release_download_url_for_tag(&tag, host.arch()) {
                    comp = comp.with_download_url(url);
                }
            } else if let Some(url) = ncd_watch_release_download_url(host.arch()) {
                comp = comp.with_download_url(url);
            }
            Arc::new(comp)
        }
        ComponentId::DesktopSelf => {
            let ver = desktop_product_version;
            Arc::new(DesktopSelfComponent::from_env(ver).unwrap_or_else(|_| {
                DesktopSelfComponent::new(ver, HostPath::from_posix("NapCatQQ-Desktop"))
            }))
        }
    };
    Ok(component)
}
