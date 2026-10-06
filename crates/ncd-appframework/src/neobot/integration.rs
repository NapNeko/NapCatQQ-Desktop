//! NeoBot 对接：应用端开反向 WS 服务端，协议 Bot 作客户端连过来
//! （`ws://127.0.0.1:<onebot_port>/`），token 落在 `app/data/config.toml` 的
//! `[adapter].reverse_ws_access_token`。
//!
//! NeoBot 的反向 WS 服务端**不按路径分发**：鉴权只认握手里的
//! `Authorization: Bearer <token>` 头或 `?access_token=` 查询参数
//! （`neobot_adapter/onebot/receiver/core.py:26-49`），所以这里给的路径只是给人看的，
//! 换任意路径都能连上。OneBot 11 惯例还是用 `/`。
//!
//! 配置改动后 NeoBot 会热重载监听设置（`adapter_supervisor` 会 stop → reconfigure → start），
//! 所以对接后不必强制重启实例。

use ncd_domain::{
    AppConfigWrite, AppFrameworkId, AppFrameworkManifest, AppInstance, BotConfig, BotId,
    MessagePostFormat, NetworkBaseFields, OneBotLinkEndpoint, OneBotLinkMode, OneBotLinkPlan,
    WebsocketClientConfig, WsRole, app_link_connection_name,
};
use ncd_traits::{AppFrameworkError, AppIntegration};

use super::manifest::{NEOBOT_CONFIG_TOML, NEOBOT_FRAMEWORK_ID, neobot_manifest};

const HEART_INTERVAL_MS: u32 = 30000;
const RECONNECT_INTERVAL_MS: u32 = 30000;

pub struct NeoBotIntegration {
    id: AppFrameworkId,
    manifest: AppFrameworkManifest,
}

impl Default for NeoBotIntegration {
    fn default() -> Self {
        Self::new()
    }
}

impl NeoBotIntegration {
    pub fn new() -> Self {
        Self {
            id: AppFrameworkId::new(NEOBOT_FRAMEWORK_ID),
            manifest: neobot_manifest(),
        }
    }

    /// 应用端监听地址：协议 Bot 作客户端连这里。与 NoneBot2 / AstrBot 同口径，
    /// 同机走回环；跨机由编排层开隧道。
    pub fn reverse_ws_url(instance: &AppInstance) -> String {
        format!("ws://127.0.0.1:{}/", instance.port)
    }
}

impl AppIntegration for NeoBotIntegration {
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
        if instance.port == 0 {
            return Err(AppFrameworkError::Validation(
                "应用实例端口未设置".to_string(),
            ));
        }
        // 反向 WS 时协议 Bot 是客户端，token 它自己带；留空则 NeoBot 不校验，
        // 但同网段任何人都能注入伪造事件，所以照 NoneBot2 要求非空。
        if access_token.trim().is_empty() {
            return Err(AppFrameworkError::Validation(
                "对接 token 不能为空（NeoBot 反向 WS 建议配置 access token 鉴权）".to_string(),
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
        let summary = format!("{NEOBOT_CONFIG_TOML} [adapter] reverse_ws_port={} / reverse_ws_access_token=<token>", instance.port);
        Ok(OneBotLinkPlan {
            mode: OneBotLinkMode::ReverseWs,
            instance_id: instance.id.clone(),
            bot_id: BotId::new(bot.bot.qq_id.to_string()),
            connection: OneBotLinkEndpoint::WsClient(connection),
            app_side_writes: vec![AppConfigWrite {
                path: NEOBOT_CONFIG_TOML.to_string(),
                summary,
            }],
            access_token: access_token.to_string(),
        })
    }

    fn webui_url(&self, instance: &AppInstance, public_host: &str) -> Option<String> {
        // 约定：调用方已经把「桌面端侧回环口（远端是 -L 隧道口）」写进 instance.port，
        // 这里只管拼 URL，不要去读配置里的真实面板口。
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
            id: AppInstanceId::new("n1"),
            framework_id: AppFrameworkId::new("neobot"),
            display_name: "NeoBot".into(),
            placement: AppPlacement::LocalNative,
            host_id: "local".into(),
            install_dir: "/c/apps/neobot/n1".into(),
            port: 8080,
            state: AppInstanceState::Installed,
            link: None,
            installed_version: None,
            last_error: None,
            created_at_ms: 0,
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
    fn plan_uses_reverse_ws_and_locked_config_path() {
        let plan = NeoBotIntegration::new()
            .plan_link(&instance(), &bot(), "tok-abc")
            .unwrap();
        assert_eq!(plan.mode, OneBotLinkMode::ReverseWs);
        let c = plan.connection.as_ws_client().expect("NeoBot 是反向对接");
        assert_eq!(c.url, "ws://127.0.0.1:8080/");
        assert_eq!(c.base.name, "ncd-app:n1");
        assert_eq!(c.base.token, "tok-abc");
        assert_eq!(plan.app_side_writes.len(), 1);
        assert_eq!(plan.app_side_writes[0].path, "app/data/config.toml");
        assert!(plan.app_side_writes[0].summary.contains("reverse_ws_port=8080"));
        assert!(
            plan.app_side_writes[0]
                .summary
                .contains("reverse_ws_access_token=<token>")
        );
        assert!(
            !plan.app_side_writes[0].summary.contains("tok-abc"),
            "预览摘要不能泄 token"
        );
    }

    #[test]
    fn empty_token_and_zero_port_are_rejected() {
        let integ = NeoBotIntegration::new();
        assert!(matches!(
            integ.plan_link(&instance(), &bot(), ""),
            Err(AppFrameworkError::Validation(_))
        ));
        let mut zero = instance();
        zero.port = 0;
        assert!(matches!(
            integ.plan_link(&zero, &bot(), "tok"),
            Err(AppFrameworkError::Validation(_))
        ));
    }

    #[test]
    fn webui_url_takes_port_from_caller() {
        // 调用方传进来的 instance.port 已经是桌面端侧回环口（远端是隧道口）
        let mut inst = instance();
        inst.port = 45678;
        assert_eq!(
            NeoBotIntegration::new().webui_url(&inst, "127.0.0.1"),
            Some("http://127.0.0.1:45678".to_string())
        );
    }
}
