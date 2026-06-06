// App 设置（后端持久化部分）IPC 服务。
// 唯一持有 get_app_settings / set_app_settings 命令名的位置（R3：单一字面量来源）。
//
// 后端 DTO（AppSettingsDto）里轮询间隔 / 性能监控间隔是 bigint（Rust u64 映射），
// 且 poller 是嵌套结构。本服务把它压平成一组前端友好的 number 字段（BackendSettings），
// 设置页 UI 不直接碰 bigint 与嵌套形状。读时 bigint→number，写时 number→bigint。
//
// 与 preferencesStore 分工：主题 / 动画 / 吉祥物等本机偏好只在设置页保存时通过
// settings-draft 写入 localStorage；closeAction 与后端 app-settings.json 同步，保存时
// 一并落盘。运行时窗口关闭仍读 preferencesStore.closeAction（保存成功后已对齐）。

import { clampPerformanceMonitorIntervalMs } from '../domain/performance/performanceSettings';
import { invoke, isTauri } from '../ipc/transport';
import type { AppSettingsDto } from '../ipc/types';
import { mockBackendSettings } from '../ipc/mock/settings.mock';
import { normalizeCloseAction } from '../../hooks/preferences/preferencesStore';
import type { CloseAction } from '../../hooks/preferences/preferencesStore';

/** 设置页消费的扁平后端设置形状（全 number，无 bigint / 嵌套）。 */
export interface BackendSettings {
    /** Bot 登录态检查轮询间隔（毫秒），已登录时生效。 */
    botLoginCheckIntervalMs: number;
    /** 主页性能监控开关。 */
    performanceMonitorEnabled: boolean;
    /** 主页性能监控采样间隔（毫秒）。 */
    performanceMonitorIntervalMs: number;
    /** GitHub Personal Access Token，空串表示未设置。 */
    githubPat: string;
    /** 标题栏关闭按钮：`close` 退出，`tray` 隐藏到托盘（落 app-settings.json）。 */
    closeAction: CloseAction;
}

/** 后端 DTO → 扁平前端形状。bigint 收窄成 number（间隔值远小于 2^53，安全）。 */
function fromDto(dto: AppSettingsDto): BackendSettings {
    return {
        botLoginCheckIntervalMs: Number(dto.settings.poller.botLoginCheckInterval),
        performanceMonitorEnabled: dto.settings.performanceMonitorEnabled,
        performanceMonitorIntervalMs: clampPerformanceMonitorIntervalMs(
            Number(dto.settings.performanceMonitorInterval),
        ),
        githubPat: dto.githubPat ?? '',
        closeAction: normalizeCloseAction(dto.settings.closeAction),
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
    };
    githubPat: string;
};

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
        // 浏览器 mock：写回内存，便于 dev 下手感连续。
        Object.assign(mockBackendSettings, settings);
    },
};
