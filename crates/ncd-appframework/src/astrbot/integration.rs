//! AstrBot 对接：协议 Bot 作 WS 客户端 → `ws://127.0.0.1:<port>/ws`。
//!
//! `instance.port` 是认领到的 `ws_reverse_port`。打开 WebUI 时编排层会先把
//! `instance.port` 换成 dashboard 的本机映射口，再调 `webui_url`。

use ncd_domain::{
    AppConfigWrite, AppFrameworkId, AppFrameworkManifest, AppInstance, BotConfig, BotId,
    MessagePostFormat, NetworkBaseFields, OneBotLinkMode, OneBotLinkPlan, WebsocketClientConfig,
    WsRole, app_link_connection_name,
};
use ncd_traits::{AppFrameworkError, AppIntegration};

use super::manifest::{
    ASTRBOT_CMD_CONFIG, ASTRBOT_FRAMEWORK_ID, ASTRBOT_REVERSE_WS_PATH, astrbot_manifest,
};

const HEART_INTERVAL_MS: u32 = 30000;
const RECONNECT_INTERVAL_MS: u32 = 30000;

pub struct AstrBotIntegration {
    id: AppFrameworkId,
    manifest: AppFrameworkManifest,
}

impl Default for AstrBotIntegration {
    fn default() -> Self {
        Self::new()
    }
}

impl AstrBotIntegration {
    pub fn new() -> Self {
        Self {
            id: AppFrameworkId::new(ASTRBOT_FRAMEWORK_ID),
            manifest: astrbot_manifest(),
        }
    }

    pub fn reverse_ws_url(instance: &AppInstance) -> String {
        format!("ws://127.0.0.1:{}{ASTRBOT_REVERSE_WS_PATH}", instance.port)
    }
}

impl AppIntegration for AstrBotIntegration {
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
                "对接 token 不能为空（AstrBot 反向 WS 与协议端对齐）".to_string(),
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
            connection,
            app_side_writes: vec![AppConfigWrite {
                path: ASTRBOT_CMD_CONFIG.to_string(),
                summary: format!(
                    "platform.aiocqhttp ws_reverse_port={} / ws_reverse_token=<token>",
                    instance.port
                ),
            }],
            access_token: access_token.to_string(),
        })
    }

    fn webui_url(&self, instance: &AppInstance, public_host: &str) -> Option<String> {
        Some(format!("http://{public_host}:{}", instance.port))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ncd_domain::{
        AdvancedConfig, AppInstanceId, AppInstanceState, AppPlacement, AutoRestartSchedule,
        BackendType, BotBasicConfig, ConnectConfig, DeploymentType, RuntimeTarget,
    };

    fn instance() -> AppInstance {
        AppInstance {
            id: AppInstanceId::new("a1"),
            framework_id: AppFrameworkId::new("astrbot"),
            display_name: "AstrBot".into(),
            placement: AppPlacement::LocalNative,
            host_id: "local".into(),
            install_dir: "/c/apps/astrbot/a1".into(),
            port: 6199,
            state: AppInstanceState::Installed,
            link: None,
            installed_version: None,
            last_error: None,
            created_at_ms: 0,
            install_renderer: false,
            origin: ncd_domain::AppInstanceOrigin::Created,
        }
    }

    fn bot() -> BotConfig {
        BotConfig {
            bot: BotBasicConfig {
                name: "b".into(),
                qq_id: 10001,
                music_sign_url: String::new(),
                auto_restart_schedule: AutoRestartSchedule::default(),
                offline_auto_restart: false,
                runtime_target: RuntimeTarget::Local,
                backend_type: BackendType::NapCat,
                deployment_type: DeploymentType::default(),
                snowluma_start_mode: None,
                webui_password_takeover: false,
            },
            connect: ConnectConfig::default(),
            advanced: AdvancedConfig::default(),
            status_command: None,
        }
    }

    #[test]
    fn plan_uses_ws_path_not_onebot_v11() {
        let plan = AstrBotIntegration::new()
            .plan_link(&instance(), &bot(), "tok-abc")
            .unwrap();
        assert_eq!(plan.mode, OneBotLinkMode::ReverseWs);
        assert_eq!(plan.connection.url, "ws://127.0.0.1:6199/ws");
        assert_eq!(plan.connection.base.name, "ncd-app:a1");
        assert_eq!(plan.connection.base.token, "tok-abc");
        assert_eq!(plan.app_side_writes[0].path, "data/cmd_config.json");
        assert!(!plan.app_side_writes[0].summary.contains("tok-abc"));
        assert!(!plan.connection.url.contains("onebot/v11"));
        assert!(!plan.connection.url.contains(":6185"));
    }

    #[test]
    fn empty_token_rejected() {
        assert!(matches!(
            AstrBotIntegration::new().plan_link(&instance(), &bot(), ""),
            Err(AppFrameworkError::Validation(_))
        ));
    }

    #[test]
    fn webui_url_uses_http_port_from_caller() {
        let mut inst = instance();
        inst.port = 6185;
        assert_eq!(
            AstrBotIntegration::new()
                .webui_url(&inst, "127.0.0.1")
                .as_deref(),
            Some("http://127.0.0.1:6185")
        );
    }
}
