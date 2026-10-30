// App 设置（后端持久化部分）IPC 服务。
// 本域的命令名只在此文件出现，后端改命令名只改这一处。
//
// 外观 / 动画 / 圆角等写入 app-settings.json 的 uiPreferences，与 localStorage 双写；
// 启动时以磁盘为准（见 useAppUiPreferencesBootstrap）。

import { clampPerformanceMonitorIntervalMs } from '../domain/performance/performanceSettings';
import {
    taskQueueCleanupFromAppSettings,
    taskQueueCleanupToStoredFields,
} from '../domain/task-queue/cleanup';
import {
    appPreferencesToAppUiPreferences,
    closeActionFromDto,
    isDefaultUiPreferencesOnDisk,
} from '../domain/settings/ui-preferences-bridge';
import {
    coerceWebhookChannels,
    DEFAULT_ONEBOT_MESSAGE,
    DEFAULT_WEBHOOK_BODY,
    type WebhookChannelDraft,
} from '../domain/settings/offline-notify-defaults';
import { normalizeFeatures, type FeatureToggles } from '../domain/settings/features';
import { invoke, isTauri } from '../ipc/transport';
import type { AppSettingsDto } from '../ipc/types';
import type { SnowLumaLinuxPackage } from '../ipc/types';
import type { AppUiPreferences } from '../ipc/generated/domain/AppUiPreferences';
import type { McpServerSettings } from '../ipc/generated/domain/McpServerSettings';
import type { OfflineDeliveryRecord } from '../ipc/generated/domain/OfflineDeliveryRecord';
import type { ReloginSettings } from '../ipc/generated/domain/ReloginSettings';
import type { GithubApiMode } from '../ipc/generated/domain/GithubApiMode';
import { defaultReloginSettings } from '../domain/settings/relogin';
import {
    defaultNetworkSettings,
    validateNetworkSettings,
    type NetworkSettings,
} from '../domain/settings/network';
import type { EnsureOneBotMessengerHttpResult } from '../ipc/generated/domain/EnsureOneBotMessengerHttpResult';
import type { OneBotMessengerCandidate } from '../ipc/generated/domain/OneBotMessengerCandidate';
import { mockBackendSettings } from '../ipc/mock/settings.mock';
import { preferencesStore } from '../domain/settings/preferencesStore';
import type {
    AfterCloseUiBehavior,
    BackendSettings,
    UiModeOnStartup,
} from '../domain/settings/backendSettings';

export {
    clientPrefsFromBackend,
    type AfterCloseUiBehavior,
    type BackendSettings,
    type UiModeOnStartup,
} from '../domain/settings/backendSettings';

export {
    DEFAULT_ONEBOT_MESSAGE,
    DEFAULT_WEBHOOK_BODY,
    type WebhookChannelDraft,
} from '../domain/settings/offline-notify-defaults';

function normalizeAfterClose(raw: unknown): AfterCloseUiBehavior {
    switch (raw) {
        case 'hide':
        case 'Hide':
            return 'hide';
        case 'delayed_lightweight':
        case 'DelayedLightweight':
            return 'delayed_lightweight';
        case 'immediate_lightweight':
        case 'ImmediateLightweight':
            return 'immediate_lightweight';
        default:
            return 'delayed_lightweight';
    }
}

function normalizeUiModeOnStartup(raw: unknown): UiModeOnStartup {
    return raw === 'tray_only' || raw === 'TrayOnly' ? 'tray_only' : 'normal';
}

function mirrorWebhookFlat(channels: WebhookChannelDraft[]): {
    url: string;
    secret: string;
    body: string;
    method: string;
} {
    const first = channels.find((c) => c.enabled && c.url.trim()) ?? channels[0];
    if (!first) {
        return {
            url: '',
            secret: '',
            body: DEFAULT_WEBHOOK_BODY,
            method: 'POST',
        };
    }
    return {
        url: first.url,
        secret: first.secret,
        body: first.bodyTemplate || DEFAULT_WEBHOOK_BODY,
        method: first.method || 'POST',
    };
}

