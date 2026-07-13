//! App 级配置类型
//!
//! - WebUiPollerSettings: NapCat WebUI 登录态轮询
//! - SnowLumaAppConfig: SnowLuma daemon 密码与端口
//! - AppSettings: 设置页聚合配置

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::macros::default_true;
use crate::offline_alert::{
    OfflineEmailSettings, OfflineNotifyBehavior, OfflineOneBotSettings, OfflineWebhookSettings,
};

string_enum! {
    /// 主窗口关闭按钮行为
    ///
    /// 未知值兜底为 Unknown(String), 保证旧配置文件里非标准值不会导致反序列化失败
    #[derive(Debug, Clone, PartialEq, Eq, Default, TS)]
    #[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
    pub enum CloseAction {
        #[default]
        Close => "close",
        Tray => "tray",
    }
}

string_enum! {
    /// 关窗行为
    #[derive(Debug, Clone, PartialEq, Eq, Default, TS)]
    #[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
    pub enum AfterCloseUiBehavior {
        Hide => "hide",
        #[default]
        DelayedLightweight => "delayed_lightweight",
        ImmediateLightweight => "immediate_lightweight",
    }
}

string_enum! {
    /// 启动时 UI 模式
    #[derive(Debug, Clone, PartialEq, Eq, Default, TS)]
    #[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
    pub enum UiModeOnStartup {
        #[default]
        Normal => "normal",
        TrayOnly => "tray_only",
    }
}

/// WebUiPollerSettings.bot_login_check_interval_ms 默认值(毫秒)
pub fn default_login_interval() -> u64 {
    5000
}

/// NapCat WebUI 登录态轮询设置
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct WebUiPollerSettings {
    /// 登录态轮询间隔(毫秒),未登录时强制 1000ms
    #[serde(rename = "botLoginCheckInterval", default = "default_login_interval")]
    pub bot_login_check_interval_ms: u64,
    /// 离线 webhook 通知开关
    #[serde(rename = "botOfflineWebHookNotice", default)]
    pub offline_webhook_notice: bool,
    /// 离线邮件通知开关
    #[serde(rename = "botOfflineEmailNotice", default)]
    pub offline_email_notice: bool,
    /// recovered / 防抖 / 历史容量
    #[serde(rename = "offlineNotifyBehavior", default)]
    pub offline_notify_behavior: OfflineNotifyBehavior,
}

impl Default for WebUiPollerSettings {
    fn default() -> Self {
        Self {
            bot_login_check_interval_ms: default_login_interval(),
            offline_webhook_notice: false,
            offline_email_notice: false,
            offline_notify_behavior: OfflineNotifyBehavior::default(),
        }
    }
}

/// SnowLumaAppConfig.webui_port 默认值
pub fn default_snowluma_port() -> u16 {
    5099
}

