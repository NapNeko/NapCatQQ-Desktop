//! 从 Bot 配置推出它有哪些调试通道、每条从桌面端怎么连过去。纯函数，不碰网络。
//!
//! 「怎么连」只看三件事：Bot 跑在哪（本机 / 远端原生 / 远端 Docker）、服务监听在哪个地址、
//! 端口有没有映射出来。能不能真的连上要等探测或调用时才知道，这里只排除注定连不上的。

use std::collections::HashSet;

use ncd_deploy::DockerDeployment;
use ncd_domain::RuntimeScenario;
use ncd_domain::bot_config::{BotConfig, WsRole};
use ncd_domain::onebot_debug::{DebugChannelId, mask_token};

/// NapCat / SnowLuma 的 Docker 编排只把这两个容器端口映射到宿主机
const DOCKER_ONEBOT_PORTS: [u16; 2] = [3000, 3001];

/// 一条通道从桌面端怎么连过去
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Reach {
    /// 走上游 WebUI 自带的调试接口，地址由后端的 WebUI 端点决定
    Internal,
    /// 桌面端直接连 `host:port`
    Direct {
        host: String,
        port: u16,
        path: String,
    },
    /// 经 SSH `-L` 隧道连远端主机上的 `remote_host:remote_port`（从远端主机自己看的地址）
    Tunnel {
        remote_host: String,
        remote_port: u16,
        path: String,
    },
    /// 注定连不上，`reason` 直接给人看
    Unsupported { reason: String },
}

/// 一条通道的静态规划：能力、连法、令牌，以及「自动」挑选时的优先级（越小越先）。
/// `Debug` 手写：令牌只显示打码后的样子，日志、panic 信息里不出现明文
#[derive(Clone, PartialEq)]
pub struct ChannelPlan {
    pub id: DebugChannelId,
    pub label: String,
    pub can_call: bool,
    pub can_receive: bool,
    pub reach: Reach,
    pub token: Option<String>,
    pub rank_call: Option<u8>,
    pub rank_events: Option<u8>,
}

impl std::fmt::Debug for ChannelPlan {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChannelPlan")
            .field("id", &self.id)
            .field("label", &self.label)
            .field("can_call", &self.can_call)
            .field("can_receive", &self.can_receive)
            .field("reach", &self.reach)
            .field("token", &self.token.as_deref().and_then(mask_token))
            .field("rank_call", &self.rank_call)
            .field("rank_events", &self.rank_events)
            .finish()
    }
}

/// 列出一个 Bot 的全部通道：内部通道打头，然后是已启用的 HTTP 服务、WS 服务，保持配置里的顺序
pub fn plan_channels(config: &BotConfig) -> Vec<ChannelPlan> {
    let scenario = RuntimeScenario::from_config(config).ok();
    let mut plans = vec![ChannelPlan {
        id: DebugChannelId::Internal,
        label: "内部通道（WebUI）".to_owned(),
        can_call: true,
        can_receive: true,
        reach: Reach::Internal,
        token: None,
        rank_call: Some(0),
        rank_events: Some(0),
    }];

    for server in config.connect.http_servers.iter().filter(|s| s.base.enable) {
        plans.push(ChannelPlan {
            id: DebugChannelId::Http {
                name: server.base.name.clone(),
            },
            label: format!("HTTP · {} :{}", server.base.name, server.port),
            can_call: true,
            can_receive: false,
            reach: reach_for(
                scenario.as_ref(),
                config,
                &server.host,
                server.port,
                &server.path,
            ),
            token: non_empty(&server.base.token),
            rank_call: Some(3),
            rank_events: None,
        });
    }

    for server in config
        .connect
        .websocket_servers
        .iter()
        .filter(|s| s.base.enable)
    {
        let (can_call, can_receive, rank_call, rank_events) = match server.role {
            WsRole::Api => (true, false, Some(2), None),
            WsRole::Event => (false, true, None, Some(2)),
            WsRole::Universal => (true, true, Some(1), Some(1)),
        };
        plans.push(ChannelPlan {
            id: DebugChannelId::Ws {
                name: server.base.name.clone(),
            },
            label: format!("WS · {} :{}", server.base.name, server.port),
            can_call,
            can_receive,
            reach: reach_for(
                scenario.as_ref(),
                config,
                &server.host,
                server.port,
                &server.path,
            ),
            token: non_empty(&server.base.token),
            rank_call,
            rank_events,
        });
    }

    plans
}

