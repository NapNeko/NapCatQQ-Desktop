// Koishi 专有接口：运行状态、插件表单来源、已装插件包、重启 worker。插件树的读写走通用的应用端配置接口。

import { invoke, isTauri } from '../ipc/transport';
import { mockAppFrameworkApi } from '../ipc/mock/app-framework.mock';
import type { KoishiPackageInfo, KoishiPluginSchema, KoishiRuntimeStatus } from '../ipc/types';

export const koishiService = {
    status: async (instanceId: string): Promise<KoishiRuntimeStatus> => {
        if (!isTauri) return mockAppFrameworkApi.koishiStatus(instanceId);
        return invoke<KoishiRuntimeStatus>('koishi_status', { instanceId });
    },

    /** 空串 = 全局设置 */
    pluginSchemas: async (instanceId: string, names: string[]): Promise<KoishiPluginSchema[]> => {
        if (!isTauri) return mockAppFrameworkApi.koishiPluginSchemas(instanceId, names);
        return invoke<KoishiPluginSchema[]>('koishi_plugin_schemas', { instanceId, names });
    },

    packages: async (instanceId: string): Promise<KoishiPackageInfo[]> => {
        if (!isTauri) return mockAppFrameworkApi.koishiPackages(instanceId);
        return invoke<KoishiPackageInfo[]>('koishi_packages', { instanceId });
    },

    restart: async (instanceId: string): Promise<void> => {
        if (!isTauri) return mockAppFrameworkApi.koishiRestart(instanceId);
        return invoke<void>('koishi_restart', { instanceId });
    },
};