/// SnowLuma daemon 配置
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct SnowLumaAppConfig {
    /// App 级 WebUI 密码覆盖,空字符串视为未设置
    #[serde(default, rename = "snowlumaWebuiPasswordOverride")]
    pub webui_password_override: String,
    /// SnowLuma daemon WebUI 端口
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

/// AppSettings.performance_monitor_interval_ms 默认值
pub fn default_perf_monitor_interval() -> u64 {
    1200
}

fn default_bot_runtime_metrics_enabled() -> bool {
    false
}

fn default_bot_runtime_metrics_interval_field() -> u64 {
    crate::bot_runtime_metrics::default_bot_runtime_metrics_interval_ms()
}

fn default_bot_runtime_metrics_retention_field() -> u32 {
    crate::bot_runtime_metrics::default_bot_runtime_metrics_retention_days()
}

/// 与前端 performanceSettings 一致:500–10000 ms
pub const PERF_MONITOR_INTERVAL_MIN_MS: u64 = 500;
pub const PERF_MONITOR_INTERVAL_MAX_MS: u64 = 10_000;

pub fn clamp_perf_monitor_interval_ms(raw: u64) -> u64 {
    raw.clamp(PERF_MONITOR_INTERVAL_MIN_MS, PERF_MONITOR_INTERVAL_MAX_MS)
}

/// 远程主机健康探活默认间隔(30s)
pub fn default_remote_host_health_probe_interval_ms() -> u64 {
    30_000
}

/// 远程主机健康探活间隔范围:10s ~ 5min
pub const REMOTE_HOST_HEALTH_PROBE_INTERVAL_MIN_MS: u64 = 10_000;
pub const REMOTE_HOST_HEALTH_PROBE_INTERVAL_MAX_MS: u64 = 300_000;

pub fn clamp_remote_host_health_probe_interval_ms(raw: u64) -> u64 {
    raw.clamp(
        REMOTE_HOST_HEALTH_PROBE_INTERVAL_MIN_MS,
        REMOTE_HOST_HEALTH_PROBE_INTERVAL_MAX_MS,
    )
}

fn default_enter_lightweight_delay_secs() -> u32 {
    300
}

/// 延迟轻量模式延迟范围:60s ~ 1800s,0 视为立即进入
pub const LIGHTWEIGHT_DELAY_MIN_SECS: u32 = 60;
pub const LIGHTWEIGHT_DELAY_MAX_SECS: u32 = 1800;

pub fn clamp_lightweight_delay_secs(raw: u32) -> u32 {
    if raw == 0 {
        return 0;
    }
    raw.clamp(LIGHTWEIGHT_DELAY_MIN_SECS, LIGHTWEIGHT_DELAY_MAX_SECS)
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

/// InfoBar 非 danger 自动关闭时长上限(毫秒),0 = 不自动关
pub const INFOBAR_DISMISS_MS_MAX: u64 = 60_000;

pub fn clamp_infobar_dismiss_ms(raw: u64) -> u64 {
    if raw == 0 {
        return 0;
    }
    raw.clamp(1000, INFOBAR_DISMISS_MS_MAX)
}

/// 任务队列终态条目保留时长下限(毫秒),0 = 关闭自动清理
pub const TASK_QUEUE_CLEANUP_LINGER_MIN_MS: u64 = 3_000;
/// 任务队列终态条目保留时长上限(毫秒)
pub const TASK_QUEUE_CLEANUP_LINGER_MAX_MS: u64 = 3_600_000;

pub fn clamp_task_queue_cleanup_linger_ms(raw: u64) -> u64 {
    if raw == 0 {
        return 0;
    }
    raw.clamp(
        TASK_QUEUE_CLEANUP_LINGER_MIN_MS,
        TASK_QUEUE_CLEANUP_LINGER_MAX_MS,
    )
}

/// 外观偏好(与 WebView localStorage 同步)
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
    /// InfoBar info tone 自动关闭毫秒,0 = 不自动关
    #[serde(
        rename = "infoBarDismissInfoMs",
        default = "default_infobar_dismiss_info_ms"
    )]
    pub info_bar_dismiss_info_ms: u64,
    /// InfoBar success tone 自动关闭毫秒
    #[serde(
        rename = "infoBarDismissSuccessMs",
        default = "default_infobar_dismiss_success_ms"
    )]
    pub info_bar_dismiss_success_ms: u64,
    /// InfoBar warning tone 自动关闭毫秒,danger 始终不自动关
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