/// 「自动」落到哪条：对应用途的优先级最小、不是注定连不上、也没被排除的那条；
/// 同优先级取配置里靠前的
pub fn pick_auto(
    plans: &[ChannelPlan],
    for_call: bool,
    excluded: &HashSet<DebugChannelId>,
) -> Option<DebugChannelId> {
    plans
        .iter()
        .filter(|p| !matches!(p.reach, Reach::Unsupported { .. }))
        .filter(|p| !excluded.contains(&p.id))
        .filter_map(|p| {
            let rank = if for_call { p.rank_call } else { p.rank_events };
            rank.map(|r| (r, p))
        })
        // min_by_key 在并列时返回第一个，正好保住配置顺序
        .min_by_key(|(rank, _)| *rank)
        .map(|(_, p)| p.id.clone())
}

/// 服务监听地址 → 桌面端的连法
fn reach_for(
    scenario: Option<&RuntimeScenario>,
    config: &BotConfig,
    host: &str,
    port: u16,
    path: &str,
) -> Reach {
    let path = normalize_path(path);
    let host = host.trim();
    match scenario {
        None => Reach::Unsupported {
            reason: "Bot 配置不完整".to_owned(),
        },
        Some(RuntimeScenario::LocalNative { .. }) => Reach::Direct {
            host: connect_host(host),
            port,
            path,
        },
        Some(RuntimeScenario::RemoteNative { .. }) => {
            if is_any_or_loopback(host) {
                Reach::Tunnel {
                    remote_host: connect_host(host),
                    remote_port: port,
                    path,
                }
            } else {
                Reach::Unsupported {
                    reason: format!("服务只监听在 {host}，暂不支持远端直连"),
                }
            }
        }
        Some(RuntimeScenario::RemoteDocker { .. }) => {
            if !DOCKER_ONEBOT_PORTS.contains(&port) {
                Reach::Unsupported {
                    reason: format!("容器没有映射 {port} 端口"),
                }
            } else if is_loopback(host) {
                // 容器里的回环只在容器内部可达，宿主机的端口映射转不进去
                Reach::Unsupported {
                    reason: "容器内只监听 127.0.0.1，映射出来也连不上".to_owned(),
                }
            } else if is_any(host) {
                let remote_port = DockerDeployment::build_spec(config)
                    .host_port_for_container(port)
                    .unwrap_or(port);
                // 连的是宿主机上发布出来的端口，不是容器里的监听地址；发布默认覆盖宿主机回环
                Reach::Tunnel {
                    remote_host: "127.0.0.1".to_owned(),
                    remote_port,
                    path,
                }
            } else {
                Reach::Unsupported {
                    reason: format!("服务只监听在 {host}，暂不支持远端直连"),
                }
            }
        }
    }
}

/// 「所有网卡」或「本机回环」：本机上能直接连到，远端上能经隧道连到
fn is_any_or_loopback(host: &str) -> bool {
    is_any(host) || is_loopback(host)
}

/// 监听地址 → 要连的地址。只有「所有网卡」换成 127.0.0.1（监听 `::` 的双栈服务照样收 IPv4）；
/// 回环照原样连：只听 `::1` 的服务连 127.0.0.1 是连不上的，`localhost` 也可能只解析到 `::1`。
/// IPv6 字面量去掉方括号，拼地址时再统一加
fn connect_host(host: &str) -> String {
    let host = host.trim();
    if is_any(host) {
        return "127.0.0.1".to_owned();
    }
    host.strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .unwrap_or(host)
        .to_owned()
}

fn is_any(host: &str) -> bool {
    matches!(host.trim(), "" | "0.0.0.0" | "::" | "[::]")
}

fn is_loopback(host: &str) -> bool {
    let host = host.trim();
    host.eq_ignore_ascii_case("localhost") || matches!(host, "127.0.0.1" | "::1" | "[::1]")
}

/// 路径缺省为 `/`，没写前导 `/` 的补上
fn normalize_path(path: &str) -> String {
    let path = path.trim();
    if path.is_empty() {
        "/".to_owned()
    } else if path.starts_with('/') {
        path.to_owned()
    } else {
        format!("/{path}")
    }
}

fn non_empty(token: &str) -> Option<String> {
    (!token.is_empty()).then(|| token.to_owned())
}

#[cfg(test)]
mod tests {
    use ncd_domain::bot_config::{
        BackendType, DeploymentType, HttpServerConfig, MessagePostFormat, NetworkBaseFields,
        WebsocketServerConfig,
    };
    use ncd_domain::kinds::RuntimeTarget;
    use ncd_test_support::BotConfigBuilder;

    use super::*;

    const HOSTS: [&str; 3] = ["0.0.0.0", "127.0.0.1", "10.0.0.5"];

