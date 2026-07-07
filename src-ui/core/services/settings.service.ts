// App 设置（后端持久化部分）IPC 服务。
// 唯一持有 get_app_settings / set_app_settings 命令名的位置（R3：单一字面量来源）。
//
// 外观 / 动画 / 圆角等写入 app-settings.json 的 uiPreferences，与 localStorage 双写；
// 启动时以磁盘为准（见 useAppUiPreferencesBootstrap）。

import { clampPerformanceMonitorIntervalMs } from '../domain/performance/performanceSettings';
import {
    taskQueueCleanupFromAppSettings,
    taskQueueCleanupToStoredFields,
    type TaskQueueCleanupPrefs,
} from '../domain/task-queue/cleanup';
import {
    appPreferencesToAppUiPreferences,
    appUiPreferencesToAppPreferences,
    closeActionFromDto,
    isDefaultUiPreferencesOnDisk,
} from '../domain/settings/ui-preferences-bridge';
import { invoke, isTauri } from '../ipc/transport';
import type { AppSettingsDto } from '../ipc/types';
import type { AppUiPreferences } from '../ipc/generated/domain/AppUiPreferences';
import { mockBackendSettings } from '../ipc/mock/settings.mock';
import {
    preferencesStore,
    type AppPreferences,
    type CloseAction,
} from '../../hooks/preferences/preferencesStore';

export type AfterCloseUiBehavior =
    | 'hide'
    | 'delayed_lightweight'
    | 'immediate_lightweight';
export type UiModeOnStartup = 'normal' | 'tray_only';

/** 设置页消费的扁平后端设置形状（全 number，无 bigint / 嵌套）。 */
export interface BackendSettings {
    botLoginCheckIntervalMs: number;
    botOfflineWebHookNotice: boolean;
    botOfflineEmailNotice: boolean;
    performanceMonitorEnabled: boolean;
    performanceMonitorIntervalMs: number;
    taskQueueCleanup: TaskQueueCleanupPrefs;
    githubPat: string;
    closeAction: CloseAction;
    afterCloseUiBehavior: AfterCloseUiBehavior;
    enterLightweightDelaySecs: number;
    uiModeOnStartup: UiModeOnStartup;
    minimizeToTrayCountsAsHidden: boolean;
    notifyOnOffline: boolean;
    notifyOnBotCrashed: boolean;
    notifyOnLoginKicked: boolean;
    uiPreferences: AppUiPreferences;
    // P1 主动探活（remote-ssh-stability）：用户可开关的后台远端主机健康探测
    remoteHostHealthProbeEnabled: boolean;
    remoteHostHealthProbeIntervalMs: number;
}

/** 由 BackendSettings 派生的客户端偏好（与 preferencesStore 一致）。 */
export function clientPrefsFromBackend(backend: BackendSettings): AppPreferences {
    return appUiPreferencesToAppPreferences(backend.uiPreferences, backend.closeAction);
}

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