function fromDto(dto: AppSettingsDto, preferDiskUi = false): BackendSettings {
    const closeAction = closeActionFromDto(dto.settings.closeAction);
    let ui = dto.settings.uiPreferences;
    if (!preferDiskUi && isDefaultUiPreferencesOnDisk(ui)) {
        const local = preferencesStore.get();
        const localUi = appPreferencesToAppUiPreferences(local);
        if (!isDefaultUiPreferencesOnDisk(localUi)) {
            ui = localUi;
        }
    }
    const wh = dto.settings.WebHook;
    const channels = coerceWebhookChannels({
        channels: wh?.channels,
        url: wh?.WebHookUrl,
        secret: wh?.WebHookSecret,
        bodyTemplate: wh?.WebHookJson,
        method: wh?.WebHookMethod,
    });
    const flat = mirrorWebhookFlat(channels);
    return {
        botLoginCheckIntervalMs: Number(dto.settings.poller.botLoginCheckInterval),
        botOfflineWebHookNotice: dto.settings.poller.botOfflineWebHookNotice ?? false,
        botOfflineEmailNotice: dto.settings.poller.botOfflineEmailNotice ?? false,
        performanceMonitorEnabled: dto.settings.performanceMonitorEnabled,
        performanceMonitorIntervalMs: clampPerformanceMonitorIntervalMs(
            Number(dto.settings.performanceMonitorInterval),
        ),
        botRuntimeMetricsEnabled: dto.settings.botRuntimeMetricsEnabled ?? false,
        botRuntimeMetricsIntervalMs: Math.max(
            1000,
            Math.min(30_000, Number(dto.settings.botRuntimeMetricsIntervalMs ?? 3000) || 3000),
        ),
        botRuntimeMetricsRetentionDays: Math.max(
            1,
            Math.min(90, Number(dto.settings.botRuntimeMetricsRetentionDays ?? 7) || 7),
        ),
        taskQueueCleanup: taskQueueCleanupFromAppSettings({
            taskQueueCleanupEnabled: dto.settings.taskQueueCleanupEnabled,
            taskQueueCleanupLingerMs: dto.settings.taskQueueCleanupLingerMs,
        }),
        githubApiMode: dto.settings.githubApiMode ?? 'proxy',
        githubPat: dto.githubPat ?? '',
        network: { ...defaultNetworkSettings(), ...dto.settings.network },
        proxyPassword: dto.proxyPassword ?? '',
        networkError: dto.networkError ?? undefined,
        closeAction,
        afterCloseUiBehavior: normalizeAfterClose(dto.settings.afterCloseUiBehavior),
        enterLightweightDelaySecs: Number(dto.settings.enterLightweightDelaySecs ?? 300),
        uiModeOnStartup: normalizeUiModeOnStartup(dto.settings.uiModeOnStartup),
        launchOnStartup: dto.settings.launchOnStartup ?? false,
        minimizeToTrayCountsAsHidden: dto.settings.minimizeToTrayCountsAsHidden ?? true,
        notifyOnOffline: dto.settings.notifyOnOffline ?? true,
        notifyOnBotCrashed: dto.settings.notifyOnBotCrashed ?? true,
        notifyOnLoginKicked: dto.settings.notifyOnLoginKicked ?? true,
        uiPreferences: ui,
        features: normalizeFeatures(dto.settings.features),
        // 不落扁平字段：MCP 设置没有设置页控件，整段带回（丢一次保存服务就被关掉）
        mcp: dto.settings.mcp,
        remoteHostHealthProbeEnabled: dto.settings.remoteHostHealthProbeEnabled ?? true,
        remoteHostHealthProbeIntervalMs: Number(
            dto.settings.remoteHostHealthProbeIntervalMs ?? 30_000,
        ),
        webHookChannels: channels,
        webHookUrl: flat.url || (wh?.WebHookUrl ?? ''),
        webHookSecret: flat.secret || (wh?.WebHookSecret ?? ''),
        webHookJson: flat.body || (wh?.WebHookJson ?? DEFAULT_WEBHOOK_BODY),
        webHookMethod: flat.method || (wh?.WebHookMethod ?? 'POST'),
        emailSender: dto.settings.Email?.EmailSender ?? '',
        emailReceiver: dto.settings.Email?.EmailReceiver ?? '',
        emailToken: dto.settings.Email?.EmailToken ?? '',
        emailSmtpServer: dto.settings.Email?.EmailStmpServer ?? '',
        emailSmtpPort: Number(dto.settings.Email?.EmailStmpPort ?? 465),
        emailEncryption: dto.settings.Email?.EmailEncryption ?? 'SSL',
        onebotNoticeEnabled: dto.settings.onebotOfflineNotice?.onebotNoticeEnabled ?? false,
        onebotRelogin: dto.settings.onebotOfflineNotice?.relogin ?? defaultReloginSettings(),
        ...normalizeOneBotIdsFromDto(dto.settings.onebotOfflineNotice),
        onebotTargetType: dto.settings.onebotOfflineNotice?.onebotTargetType ?? 'private',
        onebotMessageTemplate:
            dto.settings.onebotOfflineNotice?.onebotMessageTemplate ?? DEFAULT_ONEBOT_MESSAGE,
        notifyOnRecovered: dto.settings.poller.offlineNotifyBehavior?.notifyOnRecovered ?? false,
        offlineDebounceSeconds: Number(
            dto.settings.poller.offlineNotifyBehavior?.debounceSeconds ?? 0,
        ),
        offlineDeliveryHistoryLimit: Number(
            dto.settings.poller.offlineNotifyBehavior?.deliveryHistoryLimit ?? 50,
        ),
        snowlumaNodePath: dto.settings.snowlumaNodePath ?? null,
        snowlumaPackage: dto.settings.snowlumaPackage ?? null,
    };
}