    #[derive(Clone, Copy, Debug)]
    enum Where {
        Local,
        Remote,
        Docker,
    }

    fn base(name: &str, token: &str, enable: bool) -> NetworkBaseFields {
        NetworkBaseFields {
            enable,
            name: name.to_owned(),
            message_post_format: MessagePostFormat::Array,
            token: token.to_owned(),
            debug: false,
        }
    }

    fn http(name: &str, host: &str, port: u16) -> HttpServerConfig {
        HttpServerConfig {
            base: base(name, "", true),
            host: host.to_owned(),
            port,
            enable_cors: false,
            enable_websocket: false,
            path: "/".to_owned(),
        }
    }

    fn ws(name: &str, host: &str, port: u16, role: WsRole) -> WebsocketServerConfig {
        WebsocketServerConfig {
            base: base(name, "", true),
            host: host.to_owned(),
            port,
            report_self_message: false,
            enable_force_push_event: false,
            heart_interval: 30_000,
            path: "/".to_owned(),
            role,
        }
    }

    fn config(place: Where, qq: u64) -> BotConfig {
        let builder = BotConfigBuilder::new()
            .qq_id(qq)
            .backend_type(BackendType::NapCat);
        match place {
            Where::Local => builder.runtime_target(RuntimeTarget::Local),
            Where::Remote => builder
                .runtime_target(RuntimeTarget::server("vps"))
                .deployment_type(DeploymentType::Native),
            Where::Docker => builder
                .runtime_target(RuntimeTarget::server("vps"))
                .deployment_type(DeploymentType::Docker),
        }
        .build()
    }

    fn only_http(place: Where, host: &str, port: u16) -> ChannelPlan {
        let mut cfg = config(place, 10_001);
        cfg.connect.http_servers.push(http("h", host, port));
        let plans = plan_channels(&cfg);
        assert_eq!(plans.len(), 2);
        plans[1].clone()
    }

    fn unsupported(reason: &str) -> Reach {
        Reach::Unsupported {
            reason: reason.to_owned(),
        }
    }

    #[test]
    fn internal_channel_always_comes_first() {
        let plans = plan_channels(&config(Where::Local, 10_001));
        assert_eq!(plans.len(), 1);
        let internal = &plans[0];
        assert_eq!(internal.id, DebugChannelId::Internal);
        assert_eq!(internal.label, "内部通道（WebUI）");
        assert!(internal.can_call && internal.can_receive);
        assert_eq!(internal.reach, Reach::Internal);
        assert_eq!(
            (internal.rank_call, internal.rank_events),
            (Some(0), Some(0))
        );
        assert_eq!(internal.token, None);
    }

    #[test]
    fn reach_matrix_for_every_host_kind_and_listen_address() {
        // Docker 用 qq=10001 → 宿主机端口偏移 10001 % 500 = 1
        let docker_offset = 1;
        for host in HOSTS {
            for port in [3000_u16, 3001, 8080] {
                let local = only_http(Where::Local, host, port).reach;
                let expected_host = if host == "10.0.0.5" {
                    host
                } else {
                    "127.0.0.1"
                };
                assert_eq!(
                    local,
                    Reach::Direct {
                        host: expected_host.to_owned(),
                        port,
                        path: "/".to_owned()
                    },
                    "local {host}:{port}"
                );

                let remote = only_http(Where::Remote, host, port).reach;
                let expected = if host == "10.0.0.5" {
                    unsupported("服务只监听在 10.0.0.5，暂不支持远端直连")
                } else {
                    Reach::Tunnel {
                        remote_host: "127.0.0.1".to_owned(),
                        remote_port: port,
                        path: "/".to_owned(),
                    }
                };
                assert_eq!(remote, expected, "remote {host}:{port}");

                let docker = only_http(Where::Docker, host, port).reach;
                let expected = match (host, port) {
                    (_, 8080) => unsupported("容器没有映射 8080 端口"),
                    ("127.0.0.1", _) => unsupported("容器内只监听 127.0.0.1，映射出来也连不上"),
                    ("10.0.0.5", _) => unsupported("服务只监听在 10.0.0.5，暂不支持远端直连"),
                    _ => Reach::Tunnel {
                        remote_host: "127.0.0.1".to_owned(),
                        remote_port: port + docker_offset,
                        path: "/".to_owned(),
                    },
                };
                assert_eq!(docker, expected, "docker {host}:{port}");
            }
        }
    }