/// 设置页 App 级配置聚合
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AppSettings {
    /// WebUI 登录轮询设置
    #[serde(default)]
    pub poller: WebUiPollerSettings,
    /// 主页性能监控开关
    #[serde(rename = "performanceMonitorEnabled", default = "default_true")]
    pub performance_monitor_enabled: bool,
    /// 主页性能监控采样间隔(毫秒)
    #[serde(
        rename = "performanceMonitorInterval",
        default = "default_perf_monitor_interval"
    )]
    pub performance_monitor_interval_ms: u64,

    /// Bot 实例运行时指标（内存/网络节点）开关；默认关；启动注入下次生效
    #[serde(
        rename = "botRuntimeMetricsEnabled",
        default = "default_bot_runtime_metrics_enabled"
    )]
    pub bot_runtime_metrics_enabled: bool,
    /// 实例指标采样间隔(毫秒)
    #[serde(
        rename = "botRuntimeMetricsIntervalMs",
        default = "default_bot_runtime_metrics_interval_field"
    )]
    pub bot_runtime_metrics_interval_ms: u64,
    /// 实例指标历史保留天数(默认 7)
    #[serde(
        rename = "botRuntimeMetricsRetentionDays",
        default = "default_bot_runtime_metrics_retention_field"
    )]
    pub bot_runtime_metrics_retention_days: u32,

    /// 远程主机健康探活开关
    #[serde(rename = "remoteHostHealthProbeEnabled", default = "default_true")]
    pub remote_host_health_probe_enabled: bool,

    /// 远程主机健康探活间隔(毫秒)
    #[serde(
        rename = "remoteHostHealthProbeIntervalMs",
        default = "default_remote_host_health_probe_interval_ms"
    )]
    pub remote_host_health_probe_interval_ms: u64,

    /// 任务队列终态自动清理开关
    #[serde(
        rename = "taskQueueCleanupEnabled",
        default = "default_task_queue_cleanup_enabled"
    )]
    pub task_queue_cleanup_enabled: bool,
    /// 终态后保留时长(毫秒)
    #[serde(
        rename = "taskQueueCleanupLingerMs",
        default = "default_task_queue_cleanup_linger_ms"
    )]
    pub task_queue_cleanup_linger_ms: u64,
    /// 主窗口关闭按钮行为: close 退出程序, tray 隐藏到托盘
    #[serde(rename = "closeAction", default)]
    pub close_action: CloseAction,
    /// 关窗行为: hide / delayed_lightweight / immediate_lightweight
    #[serde(rename = "afterCloseUiBehavior", default)]
    pub after_close_ui_behavior: AfterCloseUiBehavior,
    #[serde(
        rename = "enterLightweightDelaySecs",
        default = "default_enter_lightweight_delay_secs"
    )]
    pub enter_lightweight_delay_secs: u32,
    #[serde(rename = "uiModeOnStartup", default)]
    pub ui_mode_on_startup: UiModeOnStartup,
    /// 登录当前 Windows 用户后自动启动本程序(HKCU Run,无需管理员)
    #[serde(rename = "launchOnStartup", default)]
    pub launch_on_startup: bool,
    #[serde(rename = "minimizeToTrayCountsAsHidden", default = "default_true")]
    pub minimize_to_tray_counts_as_hidden: bool,
    /// 桌面 Toast:NapCat 登录态离线
    #[serde(rename = "notifyOnOffline", default = "default_true")]
    pub notify_on_offline: bool,
    /// 桌面 Toast:Bot 进程异常退出
    #[serde(rename = "notifyOnBotCrashed", default = "default_true")]
    pub notify_on_bot_crashed: bool,
    /// 桌面 Toast:QQ 被踢下线
    #[serde(rename = "notifyOnLoginKicked", default = "default_true")]
    pub notify_on_login_kicked: bool,
    /// 离线 Webhook 通道参数(开关在 poller.offline_webhook_notice)
    #[serde(rename = "WebHook", default)]
    pub offline_webhook: OfflineWebhookSettings,
    /// 离线邮件通道参数(开关在 poller.offline_email_notice)
    #[serde(rename = "Email", default)]
    pub offline_email: OfflineEmailSettings,
    /// 用其它 Bot 的 OneBot HTTP 发告警
    #[serde(rename = "onebotOfflineNotice", default)]
    pub offline_onebot: OfflineOneBotSettings,
    /// 外观偏好
    #[serde(rename = "uiPreferences", default)]
    pub ui_preferences: AppUiPreferences,
}

/// 桌面通知开关集合
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct DesktopNotifySettings {
    #[serde(rename = "notifyOnOffline", default = "default_true")]
    pub notify_on_offline: bool,
    #[serde(rename = "notifyOnBotCrashed", default = "default_true")]
    pub notify_on_bot_crashed: bool,
    #[serde(rename = "notifyOnLoginKicked", default = "default_true")]
    pub notify_on_login_kicked: bool,
}