function fromDto(dto: AppSettingsDto): BackendSettings {
    const closeAction = closeActionFromDto(dto.settings.closeAction);
    let ui = dto.settings.uiPreferences;
    if (isDefaultUiPreferencesOnDisk(ui)) {
        const local = preferencesStore.get();
        const localUi = appPreferencesToAppUiPreferences(local);
        if (!isDefaultUiPreferencesOnDisk(localUi)) {
            ui = localUi;
        }
    }
    return {
        botLoginCheckIntervalMs: Number(dto.settings.poller.botLoginCheckInterval),
        botOfflineWebHookNotice:
            dto.settings.poller.botOfflineWebHookNotice ?? false,
        botOfflineEmailNotice:
            dto.settings.poller.botOfflineEmailNotice ?? false,
        performanceMonitorEnabled: dto.settings.performanceMonitorEnabled,
        performanceMonitorIntervalMs: clampPerformanceMonitorIntervalMs(
            Number(dto.settings.performanceMonitorInterval),
        ),
        taskQueueCleanup: taskQueueCleanupFromAppSettings({
            taskQueueCleanupEnabled: dto.settings.taskQueueCleanupEnabled,
            taskQueueCleanupLingerMs: dto.settings.taskQueueCleanupLingerMs,
        }),
        githubPat: dto.githubPat ?? '',
        closeAction,
        afterCloseUiBehavior: normalizeAfterClose(
            dto.settings.afterCloseUiBehavior,
        ),
        enterLightweightDelaySecs: Number(
            dto.settings.enterLightweightDelaySecs ?? 300,
        ),
        uiModeOnStartup: normalizeUiModeOnStartup(dto.settings.uiModeOnStartup),
        minimizeToTrayCountsAsHidden:
            dto.settings.minimizeToTrayCountsAsHidden ?? true,
        notifyOnOffline: dto.settings.notifyOnOffline ?? true,
        notifyOnBotCrashed: dto.settings.notifyOnBotCrashed ?? true,
        notifyOnLoginKicked: dto.settings.notifyOnLoginKicked ?? true,
        uiPreferences: ui,
        // P1 主动探活（与 Rust 侧 serde rename 对齐）
        remoteHostHealthProbeEnabled: dto.settings.remoteHostHealthProbeEnabled ?? true,
        remoteHostHealthProbeIntervalMs: Number(dto.settings.remoteHostHealthProbeIntervalMs ?? 30_000),
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
        };
        performanceMonitorEnabled: boolean;
        performanceMonitorInterval: number;
        taskQueueCleanupEnabled: boolean;
        taskQueueCleanupLingerMs: number;
        closeAction: string;
        afterCloseUiBehavior: string;
        enterLightweightDelaySecs: number;
        uiModeOnStartup: string;
        minimizeToTrayCountsAsHidden: boolean;
        notifyOnOffline: boolean;
        notifyOnBotCrashed: boolean;
        notifyOnLoginKicked: boolean;
        uiPreferences: AppUiPreferences;
        // P1 主动探活（与 Rust 侧 serde rename 对齐）
        remoteHostHealthProbeEnabled: boolean;
        remoteHostHealthProbeIntervalMs: number;
    };
    githubPat: string;
};

function uiPreferencesForInvoke(ui: AppUiPreferences): AppUiPreferences {
    const n = (v: bigint | number) =>
        typeof v === 'bigint' ? Number(v) : v;
    return {
        ...ui,
        infoBarDismissInfoMs: n(ui.infoBarDismissInfoMs) as unknown as bigint,
        infoBarDismissSuccessMs: n(ui.infoBarDismissSuccessMs) as unknown as bigint,
        infoBarDismissWarningMs: n(ui.infoBarDismissWarningMs) as unknown as bigint,
    };
}

function toDtoInvoke(s: BackendSettings): AppSettingsDtoInvoke {
    const tq = taskQueueCleanupToStoredFields(s.taskQueueCleanup);
    return {
        settings: {
            poller: {
                botLoginCheckInterval: Math.round(s.botLoginCheckIntervalMs),
                botOfflineWebHookNotice: s.botOfflineWebHookNotice,
                botOfflineEmailNotice: s.botOfflineEmailNotice,
            },
            performanceMonitorEnabled: s.performanceMonitorEnabled,
            performanceMonitorInterval: clampPerformanceMonitorIntervalMs(
                s.performanceMonitorIntervalMs,
            ),
            taskQueueCleanupEnabled: tq.taskQueueCleanupEnabled,
            taskQueueCleanupLingerMs: tq.taskQueueCleanupLingerMs,
            closeAction: s.closeAction === 'tray' ? 'tray' : 'close',
            afterCloseUiBehavior: s.afterCloseUiBehavior,
            enterLightweightDelaySecs: s.enterLightweightDelaySecs,
            uiModeOnStartup: s.uiModeOnStartup,
            minimizeToTrayCountsAsHidden: s.minimizeToTrayCountsAsHidden,
            notifyOnOffline: s.notifyOnOffline,
            notifyOnBotCrashed: s.notifyOnBotCrashed,
            notifyOnLoginKicked: s.notifyOnLoginKicked,
            uiPreferences: uiPreferencesForInvoke(s.uiPreferences),
            // P1 主动探活（与 Rust 侧 serde rename 对齐）
            remoteHostHealthProbeEnabled: s.remoteHostHealthProbeEnabled,
            remoteHostHealthProbeIntervalMs: Math.round(s.remoteHostHealthProbeIntervalMs),
        },
        githubPat: s.githubPat.trim(),
    };
}

export const settingsService = {
    get: async (): Promise<BackendSettings> => {
        if (isTauri) {
            const dto = await invoke<AppSettingsDto>('get_app_settings');
            return fromDto(dto);
        }
        return { ...mockBackendSettings };
    },

    set: async (settings: BackendSettings): Promise<void> => {
        if (isTauri) {
            await invoke<void>('set_app_settings', { dto: toDtoInvoke(settings) });
            return;
        }
        Object.assign(mockBackendSettings, settings);
    },
};
