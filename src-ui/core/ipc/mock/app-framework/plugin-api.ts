// 插件与商店假 API：各框架的配置文档清单、开关、装卸的分发都在这里，
// 具体写入在 karin.ts / store.ts / koishi.ts，市场清单在各框架文件。
import type {
    AppConfigDocument,
    AppConfigWriteResult,
    AppPluginAction,
    AppPluginConfigSchema,
    AppStoreInstalled,
    AppStoreMarketEntry,
    AppStoreResource,
    KarinPluginInstalled,
    KoishiInstanceConfig,
} from '../../types';
import { withMockDelay } from '../bootstrap.mock';
import { editKoishiConfig, peekKoishiConfig } from '../app-config.mock';
import { koishiMockPackages } from '../koishi.mock';
import { appendTo, koishiShortName, newPlugin, walk } from '../../../domain/apps/koishiConfig';
import { karinDefaultConfig } from '../../../domain/apps/karinConfig';
import { astrbotDefaultConfig } from '../../../domain/apps/astrbotConfig';
import { nonebot2DefaultConfig } from '../../../domain/apps/nonebot2Config';
import { yunzaiDefaultConfig } from '../../../domain/apps/yunzaiConfig';
import { require } from './state';
import {
    applyMockPluginOp,
    karinToStore,
    mockInstalled,
    mockInstalledFor,
    mockPluginMarket,
} from './karin';
import { mockNoneBotAdapters, mockNoneBotPlugins } from './nonebot2';
import { mockAstrBotPlugins } from './astrbot';
import { mockMaiBotPlugins } from './maibot';
import { applyMockKoishiStoreOp, mockKoishiPlugins } from './koishi';
import { mockYunzaiPlugins } from './yunzai';
import {
    applyMockStoreOp,
    mockStoreInstalled,
    mockStoreInstalledFor,
    STORE_MOCK_FRAMEWORKS,
    storeKey,
} from './store';