impl Default for DesktopNotifySettings {
    fn default() -> Self {
        Self {
            notify_on_offline: true,
            notify_on_bot_crashed: true,
            notify_on_login_kicked: true,
        }
    }
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            poller: WebUiPollerSettings::default(),
            performance_monitor_enabled: true,
            performance_monitor_interval_ms: default_perf_monitor_interval(),
            bot_runtime_metrics_enabled: false,
            bot_runtime_metrics_interval_ms: default_bot_runtime_metrics_interval_field(),
            bot_runtime_metrics_retention_days: default_bot_runtime_metrics_retention_field(),
            remote_host_health_probe_enabled: true,
            remote_host_health_probe_interval_ms: default_remote_host_health_probe_interval_ms(),
            task_queue_cleanup_enabled: default_task_queue_cleanup_enabled(),
            offline_webhook: OfflineWebhookSettings::default(),
            offline_email: OfflineEmailSettings::default(),
            offline_onebot: OfflineOneBotSettings::default(),
            task_queue_cleanup_linger_ms: default_task_queue_cleanup_linger_ms(),
            close_action: CloseAction::default(),
            after_close_ui_behavior: AfterCloseUiBehavior::default(),
            enter_lightweight_delay_secs: default_enter_lightweight_delay_secs(),
            ui_mode_on_startup: UiModeOnStartup::default(),
            launch_on_startup: false,
            minimize_to_tray_counts_as_hidden: true,
            notify_on_offline: true,
            notify_on_bot_crashed: true,
            notify_on_login_kicked: true,
            ui_preferences: AppUiPreferences::default(),
        }
    }
}

impl AppSettings {
    /// 规范化性能监控采样间隔
    pub fn normalize_performance_monitor(&mut self) {
        self.performance_monitor_interval_ms =
            clamp_perf_monitor_interval_ms(self.performance_monitor_interval_ms);
    }

    /// 规范化 Bot 运行时指标间隔与保留天数
    pub fn normalize_bot_runtime_metrics(&mut self) {
        self.bot_runtime_metrics_interval_ms =
            crate::bot_runtime_metrics::clamp_bot_runtime_metrics_interval_ms(
                self.bot_runtime_metrics_interval_ms,
            );
        self.bot_runtime_metrics_retention_days =
            crate::bot_runtime_metrics::clamp_bot_runtime_metrics_retention_days(
                self.bot_runtime_metrics_retention_days,
            );
    }

    pub fn desktop_notify_flags(&self) -> DesktopNotifySettings {
        DesktopNotifySettings {
            notify_on_offline: self.notify_on_offline,
            notify_on_bot_crashed: self.notify_on_bot_crashed,
            notify_on_login_kicked: self.notify_on_login_kicked,
        }
    }

    pub fn normalize_lightweight_prefs(&mut self) {
        if matches!(
            self.after_close_ui_behavior,
            AfterCloseUiBehavior::Unknown(_)
        ) {
            self.after_close_ui_behavior = AfterCloseUiBehavior::default();
        }
        if self.after_close_ui_behavior == AfterCloseUiBehavior::DelayedLightweight {
            self.enter_lightweight_delay_secs =
                clamp_lightweight_delay_secs(self.enter_lightweight_delay_secs);
            if self.enter_lightweight_delay_secs == 0 {
                self.enter_lightweight_delay_secs = default_enter_lightweight_delay_secs();
            }
        }
        if matches!(self.ui_mode_on_startup, UiModeOnStartup::Unknown(_)) {
            self.ui_mode_on_startup = UiModeOnStartup::default();
        }
    }

    /// 规范化任务队列自动清理偏好
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

    /// 规范化远程主机健康探活间隔
    pub fn normalize_remote_host_health_probe(&mut self) {
        self.remote_host_health_probe_interval_ms =
            clamp_remote_host_health_probe_interval_ms(self.remote_host_health_probe_interval_ms);
    }
}

