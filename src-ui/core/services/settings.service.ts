// App 设置（后端持久化部分）IPC 服务。
// 唯一持有 get_app_settings / set_app_settings 命令名的位置（R3：单一字面量来源）。
//
// 外观 / 动画 / 圆角等写入 app-settings.json 的 uiPreferences，与 localStorage 双写；
// 启动时以磁盘为准（见 useAppUiPreferencesBootstrap）。

import { clampPerformanceMonitorIntervalMs } from '../domain/performance/performanceSettings';
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

/** 设置页消费的扁平后端设置形状（全 number，无 bigint / 嵌套）。 */
export interface BackendSettings {
    botLoginCheckIntervalMs: number;
    performanceMonitorEnabled: boolean;
    performanceMonitorIntervalMs: number;
    githubPat: string;
    closeAction: CloseAction;
    uiPreferences: AppUiPreferences;
}

/** 由 BackendSettings 派生的客户端偏好（与 preferencesStore 一致）。 */
export function clientPrefsFromBackend(backend: BackendSettings): AppPreferences {
    return appUiPreferencesToAppPreferences(backend.uiPreferences, backend.closeAction);
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
        performanceMonitorEnabled: dto.settings.performanceMonitorEnabled,
        performanceMonitorIntervalMs: clampPerformanceMonitorIntervalMs(
            Number(dto.settings.performanceMonitorInterval),
        ),
        githubPat: dto.githubPat ?? '',
        closeAction,
        uiPreferences: ui,
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
        closeAction: string;
        uiPreferences: AppUiPreferences;
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
    return {
        settings: {
            poller: {
                botLoginCheckInterval: Math.round(s.botLoginCheckIntervalMs),
                botOfflineWebHookNotice: false,
                botOfflineEmailNotice: false,
            },
            performanceMonitorEnabled: s.performanceMonitorEnabled,
            performanceMonitorInterval: clampPerformanceMonitorIntervalMs(
                s.performanceMonitorIntervalMs,
            ),
            closeAction: s.closeAction === 'tray' ? 'tray' : 'close',
            uiPreferences: uiPreferencesForInvoke(s.uiPreferences),
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