/**
 * IPC 入参形状。ts-rs 生成的 AppSettings 用 bigint 表示 u64，但 Tauri invoke 会
 * JSON.stringify 参数，BigInt 会抛错。写命令时 u64 字段用 number 传，Rust serde 可接。
 */

type AppSettingsDtoInvoke = {
    settings: {
        poller: {
            botLoginCheckInterval: number;
            botOfflineWebHookNotice: boolean;
            botOfflineEmailNotice: boolean;
            offlineNotifyBehavior: {
                notifyOnRecovered: boolean;
                debounceSeconds: number;
                deliveryHistoryLimit: number;
            };
        };
        performanceMonitorEnabled: boolean;
        performanceMonitorInterval: number;
        botRuntimeMetricsEnabled: boolean;
        botRuntimeMetricsIntervalMs: number;
        botRuntimeMetricsRetentionDays: number;
        taskQueueCleanupEnabled: boolean;
        taskQueueCleanupLingerMs: number;
        githubApiMode: GithubApiMode;
        network: NetworkSettings;
        closeAction: string;
        afterCloseUiBehavior: string;
        enterLightweightDelaySecs: number;
        uiModeOnStartup: string;
        launchOnStartup: boolean;
        minimizeToTrayCountsAsHidden: boolean;
        notifyOnOffline: boolean;
        notifyOnBotCrashed: boolean;
        notifyOnLoginKicked: boolean;
        uiPreferences: AppUiPreferences;
        features: FeatureToggles;
        mcp: McpServerSettings;
        remoteHostHealthProbeEnabled: boolean;
        remoteHostHealthProbeIntervalMs: number;
        WebHook: {
            WebHookUrl: string;
            WebHookSecret: string;
            WebHookJson: string;
            WebHookMethod: string;
            channels: Array<{
                id: string;
                name: string;
                enabled: boolean;
                url: string;
                secret: string;
                body_template: string;
                method: string;
            }>;
        };
        Email: {
            EmailSender: string;
            EmailReceiver: string;
            EmailToken: string;
            EmailStmpServer: string;
            EmailStmpPort: number;
            EmailEncryption: string;
        };
        onebotOfflineNotice: {
            onebotNoticeEnabled: boolean;
            onebotMessengerBotId: string;
            onebotMessengerBotIds: string[];
            onebotTargetType: string;
            onebotTargetId: number;
            onebotTargetIds: number[];
            onebotMessageTemplate: string;
            relogin: ReloginSettings;
        };
        snowlumaNodePath: string | null;
        snowlumaPackage: SnowLumaLinuxPackage | null;
    };
    githubPat: string;
    proxyPassword: string;
};

