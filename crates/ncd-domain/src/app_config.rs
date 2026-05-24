//! App-level configuration types backing `AppConfig`.
//! 当前承载两类 App 级设置：
//! - `WebUiPollerSettings`（来自 `napcat-webui-login` Spec）：
//! 控制 NapCat WebUI 登录态轮询间隔与离线通知开关。
//! - `SnowLumaAppConfig`（来自 `snowluma-backend-runtime` Spec）：
//! 承载 SnowLuma daemon 的密码 override 与 WebUI 监听端口。
//! 所有字段通过 ts-rs 派生导出到 `src-ui/core/ipc/generated/` 以避免前后端漂移。

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// `WebUiPollerSettings.bot_login_check_interval_ms` 的默认值（毫秒）。
/// 单独抽成自由函数是为了给 `#[serde(default = "...")]` 复用
/// 同时方便其它模块（例如 `napcat_login_poller::PollerConfig`）在
/// 拼装默认配置时直接读取同一个常量来源。
pub fn default_login_interval() -> u64 {
    5000
}

/// 控制 NapCat WebUI 登录态轮询行为的 App 级设置。
/// 所有字段通过 `#[serde(rename = "...")]` 严格对齐 legacy JSON 字段名
/// （Pydantic schema：`botLoginCheckInterval` / `botOfflineWebHookNotice`
/// / `botOfflineEmailNotice`），并通过 `ts-rs` 派生 TypeScript 类型
/// 杜绝前后端契约漂移。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct WebUiPollerSettings {
    /// 已登录状态下的轮询间隔（毫秒）。未登录时由 Poller 内部强制使用 1000ms。
    #[serde(rename = "botLoginCheckInterval", default = "default_login_interval")]
    pub bot_login_check_interval_ms: u64,
    /// 离线 webhook 通知开关（全局）。`false` 时即使 BotConfig 启用 `offlineNotice`
    /// 也不会触发 webhook 推送。
    #[serde(rename = "botOfflineWebHookNotice", default)]
    pub offline_webhook_notice: bool,
    /// 离线邮件通知开关（全局）。语义同上。
    #[serde(rename = "botOfflineEmailNotice", default)]
    pub offline_email_notice: bool,
}

impl Default for WebUiPollerSettings {
    fn default() -> Self {
        Self {
            bot_login_check_interval_ms: default_login_interval(),
            offline_webhook_notice: false,
            offline_email_notice: false,
        }
    }
}

/// `SnowLumaAppConfig.webui_port` 的默认值（5099）。
/// 单独抽成自由函数是为了给 `#[serde(default = "...")]` 复用
/// 同时锁死本 spec 与 legacy SnowLuma daemon 已使用端口一致。
pub fn default_snowluma_port() -> u16 {
    5099
}

/// SnowLuma 后端运行时的 App 级配置。
/// 与 `WebUiPollerSettings` 同级承载在同一份 `app_config.rs` 中，**不**新建第二份。
/// 字段：
/// - `webui_password_override`：App 级密码 override，最高优先级密码来源。
/// 空字符串视作未设置；具体优先级解析在 `snowluma::session::render_daemon_globals` 内执行。
/// - `webui_port`：SnowLuma daemon WebUI 监听端口，默认 5099。
/// 字段名通过 `#[serde(rename = "...")]` 严格对齐前端 / legacy JSON 的驼峰命名
/// （`snowlumaWebuiPasswordOverride` / `snowlumaWebuiPort`），并通过 `ts-rs`
/// 派生 TypeScript 类型，杜绝前后端契约漂移。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct SnowLumaAppConfig {
    /// App 级 SnowLuma WebUI 密码 override。空字符串视为未设置。
    #[serde(default, rename = "snowlumaWebuiPasswordOverride")]
    pub webui_password_override: String,
    /// SnowLuma daemon WebUI 监听端口（默认 5099）。
    #[serde(default = "default_snowluma_port", rename = "snowlumaWebuiPort")]
    pub webui_port: u16,
}

