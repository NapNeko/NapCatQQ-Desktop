//! MaiBot 对接：方向和另外三个框架相反。协议 Bot 开一个专属 WS 服务端（`ncd-app:<id>`，只听 127.0.0.1），
//! 麦麦的 NapCat 适配器插件作客户端连过来，token 走 `Authorization: Bearer`。
//!
//! 听口要看 Bot 主机上哪些口空着，纯计划拿不到，这里填 0，由编排层分配后回填。
//! `instance.port` 是 WebUI 口，和对接无关。

use ncd_domain::{
    AppConfigWrite, AppFrameworkId, AppFrameworkManifest, AppInstance, BotConfig, BotId,
    MessagePostFormat, NetworkBaseFields, OneBotLinkEndpoint, OneBotLinkMode, OneBotLinkPlan,
    WebsocketServerConfig, WsRole, app_link_connection_name,
};
use ncd_traits::{AppFrameworkError, AppIntegration};

use super::manifest::{ADAPTER_CONFIG, MAIBOT_FRAMEWORK_ID, maibot_manifest};

const HEART_INTERVAL_MS: u32 = 30000;
/// 适配器连的是同机的 Bot，只听回环口，不对外暴露
pub const LINK_LISTEN_HOST: &str = "127.0.0.1";

pub struct MaiBotIntegration {
    id: AppFrameworkId,
    manifest: AppFrameworkManifest,
}

impl Default for MaiBotIntegration {
    fn default() -> Self {
        Self::new()
    }
}

impl MaiBotIntegration {
    pub fn new() -> Self {
        Self {
            id: AppFrameworkId::new(MAIBOT_FRAMEWORK_ID),
            manifest: maibot_manifest(),
        }
    }
}

impl AppIntegration for MaiBotIntegration {
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
                "对接 token 不能为空（Bot 的 WS 服务要鉴权）".to_string(),
            ));
        }
        let server = WebsocketServerConfig {
            base: NetworkBaseFields {
                enable: true,
                name: app_link_connection_name(&instance.id),
                message_post_format: MessagePostFormat::Array,
                token: access_token.to_string(),
                debug: false,
            },
            host: LINK_LISTEN_HOST.to_string(),
            port: 0,
            report_self_message: false,
            enable_force_push_event: true,
            heart_interval: HEART_INTERVAL_MS,
            path: "/".to_string(),
            role: WsRole::Universal,
        };
        Ok(OneBotLinkPlan {
            mode: OneBotLinkMode::ForwardWs,
            instance_id: instance.id.clone(),
            bot_id: BotId::new(bot.bot.qq_id.to_string()),
            connection: OneBotLinkEndpoint::WsServer(server),
            app_side_writes: vec![AppConfigWrite {
                path: ADAPTER_CONFIG.to_string(),
                summary: "启用适配器 / napcat_server 指向这条连接（host、port、token=<token>）"
                    .to_string(),
            }],
            access_token: access_token.to_string(),
        })
    }

    fn webui_url(&self, instance: &AppInstance, public_host: &str) -> Option<String> {
        Some(format!("http://{public_host}:{}/", instance.port))
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
            id: AppInstanceId::new("m1"),
            framework_id: AppFrameworkId::new("maibot"),
            display_name: "麦麦".into(),
            placement: AppPlacement::LocalNative,
            host_id: "local".into(),
            install_dir: "/apps/maibot/m1".into(),
            port: 23001,
            state: AppInstanceState::Installed,
            link: None,
            installed_version: None,
            last_error: None,
            created_at_ms: 1,
            install_renderer: false,
            origin: ncd_domain::AppInstanceOrigin::Created,
            auto_start: true,
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
    fn plan_is_forward_with_loopback_server_and_port_left_for_orchestrator() {
        let plan = MaiBotIntegration::new()
            .plan_link(&instance(), &bot(), "tok-abc")
            .unwrap();
        assert_eq!(plan.mode, OneBotLinkMode::ForwardWs);
        assert_eq!(plan.connection.mode(), OneBotLinkMode::ForwardWs);
        let s = plan.connection.as_ws_server().expect("正向对接是服务端条目");
        assert_eq!(s.base.name, "ncd-app:m1");
        assert_eq!(s.base.token, "tok-abc");
        assert_eq!(s.host, "127.0.0.1");
        assert_eq!(s.port, 0, "听口由编排层回填");
        assert_eq!(plan.app_side_writes[0].path, "plugins/MaiBot-Napcat-Adapter/config.toml");
        assert!(!plan.app_side_writes[0].summary.contains("tok-abc"));
    }

    #[test]
    fn empty_token_is_rejected_and_webui_is_root() {
        assert!(matches!(
            MaiBotIntegration::new().plan_link(&instance(), &bot(), " "),
            Err(AppFrameworkError::Validation(_))
        ));
        assert_eq!(
            MaiBotIntegration::new().webui_url(&instance(), "127.0.0.1").as_deref(),
            Some("http://127.0.0.1:23001/")
        );
    }
}
