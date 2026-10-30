// 设置页使用的持久化设置模型与客户端偏好转换；IPC 读写由 settings.service 负责。

import type { TaskQueueCleanupPrefs } from '../task-queue/cleanup';
import type { AppUiPreferences } from '../../ipc/generated/domain/AppUiPreferences';
import type { GithubApiMode } from '../../ipc/generated/domain/GithubApiMode';
import type { McpServerSettings } from '../../ipc/generated/domain/McpServerSettings';
import type { ReloginSettings } from '../../ipc/generated/domain/ReloginSettings';
import type { SnowLumaLinuxPackage } from '../../ipc/generated/domain/SnowLumaLinuxPackage';
import type { AppPreferences, CloseAction } from './appPreferences';
import type { FeatureToggles } from './features';
import type { NetworkSettings } from './network';
import type { WebhookChannelDraft } from './offline-notify-defaults';
import { appUiPreferencesToAppPreferences } from './ui-preferences-bridge';

export type AfterCloseUiBehavior = 'hide' | 'delayed_lightweight' | 'immediate_lightweight';
export type UiModeOnStartup = 'normal' | 'tray_only';

/** 设置页消费的扁平后端设置形状（全 number，无 bigint / 嵌套）。 */
export interface BackendSettings {
    botLoginCheckIntervalMs: number;
    botOfflineWebHookNotice: boolean;
    botOfflineEmailNotice: boolean;
    performanceMonitorEnabled: boolean;
    performanceMonitorIntervalMs: number;
    /** Bot 实例运行时指标（内存/网络节点） */
    botRuntimeMetricsEnabled: boolean;
    botRuntimeMetricsIntervalMs: number;
    botRuntimeMetricsRetentionDays: number;
    taskQueueCleanup: TaskQueueCleanupPrefs;
    githubApiMode: GithubApiMode;
    githubPat: string;
    network: NetworkSettings;
    proxyPassword: string;
    networkError?: string;
    closeAction: CloseAction;
    afterCloseUiBehavior: AfterCloseUiBehavior;
    enterLightweightDelaySecs: number;
    uiModeOnStartup: UiModeOnStartup;
    /** 登录 Windows 后自动启动本程序 */
    launchOnStartup: boolean;
    minimizeToTrayCountsAsHidden: boolean;
    notifyOnOffline: boolean;
    notifyOnBotCrashed: boolean;
    notifyOnLoginKicked: boolean;
    uiPreferences: AppUiPreferences;
    /** 可选功能模块开关（设置 · 功能） */
    features: FeatureToggles;
    /** 本机 MCP 服务与权限 */
    mcp: McpServerSettings;
    /** 后台远端主机健康探测 */
    remoteHostHealthProbeEnabled: boolean;
    remoteHostHealthProbeIntervalMs: number;
    /** 离线 Webhook：多通道 + 兼容镜像到扁平字段 */
    webHookChannels: WebhookChannelDraft[];
    webHookUrl: string;
    webHookSecret: string;
    webHookJson: string;
    webHookMethod: string;
    /** 离线邮件通道 */
    emailSender: string;
    emailReceiver: string;
    emailToken: string;
    emailSmtpServer: string;
    emailSmtpPort: number;
    emailEncryption: string;
    /** OneBot 告警 */
    onebotNoticeEnabled: boolean;
    /** 兼容镜像: 第一个发送方 */
    onebotMessengerBotId: string;
    /** 多个本机发送方, 投递时按顺序取第一个可用 */
    onebotMessengerBotIds: string[];
    onebotTargetType: string;
    /** 兼容镜像: 第一个目标 */
    onebotTargetId: number;
    /** 多个私聊/群目标, 全部投递 */
    onebotTargetIds: number[];
    onebotMessageTemplate: string;
    onebotRelogin: ReloginSettings;
    /** 上线恢复通知(默认关) */
    notifyOnRecovered: boolean;
    /** offline 边沿防抖秒数;0=关 */
    offlineDebounceSeconds: number;
    /** 内存投递历史容量;0=不记 */
    offlineDeliveryHistoryLimit: number;
    /** SnowLuma Node.js 自定义/指定运行环境路径 */
    snowlumaNodePath: string | null;
    snowlumaPackage: SnowLumaLinuxPackage | null;
}

/** 由 BackendSettings 派生的客户端偏好（与 preferencesStore 一致）。 */
export function clientPrefsFromBackend(backend: BackendSettings): AppPreferences {
    return appUiPreferencesToAppPreferences(backend.uiPreferences, backend.closeAction);
}
