// 配置渲染器已下沉到 ncd-deploy,此处 re-export 保持向后兼容
pub use ncd_deploy::backend_config_renderer::{
    DispatchRenderer, DockerConfigPayload, NapCatConfigRenderer, SnowLumaConfigRenderer,
    create_renderer, merge_unknown_top_level, output_paths_for_backend,
    render_napcat_docker_config_payloads, render_snowluma_docker_config_payloads,
};

#[cfg(test)]
mod tests {
    use super::*;
    use ncd_domain::BotId;
    use ncd_domain::bot_config::*;
    use ncd_domain::kinds::RuntimeTarget;
    use ncd_traits::BackendConfigRenderer;

    fn make_bot_id() -> BotId {
        BotId::new("10001")
    }

    fn make_basic_config() -> BotBasicConfig {
        BotBasicConfig {
            name: "test-bot".to_string(),
            qq_id: 10001,
            music_sign_url: "https://sign.example.com".to_string(),
            auto_restart_schedule: AutoRestartSchedule::default(),
            offline_auto_restart: false,
            runtime_target: RuntimeTarget::Local,
            backend_type: BackendType::NapCat,
            deployment_type: DeploymentType::Native,
            snowluma_start_mode: None,
        }
    }

    fn make_advanced_config() -> AdvancedConfig {
        AdvancedConfig {
            auto_start: true,
            offline_notice: false,
            parse_mult_msg: true,
            packet_server: "".to_string(),
            packet_backend: "auto".to_string(),
            enable_local_file_to_url: true,
            file_log: true,
            console_log: true,
            file_log_level: LogLevel::Debug,
            console_log_level: LogLevel::Info,
            o3_hook_mode: O3HookMode::On,
            bypass: BypassConfig::default(),
        }
    }

    fn make_connect_with_http_server() -> ConnectConfig {
        ConnectConfig {
            http_servers: vec![HttpServerConfig {
                base: NetworkBaseFields {
                    enable: true,
                    name: "http-main".to_string(),
                    message_post_format: MessagePostFormat::Array,
                    token: "secret123".to_string(),
                    debug: false,
                },
                host: "127.0.0.1".to_string(),
                port: 8080,
                enable_cors: true,
                enable_websocket: false,
                path: "/".to_string(),
            }],
            http_sse_servers: vec![],
            http_clients: vec![],
            websocket_servers: vec![WebsocketServerConfig {
                base: NetworkBaseFields {
                    enable: true,
                    name: "ws-main".to_string(),
                    message_post_format: MessagePostFormat::Array,
                    token: "secret123".to_string(),
                    debug: false,
                },
                host: "0.0.0.0".to_string(),
                port: 8081,
                report_self_message: false,
                enable_force_push_event: false,
                heart_interval: 30000,
                path: "/".to_string(),
                role: WsRole::Universal,
            }],
            websocket_clients: vec![],
            plugins: vec![],
        }
    }

    fn make_full_config() -> BotConfig {
        BotConfig {
            bot: make_basic_config(),
            connect: make_connect_with_http_server(),
            advanced: make_advanced_config(),
            status_command: None,
        }
    }

    // ==================== NapCat Renderer Tests ====================

    #[test]
    fn napcat_render_produces_two_files() {
        let renderer = NapCatConfigRenderer::new("/tmp/napcat/config");
        let bot_id = make_bot_id();
        let config = make_full_config();

        let txn = renderer.render(&bot_id, &config).unwrap();

        assert_eq!(txn.writes.len(), 2);
        assert!(txn.deletes.is_empty());

        let paths: Vec<_> = txn
            .writes
            .iter()
            .map(|w| w.path.to_string_lossy().to_string())
            .collect();
        assert!(paths.iter().any(|p| p.contains("onebot11_10001.json")));
        assert!(paths.iter().any(|p| p.contains("napcat_10001.json")));
    }

