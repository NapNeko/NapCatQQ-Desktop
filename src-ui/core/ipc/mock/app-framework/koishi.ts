// Koishi 相关的全部假逻辑：控制台能力转发（数据在 koishi.mock）、
// koishi.yml 插件树的假改动（对接写节点 / 商店装卸 / 开关），配置状态本体在 app-config.mock。
import type {
    AppInstance,
    AppPluginAction,
    AppStoreMarketEntry,
    KoishiInstanceConfig,
    KoishiPackageInfo,
    KoishiPluginSchema,
    KoishiRuntimeStatus,
} from '../../types';
import {
    appendTo,
    isLinkNode,
    koishiShortName,
    linkNode,
    newPlugin,
    replaceAt,
    walk,
} from '../../../domain/apps/koishiConfig';
import {
    koishiMockCommandAliases,
    koishiMockCommandUpdate,
    koishiMockCommands,
    koishiMockDatabaseRows,
    koishiMockDatabaseTables,
    koishiMockExplorerMkdir,
    koishiMockExplorerRead,
    koishiMockExplorerRemove,
    koishiMockExplorerRename,
    koishiMockExplorerTree,
    koishiMockExplorerWrite,
    koishiMockPackages,
    koishiMockSandboxMessages,
    koishiMockSandboxSend,
    koishiMockSchemas,
    koishiMockStatus,
} from '../koishi.mock';
import { editKoishiConfig, peekKoishiConfig } from '../app-config.mock';
import { withMockDelay } from '../bootstrap.mock';
import { playMockAppRun } from '../app-log.mock';
import { require } from './state';

/** 对接写 / 摘 koishi.yml 里的 adapter-onebot:ncd-link（真机由后端 apply_link / unlink 写） */
export function syncKoishiLink(inst: AppInstance, botId: string | null) {
    editKoishiConfig(inst, (cfg: KoishiInstanceConfig) => {
        const existing = linkNode(cfg);
        if (!botId) {
            if (!existing) return cfg;
            const path = findPath(cfg, isLinkNode);
            return path ? replaceAt(cfg, path, (n) => ({ ...n, enabled: false })) : cfg;
        }
        const config = {
            selfId: botId,
            token: 'mockmockmockmockmockmock',
            protocol: 'ws-reverse',
            path: '/onebot/ncd',
        };
        const path = findPath(cfg, isLinkNode);
        if (path)
            return replaceAt(cfg, path, (n) => ({
                ...n,
                enabled: true,
                config: { ...n.config, ...config },
            }));
        const group = cfg.plugins.findIndex(
            (n) => n.name === 'group' && n.ident === 'adapter' && n.enabled,
        );
        const node = { ...newPlugin(cfg, 'adapter-onebot', true), ident: 'ncd-link', config };
        return appendTo(cfg, group >= 0 ? [group] : [], node);
    });
}

function findPath(
    cfg: KoishiInstanceConfig,
    pred: (n: KoishiInstanceConfig['plugins'][number]) => boolean,
): number[] | null {
    const go = (list: KoishiInstanceConfig['plugins'], base: number[]): number[] | null => {
        for (let i = 0; i < list.length; i += 1) {
            if (pred(list[i])) return [...base, i];
            const hit = go(list[i].children, [...base, i]);
            if (hit) return hit;
        }
        return null;
    };
    return go(cfg.plugins, []);
}

/** 装 = 插件树里加一条停用的；卸 = 所有同名条目摘掉（真机后端还要 yarn add / remove） */
export function applyMockKoishiStoreOp(
    inst: AppInstance,
    pluginName: string,
    action: AppPluginAction,
) {
    const short = koishiShortName(pluginName);
    editKoishiConfig(inst, (cfg) => {
        if (action === 'uninstall') {
            const drop = (list: KoishiInstanceConfig['plugins']): KoishiInstanceConfig['plugins'] =>
                list
                    .filter((n) => n.name !== short)
                    .map((n) => ({ ...n, children: drop(n.children) }));
            return { ...cfg, plugins: drop(cfg.plugins) };
        }
        if (walk(cfg.plugins).some((n) => n.name === short)) return cfg;
        return appendTo(cfg, [], newPlugin(cfg, short, false));
    });
}