export const pluginApi = {
    listPluginConfigDocs: async (
        instanceId: string,
        pluginName: string,
    ): Promise<AppConfigDocument[]> => {
        const inst = require(instanceId);
        if (inst.framework_id === 'astrbot') {
            const dir = pluginName.split('/').pop() ?? pluginName;
            return withMockDelay([
                {
                    id: `plugin:${dir}`,
                    label: `${dir}_config.json`,
                    rel_path: `data/config/${dir}_config.json`,
                    format: 'json',
                    hot_reload: false,
                },
            ]);
        }
        if (inst.framework_id === 'nonebot2') {
            return withMockDelay([
                {
                    id: 'env_prod',
                    label: '.env.prod',
                    rel_path: '.env.prod',
                    format: 'dot_env',
                    hot_reload: false,
                },
            ]);
        }
        if (inst.framework_id === 'yunzai') {
            // 单 JS 插件没有配置目录；目录插件列 config/ 下的 yaml
            if (pluginName.endsWith('.js')) return withMockDelay([]);
            return withMockDelay(
                ['config/cfg.yaml', 'config/profile.yaml'].map((rel) => ({
                    id: `plugin:${pluginName}:${rel}`,
                    label: rel,
                    rel_path: `plugins/${pluginName}/${rel}`,
                    format: 'yaml' as const,
                    hot_reload: true,
                })),
            );
        }
        if (inst.framework_id === 'maibot') {
            const dir = pluginName.replaceAll('.', '_');
            return withMockDelay([
                {
                    id: `plugin:${dir}`,
                    label: `${dir}/config.toml`,
                    rel_path: `plugins/${dir}/config.toml`,
                    format: 'toml',
                    hot_reload: true,
                },
            ]);
        }
        const dir = pluginName.replaceAll('/', '-');
        return withMockDelay([
            {
                id: `plugin:${pluginName}:config/config.json`,
                label: 'config.json',
                rel_path: `@karinjs/${dir}/config/config.json`,
                format: 'json',
                hot_reload: true,
            },
        ]);
    },

    pluginConfigSchema: async (
        instanceId: string,
        pluginName: string,
    ): Promise<AppPluginConfigSchema | null> => {
        const inst = require(instanceId);
        if (inst.framework_id !== 'astrbot') return withMockDelay(null);
        const dir = pluginName.split('/').pop() ?? pluginName;
        return withMockDelay({
            doc_id: `plugin:${dir}`,
            fields: [
                {
                    key: 'token',
                    kind: 'string',
                    label: 'Bot Token',
                    hint: '从上游平台复制',
                    obvious_hint: true,
                    secret: true,
                    options: [],
                    items: [],
                },
                {
                    key: 'mode',
                    kind: 'string',
                    label: '模式',
                    hint: '',
                    obvious_hint: false,
                    secret: false,
                    options: ['chat', 'agent'],
                    items: [],
                },
                {
                    key: 'prompt',
                    kind: 'text',
                    label: '系统提示词',
                    hint: '',
                    obvious_hint: false,
                    secret: false,
                    options: [],
                    items: [],
                },
                {
                    key: 'enabled_groups',
                    kind: 'list',
                    label: '启用的群',
                    hint: '留空表示全部',
                    obvious_hint: false,
                    secret: false,
                    options: [],
                    items: [],
                },
                {
                    key: 'limits',
                    kind: 'object',
                    label: '限额',
                    hint: '',
                    obvious_hint: false,
                    secret: false,
                    options: [],
                    items: [
                        {
                            key: 'per_user',
                            kind: 'int',
                            label: '每人每日',
                            hint: '',
                            obvious_hint: false,
                            secret: false,
                            options: [],
                            items: [],
                        },
                        {
                            key: 'strict',
                            kind: 'bool',
                            label: '超限直接拒绝',
                            hint: '',
                            obvious_hint: false,
                            secret: false,
                            options: [],
                            items: [],
                        },
                    ],
                },
                {
                    key: 'extra',
                    kind: 'json',
                    label: '附加参数',
                    hint: '',
                    obvious_hint: false,
                    secret: false,
                    options: [],
                    items: [],
                },
            ],
        });
    },

    listPluginMarket: () => withMockDelay(mockPluginMarket.slice()),

    listStore: async (
        frameworkId: string,
        resource: AppStoreResource,
    ): Promise<AppStoreMarketEntry[]> => {
        if (frameworkId === 'karin' && resource === 'plugin') {
            return withMockDelay(mockPluginMarket.map(karinToStore));
        }
        if (frameworkId === 'nonebot2' && resource === 'adapter') {
            return withMockDelay(mockNoneBotAdapters.slice());
        }
        if (frameworkId === 'nonebot2' && resource === 'plugin') {
            return withMockDelay(mockNoneBotPlugins.slice());
        }
        if (frameworkId === 'astrbot' && resource === 'plugin') {
            return withMockDelay(mockAstrBotPlugins.slice());
        }
        if (frameworkId === 'maibot' && resource === 'plugin') {
            return withMockDelay(mockMaiBotPlugins.slice());
        }
        if (frameworkId === 'koishi' && resource === 'plugin') {
            return withMockDelay(mockKoishiPlugins.slice());
        }
        if (frameworkId === 'yunzai' && resource === 'plugin') {
            return withMockDelay(mockYunzaiPlugins.slice());
        }
        return withMockDelay([]);
    },

    listStoreInstalled: async (
        instanceId: string,
        resource: AppStoreResource,
    ): Promise<AppStoreInstalled[]> => {
        const inst = require(instanceId);
        if (inst.framework_id === 'koishi') {
            const cfg = peekKoishiConfig(inst);
            const on = new Set(
                walk(cfg.plugins)
                    .filter((n) => n.enabled)
                    .map((n) => n.name),
            );
            return withMockDelay(
                koishiMockPackages(cfg).map((p) => ({
                    id: p.package,
                    name: p.name,
                    resource: 'plugin' as const,
                    flavor: 'npm' as const,
                    version: p.version ?? undefined,
                    enabled: on.has(p.name),
                    package: p.package,
                    locked: [
                        'server',
                        'console',
                        'config',
                        'market',
                        'logger',
                        'adapter-onebot',
                    ].includes(p.name),
                })),
            );
        }
        return withMockDelay(mockStoreInstalledFor(instanceId, resource));
    },

    listPlugins: async (instanceId: string): Promise<KarinPluginInstalled[]> => {
        require(instanceId);
        return withMockDelay(mockInstalledFor(instanceId));
    },

    submitPluginOp: async (
        instanceId: string,
        pluginName: string,
        action: AppPluginAction,
        resource?: AppStoreResource,
    ): Promise<string> => {
        const target = require(instanceId);
        if (target.framework_id === 'koishi') {
            applyMockKoishiStoreOp(target, pluginName, action);
            return withMockDelay(`mock-plugin-${instanceId}-${pluginName}`);
        }
        if (STORE_MOCK_FRAMEWORKS.has(target.framework_id)) {
            applyMockStoreOp(instanceId, pluginName, action, resource ?? 'plugin');
        } else {
            applyMockPluginOp(instanceId, pluginName, action);
        }
        return withMockDelay(`mock-plugin-${instanceId}-${pluginName}`);
    },

    setPluginEnabled: async (
        instanceId: string,
        pluginName: string,
        enabled: boolean,
        _overwrite?: boolean,
        resource?: AppStoreResource,
    ): Promise<AppConfigWriteResult> => {
        const inst = require(instanceId);
        if (inst.framework_id === 'koishi') {
            const short = koishiShortName(pluginName);
            editKoishiConfig(inst, (cfg) => {
                let next = cfg;
                if (!walk(cfg.plugins).some((n) => n.name === short)) {
                    next = appendTo(next, [], newPlugin(next, short, enabled));
                }
                const flip = (
                    list: KoishiInstanceConfig['plugins'],
                ): KoishiInstanceConfig['plugins'] =>
                    list.map((n) => ({
                        ...n,
                        enabled: n.name === short ? enabled : n.enabled,
                        children: flip(n.children),
                    }));
                return { ...next, plugins: flip(next.plugins) };
            });
            return withMockDelay({
                config: { framework: 'koishi', data: peekKoishiConfig(inst) },
                revision: 'mock-r-plugin',
                documents: [],
                restart_required: false,
                relinked: false,
                port_changed: false,
            });
        }
        if (STORE_MOCK_FRAMEWORKS.has(inst.framework_id)) {
            const key = storeKey(instanceId, resource ?? 'plugin');
            const list = mockStoreInstalledFor(instanceId, resource ?? 'plugin');
            mockStoreInstalled.set(
                key,
                list.map((p) =>
                    p.id === pluginName || p.name === pluginName ? { ...p, enabled } : p,
                ),
            );
            if (inst.framework_id === 'yunzai') {
                // 单 JS 插件改名成 .js.disabled，云崽自己热卸载；没有配置文件要写
                return withMockDelay({
                    config: { framework: 'yunzai', data: yunzaiDefaultConfig(inst.port) },
                    revision: 'mock-r-plugin',
                    documents: [],
                    restart_required: false,
                    relinked: false,
                    port_changed: false,
                });
            }
            if (inst.framework_id === 'astrbot') {
                const config = astrbotDefaultConfig(inst.port);
                return withMockDelay({
                    config: { framework: 'astrbot', data: config },
                    revision: 'mock-r-plugin',
                    documents: [],
                    restart_required: inst.state === 'running',
                    relinked: false,
                    port_changed: false,
                });
            }
            const config = nonebot2DefaultConfig(inst.port);
            return withMockDelay({
                config: { framework: 'nonebot2', data: config },
                revision: 'mock-r-plugin',
                documents: [],
                restart_required: inst.state === 'running',
                relinked: false,
                port_changed: false,
            });
        }
        const list = mockInstalledFor(instanceId);
        const next = list.map((p) => (p.name === pluginName ? { ...p, enabled } : p));
        mockInstalled.set(instanceId, next);
        const config = karinDefaultConfig(inst.port);
        return withMockDelay({
            config: { framework: 'karin', data: config },
            revision: 'mock-r-plugin',
            documents: [],
            restart_required: false,
            relinked: false,
            port_changed: false,
        });
    },
};
