// 浏览器预览模式下的 Koishi：插件树（按真装出来的 koishi.yml 整理）、插件表单（真实例里导出的 schema）、
// 已装插件包、运行状态。插件树的读写状态放在 app-config.mock.ts 里，和另外几个框架一样。

import type {
    AppInstance,
    KoishiInstanceConfig,
    KoishiPackageInfo,
    KoishiPluginNode,
    KoishiPluginSchema,
    KoishiRuntimeStatus,
} from '../types';
import { KOISHI_LINK_IDENT, KOISHI_LINK_NAME, walk } from '../../domain/apps/koishiConfig';
import { withMockDelay } from './bootstrap.mock';
import schemaTable from './koishi-schemas.json';

const p = (name: string, ident: string, enabled = true, config: Record<string, unknown> = {}): KoishiPluginNode => ({
    name,
    ident,
    enabled,
    meta: {},
    config,
    children: [],
});

const g = (ident: string, children: KoishiPluginNode[], meta: Record<string, unknown> = {}): KoishiPluginNode => ({
    name: 'group',
    ident,
    enabled: true,
    meta,
    config: {},
    children,
});

export function koishiMockConfig(port: number): KoishiInstanceConfig {
    return {
        global: {},
        entry_meta: {},
        plugins: [
            g('server', [
                p('server', 'cj4vi7', true, { port, host: '127.0.0.1' }),
                p('server-satori', 'se7wph', false),
                p('server-temp', 'fsqsq4', false),
            ]),
            g('basic', [
                p('admin', '9ypkqi', false),
                p('bind', '5cq4q4', false),
                p('commands', 'e5l0oc'),
                p('help', 'ejb1rf'),
                p('http', '51zkub'),
                p('inspect', 'wppufr', false),
                p('locales', '6583jo'),
                p('proxy-agent', '2xtugm'),
                p('rate-limit', 't7ilti'),
                p('telemetry', '1teoz6'),
            ]),
            g('console', [
                p('actions', 'iq2hre'),
                p('analytics', '7y13u6'),
                { ...p('android', 'u3ymjh'), meta: { $if: "env.KOISHI_AGENT?.includes('Android')" } },
                p('auth', 'ur640t', false),
                p('config', '3qwe05'),
                p('console', 'helk2a', true, { open: false }),
                p('dataview', '9km3k7'),
                p('explorer', 'ylb65g'),
                p('logger', 'ec6dby'),
                p('insight', 'xtjkzs'),
                p('market', 'op0ext', true, { search: { endpoint: 'https://registry.koishi.chat/index.json' } }),
                p('notifier', 'welnss'),
                p('oobe', 'eieen6'),
                p('sandbox', 'ivxeh4'),
                p('status', '9dlk7e'),
                p('theme-vanilla', 'ut2rsl'),
            ]),
            g('storage', [
                p('database-mysql', '1dabfu', false, { database: 'koishi' }),
                p('database-sqlite', 'cnm7l5', true, { path: 'data/koishi.db' }),
                p('assets-local', '3d1jwo'),
            ]),
            g('adapter', [
                p('adapter-discord', 'a76qng', false),
                p('adapter-qq', 'p1wspk', false),
                p('adapter-telegram', '6jsf0v', false),
            ]),
            g('develop', [p('hmr', 'k4b2ot', true, { root: '.' })], { $if: "env.NODE_ENV === 'development'" }),
        ],
    };
}

const PACKAGE_OF: Record<string, string> = {
    'adapter-onebot': 'koishi-plugin-adapter-onebot',
    'database-sqlite': '@koishijs/plugin-database-sqlite',
    'rate-limit': 'koishi-plugin-rate-limit',
    'theme-vanilla': 'koishi-plugin-theme-vanilla',
    dataview: 'koishi-plugin-dataview',
    'assets-local': 'koishi-plugin-assets-local',
    telemetry: 'koishi-plugin-telemetry',
    android: 'koishi-plugin-android',
};

const packageOf = (name: string) => PACKAGE_OF[name] ?? `@koishijs/plugin-${name}`;

const DESCRIPTIONS: Record<string, string> = {
    server: '提供 Koishi 的 HTTP / WebSocket 服务',
    console: 'Koishi 控制台',
    help: '帮助指令',
    commands: '指令管理',
    market: '插件市场',
    logger: '日志查看',
    'adapter-onebot': 'OneBot 适配器',
    'database-sqlite': 'SQLite 数据库支持',
    sandbox: '沙盒，在控制台里模拟聊天',
    status: '运行状态',
};

/** 包列表按树里出现过的插件造，外加一个装了还没进树的 */
export function koishiMockPackages(cfg: KoishiInstanceConfig): KoishiPackageInfo[] {
    const names = new Set(walk(cfg.plugins).filter((n) => n.name !== 'group').map((n) => n.name));
    names.add('adapter-onebot');
    names.add('echo');
    return [...names].sort().map((name) => ({
        package: name === 'echo' ? 'koishi-plugin-echo' : packageOf(name),
        name,
        request: '^1.0.0',
        version: name === 'echo' ? '2.2.5' : '1.0.0',
        description: DESCRIPTIONS[name] ?? (name === 'echo' ? '复读消息' : ''),
    }));
}

const table = schemaTable as Record<string, { schema: unknown; usage: string | null }>;

export function koishiMockSchemas(names: string[]): KoishiPluginSchema[] {
    return names.map((name) => {
        const hit = table[name];
        if (name === 'nope' || (!hit && name.startsWith('missing'))) {
            return { name, package: null, version: null, schema: null, usage: null, error: `Cannot find module '${name}'` };
        }
        return {
            name,
            package: name ? packageOf(name) : 'koishi',
            version: name ? '1.0.0' : '4.18.11',
            schema: hit?.schema ?? null,
            usage: hit?.usage ?? null,
            error: null,
        };
    });
}

export function koishiMockStatus(instance: AppInstance, cfg: KoishiInstanceConfig): Promise<KoishiRuntimeStatus> {
    if (instance.state !== 'running') {
        return withMockDelay({ gate: 'not_running', message: null, bots: [], memory: null, cpu: null });
    }
    const link = walk(cfg.plugins).find((n) => n.name === KOISHI_LINK_NAME && n.ident === KOISHI_LINK_IDENT);
    const selfId = typeof link?.config.selfId === 'string' ? link.config.selfId : '';
    const bots =
        link?.enabled && selfId
            ? [
                  {
                      sid: `onebot:${selfId}`,
                      platform: 'onebot',
                      self_id: selfId,
                      name: '小 k',
                      avatar: '',
                      state: 'online' as const,
                      error: null,
                      message_sent: 12,
                      message_received: 87,
                      paths: [KOISHI_LINK_IDENT],
                  },
              ]
            : [];
    return withMockDelay({ gate: 'ok', message: null, bots, memory: [0.012, 0.46], cpu: [0.004, 0.13] });
}
