//! App-level configuration types backing `AppConfig`.
//!
//! 当前承载两类 App 级设置:
//! - `WebUiPollerSettings`:控制 NapCat WebUI 登录态轮询间隔与离线通知开关。
//! - `SnowLumaAppConfig`:承载 SnowLuma daemon 的密码 override 与 WebUI 监听端口。
//!
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
/// 单独抽成自由函数是为了给 `#[serde(default = "...")]` 复用，
/// 同时锁死与 legacy SnowLuma daemon 已使用端口一致。
pub fn default_snowluma_port() -> u16 {
    5099
}

/// SnowLuma 后端运行时的 App 级配置。
/// 与 `WebUiPollerSettings` 同级承载在同一份 `app_config.rs` 中，不新建第二份。
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

/// `AppSettings.performance_monitor_interval_ms` 默认值（毫秒）。
/// 对齐 legacy `Performance.MonitorInterval` 默认 1200。
pub fn default_perf_monitor_interval() -> u64 {
    1200
}

/// 与前端 `performanceSettings` 一致：500–10000 ms。
pub const PERF_MONITOR_INTERVAL_MIN_MS: u64 = 500;
pub const PERF_MONITOR_INTERVAL_MAX_MS: u64 = 10_000;

pub fn clamp_perf_monitor_interval_ms(raw: u64) -> u64 {
    raw.clamp(PERF_MONITOR_INTERVAL_MIN_MS, PERF_MONITOR_INTERVAL_MAX_MS)
}

fn default_true() -> bool {
    true
}

fn default_close_action() -> String {
    "close".to_string()
}

fn default_ui_theme() -> String {
    "auto".to_string()
}

fn default_ui_motion_level() -> String {
    "standard".to_string()
}

fn default_ui_radius_style() -> String {
    "standard".to_string()
}

fn default_ui_motion_speed() -> f64 {
    0.5
}

fn default_infobar_dismiss_info_ms() -> u64 {
    5000
}

fn default_infobar_dismiss_success_ms() -> u64 {
    4000
}

fn default_infobar_dismiss_warning_ms() -> u64 {
    6000
}

fn default_task_queue_cleanup_enabled() -> bool {
    true
}

fn default_task_queue_cleanup_linger_ms() -> u64 {
    600_000
}

/// InfoBar 非 danger 自动关闭时长上限（毫秒）。0 = 不自动关。
pub const INFOBAR_DISMISS_MS_MAX: u64 = 60_000;

pub fn clamp_infobar_dismiss_ms(raw: u64) -> u64 {
    if raw == 0 {
        return 0;
    }
    raw.clamp(1000, INFOBAR_DISMISS_MS_MAX)
}

/// 任务队列终态条目保留时长下限（毫秒）。0 = 关闭自动清理。
pub const TASK_QUEUE_CLEANUP_LINGER_MIN_MS: u64 = 3_000;
/// 任务队列终态条目保留时长上限（毫秒）。
pub const TASK_QUEUE_CLEANUP_LINGER_MAX_MS: u64 = 3_600_000;

pub fn clamp_task_queue_cleanup_linger_ms(raw: u64) -> u64 {
    if raw == 0 {
        return 0;
    }
    raw.clamp(TASK_QUEUE_CLEANUP_LINGER_MIN_MS, TASK_QUEUE_CLEANUP_LINGER_MAX_MS)
}

