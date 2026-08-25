//! 把库存里的 Bot 指纹对照本机 bot.json，生成可导入列表。

use ncd_domain::{BotConfig, DiscoveredRemoteBot, ImportableRemoteBot};

use crate::server_manager::ServerProfile;

pub fn collect_importable_remote_bots(
    servers: &[ServerProfile],
    existing: &[BotConfig],
) -> Vec<ImportableRemoteBot> {
    let mut out = Vec::new();
    for server in servers {
        let Some(inv) = server.inventory.as_ref() else {
            continue;
        };
        for bot in &inv.bots {
            out.push(to_importable(server, bot, existing));
        }
    }
    out.sort_by(|a, b| {
        a.server_name
            .cmp(&b.server_name)
            .then(a.qq_id.cmp(&b.qq_id))
            .then_with(|| format!("{:?}", a.backend).cmp(&format!("{:?}", b.backend)))
    });
    out
}

fn to_importable(
    server: &ServerProfile,
    bot: &DiscoveredRemoteBot,
    existing: &[BotConfig],
) -> ImportableRemoteBot {
    let same_qq = existing.iter().find(|c| c.bot.qq_id == bot.qq_id);
    let (already_imported, selectable, skip_reason) = match same_qq {
        Some(cfg) if cfg.bot.runtime_target.server_id() == Some(server.id.as_str()) => {
            (true, false, Some("已导入".to_string()))
        }
        Some(_) => (false, false, Some("该 QQ 已登记为其他实例".to_string())),
        None => (false, true, None),
    };
    ImportableRemoteBot {
        server_id: server.id.clone(),
        server_name: server.name.clone(),
        qq_id: bot.qq_id,
        backend: bot.backend,
        deployment: bot.deployment,
        source: bot.source,
        docker_name: bot.docker_name.clone(),
        already_imported,
        selectable,
        skip_reason,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server_manager::{AuthMethod, ServerState};
    use ncd_domain::{
        BackendType, DeploymentType, DiscoveredRemoteBotSource, REMOTE_INVENTORY_VERSION,
        RemoteInventory, RemoteSelectedPaths,
    };

    fn discovered(qq: u64, backend: BackendType) -> DiscoveredRemoteBot {
        DiscoveredRemoteBot {
            qq_id: qq,
            backend,
            deployment: DeploymentType::Native,
            source: DiscoveredRemoteBotSource::ConfigFile,
            docker_name: None,
        }
    }

    fn server(id: &str, name: &str, bots: Vec<DiscoveredRemoteBot>) -> ServerProfile {
        ServerProfile {
            id: id.into(),
            name: name.into(),
            host: "10.0.0.1".into(),
            port: 22,
            username: "root".into(),
            auth_method: AuthMethod::Key,
            private_key_path: None,
            remember_credential: false,
            state: ServerState::Connected,
            health: None,
            webui_url: None,
            path_overrides: None,
            inventory: Some(RemoteInventory {
                v: REMOTE_INVENTORY_VERSION,
                probed_at: "t".into(),
                home: "/root".into(),
                items: Vec::new(),
                selected: RemoteSelectedPaths {
                    home: "/root".into(),
                    needs_sudo: false,
                    ..RemoteSelectedPaths::default()
                },
                bots,
            }),
        }
    }

    fn existing_bot(qq: u64, target: &str) -> BotConfig {
        serde_json::from_value(serde_json::json!({
            "bot": { "name": "x", "QQID": qq, "runtime_target": target },
            "connect": {},
            "advanced": {}
        }))
        .unwrap()
    }

    #[test]
    fn marks_imported_and_conflict() {
        let servers = vec![server(
            "s1",
            "kunming",
            vec![
                discovered(10001, BackendType::SnowLuma),
                discovered(10002, BackendType::NapCat),
            ],
        )];
        let existing = vec![existing_bot(10001, "s1"), existing_bot(10002, "local")];
        let rows = collect_importable_remote_bots(&servers, &existing);
        assert_eq!(rows.len(), 2);
        let a = rows.iter().find(|r| r.qq_id == 10001).unwrap();
        assert!(a.already_imported);
        assert!(!a.selectable);
        let b = rows.iter().find(|r| r.qq_id == 10002).unwrap();
        assert!(!b.already_imported);
        assert!(!b.selectable);
        assert_eq!(b.skip_reason.as_deref(), Some("该 QQ 已登记为其他实例"));
    }

    #[test]
    fn fresh_discovery_is_selectable() {
        let servers = vec![server(
            "s1",
            "kunming",
            vec![discovered(30003, BackendType::SnowLuma)],
        )];
        let rows = collect_importable_remote_bots(&servers, &[]);
        assert_eq!(rows.len(), 1);
        assert!(rows[0].selectable);
        assert!(!rows[0].already_imported);
    }
}
