//! Docker 反向 WS 到宿主机回环的桥：宿主机自 SSH，只绑定 Docker 网桥地址。
//!
//! 与跨服务器常驻隧道使用不同的目录和授权公钥；不要求 sshd GatewayPorts，
//! 不修改应用监听地址，也不把应用或隧道端口暴露到宿主机公网网卡。

use std::net::Ipv4Addr;
use std::time::Duration;

use ncd_deploy::DockerCli;
use ncd_domain::{AppInstance, AppInstanceId};
use ncd_host::{Host, HostPath, Os};
use ncd_traits::AppFrameworkError;
use serde::{Deserialize, Serialize};

use super::resident_link::{self, ResidentForward, ResidentLinkSpec};

const STATE_FILE: &str = "docker-bridge.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct DockerBridgeEndpoint {
    pub host: String,
    pub port: u16,
}

#[derive(Serialize, Deserialize)]
struct SavedBridge {
    endpoint: DockerBridgeEndpoint,
    target_port: u16,
}

fn bridge_instance(instance: &AppInstance) -> AppInstance {
    let mut bridge = instance.clone();
    bridge.id = AppInstanceId::new(format!("{}-docker-bridge", instance.id.as_str()));
    bridge
}

fn parse_gateway(text: &str) -> Result<Ipv4Addr, AppFrameworkError> {
    #[derive(Deserialize)]
    struct Ipam {
        #[serde(rename = "Gateway", default)]
        gateway: String,
    }
    let configs: Vec<Ipam> = serde_json::from_str(text)
        .map_err(|e| AppFrameworkError::Host(format!("解析 Docker 网桥地址失败：{e}")))?;
    configs
        .into_iter()
        .filter_map(|c| c.gateway.parse::<Ipv4Addr>().ok())
        .find(|ip| {
            !ip.is_unspecified() && !ip.is_loopback() && !ip.is_multicast() && !ip.is_broadcast()
        })
        .ok_or_else(|| {
            AppFrameworkError::Host("Docker 默认网桥没有可供容器访问的 IPv4 网关".into())
        })
}

async fn gateway(host: &dyn Host) -> Result<Ipv4Addr, AppFrameworkError> {
    let cli = DockerCli::new(host);
    cli.ensure_daemon_ready()
        .await
        .map_err(|e| AppFrameworkError::Host(format!("Docker 未就绪：{e}")))?;
    let command = cli
        .command()
        .arg("network")
        .arg("inspect")
        .arg("bridge")
        .arg("--format")
        .arg("{{json .IPAM.Config}}")
        .timeout(Duration::from_secs(10));
    let out = host.run_to_string(command).await.map_err(host_err)?;
    if !out.success() {
        return Err(AppFrameworkError::Host(format!(
            "读取 Docker 网桥地址失败：{}",
            out.stderr.trim()
        )));
    }
    parse_gateway(&out.stdout)
}

