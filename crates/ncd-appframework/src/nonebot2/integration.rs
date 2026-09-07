//! NoneBot2 对接：协议 Bot 作 WS 客户端 → `ws://127.0.0.1:<port>/onebot/v11/ws`，
//! token 落在 `.env.prod` 的 `ONEBOT_ACCESS_TOKEN`（NoneBot2 不热重载 .env，对接后需重启实例）。

use ncd_domain::{
    AppConfigWrite, AppFrameworkId, AppFrameworkManifest, AppInstance, BotConfig, BotId,
    MessagePostFormat, NetworkBaseFields, OneBotLinkMode, OneBotLinkPlan, WebsocketClientConfig,
    WsRole, app_link_connection_name,
};
use ncd_traits::{AppFrameworkError, AppIntegration};

use super::manifest::{
    ENV_ONEBOT_ACCESS_TOKEN, ENV_PORT, NONEBOT2_ENV_PROD_FILE, NONEBOT2_FRAMEWORK_ID,
    NONEBOT2_REVERSE_WS_PATH, nonebot2_manifest,
};
use crate::env_file::EnvWrite;

const HEART_INTERVAL_MS: u32 = 30000;
const RECONNECT_INTERVAL_MS: u32 = 30000;

pub struct NoneBot2Integration {
    id: AppFrameworkId,
    manifest: AppFrameworkManifest,
}

impl Default for NoneBot2Integration {
    fn default() -> Self {
        Self::new()
    }
}

impl NoneBot2Integration {
    pub fn new() -> Self {
        Self {
            id: AppFrameworkId::new(NONEBOT2_FRAMEWORK_ID),
            manifest: nonebot2_manifest(),
        }
    }

    pub fn reverse_ws_url(instance: &AppInstance) -> String {
        format!("ws://127.0.0.1:{}{NONEBOT2_REVERSE_WS_PATH}", instance.port)
    }

    /// `.env.prod` 要写的键（apply_link 与预览共用）
    pub fn env_writes(instance: &AppInstance, access_token: &str) -> Vec<EnvWrite> {
        vec![
            EnvWrite::new(ENV_PORT, instance.port.to_string()),
            EnvWrite::new(ENV_ONEBOT_ACCESS_TOKEN, access_token),
        ]
    }
}

impl AppIntegration for NoneBot2Integration {
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
                "对接 token 不能为空（NoneBot2 反向 WS 需要鉴权）".to_string(),
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
        let summary = Self::env_writes(instance, access_token)
            .iter()
            .map(|w| {
                if w.key == ENV_ONEBOT_ACCESS_TOKEN {
                    format!("{}=<token>", w.key)
                } else {
                    format!("{}={}", w.key, w.value)
                }
            })
            .collect::<Vec<_>>()
            .join(" / ");
        Ok(OneBotLinkPlan {
            mode: OneBotLinkMode::ReverseWs,
            instance_id: instance.id.clone(),
            bot_id: BotId::new(bot.bot.qq_id.to_string()),
            connection,
            app_side_writes: vec![AppConfigWrite {
                path: NONEBOT2_ENV_PROD_FILE.to_string(),
                summary,
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
    use ncd_domain::{
        AdvancedConfig, AppInstanceId, AppInstanceState, AppPlacement, AutoRestartSchedule,
        BackendType, BotBasicConfig, ConnectConfig, DeploymentType, RuntimeTarget,
    };

    fn instance() -> AppInstance {
        AppInstance {
            id: AppInstanceId::new("n1"),
            framework_id: AppFrameworkId::new("nonebot2"),
            display_name: "NoneBot2".into(),
            placement: AppPlacement::LocalNative,
            host_id: "local".into(),
            install_dir: "/c/apps/nonebot2/n1".into(),
            port: 8081,
            state: AppInstanceState::Installed,
            link: None,
            installed_version: None,
            last_error: None,
            created_at_ms: 0,
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
    fn plan_uses_locked_ws_path_and_env_prod() {
        let plan = NoneBot2Integration::new()
            .plan_link(&instance(), &bot(), "tok-abc")
            .unwrap();
        assert_eq!(plan.mode, OneBotLinkMode::ReverseWs);
        assert_eq!(plan.connection.url, "ws://127.0.0.1:8081/onebot/v11/ws");
        assert_eq!(plan.connection.base.name, "ncd-app:n1");
        assert_eq!(plan.connection.base.token, "tok-abc");
        assert_eq!(plan.app_side_writes.len(), 1);
        assert_eq!(plan.app_side_writes[0].path, ".env.prod");
        assert!(plan.app_side_writes[0].summary.contains("PORT=8081"));
        assert!(plan.app_side_writes[0].summary.contains("ONEBOT_ACCESS_TOKEN=<token>"));
        assert!(!plan.app_side_writes[0].summary.contains("tok-abc"));
    }

    #[test]
    fn no_webui_and_empty_token_rejected() {
        let integ = NoneBot2Integration::new();
        assert_eq!(integ.webui_url(&instance(), "127.0.0.1"), None);
        assert!(matches!(
            integ.plan_link(&instance(), &bot(), ""),
            Err(AppFrameworkError::Validation(_))
        ));
    }
}
