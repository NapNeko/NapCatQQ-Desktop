//! Windows 库存只探测目标机的受管目录、明确覆盖路径和系统注册表。
use super::*;
use ncd_host::{HostPath, Os};

fn absolute(value: &str) -> Result<HostPath, String> {
    let path = HostPath::from_windows(value);
    if !path.is_absolute() || value.contains(['\0', '\r', '\n']) {
        return Err("远端 Windows 路径必须是绝对路径".into());
    }
    Ok(path)
}

fn configured(value: Option<&String>, fallback: HostPath) -> Result<HostPath, String> {
    value.map(|s| absolute(s)).unwrap_or(Ok(fallback))
}

fn comparable(value: &str) -> Option<String> {
    let path = absolute(value).ok()?;
    if path
        .as_posix()
        .split('/')
        .any(|part| matches!(part, "." | ".."))
    {
        return None;
    }
    Some(path.as_posix().trim_end_matches('/').to_ascii_lowercase())
}

pub(super) fn selection_matches(
    inventory: &RemoteInventory,
    host: &dyn Host,
    root: &HostPath,
    overrides: Option<&RemotePathOverrides>,
) -> bool {
    let empty = RemotePathOverrides::default();
    let overrides = overrides.unwrap_or(&empty);
    let selected = &inventory.selected;
    let expected = |configured: Option<&String>, suffix: &str| {
        configured
            .cloned()
            .unwrap_or_else(|| root.join(suffix).render_for(Os::Windows))
    };
    let equal = |actual: Option<&str>, expected: &str| {
        actual
            .and_then(comparable)
            .zip(comparable(expected))
            .is_some_and(|(actual, expected)| actual == expected)
    };
    let home_ok = host.target_home().is_some_and(|home| {
        equal(Some(&inventory.home), &home.render_for(Os::Windows))
            && equal(Some(&selected.home), &home.render_for(Os::Windows))
    });
    let qq = expected(overrides.qq_install_base.as_ref(), "components/QQ");
    let napcat = expected(overrides.napcat_root.as_ref(), "components/NapCatQQ");
    let snowluma = expected(overrides.snowluma_dir.as_ref(), "components/SnowLuma");
    let watch = expected(overrides.ncd_watch_root.as_ref(), "ncd-watch");
    let roots_ok = equal(selected.qq_install_base.as_deref(), &qq)
        && equal(selected.napcat_root.as_deref(), &napcat)
        && equal(selected.snowluma_dir.as_deref(), &snowluma)
        && equal(selected.snowluma_workspace.as_deref(), &snowluma)
        && equal(selected.ncd_watch_root.as_deref(), &watch);
    let qq_ok = selected.qq_bin.as_deref().is_none_or(|bin| {
        comparable(bin)
            .zip(comparable(&qq))
            .is_some_and(|(bin, root)| bin.starts_with(&format!("{root}/")))
            || inventory.items.iter().any(|item| {
                item.verified
                    && item.kind == RemoteInventoryKind::Qq
                    && item.source == RemoteInventorySource::SystemPackage
                    && item
                        .qq_bin
                        .as_deref()
                        .is_some_and(|found| equal(Some(bin), found))
            })
    });
    let node_ok = selected.node_bin.as_deref().is_none_or(|bin| {
        if let Some(configured) = &overrides.node_bin {
            return equal(Some(bin), configured);
        }
        equal(
            Some(bin),
            &root
                .join("components/NodeJs/node.exe")
                .render_for(Os::Windows),
        ) || inventory.items.iter().any(|item| {
            item.verified
                && item.kind == RemoteInventoryKind::NodeJs
                && item.source == RemoteInventorySource::PathLookup
                && item
                    .node_bin
                    .as_deref()
                    .is_some_and(|found| equal(Some(bin), found))
        })
    });
    home_ok && roots_ok && qq_ok && node_ok && !selected.needs_sudo
}

