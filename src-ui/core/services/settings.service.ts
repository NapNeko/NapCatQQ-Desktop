// App 设置（后端持久化部分）IPC 服务。
// 唯一持有 get_app_settings / set_app_settings 命令名的位置（R3：单一字面量来源）。
//
// 后端 DTO（AppSettingsDto）里轮询间隔 / 性能监控间隔是 bigint（Rust u64 映射），
// 且 poller 是嵌套结构。本服务把它压平成一组前端友好的 number 字段（BackendSettings），
// 设置页 UI 不直接碰 bigint 与嵌套形状。读时 bigint→number，写时 number→bigint。
//
// 与 preferencesStore（纯客户端 localStorage 偏好）互补：那边管主题 / 吉祥物 /
// 窗口不透明度等不落后端的偏好，这边管需要后端持久化的偏好（轮询间隔 / 性能监控 /
// GitHub PAT）。两者不混在一起。

import { invoke, isTauri } from '../ipc/transport';
import type { AppSettingsDto } from '../ipc/types';
import { mockBackendSettings } from '../ipc/mock/settings.mock';

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
}

/** 后端 DTO → 扁平前端形状。bigint 收窄成 number（间隔值远小于 2^53，安全）。 */
function fromDto(dto: AppSettingsDto): BackendSettings {
    return {
        botLoginCheckIntervalMs: Number(dto.settings.poller.botLoginCheckInterval),
        performanceMonitorEnabled: dto.settings.performanceMonitorEnabled,
        performanceMonitorIntervalMs: Number(dto.settings.performanceMonitorInterval),
        githubPat: dto.githubPat ?? '',
    };
}

/** 扁平前端形状 → 后端 DTO。number → bigint 还原 u64 字段。 */
function toDto(s: BackendSettings): AppSettingsDto {
    return {
        settings: {
            poller: {
                botLoginCheckInterval: BigInt(Math.round(s.botLoginCheckIntervalMs)),
                // 离线通知后端为 noop，设置页不暴露；写回 false 保持稳定。
                botOfflineWebHookNotice: false,
                botOfflineEmailNotice: false,
            },
            performanceMonitorEnabled: s.performanceMonitorEnabled,
            performanceMonitorInterval: BigInt(Math.round(s.performanceMonitorIntervalMs)),
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
            await invoke<void>('set_app_settings', { dto: toDto(settings) });
            return;
        }
        // 浏览器 mock：写回内存，便于 dev 下手感连续。
        Object.assign(mockBackendSettings, settings);
    },
};
