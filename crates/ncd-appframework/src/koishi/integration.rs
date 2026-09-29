//! Koishi 对接：协议 Bot 作 WS 客户端 → `ws://127.0.0.1:<port>/onebot/ncd`（server 插件的口）。
//! Koishi 侧是 `group:adapter` 里的 `adapter-onebot:ncd-link`，selfId 必须等于 Bot 的 QQ 号：
//! 上游 ws-reverse 只凭请求头 `X-Self-ID` 认 Bot，不看 token。token 仍写进条目（同一把钥匙，
//! 用户改成 http 协议时就用得上），Bot 侧照常带 `Authorization`。

use ncd_domain::{
    AppConfigWrite, AppFrameworkId, AppFrameworkManifest, AppInstance, BotConfig, BotId,
    MessagePostFormat, NetworkBaseFields, OneBotLinkEndpoint, OneBotLinkMode, OneBotLinkPlan,
    WebsocketClientConfig, WsRole, app_link_connection_name,
};
use ncd_traits::{AppFrameworkError, AppIntegration};
use serde_json::{Map, Value};

use super::manifest::{
    KOISHI_FRAMEWORK_ID, KOISHI_REVERSE_WS_PATH, KOISHI_YML, LINK_IDENT, ONEBOT_ADAPTER_NAME,
    koishi_manifest,
};

const HEART_INTERVAL_MS: u32 = 30000;
const RECONNECT_INTERVAL_MS: u32 = 30000;

pub struct KoishiIntegration {
    id: AppFrameworkId,
    manifest: AppFrameworkManifest,
}

impl Default for KoishiIntegration {
    fn default() -> Self {
        Self::new()
    }
}

impl KoishiIntegration {
    pub fn new() -> Self {
        Self {
            id: AppFrameworkId::new(KOISHI_FRAMEWORK_ID),
            manifest: koishi_manifest(),
        }
    }

    /// 计划里的 loopback URL；跨机由编排层改写成隧道口
    pub fn reverse_ws_url(instance: &AppInstance) -> String {
        format!("ws://127.0.0.1:{}{KOISHI_REVERSE_WS_PATH}", instance.port)
    }
}

/// 对接条目的配置（apply_link 与预览共用）
pub fn link_entry_config(self_id: &str, token: &str) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert("selfId".into(), Value::from(self_id));
    if !token.is_empty() {
        m.insert("token".into(), Value::from(token));
    }
    m.insert("protocol".into(), Value::from("ws-reverse"));
    m.insert("path".into(), Value::from(KOISHI_REVERSE_WS_PATH));
    m
}

impl AppIntegration for KoishiIntegration {
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
        if bot.bot.qq_id == 0 {
            return Err(AppFrameworkError::Validation(
                "这个 Bot 还没有 QQ 号：Koishi 要按 QQ 号认 Bot，先登录一次再对接".to_string(),
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
        let qq = bot.bot.qq_id.to_string();
        Ok(OneBotLinkPlan {
            mode: OneBotLinkMode::ReverseWs,
            instance_id: instance.id.clone(),
            bot_id: BotId::new(qq.clone()),
            connection: OneBotLinkEndpoint::WsClient(connection),
            app_side_writes: vec![AppConfigWrite {
                path: KOISHI_YML.to_string(),
                summary: format!(
                    "{ONEBOT_ADAPTER_NAME}:{LINK_IDENT}（selfId={qq}，ws-reverse {KOISHI_REVERSE_WS_PATH}）"
                ),
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
            id: AppInstanceId::new("ko1"),
            framework_id: AppFrameworkId::new("koishi"),
            display_name: "Koishi".into(),
            placement: AppPlacement::LocalNative,
            host_id: "local".into(),
            install_dir: "/c/apps/koishi/ko1".into(),
            port: 5140,
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

    fn bot(qq: u64) -> BotConfig {
        BotConfig {
            bot: BotBasicConfig {
                name: "b".into(),
                qq_id: qq,
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
    fn plan_points_bot_at_dedicated_reverse_path() {
        let plan = KoishiIntegration::new()
            .plan_link(&instance(), &bot(10001), "tok")
            .unwrap();
        assert_eq!(plan.mode, OneBotLinkMode::ReverseWs);
        let c = plan.connection.as_ws_client().expect("Koishi 是反向对接");
        assert_eq!(c.url, "ws://127.0.0.1:5140/onebot/ncd");
        assert_eq!(c.base.name, "ncd-app:ko1");
        assert_eq!(
            c.role,
            WsRole::Universal,
            "上游只接 X-Client-Role: Universal"
        );
        assert_eq!(plan.bot_id.as_str(), "10001");
        assert_eq!(plan.app_side_writes[0].path, "koishi.yml");
        assert!(plan.app_side_writes[0].summary.contains("selfId=10001"));
        assert!(!plan.app_side_writes[0].summary.contains("tok"));
    }

    #[test]
    fn bot_without_qq_cannot_link() {
        assert!(matches!(
            KoishiIntegration::new().plan_link(&instance(), &bot(0), "t"),
            Err(AppFrameworkError::Validation(_))
        ));
    }

    #[test]
    fn link_entry_matches_adapter_schema() {
        let m = link_entry_config("10001", "k");
        assert_eq!(m["selfId"], "10001", "selfId 在上游 schema 里是字符串");
        assert_eq!(m["protocol"], "ws-reverse");
        assert_eq!(m["path"], "/onebot/ncd");
        assert!(!link_entry_config("1", "").contains_key("token"));
    }

    #[test]
    fn webui_is_console_root() {
        let url = KoishiIntegration::new().webui_url(&instance(), "127.0.0.1");
        assert_eq!(url.as_deref(), Some("http://127.0.0.1:5140/"));
    }
}