/// 返回容器可访问的宿主机地址。仅在 SSH 认证和实际网桥监听均确认后返回成功。
pub(super) async fn ensure_docker_loopback_bridge(
    host: &dyn Host,
    instance: &AppInstance,
    target_port: u16,
    preferred_endpoint: Option<&DockerBridgeEndpoint>,
) -> Result<DockerBridgeEndpoint, AppFrameworkError> {
    if host.os() != Os::Linux || target_port == 0 {
        return Err(AppFrameworkError::Validation(
            "Docker 桥接需要 Linux 宿主机和有效目标端口".into(),
        ));
    }
    let listen_host = gateway(host).await?;
    let bridge = bridge_instance(instance);
    let home = resident_link::remote_home(host).await?;
    let dir = resident_link::remote_link_dir(&home, &bridge.id);
    let saved = read_saved(host, &dir).await?;
    if let Some(saved) = &saved
        && saved.target_port == target_port
        && saved.endpoint.host == listen_host.to_string()
        && resident_link::bridge_ready(host, &dir, listen_host, saved.endpoint.port).await?
    {
        return Ok(saved.endpoint.clone());
    }
    let port = resident_link::ensure_resident_link(
        host,
        host,
        ResidentLinkSpec {
            instance: &bridge,
            reuse_port: saved
                .as_ref()
                .map(|s| s.endpoint.port)
                .or(preferred_endpoint
                    .filter(|e| e.host == listen_host.to_string() && e.port != 0)
                    .map(|e| e.port)),
            taken_ports: &[],
            forward: ResidentForward::BridgeLoopback {
                listen_host,
                target_port,
            },
        },
    )
    .await?;
    let endpoint = DockerBridgeEndpoint {
        host: listen_host.to_string(),
        port,
    };
    let saved = SavedBridge {
        endpoint: endpoint.clone(),
        target_port,
    };
    let bytes =
        serde_json::to_vec(&saved).map_err(|e| AppFrameworkError::Runtime(e.to_string()))?;
    if let Err(error) = host.write_file(&dir.join(STATE_FILE), &bytes).await {
        if let Err(cleanup) =
            resident_link::teardown_resident_link(host, Some(host), &bridge.id).await
        {
            return Err(AppFrameworkError::Host(format!(
                "写入 Docker 桥接记录失败：{error}；清理失败：{cleanup}"
            )));
        }
        return Err(host_err(error));
    }
    Ok(endpoint)
}

pub(super) async fn teardown_docker_loopback_bridge(
    host: &dyn Host,
    instance: &AppInstance,
) -> Result<(), AppFrameworkError> {
    resident_link::teardown_resident_link(host, Some(host), &bridge_instance(instance).id).await
}

async fn read_saved(
    host: &dyn Host,
    dir: &HostPath,
) -> Result<Option<SavedBridge>, AppFrameworkError> {
    let path = dir.join(STATE_FILE);
    if !host.exists(&path).await.map_err(host_err)? {
        return Ok(None);
    }
    let bytes = host.read_file(&path).await.map_err(host_err)?;
    let saved: SavedBridge = serde_json::from_slice(&bytes)
        .map_err(|e| AppFrameworkError::Runtime(format!("Docker 桥接记录损坏：{e}")))?;
    if saved.endpoint.port == 0 || saved.target_port == 0 {
        return Err(AppFrameworkError::Runtime("Docker 桥接记录端口无效".into()));
    }
    Ok(Some(saved))
}

fn host_err(e: ncd_host::HostError) -> AppFrameworkError {
    AppFrameworkError::Host(e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gateway_comes_from_docker_ipv4_ipam_and_rejects_wildcard_or_loopback() {
        assert_eq!(
            parse_gateway(r#"[{"Gateway":"fd00::1"},{"Gateway":"172.17.0.1"}]"#).unwrap(),
            Ipv4Addr::new(172, 17, 0, 1)
        );
        for bad in [
            "[]",
            r#"[{"Gateway":"0.0.0.0"}]"#,
            r#"[{"Gateway":"127.0.0.1"}]"#,
            "invalid",
        ] {
            assert!(parse_gateway(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn bridge_authorization_does_not_replace_primary_resident_key() {
        let primary = AppInstanceId::new("app1");
        let bridge = AppInstanceId::new("app1-docker-bridge");
        let primary_line = resident_link::authorized_keys_line(
            "ssh-ed25519 AAAprimary x",
            &primary,
            ResidentForward::ExposeApp { app_port: 8080 },
            25001,
        )
        .unwrap();
        let bridge_line = resident_link::authorized_keys_line(
            "ssh-ed25519 AAAbridge x",
            &bridge,
            ResidentForward::BridgeLoopback {
                listen_host: Ipv4Addr::new(172, 17, 0, 1),
                target_port: 25001,
            },
            25002,
        )
        .unwrap();
        let both = resident_link::upsert_authorized_key(&primary_line, &bridge_line, &bridge);
        assert!(both.contains("ncd-link:app1\n"));
        assert!(both.contains("ncd-link:app1-docker-bridge"));
        assert!(bridge_line.contains("permitopen=\"127.0.0.1:25001\""));
        assert_eq!(
            resident_link::remove_authorized_key(&both, &bridge),
            format!("{primary_line}\n")
        );
    }
}