/// 设置页 IPC 边界 DTO
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/domain/")]
pub struct AppSettingsDto {
    pub settings: AppSettings,
    /// GitHub PAT,空串视为清除
    #[serde(rename = "githubPat", default)]
    pub github_pat: String,
}

// SystemResourceSnapshot 原在 system_resource.rs, 合并到 app_config:
// 它是概览页性能监控的 IPC 边界 DTO, 属于 AppSettings 的下游消费方

/// 当前时刻的全局 CPU,内存占用百分比(0–100)
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../src-ui/core/ipc/generated/")]
pub struct SystemResourceSnapshot {
    #[serde(rename = "cpuPercent")]
    pub cpu_percent: f64,
    #[serde(rename = "ramPercent")]
    pub ram_percent: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Legacy PySide6 版本规范 JSON
    const LEGACY_CANONICAL_JSON: &str = r#"{"botLoginCheckInterval":5000,"botOfflineWebHookNotice":false,"botOfflineEmailNotice":false}"#;

    /// 字节级 round-trip
    #[test]
    fn round_trip_matches_legacy_canonical_json() {
        let parsed: WebUiPollerSettings = serde_json::from_str(LEGACY_CANONICAL_JSON).unwrap();

        assert_eq!(parsed.bot_login_check_interval_ms, 5000);
        assert!(!parsed.offline_webhook_notice);
        assert!(!parsed.offline_email_notice);

        // 新增 offlineNotifyBehavior 后不再要求与旧三字段 JSON 字节级全等
        assert_eq!(parsed.bot_login_check_interval_ms, 5000);
        assert!(!parsed.offline_webhook_notice);
        assert!(!parsed.offline_email_notice);
        assert_eq!(
            parsed.offline_notify_behavior,
            OfflineNotifyBehavior::default()
        );
    }

    /// 缺字段时回落到默认值
    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let parsed: WebUiPollerSettings = serde_json::from_str("{}").unwrap();