function uiPreferencesForInvoke(ui: AppUiPreferences): AppUiPreferences {
    const n = (v: bigint | number) => (typeof v === 'bigint' ? Number(v) : v);
    return {
        ...ui,
        infoBarDismissInfoMs: n(ui.infoBarDismissInfoMs) as unknown as bigint,
        infoBarDismissSuccessMs: n(ui.infoBarDismissSuccessMs) as unknown as bigint,
        infoBarDismissWarningMs: n(ui.infoBarDismissWarningMs) as unknown as bigint,
    };
}

function toDtoInvoke(s: BackendSettings): AppSettingsDtoInvoke {
    const tq = taskQueueCleanupToStoredFields(s.taskQueueCleanup);
    const channels =
        s.webHookChannels.length > 0
            ? s.webHookChannels
            : coerceWebhookChannels({
                  url: s.webHookUrl,
                  secret: s.webHookSecret,
                  bodyTemplate: s.webHookJson,
                  method: s.webHookMethod,
              });
    const flat = mirrorWebhookFlat(channels);
    return {
        settings: {
            poller: {
                botLoginCheckInterval: Math.round(s.botLoginCheckIntervalMs),
                botOfflineWebHookNotice: s.botOfflineWebHookNotice,
                botOfflineEmailNotice: s.botOfflineEmailNotice,
                offlineNotifyBehavior: {
                    notifyOnRecovered: s.notifyOnRecovered,
                    debounceSeconds: Math.max(0, Math.round(s.offlineDebounceSeconds) || 0),
                    deliveryHistoryLimit: Math.max(
                        0,
                        Math.round(s.offlineDeliveryHistoryLimit) || 0,
                    ),
                },
            },
            performanceMonitorEnabled: s.performanceMonitorEnabled,
            performanceMonitorInterval: clampPerformanceMonitorIntervalMs(
                s.performanceMonitorIntervalMs,
            ),
            botRuntimeMetricsEnabled: s.botRuntimeMetricsEnabled,
            botRuntimeMetricsIntervalMs: Math.max(
                1000,
                Math.min(30_000, Math.round(s.botRuntimeMetricsIntervalMs) || 3000),
            ),
            botRuntimeMetricsRetentionDays: Math.max(
                1,
                Math.min(90, Math.round(s.botRuntimeMetricsRetentionDays) || 7),
            ),
            taskQueueCleanupEnabled: tq.taskQueueCleanupEnabled,
            taskQueueCleanupLingerMs: tq.taskQueueCleanupLingerMs,
            githubApiMode: s.githubApiMode,
            network: { ...s.network },
            closeAction: s.closeAction === 'tray' ? 'tray' : 'close',
            afterCloseUiBehavior: s.afterCloseUiBehavior,
            enterLightweightDelaySecs: s.enterLightweightDelaySecs,
            uiModeOnStartup: s.uiModeOnStartup,
            launchOnStartup: s.launchOnStartup,
            minimizeToTrayCountsAsHidden: s.minimizeToTrayCountsAsHidden,
            notifyOnOffline: s.notifyOnOffline,
            notifyOnBotCrashed: s.notifyOnBotCrashed,
            notifyOnLoginKicked: s.notifyOnLoginKicked,
            uiPreferences: uiPreferencesForInvoke(s.uiPreferences),
            features: normalizeFeatures(s.features),
            mcp: s.mcp,
            remoteHostHealthProbeEnabled: s.remoteHostHealthProbeEnabled,
            remoteHostHealthProbeIntervalMs: Math.round(s.remoteHostHealthProbeIntervalMs),
            WebHook: {
                WebHookUrl: flat.url.trim(),
                WebHookSecret: flat.secret,
                WebHookJson: flat.body || DEFAULT_WEBHOOK_BODY,
                WebHookMethod: flat.method || 'POST',
                channels: channels.map((ch) => ({
                    id: ch.id,
                    name: ch.name,
                    enabled: ch.enabled,
                    url: ch.url.trim(),
                    secret: ch.secret,
                    body_template: ch.bodyTemplate || DEFAULT_WEBHOOK_BODY,
                    method: (ch.method || 'POST').toUpperCase(),
                })),
            },
            Email: {
                EmailSender: s.emailSender.trim(),
                EmailReceiver: s.emailReceiver.trim(),
                EmailToken: s.emailToken,
                EmailStmpServer: s.emailSmtpServer.trim(),
                EmailStmpPort: Math.round(s.emailSmtpPort) || 465,
                EmailEncryption: s.emailEncryption || 'SSL',
            },
            onebotOfflineNotice: {
                onebotNoticeEnabled: s.onebotNoticeEnabled,
                ...normalizeOneBotIdsForInvoke(s),
                onebotTargetType: s.onebotTargetType || 'private',
                onebotMessageTemplate: s.onebotMessageTemplate || DEFAULT_ONEBOT_MESSAGE,
                relogin: s.onebotRelogin,
            },
            snowlumaNodePath: s.snowlumaNodePath || null,
            snowlumaPackage: s.snowlumaPackage || null,
        },
        githubPat: s.githubPat.trim(),
        proxyPassword: s.proxyPassword,
    };
}