    #[test]
    fn any_and_loopback_spellings_are_recognized() {
        for host in ["", " ", "0.0.0.0", "::", "[::]"] {
            assert!(is_any(host), "{host:?}");
            assert!(!is_loopback(host), "{host:?}");
        }
        for host in ["127.0.0.1", "localhost", "LocalHost", "::1", "[::1]"] {
            assert!(is_loopback(host), "{host:?}");
            assert!(!is_any(host), "{host:?}");
        }
        for host in ["10.0.0.5", "192.168.1.2", "fe80::1", "example.com"] {
            assert!(!is_any_or_loopback(host), "{host:?}");
        }
        // Docker 里写 localhost / ::1 同样出不了容器
        assert_eq!(
            only_http(Where::Docker, "localhost", 3000).reach,
            unsupported("容器内只监听 127.0.0.1，映射出来也连不上")
        );
        assert_eq!(
            only_http(Where::Docker, "::", 3001).reach,
            Reach::Tunnel {
                remote_host: "127.0.0.1".to_owned(),
                remote_port: 3002,
                path: "/".to_owned()
            }
        );
    }

    #[test]
    fn loopback_is_kept_as_written_and_only_any_becomes_ipv4_loopback() {
        // (监听地址, 要连的地址)。只听 ::1 的服务连 127.0.0.1 是连不上的；
        // localhost 可能只解析到 ::1，照原样交给解析器
        let cases = [
            ("", "127.0.0.1"),
            ("0.0.0.0", "127.0.0.1"),
            ("::", "127.0.0.1"),
            ("[::]", "127.0.0.1"),
            ("127.0.0.1", "127.0.0.1"),
            ("::1", "::1"),
            ("[::1]", "::1"),
            ("localhost", "localhost"),
            (" LocalHost ", "LocalHost"),
        ];
        for (listen, connect) in cases {
            assert_eq!(
                only_http(Where::Local, listen, 3000).reach,
                Reach::Direct {
                    host: connect.to_owned(),
                    port: 3000,
                    path: "/".to_owned()
                },
                "local {listen:?}"
            );
            assert_eq!(
                only_http(Where::Remote, listen, 3000).reach,
                Reach::Tunnel {
                    remote_host: connect.to_owned(),
                    remote_port: 3000,
                    path: "/".to_owned()
                },
                "remote {listen:?}"
            );
        }
        // 具体的 IPv6 地址本机照连，去掉方括号，拼 URL 时再加
        assert_eq!(
            only_http(Where::Local, "[fe80::1]", 3000).reach,
            Reach::Direct {
                host: "fe80::1".to_owned(),
                port: 3000,
                path: "/".to_owned()
            }
        );
        assert_eq!(connect_host("fe80::1"), "fe80::1");
    }

    #[test]
    fn ws_roles_decide_capabilities_and_ranks() {
        for place in [Where::Local, Where::Remote, Where::Docker] {
            for host in HOSTS {
                let mut cfg = config(place, 10_001);
                cfg.connect.websocket_servers = vec![
                    ws("api", host, 3001, WsRole::Api),
                    ws("event", host, 3001, WsRole::Event),
                    ws("uni", host, 3001, WsRole::Universal),
                ];
                let plans = plan_channels(&cfg);
                assert_eq!(plans.len(), 4, "{place:?} {host}");
                let caps: Vec<_> = plans[1..]
                    .iter()
                    .map(|p| (p.can_call, p.can_receive, p.rank_call, p.rank_events))
                    .collect();
                assert_eq!(
                    caps,
                    vec![
                        (true, false, Some(2), None),
                        (false, true, None, Some(2)),
                        (true, true, Some(1), Some(1)),
                    ],
                    "{place:?} {host}"
                );
                // 同一个监听地址，三种角色的连法一致
                assert!(plans[1..].iter().all(|p| p.reach == plans[1].reach));
                assert_eq!(plans[3].label, "WS · uni :3001");
            }
        }
    }

    #[test]
    fn http_plan_carries_label_token_and_rank() {
        let mut cfg = config(Where::Local, 10_001);
        let mut server = http("main", "0.0.0.0", 3000);
        server.base.token = "secret-token".into();
        server.path = "onebot".into();
        cfg.connect.http_servers.push(server);
        let plan = &plan_channels(&cfg)[1];
        assert_eq!(
            plan.id,
            DebugChannelId::Http {
                name: "main".into()
            }
        );
        assert_eq!(plan.label, "HTTP · main :3000");
        assert!(plan.can_call && !plan.can_receive);
        assert_eq!((plan.rank_call, plan.rank_events), (Some(3), None));
        assert_eq!(plan.token.as_deref(), Some("secret-token"));
        assert_eq!(
            plan.reach,
            Reach::Direct {
                host: "127.0.0.1".into(),
                port: 3000,
                path: "/onebot".into()
            }
        );
    }