/// 设置页「外观」Tab 的客户端偏好，与前端 `preferencesStore` / `SettingsDraft` 对齐。
/// 落盘在 app-settings.json 的 `uiPreferences` 字段，避免仅依赖 WebView localStorage。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AppUiPreferences {
    #[serde(rename = "theme", default = "default_ui_theme")]
    pub theme: String,
    #[serde(rename = "showMascot", default = "default_true")]
    pub show_mascot: bool,
    #[serde(rename = "motionEnabled", default = "default_true")]
    pub motion_enabled: bool,
    #[serde(rename = "motionLevel", default = "default_ui_motion_level")]
    pub motion_level: String,
    #[serde(rename = "motionSpeed", default = "default_ui_motion_speed")]
    pub motion_speed: f64,
    #[serde(rename = "radiusStyle", default = "default_ui_radius_style")]
    pub radius_style: String,
    /// 顶部 InfoBar：`info`  tone 自动关闭毫秒；0 = 不自动关。
    #[serde(
        rename = "infoBarDismissInfoMs",
        default = "default_infobar_dismiss_info_ms"
    )]
    pub info_bar_dismiss_info_ms: u64,
    /// `success` tone 自动关闭毫秒；0 = 不自动关。
    #[serde(
        rename = "infoBarDismissSuccessMs",
        default = "default_infobar_dismiss_success_ms"
    )]
    pub info_bar_dismiss_success_ms: u64,
    /// `warning` tone 自动关闭毫秒；`danger` 始终不自动关（前端强制）。
    #[serde(
        rename = "infoBarDismissWarningMs",
        default = "default_infobar_dismiss_warning_ms"
    )]
    pub info_bar_dismiss_warning_ms: u64,
}

impl Default for AppUiPreferences {
    fn default() -> Self {
        Self {
            theme: default_ui_theme(),
            show_mascot: true,
            motion_enabled: true,
            motion_level: default_ui_motion_level(),
            motion_speed: default_ui_motion_speed(),
            radius_style: default_ui_radius_style(),
            info_bar_dismiss_info_ms: default_infobar_dismiss_info_ms(),
            info_bar_dismiss_success_ms: default_infobar_dismiss_success_ms(),
            info_bar_dismiss_warning_ms: default_infobar_dismiss_warning_ms(),
        }
    }
}

/// 设置页可读写的 App 级聚合配置。
///
/// 与按子系统拆开的 `WebUiPollerSettings` / `SnowLumaAppConfig` 不同，本结构
/// 是设置页一次性读写的"非敏感偏好集合"，序列化到
/// `<data_root>/runtime/config/app-settings.json`。GitHub PAT 这类敏感凭证
/// 不在此结构内，由 SecretStore（keyring）单独承载，避免明文落盘。
///
/// `poller` 直接复用 `WebUiPollerSettings`：其中
/// `bot_login_check_interval_ms` 是后端登录轮询真正消费的字段，启动时由
/// `set_app_settings` 写回的值会在下次 Poller 创建时生效。两个离线通知开关
/// 当前后端为 noop 实现，设置页不暴露，保留字段仅为 round-trip 兼容。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AppSettings {
    /// WebUI 登录轮询设置（含 Bot 登录检查间隔）。
    #[serde(default)]
    pub poller: WebUiPollerSettings,
    /// 主页性能监控开关。对齐 legacy `Performance.MonitorEnabled`。
    #[serde(rename = "performanceMonitorEnabled", default = "default_true")]
    pub performance_monitor_enabled: bool,
    /// 主页性能监控采样间隔（毫秒）。对齐 legacy `Performance.MonitorInterval`。
    #[serde(
        rename = "performanceMonitorInterval",
        default = "default_perf_monitor_interval"
    )]
    pub performance_monitor_interval_ms: u64,
    /// 任务队列是否在终态后自动从列表移除。
    #[serde(rename = "taskQueueCleanupEnabled", default = "default_task_queue_cleanup_enabled")]
    pub task_queue_cleanup_enabled: bool,
    /// 终态后保留时长（毫秒）；`task_queue_cleanup_enabled == false` 时落盘为 0。
    #[serde(
        rename = "taskQueueCleanupLingerMs",
        default = "default_task_queue_cleanup_linger_ms"
    )]
    pub task_queue_cleanup_linger_ms: u64,
    /// 主窗口关闭按钮行为：`close` 退出程序，`tray` 隐藏到托盘。与前端 `preferencesStore.closeAction` 对齐。
    #[serde(rename = "closeAction", default = "default_close_action")]
    pub close_action: String,
    /// 外观 / 动画 / 圆角等 UI 偏好（与 localStorage 双写，启动以磁盘为准）。
    #[serde(rename = "uiPreferences", default)]
    pub ui_preferences: AppUiPreferences,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            poller: WebUiPollerSettings::default(),
            performance_monitor_enabled: true,
            performance_monitor_interval_ms: default_perf_monitor_interval(),
            task_queue_cleanup_enabled: default_task_queue_cleanup_enabled(),
            task_queue_cleanup_linger_ms: default_task_queue_cleanup_linger_ms(),
            close_action: default_close_action(),
            ui_preferences: AppUiPreferences::default(),
        }
    }
}