function normalizeOneBotIdsFromDto(
    raw:
        | {
              onebotMessengerBotId?: string;
              onebotMessengerBotIds?: string[];
              onebotTargetId?: number;
              onebotTargetIds?: number[];
          }
        | null
        | undefined,
): Pick<
    BackendSettings,
    'onebotMessengerBotId' | 'onebotMessengerBotIds' | 'onebotTargetId' | 'onebotTargetIds'
> {
    const messengerIds = dedupeStrings([
        ...(raw?.onebotMessengerBotIds ?? []),
        ...(raw?.onebotMessengerBotId ? raw.onebotMessengerBotId.split(/[,，;\s]+/) : []),
    ]);
    const targetIds = dedupeNumbers([
        ...(raw?.onebotTargetIds ?? []).map((n) => Number(n) || 0),
        Number(raw?.onebotTargetId ?? 0) || 0,
    ]);
    return {
        onebotMessengerBotIds: messengerIds,
        onebotMessengerBotId: messengerIds[0] ?? '',
        onebotTargetIds: targetIds,
        onebotTargetId: targetIds[0] ?? 0,
    };
}

function normalizeOneBotIdsForInvoke(s: BackendSettings): {
    onebotMessengerBotId: string;
    onebotMessengerBotIds: string[];
    onebotTargetId: number;
    onebotTargetIds: number[];
} {
    const messengerIds = dedupeStrings([
        ...s.onebotMessengerBotIds,
        ...s.onebotMessengerBotId.split(/[,，;\s]+/),
    ]);
    const targetIds = dedupeNumbers([
        ...s.onebotTargetIds.map((n) => Math.round(n) || 0),
        Math.round(s.onebotTargetId) || 0,
    ]);
    return {
        onebotMessengerBotIds: messengerIds,
        onebotMessengerBotId: messengerIds[0] ?? '',
        onebotTargetIds: targetIds,
        onebotTargetId: targetIds[0] ?? 0,
    };
}

function dedupeStrings(values: string[]): string[] {
    const out: string[] = [];
    const seen = new Set<string>();
    for (const raw of values) {
        const value = raw.trim();
        if (!value || seen.has(value)) continue;
        seen.add(value);
        out.push(value);
    }
    return out;
}

function dedupeNumbers(values: number[]): number[] {
    const out: number[] = [];
    const seen = new Set<number>();
    for (const raw of values) {
        const value = Math.round(raw) || 0;
        if (value <= 0 || seen.has(value)) continue;
        seen.add(value);
        out.push(value);
    }
    return out;
}

