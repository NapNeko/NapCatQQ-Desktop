//! 从协议 Bot 配置投影 OneBot HTTP 出口（应用端对接输入）。

use ncd_domain::{BackendType, BotConfig, BotId, OneBotEndpointExport};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum OneBotExportError {
    #[error("no enabled OneBot HTTP server on protocol bot")]
    NoHttpServer,
    #[error("http server port is zero")]
    InvalidPort,
}

/// 从协议 Bot 的 `connect.httpServers` 取第一个 enable 且 port>0 的服务。
/// host 空 / `0.0.0.0` 规范为 `127.0.0.1`（与 notify 本机 messenger 规则一致）。
pub fn export_onebot_endpoint(config: &BotConfig) -> Result<OneBotEndpointExport, OneBotExportError> {
    let server = config
        .connect
        .http_servers
        .iter()
        .find(|s| s.base.enable && s.port > 0)
        .ok_or(OneBotExportError::NoHttpServer)?;
    if server.port == 0 {
        return Err(OneBotExportError::InvalidPort);
    }
    let host = if server.host.trim().is_empty() || server.host == "0.0.0.0" {
        "127.0.0.1"
    } else {
        server.host.trim()
    };
    let protocol_backend = match config.bot.backend_type {
        BackendType::NapCat => "napcat",
        BackendType::SnowLuma => "snowluma",
    };
    Ok(OneBotEndpointExport {
        bot_id: BotId::new(config.bot.qq_id.to_string()),
        protocol_backend: protocol_backend.into(),
        base_url: format!("http://{host}:{}", server.port),
        access_token: server.base.token.clone(),
        host: host.to_string(),
        port: server.port,
        runtime_target: config.bot.runtime_target.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ncd_domain::{
        AdvancedConfig, AutoRestartSchedule, BotBasicConfig, ConnectConfig, DeploymentType,
        HttpServerConfig, NetworkBaseFields,
    };
    use ncd_domain::kinds::RuntimeTarget;

    fn sample_config(host: &str, port: u16, enable: bool) -> BotConfig {
        BotConfig {
            bot: BotBasicConfig {
                name: "t".into(),
                qq_id: 10001,
                music_sign_url: String::new(),
                auto_restart_schedule: AutoRestartSchedule::default(),
                offline_auto_restart: false,
                runtime_target: RuntimeTarget::Local,
                backend_type: BackendType::NapCat,
                deployment_type: DeploymentType::Native,
                snowluma_start_mode: None,
            },
            connect: ConnectConfig {
                http_servers: vec![HttpServerConfig {
                    base: NetworkBaseFields {
                        enable,
                        name: "http".into(),
                        message_post_format: Default::default(),
                        token: "secret".into(),
                        debug: false,
                    },
                    host: host.into(),
                    port,
                    enable_cors: false,
                    enable_websocket: false,
                    path: "/".into(),
                }],
                ..Default::default()
            },
            advanced: AdvancedConfig::default(),
            status_command: None,
        }
    }

    #[test]
    fn exports_first_enabled_http_server() {
        let exp = export_onebot_endpoint(&sample_config("127.0.0.1", 3000, true)).unwrap();
        assert_eq!(exp.base_url, "http://127.0.0.1:3000");
        assert_eq!(exp.access_token, "secret");
        assert_eq!(exp.protocol_backend, "napcat");
        assert_eq!(exp.bot_id.as_str(), "10001");
    }

    #[test]
    fn normalizes_wildcard_host_to_loopback() {
        let exp = export_onebot_endpoint(&sample_config("0.0.0.0", 3100, true)).unwrap();
        assert_eq!(exp.host, "127.0.0.1");
        assert_eq!(exp.base_url, "http://127.0.0.1:3100");
    }

    #[test]
    fn rejects_when_no_enabled_server() {
        let err = export_onebot_endpoint(&sample_config("127.0.0.1", 3000, false)).unwrap_err();
        assert_eq!(err, OneBotExportError::NoHttpServer);
    }
}