    #[test]
    fn napcat_onebot_payload_contains_network_and_fields() {
        let renderer = NapCatConfigRenderer::new("/tmp/napcat/config");
        let bot_id = make_bot_id();
        let config = make_full_config();

        let txn = renderer.render(&bot_id, &config).unwrap();
        let onebot_write = txn
            .writes
            .iter()
            .find(|w| w.path.to_string_lossy().contains("onebot11_"))
            .unwrap();

        let payload = &onebot_write.payload;
        assert!(payload.get("network").is_some());
        assert_eq!(payload["musicSignUrl"], "https://sign.example.com");
        assert_eq!(payload["enableLocalFile2Url"], true);
        assert_eq!(payload["parseMultMsg"], true);

        let network = &payload["network"];
        assert!(network["httpServers"].is_array());
        assert_eq!(network["httpServers"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn napcat_napcat_payload_contains_log_and_bypass() {
        let renderer = NapCatConfigRenderer::new("/tmp/napcat/config");
        let bot_id = make_bot_id();
        let config = make_full_config();

        let txn = renderer.render(&bot_id, &config).unwrap();
        let napcat_write = txn
            .writes
            .iter()
            .find(|w| w.path.to_string_lossy().contains("napcat_"))
            .unwrap();

        let payload = &napcat_write.payload;
        assert_eq!(payload["fileLog"], true);
        assert_eq!(payload["consoleLog"], true);
        assert_eq!(payload["fileLogLevel"], "debug");
        assert_eq!(payload["consoleLogLevel"], "info");
        assert_eq!(payload["packetBackend"], "auto");
        assert_eq!(payload["o3HookMode"], 1);
        assert!(payload.get("bypass").is_some());
    }

    #[test]
    fn napcat_output_paths_returns_two() {
        let renderer = NapCatConfigRenderer::new("/tmp/napcat/config");
        let bot_id = make_bot_id();
        let paths = renderer.output_paths(&bot_id);
        assert_eq!(paths.len(), 2);
    }

    // ==================== SnowLuma Renderer Tests ====================

    #[test]
    fn snowluma_render_produces_one_file() {
        let renderer = SnowLumaConfigRenderer::new("/tmp/snowluma");
        let bot_id = make_bot_id();
        let config = make_full_config();

        let txn = renderer.render(&bot_id, &config).unwrap();

        assert_eq!(txn.writes.len(), 1);
        assert!(txn.deletes.is_empty());
        assert!(
            txn.writes[0]
                .path
                .to_string_lossy()
                .contains("onebot_10001.json")
        );
    }

    #[test]
    fn snowluma_field_mapping_enable_to_enabled() {
        let renderer = SnowLumaConfigRenderer::new("/tmp/snowluma");
        let bot_id = make_bot_id();
        let config = make_full_config();

        let txn = renderer.render(&bot_id, &config).unwrap();
        let payload = &txn.writes[0].payload;

        let http = &payload["networks"]["httpServers"][0];
        assert_eq!(http["enabled"], true);
        assert!(http.get("enable").is_none());
    }

    #[test]
    fn snowluma_field_mapping_token_to_access_token() {
        let renderer = SnowLumaConfigRenderer::new("/tmp/snowluma");
        let bot_id = make_bot_id();
        let config = make_full_config();

        let txn = renderer.render(&bot_id, &config).unwrap();
        let payload = &txn.writes[0].payload;

        let http = &payload["networks"]["httpServers"][0];
        assert_eq!(http["accessToken"], "secret123");
        assert!(http.get("token").is_none());
    }

    #[test]
    fn snowluma_field_mapping_message_format() {
        let renderer = SnowLumaConfigRenderer::new("/tmp/snowluma");
        let bot_id = make_bot_id();
        let config = make_full_config();

        let txn = renderer.render(&bot_id, &config).unwrap();
        let payload = &txn.writes[0].payload;

        let http = &payload["networks"]["httpServers"][0];
        assert_eq!(http["messageFormat"], "array");
        assert!(http.get("messagePostFormat").is_none());
    }

    #[test]
    fn snowluma_ws_servers_key_name() {
        let renderer = SnowLumaConfigRenderer::new("/tmp/snowluma");
        let bot_id = make_bot_id();
        let config = make_full_config();

        let txn = renderer.render(&bot_id, &config).unwrap();
        let networks = &txn.writes[0].payload["networks"];

        assert!(networks.get("wsServers").is_some());
        assert!(networks.get("websocketServers").is_none());
        assert_eq!(networks["wsServers"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn snowluma_drops_napcat_only_fields() {
        let renderer = SnowLumaConfigRenderer::new("/tmp/snowluma");
        let bot_id = make_bot_id();
        let config = make_full_config();

        let txn = renderer.render(&bot_id, &config).unwrap();
        let payload = &txn.writes[0].payload;

        let http = &payload["networks"]["httpServers"][0];
        assert!(http.get("enableCors").is_none());
        assert!(http.get("enableWebsocket").is_none());
        assert!(http.get("debug").is_none());

        let ws = &payload["networks"]["wsServers"][0];
        assert!(ws.get("enableForcePushEvent").is_none());
        assert!(ws.get("heartInterval").is_none());
    }

    #[test]
    fn snowluma_drops_http_sse_servers() {
        let renderer = SnowLumaConfigRenderer::new("/tmp/snowluma");
        let bot_id = make_bot_id();
        let config = make_full_config();

        let txn = renderer.render(&bot_id, &config).unwrap();
        let networks = &txn.writes[0].payload["networks"];

        assert!(networks.get("httpSseServers").is_none());
    }

    #[test]
    fn snowluma_fallback_when_no_servers() {
        let renderer = SnowLumaConfigRenderer::new("/tmp/snowluma");
        let bot_id = make_bot_id();
        let config = BotConfig {
            bot: make_basic_config(),
            connect: ConnectConfig::default(),
            advanced: make_advanced_config(),
            status_command: None,
        };

        let txn = renderer.render(&bot_id, &config).unwrap();
        let networks = &txn.writes[0].payload["networks"];

        assert_eq!(networks["httpServers"].as_array().unwrap().len(), 1);
        assert_eq!(networks["wsServers"].as_array().unwrap().len(), 1);

        let http = &networks["httpServers"][0];
        assert_eq!(http["name"], "http-default");
        assert_eq!(http["port"], 3000);

        let ws = &networks["wsServers"][0];
        assert_eq!(ws["name"], "ws-default");
        assert_eq!(ws["port"], 3001);
        assert_eq!(ws["role"], "Universal");
    }

    #[tokio::test]
    async fn snowluma_render_output_is_drift_clean_for_empty_connect() {
        let dir = tempfile::tempdir().unwrap();
        let renderer = SnowLumaConfigRenderer::new(dir.path());
        let bot_id = make_bot_id();
        let config = BotConfig {
            bot: BotBasicConfig {
                backend_type: BackendType::SnowLuma,
                ..make_basic_config()
            },
            connect: ConnectConfig::default(),
            advanced: make_advanced_config(),
            status_command: None,
        };

        let txn = renderer.render(&bot_id, &config).unwrap();
        let write = &txn.writes[0];
        std::fs::write(
            &write.path,
            serde_json::to_vec_pretty(&write.payload).unwrap(),
        )
        .unwrap();

        let drift = crate::drift::detect_drift(&bot_id, &config, &renderer)
            .await
            .unwrap();
        assert!(
            drift.is_clean(),
            "空连接 render 输出不应自漂移: added={:?} modified={:?}",
            drift.added,
            drift.modified
        );
    }

    #[test]
    fn snowluma_ws_client_reconnect_interval_clamped() {
        let renderer = SnowLumaConfigRenderer::new("/tmp/snowluma");
        let bot_id = make_bot_id();
        let config = BotConfig {
            bot: make_basic_config(),
            connect: ConnectConfig {
                websocket_clients: vec![WebsocketClientConfig {
                    base: NetworkBaseFields {
                        enable: true,
                        name: "ws-client".to_string(),
                        message_post_format: MessagePostFormat::Array,
                        token: "".to_string(),
                        debug: false,
                    },
                    url: "ws://127.0.0.1:6700".to_string(),
                    report_self_message: false,
                    heart_interval: 30000,
                    reconnect_interval: 500,
                    role: WsRole::Universal,
                }],
                http_servers: vec![HttpServerConfig {
                    base: NetworkBaseFields {
                        enable: true,
                        name: "http-placeholder".to_string(),
                        message_post_format: MessagePostFormat::Array,
                        token: "".to_string(),
                        debug: false,
                    },
                    host: "0.0.0.0".to_string(),
                    port: 3000,
                    enable_cors: false,
                    enable_websocket: false,
                    path: "/".to_string(),
                }],
                ..ConnectConfig::default()
            },
            advanced: make_advanced_config(),
            status_command: None,
        };

        let txn = renderer.render(&bot_id, &config).unwrap();
        let ws_client = &txn.writes[0].payload["networks"]["wsClients"][0];
        assert_eq!(ws_client["reconnectIntervalMs"], 1000);
    }

    #[test]
    fn snowluma_ws_client_reconnect_interval_passthrough_when_above_minimum() {
        let renderer = SnowLumaConfigRenderer::new("/tmp/snowluma");
        let bot_id = make_bot_id();
        let config = BotConfig {
            bot: make_basic_config(),
            connect: ConnectConfig {
                websocket_clients: vec![WebsocketClientConfig {
                    base: NetworkBaseFields {
                        enable: true,
                        name: "ws-client".to_string(),
                        message_post_format: MessagePostFormat::Array,
                        token: "".to_string(),
                        debug: false,
                    },
                    url: "ws://127.0.0.1:6700".to_string(),
                    report_self_message: false,
                    heart_interval: 30000,
                    reconnect_interval: 5000,
                    role: WsRole::Universal,
                }],
                http_servers: vec![HttpServerConfig {
                    base: NetworkBaseFields {
                        enable: true,
                        name: "http-placeholder".to_string(),
                        message_post_format: MessagePostFormat::Array,
                        token: "".to_string(),
                        debug: false,
                    },
                    host: "0.0.0.0".to_string(),
                    port: 3000,
                    enable_cors: false,
                    enable_websocket: false,
                    path: "/".to_string(),
                }],
                ..ConnectConfig::default()
            },
            advanced: make_advanced_config(),
            status_command: None,
        };

        let txn = renderer.render(&bot_id, &config).unwrap();
        let ws_client = &txn.writes[0].payload["networks"]["wsClients"][0];
        assert_eq!(ws_client["reconnectIntervalMs"], 5000);
    }

    #[test]
    fn snowluma_music_sign_url_in_payload() {
        let renderer = SnowLumaConfigRenderer::new("/tmp/snowluma");
        let bot_id = make_bot_id();
        let config = make_full_config();

        let txn = renderer.render(&bot_id, &config).unwrap();
        assert_eq!(
            txn.writes[0].payload["musicSignUrl"],
            "https://sign.example.com"
        );
    }

    #[test]
    fn snowluma_http_client_timeout_optional() {
        let renderer = SnowLumaConfigRenderer::new("/tmp/snowluma");
        let bot_id = make_bot_id();

        let with_timeout = BotConfig {
            bot: make_basic_config(),
            connect: ConnectConfig {
                http_servers: vec![HttpServerConfig {
                    base: NetworkBaseFields {
                        enable: true,
                        name: "http-s".to_string(),
                        message_post_format: MessagePostFormat::Array,
                        token: "".to_string(),
                        debug: false,
                    },
                    host: "0.0.0.0".to_string(),
                    port: 3000,
                    enable_cors: false,
                    enable_websocket: false,
                    path: "/".to_string(),
                }],
                http_clients: vec![HttpClientConfig {
                    base: NetworkBaseFields {
                        enable: true,
                        name: "http-c".to_string(),
                        message_post_format: MessagePostFormat::Array,
                        token: "t".to_string(),
                        debug: false,
                    },
                    url: "http://example.com".to_string(),
                    report_self_message: false,
                    timeout_ms: Some(3000),
                }],
                ..ConnectConfig::default()
            },
            advanced: make_advanced_config(),
            status_command: None,
        };

        let txn = renderer.render(&bot_id, &with_timeout).unwrap();
        let client = &txn.writes[0].payload["networks"]["httpClients"][0];
        assert_eq!(client["timeoutMs"], 3000);

        let without_timeout = BotConfig {
            bot: make_basic_config(),
            connect: ConnectConfig {
                http_servers: vec![HttpServerConfig {
                    base: NetworkBaseFields {
                        enable: true,
                        name: "http-s".to_string(),
                        message_post_format: MessagePostFormat::Array,
                        token: "".to_string(),
                        debug: false,
                    },
                    host: "0.0.0.0".to_string(),
                    port: 3000,
                    enable_cors: false,
                    enable_websocket: false,
                    path: "/".to_string(),
                }],
                http_clients: vec![HttpClientConfig {
                    base: NetworkBaseFields {
                        enable: true,
                        name: "http-c".to_string(),
                        message_post_format: MessagePostFormat::Array,
                        token: "t".to_string(),
                        debug: false,
                    },
                    url: "http://example.com".to_string(),
                    report_self_message: false,
                    timeout_ms: None,
                }],
                ..ConnectConfig::default()
            },
            advanced: make_advanced_config(),
            status_command: None,
        };

        let txn = renderer.render(&bot_id, &without_timeout).unwrap();
        let client = &txn.writes[0].payload["networks"]["httpClients"][0];
        assert!(client.get("timeoutMs").is_none());
    }

    // ==================== Factory Tests ====================

    #[test]
    fn factory_creates_napcat_renderer() {
        let renderer = create_renderer(BackendType::NapCat, "/tmp/config");
        let bot_id = make_bot_id();
        let paths = renderer.output_paths(&bot_id);
        assert_eq!(paths.len(), 2);
        assert!(paths[0].to_string_lossy().contains("onebot11_"));
    }

    #[test]
    fn factory_creates_snowluma_renderer() {
        let renderer = create_renderer(BackendType::SnowLuma, "/tmp/config");
        let bot_id = make_bot_id();
        let paths = renderer.output_paths(&bot_id);
        assert_eq!(paths.len(), 1);
        assert!(paths[0].to_string_lossy().contains("onebot_"));
    }
}
