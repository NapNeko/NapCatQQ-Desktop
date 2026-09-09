//! Karin 对接：协议 Bot 作 WS 客户端 → `ws://127.0.0.1:<port>/onebot/v11/ws`，
//! token 落在 Karin `.env` 的 `WS_SERVER_AUTH_KEY`（Karin 监听 .env 改动会热生效）。

use ncd_domain::{
    AppConfigWrite, AppFrameworkId, AppFrameworkManifest, AppInstance, BotConfig, BotId,
    MessagePostFormat, NetworkBaseFields, OneBotLinkMode, OneBotLinkPlan, WebsocketClientConfig,
    WsRole, app_link_connection_name,
};
use ncd_traits::{AppFrameworkError, AppIntegration};

use super::manifest::{
    ENV_HTTP_PORT, ENV_WS_SERVER_AUTH_KEY, KARIN_ENV_FILE, KARIN_REVERSE_WS_PATH,
    KARIN_WEBUI_PATH, karin_manifest,
};
use crate::env_file::EnvWrite;

/// NapCat 侧 WS 客户端心跳 / 重连间隔（毫秒），与桌面端新增连接的默认值一致
const HEART_INTERVAL_MS: u32 = 30000;
const RECONNECT_INTERVAL_MS: u32 = 30000;

pub struct KarinIntegration {
    id: AppFrameworkId,
    manifest: AppFrameworkManifest,
}

impl Default for KarinIntegration {
    fn default() -> Self {
        Self::new()
    }
}

impl KarinIntegration {
    pub fn new() -> Self {
        Self {
            id: AppFrameworkId::new(super::manifest::KARIN_FRAMEWORK_ID),
            manifest: karin_manifest(),
        }
    }

    /// 计划里的 loopback URL；跨机由 AppManager 改写成隧道口
    pub fn reverse_ws_url(instance: &AppInstance) -> String {
        format!("ws://127.0.0.1:{}{KARIN_REVERSE_WS_PATH}", instance.port)
    }

    /// 应用端侧要写的 `.env` 键（apply_link 与预览共用）
    pub fn env_writes(instance: &AppInstance, access_token: &str) -> Vec<EnvWrite> {
        vec![
            EnvWrite::new(ENV_HTTP_PORT, instance.port.to_string()),
            EnvWrite::new(ENV_WS_SERVER_AUTH_KEY, access_token),
        ]
    }
}

impl AppIntegration for KarinIntegration {
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
                "对接 token 不能为空（Karin 反向 WS 需要鉴权）".to_string(),
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
        let writes = Self::env_writes(instance, access_token);
        let summary = writes
            .iter()
            .map(|w| {
                if w.key == ENV_WS_SERVER_AUTH_KEY {
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
                path: KARIN_ENV_FILE.to_string(),
                summary,
            }],
            access_token: access_token.to_string(),
        })
    }

    fn webui_url(&self, instance: &AppInstance, public_host: &str) -> Option<String> {
        Some(format!(
            "http://{public_host}:{}{KARIN_WEBUI_PATH}",
            instance.port
        ))
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
            id: AppInstanceId::new("k1"),
            framework_id: AppFrameworkId::new("karin"),
            display_name: "Karin".into(),
            placement: AppPlacement::LocalNative,
            host_id: "local".into(),
            install_dir: "/c/apps/karin/k1".into(),
            port: 7801,
            state: AppInstanceState::Installed,
            link: None,
            installed_version: None,
            last_error: None,
            created_at_ms: 0,
            install_renderer: true,
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
    fn plan_uses_locked_ws_path_and_named_connection() {
        let plan = KarinIntegration::new()
            .plan_link(&instance(), &bot(), "tok-abc")
            .unwrap();
        assert_eq!(plan.mode, OneBotLinkMode::ReverseWs);
        assert_eq!(plan.connection.url, "ws://127.0.0.1:7801/onebot/v11/ws");
        assert_eq!(plan.connection.base.name, "ncd-app:k1");
        assert_eq!(plan.connection.base.token, "tok-abc");
        assert!(plan.connection.base.enable);
        assert_eq!(plan.bot_id.as_str(), "10001");
        assert_eq!(plan.app_side_writes.len(), 1);
        assert_eq!(plan.app_side_writes[0].path, ".env");
        assert!(plan.app_side_writes[0].summary.contains("HTTP_PORT=7801"));
        assert!(plan.app_side_writes[0].summary.contains("WS_SERVER_AUTH_KEY=<token>"));
        assert!(!plan.app_side_writes[0].summary.contains("tok-abc"));
    }

    #[test]
    fn plan_round_trips_json_with_camel_case_connection() {
        let plan = KarinIntegration::new()
            .plan_link(&instance(), &bot(), "t")
            .unwrap();
        let json = serde_json::to_string(&plan).unwrap();
        assert!(json.contains("\"reconnectInterval\""));
        assert!(json.contains("\"mode\":\"reverse_ws\""));
        let back: OneBotLinkPlan = serde_json::from_str(&json).unwrap();
        assert_eq!(back, plan);
    }

    #[test]
    fn empty_token_is_rejected() {
        assert!(matches!(
            KarinIntegration::new().plan_link(&instance(), &bot(), "  "),
            Err(AppFrameworkError::Validation(_))
        ));
    }

    #[test]
    fn webui_uses_web_path() {
        let url = KarinIntegration::new().webui_url(&instance(), "127.0.0.1");
        assert_eq!(url.as_deref(), Some("http://127.0.0.1:7801/web"));
    }
}