fn item(
    kind: RemoteInventoryKind,
    root: &HostPath,
    source: RemoteInventorySource,
) -> RemoteInventoryItem {
    RemoteInventoryItem {
        kind,
        root: root.render_for(Os::Windows),
        source,
        verified: true,
        qq_bin: None,
        napcat_mjs: None,
        load_napcat_js: None,
        index_mjs: None,
        runtime_json: None,
        node_bin: None,
        docker_name: None,
    }
}

pub(super) async fn probe(
    host: &dyn Host,
    overrides: Option<&RemotePathOverrides>,
) -> Result<RemoteInventory, String> {
    let info = host
        .remote_windows_info()
        .await
        .map_err(|e| format!("读取远端 Windows 目录失败：{e}"))?;
    let root = absolute(&info.user_root)?;
    let empty = RemotePathOverrides::default();
    let overrides = overrides.unwrap_or(&empty);
    let qq = configured(
        overrides.qq_install_base.as_ref(),
        root.join("components/QQ"),
    )?;
    let napcat = configured(
        overrides.napcat_root.as_ref(),
        root.join("components/NapCatQQ"),
    )?;
    let snowluma = configured(
        overrides.snowluma_dir.as_ref(),
        root.join("components/SnowLuma"),
    )?;
    let watch = configured(overrides.ncd_watch_root.as_ref(), root.join("ncd-watch"))?;
    let node_default = configured(
        overrides.node_bin.as_ref(),
        root.join("components/NodeJs/node.exe"),
    )?;
    let mut selected = RemoteSelectedPaths {
        home: info.home_dir.clone(),
        qq_install_base: Some(qq.render_for(Os::Windows)),
        napcat_root: Some(napcat.render_for(Os::Windows)),
        snowluma_dir: Some(snowluma.render_for(Os::Windows)),
        snowluma_workspace: Some(snowluma.render_for(Os::Windows)),
        ncd_watch_root: Some(watch.render_for(Os::Windows)),
        ..Default::default()
    };
    let result = async {
        let mut items = Vec::new();
        let pointer = qq.join("current.txt");
        if host.exists(&pointer).await? {
            let bytes = host.read_file(&pointer).await?;
            let version = String::from_utf8_lossy(&bytes);
            let version = version.trim();
            if version.is_empty()
                || version == "."
                || version == ".."
                || !version
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'-'))
            {
                return Err(ncd_host::HostError::Unsupported {
                    operation: "invalid-remote-qq-current",
                });
            }
            let bin = qq.join("versions").join(version).join("QQ.exe");
            if host.exists(&bin).await? {
                selected.qq_bin = Some(bin.render_for(Os::Windows));
                let mut found = item(
                    RemoteInventoryKind::Qq,
                    &qq,
                    RemoteInventorySource::DesktopOwned,
                );
                found.qq_bin = selected.qq_bin.clone();
                items.push(found);
            }
        }
        if selected.qq_bin.is_none() {
            let out = host
                .run_to_string(HostCommand::new("reg.exe").args([
                    "query",
                    r"HKLM\SOFTWARE\WOW6432Node\Tencent\QQNT",
                    "/v",
                    "Install",
                ]))
                .await?;
            if out.success() {
                if let Some(path) = out
                    .stdout
                    .lines()
                    .find_map(|line| line.split_once("REG_SZ").map(|(_, path)| path.trim()))
                {
                    let install = HostPath::from_windows(path);
                    let bin = install.join("QQ.exe");
                    if host.exists(&bin).await? {
                        selected.qq_bin = Some(bin.render_for(Os::Windows));
                        let mut found = item(
                            RemoteInventoryKind::Qq,
                            &install,
                            RemoteInventorySource::SystemPackage,
                        );
                        found.qq_bin = selected.qq_bin.clone();
                        items.push(found);
                    }
                }
            }
        }
        let mjs = napcat.join("napcat.mjs");
        if host.exists(&mjs).await? {
            let mut found = item(
                RemoteInventoryKind::NapCat,
                &napcat,
                if overrides.napcat_root.is_some() {
                    RemoteInventorySource::UserOverride
                } else {
                    RemoteInventorySource::DesktopOwned
                },
            );
            found.napcat_mjs = Some(mjs.render_for(Os::Windows));
            items.push(found);
        }
        if host.exists(&snowluma.join("index.mjs")).await? {
            let mut found = item(
                RemoteInventoryKind::SnowLuma,
                &snowluma,
                if overrides.snowluma_dir.is_some() {
                    RemoteInventorySource::UserOverride
                } else {
                    RemoteInventorySource::DesktopOwned
                },
            );
            found.index_mjs = Some(snowluma.join("index.mjs").render_for(Os::Windows));
            found.runtime_json = Some(snowluma.join("config/runtime.json").render_for(Os::Windows));
            items.push(found);
        }
        let node = node_default;
        let node = if host.exists(&node).await? {
            Some(node)
        } else {
            host.which("node.exe")
                .await?
                .map(|p| HostPath::from_windows(&p))
        };
        if let Some(node) = node {
            selected.node_bin = Some(node.render_for(Os::Windows));
            let mut found = item(
                RemoteInventoryKind::NodeJs,
                node.parent().as_ref().unwrap_or(&node),
                if overrides.node_bin.is_some() {
                    RemoteInventorySource::UserOverride
                } else {
                    RemoteInventorySource::PathLookup
                },
            );
            found.node_bin = selected.node_bin.clone();
            items.push(found);
        }
        if !host.exists(&watch.join("config/disabled")).await?
            && info.agent.as_ref().is_some_and(|status| {
                status.service_running
                    && status
                        .capabilities
                        .iter()
                        .any(|feature| feature == "watch_v1")
            })
        {
            items.push(item(
                RemoteInventoryKind::NcdWatch,
                &watch,
                RemoteInventorySource::DesktopOwned,
            ));
        }
        Ok::<_, ncd_host::HostError>(items)
    }
    .await
    .map_err(|e| format!("探测 Windows 安装库存失败：{e}"))?;
    let mut bots = Vec::new();
    for (backend, dir, prefixes) in [
        (
            BackendType::NapCat,
            napcat.join("config"),
            vec!["onebot11_", "napcat_"],
        ),
        (
            BackendType::SnowLuma,
            snowluma.join("config"),
            vec!["onebot_"],
        ),
    ] {
        if !host
            .exists(&dir)
            .await
            .map_err(|e| format!("读取 Windows Bot 库存失败：{e}"))?
        {
            continue;
        }
        let entries = host
            .list_dir(&dir)
            .await
            .map_err(|e| format!("读取 Windows Bot 库存失败：{e}"))?;
        for entry in entries
            .into_iter()
            .filter(|entry| !entry.is_dir && !entry.is_symlink)
        {
            let Some(stem) = entry.name.strip_suffix(".json") else {
                continue;
            };
            let Some(raw) = prefixes
                .iter()
                .find_map(|prefix| stem.strip_prefix(*prefix))
            else {
                continue;
            };
            if raw.is_empty() || !raw.bytes().all(|ch| ch.is_ascii_digit()) {
                continue;
            }
            let Some(qq_id) = raw.parse::<u64>().ok().filter(|id| *id != 0) else {
                continue;
            };
            if bots
                .iter()
                .any(|bot: &DiscoveredRemoteBot| bot.qq_id == qq_id && bot.backend == backend)
            {
                continue;
            }
            bots.push(DiscoveredRemoteBot {
                qq_id,
                backend,
                deployment: DeploymentType::Native,
                source: DiscoveredRemoteBotSource::ConfigFile,
                docker_name: None,
            });
        }
    }
    Ok(RemoteInventory {
        v: REMOTE_INVENTORY_VERSION,
        probed_at: chrono::Utc::now().to_rfc3339(),
        home: info.home_dir,
        items: result,
        selected,
        bots,
        snowluma_linux_package: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn target_paths_reject_relative_and_control_characters() {
        assert!(absolute(r"C:\ProgramData\NapCatQQ Desktop\remote").is_ok());
        assert!(absolute("components/QQ").is_err());
        assert!(absolute("C:\\bad\npath").is_err());
    }
}
