// 通用商店已装表：nonebot2 / astrbot / yunzai（麦麦、Koishi 各自的逻辑在对应模块）。
import type { AppPluginAction, AppStoreInstalled, AppStoreResource } from '../../types';
import { instances } from './state';
import { mockNoneBotAdapters, mockNoneBotPlugins } from './nonebot2';
import { mockAstrBotPlugins } from './astrbot';
import { mockMaiBotPlugins } from './maibot';
import { mockYunzaiPlugins } from './yunzai';

/** 这几个框架的商店 mock 走通用的已装表（Karin、麦麦走各自的） */
export const STORE_MOCK_FRAMEWORKS = new Set(['nonebot2', 'astrbot', 'yunzai']);

export const mockStoreInstalled = new Map<string, AppStoreInstalled[]>();

export function storeKey(instanceId: string, resource: AppStoreResource): string {
    return `${instanceId}:${resource}`;
}

export function seedStoreInstalled(
    framework: string | undefined,
    resource: AppStoreResource,
): AppStoreInstalled[] {
    if (framework === 'yunzai') {
        // 装了 TRSS 插件和一个停着的单 JS，没装 genshin / 喵喵：概览会提示去装
        return resource === 'plugin'
            ? [
                  {
                      id: 'TRSS-Plugin',
                      name: 'TRSS-Plugin',
                      resource: 'plugin',
                      flavor: 'git',
                      version: '1.0.0',
                      enabled: true,
                      package: 'TRSS-Plugin',
                      locked: false,
                  },
                  {
                      id: 'chuo.js',
                      name: 'chuo.js',
                      resource: 'plugin',
                      flavor: 'app',
                      enabled: false,
                      package: 'chuo.js',
                      locked: false,
                  },
              ]
            : [];
    }
    if (framework === 'maibot') {
        return resource === 'plugin'
            ? [
                  {
                      id: 'maibot-team.napcat-adapter',
                      name: 'Napcat_Adapter 适配器',
                      resource: 'plugin',
                      flavor: 'git',
                      version: '1.4.0',
                      enabled: true,
                      package: 'https://github.com/Mai-with-u/MaiBot-Napcat-Adapter',
                      locked: true,
                  },
                  {
                      id: 'maibot-team.hello-world-plugin',
                      name: 'Hello World 示例插件',
                      resource: 'plugin',
                      flavor: 'git',
                      version: '2.0.0',
                      enabled: false,
                      package: '',
                      locked: false,
                  },
              ]
            : [];
    }
    if (resource === 'adapter') {
        return framework === 'astrbot'
            ? []
            : [
                  {
                      id: 'nonebot.adapters.onebot.v11',
                      name: 'OneBot V11',
                      resource: 'adapter',
                      flavor: 'pypi',
                      version: '2.4.6',
                      enabled: true,
                      package: 'nonebot-adapter-onebot',
                      locked: false,
                  },
              ];
    }
    return framework === 'astrbot'
        ? [
              {
                  id: 'soulter/helloworld',
                  name: 'helloworld',
                  resource: 'plugin',
                  flavor: 'git',
                  version: '1.2.0',
                  enabled: true,
                  package: 'https://github.com/Soulter/helloworld',
                  locked: false,
              },
          ]
        : [
              {
                  id: 'nonebot_plugin_status',
                  name: 'Status',
                  resource: 'plugin',
                  flavor: 'pypi',
                  version: '0.9.0',
                  enabled: true,
                  package: 'nonebot-plugin-status',
                  locked: false,
              },
          ];
}

export function mockStoreInstalledFor(
    instanceId: string,
    resource: AppStoreResource,
): AppStoreInstalled[] {
    const key = storeKey(instanceId, resource);
    if (!mockStoreInstalled.has(key)) {
        const inst = instances.find((i) => i.id === instanceId);
        mockStoreInstalled.set(key, seedStoreInstalled(inst?.framework_id, resource));
    }
    return mockStoreInstalled.get(key) ?? [];
}

export function applyMockStoreOp(
    instanceId: string,
    pluginName: string,
    action: AppPluginAction,
    resource: AppStoreResource,
) {
    const key = storeKey(instanceId, resource);
    const current = mockStoreInstalledFor(instanceId, resource);
    if (action === 'uninstall') {
        mockStoreInstalled.set(
            key,
            current.filter((p) => p.id !== pluginName && p.name !== pluginName),
        );
        return;
    }
    if (current.some((p) => p.id === pluginName || p.name === pluginName)) return;
    const market = [
        ...(resource === 'adapter' ? mockNoneBotAdapters : mockNoneBotPlugins),
        ...mockAstrBotPlugins,
        ...mockMaiBotPlugins,
        ...mockYunzaiPlugins,
    ].find((e) => e.id === pluginName || e.name === pluginName);
    mockStoreInstalled.set(key, [
        ...current,
        {
            id: market?.id ?? pluginName,
            name: market?.name ?? pluginName,
            resource,
            flavor: market?.flavor ?? 'pypi',
            version: '1.0.0',
            enabled: true,
            package: market?.package ?? '',
            locked: false,
        },
    ]);
}