impl Default for SnowLumaAppConfig {
    fn default() -> Self {
        Self {
            webui_password_override: String::new(),
            webui_port: default_snowluma_port(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Legacy NapCat 桌面端 PySide6 版本写出的规范 JSON 字面量。
    /// 三个字段顺序、命名（驼峰大小写）必须保持稳定，否则前后端契约漂移。
    const LEGACY_CANONICAL_JSON: &str = r#"{"botLoginCheckInterval":5000,"botOfflineWebHookNotice":false,"botOfflineEmailNotice":false}"#;

    /// 字节级 round-trip：legacy JSON → `WebUiPollerSettings` → JSON 应当字节相等。
    /// 同时锁死三个 `#[serde(rename = ...)]` 字段名，避免契约漂移。
    #[test]
    fn round_trip_matches_legacy_canonical_json() {
        let parsed: WebUiPollerSettings =
            serde_json::from_str(LEGACY_CANONICAL_JSON).expect("legacy JSON 应可被反序列化");

        // 语义层断言三个字段都还原成默认配置（5000 / false / false）。
        assert_eq!(parsed.bot_login_check_interval_ms, 5000);
        assert!(!parsed.offline_webhook_notice);
        assert!(!parsed.offline_email_notice);

        // 字节级断言：序列化输出与 legacy JSON 完全一致（含字段顺序）。
        let serialized = serde_json::to_string(&parsed).expect("serialize 不应失败");
        assert_eq!(
            serialized.as_bytes(),
            LEGACY_CANONICAL_JSON.as_bytes(),
            "serialize 输出与 legacy JSON 字节不一致：实际 = {serialized}"
        );
    }

    /// 缺字段时三个 `#[serde(default ...)]` 必须把 struct 还原到 `Default` 等价值。
    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let parsed: WebUiPollerSettings =
            serde_json::from_str("{}").expect("空对象应能反序列化为默认值");

        assert_eq!(parsed, WebUiPollerSettings::default());
        assert_eq!(parsed.bot_login_check_interval_ms, default_login_interval());
        assert!(!parsed.offline_webhook_notice);
        assert!(!parsed.offline_email_notice);
    }

    /// 部分字段缺失：`bot_login_check_interval_ms` 缺失走 `default_login_interval()`
    /// 其它字段以输入为准。覆盖 `#[serde(default)]` 与 `#[serde(default = "...")]` 两种形态。
    #[test]
    fn partial_fields_use_per_field_defaults() {
        let parsed: WebUiPollerSettings = serde_json::from_str(
            r#"{"botOfflineWebHookNotice":true,"botOfflineEmailNotice":true}"#,
        )
        .expect("缺失 botLoginCheckInterval 仍应反序列化");

        assert_eq!(parsed.bot_login_check_interval_ms, default_login_interval());
        assert!(parsed.offline_webhook_notice);
        assert!(parsed.offline_email_notice);
    }

    /// 非默认值整组写入也应当完成字节级 round-trip
    /// 防止未来有人把字段顺序改了。
    #[test]
    fn round_trip_preserves_non_default_values() {
        let canonical = r#"{"botLoginCheckInterval":2500,"botOfflineWebHookNotice":true,"botOfflineEmailNotice":true}"#;
        let parsed: WebUiPollerSettings = serde_json::from_str(canonical).expect("反序列化失败");
        assert_eq!(parsed.bot_login_check_interval_ms, 2500);
        assert!(parsed.offline_webhook_notice);
        assert!(parsed.offline_email_notice);

        let serialized = serde_json::to_string(&parsed).expect("serialize 不应失败");
        assert_eq!(serialized.as_bytes(), canonical.as_bytes());
    }

    // ---------------------------------------------------------------------
    // SnowLumaAppConfig
    // ---------------------------------------------------------------------

    /// 默认实例的规范 JSON 字面量：空字符串 override + 端口 5099。
    /// 字段顺序 = struct 声明顺序（password_override 在前）。
    const SNOWLUMA_DEFAULT_CANONICAL_JSON: &str =
        r#"{"snowlumaWebuiPasswordOverride":"","snowlumaWebuiPort":5099}"#;

    /// `Default` 实例的语义与字节级一致性。
    #[test]
    fn snowluma_default_matches_canonical_json() {
        let cfg = SnowLumaAppConfig::default();
        assert_eq!(cfg.webui_password_override, "");
        assert_eq!(cfg.webui_port, default_snowluma_port());
        assert_eq!(cfg.webui_port, 5099);

        let serialized = serde_json::to_string(&cfg).expect("serialize 不应失败");
        assert_eq!(
            serialized.as_bytes(),
            SNOWLUMA_DEFAULT_CANONICAL_JSON.as_bytes(),
            "serialize 输出与默认 JSON 字节不一致：实际 = {serialized}"
        );

        let parsed: SnowLumaAppConfig = serde_json::from_str(SNOWLUMA_DEFAULT_CANONICAL_JSON)
            .expect("默认 JSON 应可被反序列化");
        assert_eq!(parsed, SnowLumaAppConfig::default());
    }

    /// 空对象走 `#[serde(default ...)]` 路径，应当还原成 `Default` 等价值
    /// 同时锁死字段名 `snowlumaWebuiPasswordOverride` / `snowlumaWebuiPort`。
    #[test]
    fn snowluma_missing_fields_fall_back_to_defaults() {
        let parsed: SnowLumaAppConfig =
            serde_json::from_str("{}").expect("空对象应能反序列化为默认值");

        assert_eq!(parsed, SnowLumaAppConfig::default());
        assert_eq!(parsed.webui_password_override, "");
        assert_eq!(parsed.webui_port, default_snowluma_port());
    }

    /// 自定义 password override + 端口走默认。覆盖
    /// `#[serde(default)]` 单独缺失 + 字符串字段非空两种条件。
    #[test]
    fn snowluma_custom_password_override_with_default_port_round_trips() {
        let canonical =
            r#"{"snowlumaWebuiPasswordOverride":"hunter2!@#","snowlumaWebuiPort":5099}"#;
        let parsed: SnowLumaAppConfig = serde_json::from_str(canonical).expect("反序列化失败");
        assert_eq!(parsed.webui_password_override, "hunter2!@#");
        assert_eq!(parsed.webui_port, 5099);
        assert_eq!(parsed.webui_port, default_snowluma_port());

        let serialized = serde_json::to_string(&parsed).expect("serialize 不应失败");
        assert_eq!(
            serialized.as_bytes(),
            canonical.as_bytes(),
            "serialize 输出与 canonical JSON 字节不一致：实际 = {serialized}"
        );

        // 仅给 password override，端口走 default = "default_snowluma_port"。
        let partial: SnowLumaAppConfig =
            serde_json::from_str(r#"{"snowlumaWebuiPasswordOverride":"hunter2!@#"}"#)
                .expect("缺失 snowlumaWebuiPort 仍应反序列化");
        assert_eq!(partial.webui_password_override, "hunter2!@#");
        assert_eq!(partial.webui_port, default_snowluma_port());
    }

    /// 默认 password override + 自定义端口（6000）。覆盖
    /// `#[serde(default = "...")]` 单独缺失 + 端口字段被显式设置两种条件。
    #[test]
    fn snowluma_default_password_override_with_custom_port_round_trips() {
        let canonical = r#"{"snowlumaWebuiPasswordOverride":"","snowlumaWebuiPort":6000}"#;
        let parsed: SnowLumaAppConfig = serde_json::from_str(canonical).expect("反序列化失败");
        assert_eq!(parsed.webui_password_override, "");
        assert_eq!(parsed.webui_port, 6000);

        let serialized = serde_json::to_string(&parsed).expect("serialize 不应失败");
        assert_eq!(
            serialized.as_bytes(),
            canonical.as_bytes(),
            "serialize 输出与 canonical JSON 字节不一致：实际 = {serialized}"
        );

        // 仅给端口，password override 走 `#[serde(default)]`（空字符串）。
        let partial: SnowLumaAppConfig = serde_json::from_str(r#"{"snowlumaWebuiPort":6000}"#)
            .expect("缺失 snowlumaWebuiPasswordOverride 仍应反序列化");
        assert_eq!(partial.webui_password_override, "");
        assert_eq!(partial.webui_port, 6000);
    }
}