export const settingsService = {
    get: async (preferDiskUi = false): Promise<BackendSettings> => {
        if (isTauri) {
            const dto = await invoke<AppSettingsDto>('get_app_settings');
            return fromDto(dto, preferDiskUi);
        }
        return {
            ...mockBackendSettings,
            network: { ...mockBackendSettings.network },
            webHookChannels: mockBackendSettings.webHookChannels.map((c) => ({
                ...c,
            })),
        };
    },

    set: async (settings: BackendSettings): Promise<void> => {
        const validationError = Object.values(
            validateNetworkSettings(settings.network, settings.proxyPassword),
        )[0];
        if (validationError) throw new Error(validationError);
        if (isTauri) {
            await invoke<void>('set_app_settings', { dto: toDtoInvoke(settings) });
            return;
        }
        Object.assign(mockBackendSettings, {
            ...settings,
            network: { ...settings.network },
            webHookChannels: settings.webHookChannels.map((c) => ({ ...c })),
        });
    },

    testWebhook: async (channelId?: string, channel?: WebhookChannelDraft): Promise<void> => {
        if (isTauri) {
            await invoke<void>('test_offline_webhook', {
                channelId: channelId ?? null,
                channel: channel
                    ? {
                          id: channel.id,
                          name: channel.name,
                          enabled: channel.enabled,
                          url: channel.url,
                          secret: channel.secret,
                          body_template: channel.bodyTemplate,
                          method: channel.method,
                      }
                    : null,
            });
            return;
        }
        throw new Error('浏览器模式不支持发送 Webhook 测试');
    },

    testEmail: async (): Promise<void> => {
        if (isTauri) {
            await invoke<void>('test_offline_email');
            return;
        }
        throw new Error('浏览器模式不支持发送邮件测试');
    },

    listOfflineDeliveryHistory: async (): Promise<OfflineDeliveryRecord[]> => {
        if (!isTauri) return [];
        return invoke<OfflineDeliveryRecord[]>('list_offline_delivery_history');
    },

    clearOfflineDeliveryHistory: async (): Promise<void> => {
        if (!isTauri) return;
        await invoke('clear_offline_delivery_history');
    },

    listOneBotMessengerCandidates: async (): Promise<OneBotMessengerCandidate[]> => {
        if (!isTauri) {
            return [
                {
                    bot_id: '10001',
                    name: '示例 Bot A',
                    state: 'running',
                    backend_type: 'napcat',
                    has_local_http: true,
                    http_port: 3010,
                    has_relogin_ws: true,
                    eligible: true,
                    can_enable_http: false,
                    scope: 'local',
                    server_id: null,
                    server_label: '本机',
                },
                {
                    bot_id: '10002',
                    name: '示例 Bot B',
                    state: 'stopped',
                    backend_type: 'snowluma',
                    has_local_http: false,
                    http_port: 0,
                    has_relogin_ws: false,
                    eligible: false,
                    can_enable_http: true,
                    scope: 'local',
                    server_id: null,
                    server_label: '本机',
                },
                {
                    bot_id: '2707600964',
                    name: '远端甲',
                    state: 'running',
                    backend_type: 'napcat',
                    has_local_http: true,
                    http_port: 3001,
                    has_relogin_ws: true,
                    eligible: false,
                    can_enable_http: false,
                    scope: 'remote',
                    server_id: 'srv-demo',
                    server_label: '机房-A',
                },
                {
                    bot_id: '2707600965',
                    name: '远端乙',
                    state: 'stopped',
                    backend_type: 'napcat',
                    has_local_http: false,
                    http_port: 0,
                    has_relogin_ws: false,
                    eligible: false,
                    can_enable_http: true,
                    scope: 'remote',
                    server_id: 'srv-demo',
                    server_label: '机房-A',
                },
            ];
        }
        return invoke<OneBotMessengerCandidate[]>('list_onebot_messenger_candidates');
    },

    ensureOneBotMessengerHttp: async (botId: string): Promise<EnsureOneBotMessengerHttpResult> => {
        if (!isTauri) {
            return {
                bot_id: botId,
                action: 'created',
                port: 3011,
                candidate: {
                    bot_id: botId,
                    name: botId,
                    state: 'running',
                    backend_type: 'napcat',
                    has_local_http: true,
                    http_port: 3011,
                    has_relogin_ws: true,
                    eligible: true,
                    can_enable_http: false,
                    scope: 'local',
                    server_id: null,
                    server_label: '本机',
                },
            };
        }
        return invoke<EnsureOneBotMessengerHttpResult>('ensure_onebot_messenger_http', {
            botId,
        });
    },
};