export const mockKoishiApi = {
    koishiStatus: (instanceId: string): Promise<KoishiRuntimeStatus> => {
        const inst = require(instanceId);
        return koishiMockStatus(inst, peekKoishiConfig(inst));
    },
    koishiPluginSchemas: (instanceId: string, names: string[]): Promise<KoishiPluginSchema[]> => {
        require(instanceId);
        return withMockDelay(koishiMockSchemas(names));
    },
    koishiPackages: (instanceId: string): Promise<KoishiPackageInfo[]> =>
        withMockDelay(koishiMockPackages(peekKoishiConfig(require(instanceId)))),
    koishiRestart: async (instanceId: string): Promise<void> => {
        const inst = require(instanceId);
        if (inst.state !== 'running') throw new Error('Koishi 没在运行');
        await withMockDelay(undefined);
        playMockAppRun(inst, () => require(instanceId).state === 'running');
    },
    koishiSandboxSend: (
        instanceId: string,
        msg: { platform: string; user: string; channel: string; content: string },
    ) => koishiMockSandboxSend(instanceId, msg),
    koishiSandboxMessages: (instanceId: string) => koishiMockSandboxMessages(instanceId),
    koishiExplorerTree: (instanceId: string) => koishiMockExplorerTree(instanceId),
    koishiExplorerRead: (instanceId: string, path: string) =>
        koishiMockExplorerRead(instanceId, path),
    koishiExplorerWrite: (instanceId: string, path: string, content: string, binary?: boolean) =>
        koishiMockExplorerWrite(instanceId, path, content, binary),
    koishiExplorerMkdir: (instanceId: string, path: string) =>
        koishiMockExplorerMkdir(instanceId, path),
    koishiExplorerRemove: (instanceId: string, path: string) =>
        koishiMockExplorerRemove(instanceId, path),
    koishiExplorerRename: (instanceId: string, from: string, to: string) =>
        koishiMockExplorerRename(instanceId, from, to),
    koishiDatabaseTables: (_instanceId: string) => koishiMockDatabaseTables(),
    koishiDatabaseRows: (_instanceId: string, table: string, offset: number, limit: number) =>
        koishiMockDatabaseRows(table, offset, limit),
    koishiCommands: (instanceId: string) => koishiMockCommands(instanceId),
    koishiCommandUpdate: (instanceId: string, name: string, config: Record<string, unknown>) =>
        koishiMockCommandUpdate(instanceId, name, config),
    koishiCommandAliases: (instanceId: string, name: string, aliases: string[]) =>
        koishiMockCommandAliases(instanceId, name, aliases),
};

export const koishiMarketEntry = (
    pkg: string,
    description: string,
    author: string,
    tags: string[],
    version = '1.0.0',
): AppStoreMarketEntry => ({
    resource: 'plugin',
    id: pkg,
    name: koishiShortName(pkg),
    description,
    version,
    author,
    homepage: `https://www.npmjs.com/package/${pkg}`,
    time: '2026-06-05T06:17:20.210Z',
    package: pkg,
    module_name: koishiShortName(pkg),
    flavor: 'npm',
    is_official: pkg.startsWith('@koishijs/'),
    valid: true,
    tags,
    supported_adapters: [],
    authors: [],
    repos: [],
    files: [],
    allow_build: [],
});

export const mockKoishiPlugins: AppStoreMarketEntry[] = [
    koishiMarketEntry(
        'koishi-plugin-adapter-onebot',
        'OneBot 适配器',
        'shigma',
        ['适配器', 'onebot'],
        '6.9.4',
    ),
    koishiMarketEntry('koishi-plugin-echo', '复读消息', 'shigma', ['实用工具'], '2.2.5'),
    koishiMarketEntry(
        'koishi-plugin-chatluna',
        '多平台模型接入的大语言模型聊天服务',
        'dingyi222666',
        ['人工智能'],
        '1.3.0',
    ),
    koishiMarketEntry(
        'koishi-plugin-puppeteer',
        '网页截图和图片渲染服务',
        'shigma',
        ['扩展功能'],
        '3.9.0',
    ),
    koishiMarketEntry('@koishijs/plugin-help', '帮助指令', 'shigma', ['实用工具'], '2.4.6'),
];