        assert_eq!(parsed, WebUiPollerSettings::default());
        assert_eq!(parsed.bot_login_check_interval_ms, default_login_interval());
        assert!(!parsed.offline_webhook_notice);
        assert!(!parsed.offline_email_notice);
    }

    /// 部分字段缺失时使用 per-field 默认值
    #[test]
    fn partial_fields_use_per_field_defaults() {
        let parsed: WebUiPollerSettings = serde_json::from_str(
            r#"{"botOfflineWebHookNotice":true,"botOfflineEmailNotice":true}"#,
        )
        .unwrap();

        assert_eq!(parsed.bot_login_check_interval_ms, default_login_interval());
        assert!(parsed.offline_webhook_notice);
        assert!(parsed.offline_email_notice);
    }

    /// 非默认值字节级 round-trip
    #[test]
    fn round_trip_preserves_non_default_values() {
        let canonical = r#"{"botLoginCheckInterval":2500,"botOfflineWebHookNotice":true,"botOfflineEmailNotice":true}"#;
        let parsed: WebUiPollerSettings = serde_json::from_str(canonical).unwrap();
        assert_eq!(parsed.bot_login_check_interval_ms, 2500);
        assert!(parsed.offline_webhook_notice);
        assert!(parsed.offline_email_notice);

        assert_eq!(parsed.bot_login_check_interval_ms, 2500);
        assert!(parsed.offline_webhook_notice);
        assert!(parsed.offline_email_notice);
    }

    /// SnowLuma 默认 JSON 字面量
    const SNOWLUMA_DEFAULT_CANONICAL_JSON: &str =
        r#"{"snowlumaWebuiPasswordOverride":"","snowlumaWebuiPort":5099}"#;

    /// SnowLuma 默认值字节稳定
    #[test]
    fn snowluma_default_matches_canonical_json() {
        let cfg = SnowLumaAppConfig::default();
        assert_eq!(cfg.webui_password_override, "");
        assert_eq!(cfg.webui_port, default_snowluma_port());
        assert_eq!(cfg.webui_port, 5099);

        let serialized = serde_json::to_string(&cfg).unwrap();
        assert_eq!(
            serialized.as_bytes(),
            SNOWLUMA_DEFAULT_CANONICAL_JSON.as_bytes(),
            "实际 = {serialized}"
        );

        let parsed: SnowLumaAppConfig =
            serde_json::from_str(SNOWLUMA_DEFAULT_CANONICAL_JSON).unwrap();
        assert_eq!(parsed, SnowLumaAppConfig::default());
    }

    /// SnowLuma 空对象回落到默认值
    #[test]
    fn snowluma_missing_fields_fall_back_to_defaults() {
        let parsed: SnowLumaAppConfig = serde_json::from_str("{}").unwrap();

        assert_eq!(parsed, SnowLumaAppConfig::default());
        assert_eq!(parsed.webui_password_override, "");
        assert_eq!(parsed.webui_port, default_snowluma_port());
    }

    /// 自定义 password override + 端口走默认覆盖
    /// #[serde(default)] 单独缺失 + 字符串字段非空两种条件
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

        // 仅给 password override,端口走 default = "default_snowluma_port"
        let partial: SnowLumaAppConfig =
            serde_json::from_str(r#"{"snowlumaWebuiPasswordOverride":"hunter2!@#"}"#)
                .expect("缺失 snowlumaWebuiPort 仍应反序列化");
        assert_eq!(partial.webui_password_override, "hunter2!@#");
        assert_eq!(partial.webui_port, default_snowluma_port());
    }

    /// 默认 password override + 自定义端口(6000)覆盖
    /// #[serde(default = "...")] 单独缺失 + 端口字段被显式设置两种条件
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

        // 仅给端口,password override 走 #[serde(default)](空字符串)
        let partial: SnowLumaAppConfig = serde_json::from_str(r#"{"snowlumaWebuiPort":6000}"#)
            .expect("缺失 snowlumaWebuiPasswordOverride 仍应反序列化");
        assert_eq!(partial.webui_password_override, "");
        assert_eq!(partial.webui_port, 6000);
    }

    // ---------------------------------------------------------------------
    // AppSettings
    // ---------------------------------------------------------------------

    /// 空对象反序列化必须还原成 Default:保证旧用户没有 app-settings.json 时
    /// 读到的就是一组合理默认值
    #[test]
    fn app_settings_empty_object_falls_back_to_defaults() {
        let parsed: AppSettings = serde_json::from_str("{}").expect("空对象应能反序列化为默认值");
        assert_eq!(parsed, AppSettings::default());
        assert!(parsed.performance_monitor_enabled);
        assert_eq!(parsed.performance_monitor_interval_ms, 1200);
        assert_eq!(
            parsed.poller.bot_login_check_interval_ms,
            default_login_interval()
        );
    }

    /// 非默认值整组写入的 round-trip:嵌套的 poller 用 legacy 驼峰字段名,
    /// 顶层性能字段用各自 rename,序列化回来语义不丢
    #[test]
    fn app_settings_round_trips_non_default_values() {
        let cfg = AppSettings {
            poller: WebUiPollerSettings {
                bot_login_check_interval_ms: 2500,
                offline_webhook_notice: true,
                offline_email_notice: false,
                offline_notify_behavior: OfflineNotifyBehavior::default(),
            },
            performance_monitor_enabled: false,
            performance_monitor_interval_ms: 3000,
            ..AppSettings::default()
        };
        let json = serde_json::to_string(&cfg).expect("serialize 不应失败");
        let back: AppSettings = serde_json::from_str(&json).expect("反序列化失败");
        assert_eq!(back, cfg);
    }

    /// 缺失顶层性能字段时各自走 per-field default,poller 缺失走 Default
    #[test]
    fn app_settings_partial_fields_use_defaults() {
        let parsed: AppSettings = serde_json::from_str(r#"{"performanceMonitorInterval":5000}"#)
            .expect("仅给一个字段也应反序列化");
        assert_eq!(parsed.performance_monitor_interval_ms, 5000);
        assert!(parsed.performance_monitor_enabled);
        assert_eq!(parsed.poller, WebUiPollerSettings::default());
    }
}
