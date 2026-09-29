//! 云崽对接：协议 Bot 作 WS 客户端连 `ws://127.0.0.1:<port>/OneBotv11`，带 `Authorization: Bearer <token>`；
//! 应用端这边把 server.yaml 的 `auth` 设成只有这一条（上游要求 auth 里每一条都对上，NapCat 只带这一个），
//! 顺带把 `port` / `url` 对齐实例口。server.yaml 有文件监听，auth 改了几秒后生效，不用重启。

use ncd_domain::{
    AppConfigWrite, AppFrameworkId, AppFrameworkManifest, AppInstance, BotConfig, BotId,
    MessagePostFormat, NetworkBaseFields, OneBotLinkEndpoint, OneBotLinkMode, OneBotLinkPlan,
    WebsocketClientConfig, WsRole, app_link_connection_name,
};
use ncd_traits::{AppFrameworkError, AppIntegration};
use serde_yaml::Value;

use super::config::{auth_value, url_with_port};
use super::manifest::{YUNZAI_FRAMEWORK_ID, YUNZAI_ONEBOT_PATH, yunzai_manifest};
use crate::yaml_patch;

/// 与桌面端新增连接的默认值一致
const HEART_INTERVAL_MS: u32 = 30000;
const RECONNECT_INTERVAL_MS: u32 = 30000;

pub struct YunzaiIntegration {
    id: AppFrameworkId,
    manifest: AppFrameworkManifest,
}

impl Default for YunzaiIntegration {
    fn default() -> Self {
        Self::new()
    }
}

impl YunzaiIntegration {
    pub fn new() -> Self {
        Self {
            id: AppFrameworkId::new(YUNZAI_FRAMEWORK_ID),
            manifest: yunzai_manifest(),
        }
    }

    /// 计划里的 loopback URL；跨机由 AppManager 改写成隧道口
    pub fn reverse_ws_url(instance: &AppInstance) -> String {
        format!("ws://127.0.0.1:{}{YUNZAI_ONEBOT_PATH}", instance.port)
    }
}

/// 对接时 server.yaml 的新内容：端口 = 实例口、文件链接地址跟着换口、auth 只留 Bearer token
pub fn link_server_yaml(text: &str, port: u16, token: &str) -> Result<String, AppFrameworkError> {
    let current: Value = serde_yaml::from_str(text).unwrap_or(Value::Null);
    let old_port = current
        .get("port")
        .and_then(Value::as_u64)
        .and_then(|p| u16::try_from(p).ok())
        .unwrap_or(super::manifest::YUNZAI_DEFAULT_PORT);
    let mut out = yaml_patch::set_value(text, &["port"], &Value::Number(port.into()))
        .map_err(AppFrameworkError::Integration)?;
    let url = current.get("url").and_then(Value::as_str).unwrap_or_default();
    let next_url = if url.is_empty() {
        Some(format!("http://localhost:{port}"))
    } else {
        url_with_port(url, old_port, port)
    };
    if let Some(next_url) = next_url {
        out = yaml_patch::set_value(&out, &["url"], &Value::String(next_url))
            .map_err(AppFrameworkError::Integration)?;
    }
    yaml_patch::set_value(&out, &["auth"], &auth_value(current.get("auth"), token, false))
        .map_err(AppFrameworkError::Integration)
}

impl AppIntegration for YunzaiIntegration {
    fn framework_id(&self) -> &AppFrameworkId {
        &self.id
    }

    fn manifest(&self) -> &AppFrameworkManifest {
        &self.manifest
    }

    fn plan_link(
        &self,
        instance: &AppInstance,
        bot: &BotConfig,
        access_token: &str,
    ) -> Result<OneBotLinkPlan, AppFrameworkError> {
        if access_token.trim().is_empty() {
            return Err(AppFrameworkError::Validation(
                "对接 token 不能为空（云崽监听全部网卡，反向 WS 要鉴权）".to_string(),
            ));
        }
        if instance.port == 0 {
            return Err(AppFrameworkError::Validation(
                "应用实例端口未设置".to_string(),
            ));
        }
        let connection = WebsocketClientConfig {
            base: NetworkBaseFields {
                enable: true,
                name: app_link_connection_name(&instance.id),
                message_post_format: MessagePostFormat::Array,
                token: access_token.to_string(),
                debug: false,
            },
            url: Self::reverse_ws_url(instance),
            report_self_message: false,
            heart_interval: HEART_INTERVAL_MS,
            reconnect_interval: RECONNECT_INTERVAL_MS,
            role: WsRole::Universal,
        };
        Ok(OneBotLinkPlan {
            mode: OneBotLinkMode::ReverseWs,
            instance_id: instance.id.clone(),
            bot_id: BotId::new(bot.bot.qq_id.to_string()),
            connection: OneBotLinkEndpoint::WsClient(connection),
            app_side_writes: vec![AppConfigWrite {
                path: super::config::config_rel("server"),
                summary: format!("port={} / auth.Authorization=Bearer <token>", instance.port),
            }],
            access_token: access_token.to_string(),
        })
    }

    fn webui_url(&self, _instance: &AppInstance, _public_host: &str) -> Option<String> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SERVER: &str = "# 服务器地址\nurl: http://localhost:2536\n# 服务器端口\nport: 2536\n# 服务器缺省跳转地址\nredirect: https://git.trss.me/Yunzai\n# 服务器鉴权\nauth:\n# Authorization: Bearer <access_token>\nhttps:\n";

    #[test]
    fn link_writes_port_url_and_bearer_only() {
        let out = link_server_yaml(SERVER, 24100, "tok").unwrap();
        assert!(out.contains("url: http://localhost:24100\n"));
        assert!(out.contains("port: 24100\n"));
        assert!(out.contains("auth:\n  Authorization: Bearer tok\n# Authorization: Bearer <access_token>\nhttps:\n"));

        let with_extra = out.replace("  Authorization: Bearer tok\n", "  X-Key: a\n  Authorization: Bearer old\n");
        let again = link_server_yaml(&with_extra, 24100, "new").unwrap();
        let v: Value = serde_yaml::from_str(&again).unwrap();
        assert_eq!(v["auth"]["Authorization"], Value::String("Bearer new".into()));
        assert!(v["auth"].get("X-Key").is_none(), "NapCat 带不了别的头，对接时去掉");
    }

    #[test]
    fn link_keeps_custom_url_host() {
        let text = "url: https://bot.example.com\nport: 2536\nauth:\n";
        let out = link_server_yaml(text, 24100, "tok").unwrap();
        assert!(out.starts_with("url: https://bot.example.com\nport: 24100\n"));
    }
}