    #[test]
    fn disabled_servers_are_skipped() {
        let mut cfg = config(Where::Local, 10_001);
        let mut off_http = http("off-http", "0.0.0.0", 3000);
        off_http.base.enable = false;
        let mut off_ws = ws("off-ws", "0.0.0.0", 3001, WsRole::Universal);
        off_ws.base.enable = false;
        cfg.connect.http_servers = vec![off_http, http("on-http", "0.0.0.0", 3002)];
        cfg.connect.websocket_servers = vec![off_ws];
        let ids: Vec<_> = plan_channels(&cfg).into_iter().map(|p| p.id).collect();
        assert_eq!(
            ids,
            vec![
                DebugChannelId::Internal,
                DebugChannelId::Http {
                    name: "on-http".into()
                }
            ]
        );
    }

    #[test]
    fn broken_scenario_marks_every_real_channel_unsupported() {
        // 本机 + Docker 是不支持的组合
        let mut cfg = BotConfigBuilder::new()
            .runtime_target(RuntimeTarget::Local)
            .deployment_type(DeploymentType::Docker)
            .build();
        cfg.connect.http_servers.push(http("h", "0.0.0.0", 3000));
        cfg.connect
            .websocket_servers
            .push(ws("w", "0.0.0.0", 3001, WsRole::Universal));
        let plans = plan_channels(&cfg);
        assert_eq!(plans[0].reach, Reach::Internal);
        for plan in &plans[1..] {
            assert_eq!(plan.reach, unsupported("Bot 配置不完整"));
        }
    }

    #[test]
    fn pick_auto_prefers_lower_rank_and_honours_exclusion() {
        let mut cfg = config(Where::Local, 10_001);
        cfg.connect.http_servers.push(http("h", "0.0.0.0", 3000));
        cfg.connect.websocket_servers = vec![
            ws("api", "0.0.0.0", 3001, WsRole::Api),
            ws("event", "0.0.0.0", 3002, WsRole::Event),
            ws("uni", "0.0.0.0", 3003, WsRole::Universal),
        ];
        let plans = plan_channels(&cfg);
        let none = HashSet::new();
        let uni = DebugChannelId::Ws { name: "uni".into() };
        let api = DebugChannelId::Ws { name: "api".into() };
        let event = DebugChannelId::Ws {
            name: "event".into(),
        };
        let http_id = DebugChannelId::Http { name: "h".into() };

        assert_eq!(
            pick_auto(&plans, true, &none),
            Some(DebugChannelId::Internal)
        );
        assert_eq!(
            pick_auto(&plans, false, &none),
            Some(DebugChannelId::Internal)
        );

        let mut excluded = HashSet::from([DebugChannelId::Internal]);
        assert_eq!(pick_auto(&plans, true, &excluded), Some(uni.clone()));
        assert_eq!(pick_auto(&plans, false, &excluded), Some(uni.clone()));

        excluded.insert(uni);
        assert_eq!(pick_auto(&plans, true, &excluded), Some(api.clone()));
        assert_eq!(pick_auto(&plans, false, &excluded), Some(event.clone()));

        excluded.insert(api);
        excluded.insert(event);
        assert_eq!(pick_auto(&plans, true, &excluded), Some(http_id.clone()));
        // HTTP 收不了事件
        assert_eq!(pick_auto(&plans, false, &excluded), None);

        excluded.insert(http_id);
        assert_eq!(pick_auto(&plans, true, &excluded), None);
    }

    #[test]
    fn pick_auto_skips_unsupported_and_keeps_config_order_on_ties() {
        let mut cfg = config(Where::Remote, 10_001);
        cfg.connect.websocket_servers = vec![
            ws("pinned", "10.0.0.5", 3001, WsRole::Universal),
            ws("first", "0.0.0.0", 3002, WsRole::Universal),
            ws("second", "0.0.0.0", 3003, WsRole::Universal),
        ];
        let plans = plan_channels(&cfg);
        let excluded = HashSet::from([DebugChannelId::Internal]);
        assert_eq!(
            pick_auto(&plans, true, &excluded),
            Some(DebugChannelId::Ws {
                name: "first".into()
            })
        );
    }

    #[test]
    fn empty_path_becomes_root() {
        assert_eq!(normalize_path(""), "/");
        assert_eq!(normalize_path(" /x "), "/x");
        assert_eq!(normalize_path("x/y"), "/x/y");
    }
}