impl AppSettings {
    /// 写入前规范化性能监控采样间隔，避免异常配置拖垮 IPC 采样。
    pub fn normalize_performance_monitor(&mut self) {
        self.performance_monitor_interval_ms =
            clamp_perf_monitor_interval_ms(self.performance_monitor_interval_ms);
    }

    /// 写入前规范化任务队列自动清理偏好。
    pub fn normalize_task_queue_cleanup(&mut self) {
        if !self.task_queue_cleanup_enabled {
            self.task_queue_cleanup_linger_ms = 0;
        } else {
            self.task_queue_cleanup_linger_ms =
                clamp_task_queue_cleanup_linger_ms(self.task_queue_cleanup_linger_ms);
            if self.task_queue_cleanup_linger_ms == 0 {
                self.task_queue_cleanup_linger_ms = default_task_queue_cleanup_linger_ms();
            }
        }
    }
}

/// 设置页一次性读写的 App 设置 DTO（IPC 边界类型）。
///
/// `settings` 是非敏感偏好（落 app-settings.json）；`github_pat` 是敏感凭证
/// （走 SecretStore，不与 settings 同文件）。command 层负责把这两半拆开落到
/// 各自存储，前端只面对这一个聚合形状。空 `github_pat` 表示未设置 / 清除。
///
/// 定义在 domain（与 `AppSettings` 同 crate、同 ts-rs export 路径），让派生的
/// TypeScript import 相对路径正确；放 tauri 层会因跨 crate export_to 深度不同
/// 拼出畸形相对路径。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AppSettingsDto {
    pub settings: AppSettings,
    /// GitHub Personal Access Token。读取时回填，写入时空串视为清除。
    #[serde(rename = "githubPat", default)]
    pub github_pat: String,
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

    // ---------------------------------------------------------------------
    // AppSettings
    // ---------------------------------------------------------------------

    /// 空对象反序列化必须还原成 Default：保证旧用户没有 app-settings.json 时
    /// 读到的就是一组合理默认值。
    #[test]
    fn app_settings_empty_object_falls_back_to_defaults() {
        let parsed: AppSettings =
            serde_json::from_str("{}").expect("空对象应能反序列化为默认值");
        assert_eq!(parsed, AppSettings::default());
        assert!(parsed.performance_monitor_enabled);
        assert_eq!(parsed.performance_monitor_interval_ms, 1200);
        assert_eq!(parsed.poller.bot_login_check_interval_ms, default_login_interval());
    }

    /// 非默认值整组写入的 round-trip：嵌套的 poller 用 legacy 驼峰字段名，
    /// 顶层性能字段用各自 rename，序列化回来语义不丢。
    #[test]
    fn app_settings_round_trips_non_default_values() {
        let cfg = AppSettings {
            poller: WebUiPollerSettings {
                bot_login_check_interval_ms: 2500,
                offline_webhook_notice: true,
                offline_email_notice: false,
            },
            performance_monitor_enabled: false,
            performance_monitor_interval_ms: 3000,
            task_queue_cleanup_enabled: default_task_queue_cleanup_enabled(),
            task_queue_cleanup_linger_ms: default_task_queue_cleanup_linger_ms(),
            close_action: default_close_action(),
            ui_preferences: AppUiPreferences::default(),
        };
        let json = serde_json::to_string(&cfg).expect("serialize 不应失败");
        let back: AppSettings = serde_json::from_str(&json).expect("反序列化失败");
        assert_eq!(back, cfg);
    }

    /// 缺失顶层性能字段时各自走 per-field default，poller 缺失走 Default。
    #[test]
    fn app_settings_partial_fields_use_defaults() {
        let parsed: AppSettings =
            serde_json::from_str(r#"{"performanceMonitorInterval":5000}"#)
                .expect("仅给一个字段也应反序列化");
        assert_eq!(parsed.performance_monitor_interval_ms, 5000);
        assert!(parsed.performance_monitor_enabled);
        assert_eq!(parsed.poller, WebUiPollerSettings::default());
    }
}
