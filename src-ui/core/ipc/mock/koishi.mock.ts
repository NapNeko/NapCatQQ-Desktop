// 浏览器预览模式下的 Koishi：插件树（按真装出来的 koishi.yml 整理）、插件表单（真实例里导出的 schema）、
// 已装插件包、运行状态。插件树的读写状态放在 app-config.mock.ts 里，和另外几个框架一样。

import type {
    AppInstance,
    KoishiCommandRow,
    KoishiDatabaseTable,
    KoishiFileContent,
    KoishiFileEntry,
    KoishiInstanceConfig,
    KoishiPackageInfo,
    KoishiPluginNode,
    KoishiPluginSchema,
    KoishiRuntimeStatus,
    KoishiSandboxMessage,
} from '../types';
import { KOISHI_LINK_IDENT, KOISHI_LINK_NAME, walk } from '../../domain/apps/koishiConfig';
import { withMockDelay } from './bootstrap.mock';
import schemaTable from './koishi-schemas.json';

const p = (
    name: string,
    ident: string,
    enabled = true,
    config: Record<string, unknown> = {},
): KoishiPluginNode => ({
    name,
    ident,
    enabled,
    meta: {},
    config,
    children: [],
});

const g = (
    ident: string,
    children: KoishiPluginNode[],
    meta: Record<string, unknown> = {},
): KoishiPluginNode => ({
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
                {
                    ...p('android', 'u3ymjh'),
                    meta: { $if: "env.KOISHI_AGENT?.includes('Android')" },
                },
                p('auth', 'ur640t', false),
                p('config', '3qwe05'),
                p('console', 'helk2a', true, { open: false }),
                p('dataview', '9km3k7'),
                p('explorer', 'ylb65g'),
                p('logger', 'ec6dby'),
                p('insight', 'xtjkzs'),
                p('market', 'op0ext', true, {
                    search: { endpoint: 'https://registry.koishi.chat/index.json' },
                }),
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
            g('develop', [p('hmr', 'k4b2ot', true, { root: '.' })], {
                $if: "env.NODE_ENV === 'development'",
            }),
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
    const names = new Set(
        walk(cfg.plugins)
            .filter((n) => n.name !== 'group')
            .map((n) => n.name),
    );
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
            return {
                name,
                package: null,
                version: null,
                schema: null,
                usage: null,
                error: `Cannot find module '${name}'`,
            };
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

export function koishiMockStatus(
    instance: AppInstance,
    cfg: KoishiInstanceConfig,
): Promise<KoishiRuntimeStatus> {
    if (instance.state !== 'running') {
        return withMockDelay({
            gate: 'not_running',
            message: null,
            bots: [],
            memory: null,
            cpu: null,
        });
    }
    const link = walk(cfg.plugins).find(
        (n) => n.name === KOISHI_LINK_NAME && n.ident === KOISHI_LINK_IDENT,
    );
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
    return withMockDelay({
        gate: 'ok',
        message: null,
        bots,
        memory: [0.012, 0.46],
        cpu: [0.004, 0.13],
    });
}

// ---------------------------------------------------------------------------
// 控制台功能 mock：沙盒试聊 / 指令 / 数据库 / 文件
// ---------------------------------------------------------------------------

const sandboxLogs = new Map<string, KoishiSandboxMessage[]>();

/** 迷你指令引擎：help / echo 给个真的回复，其余复读，让试聊页有来有回 */
function sandboxReply(content: string): string {
    const text = content.trim();
    if (text === 'help' || text === '帮助') {
        return '当前可用指令：\nhelp  显示帮助\necho <text>  复读\nbind  绑定账号';
    }
    if (text.startsWith('echo ')) return text.slice(5);
    if (text.startsWith('/')) return `未知指令 ${text}。输入 help 查看帮助。`;
    return `沙盒收到：${text}（没接 QQ，指令以外的消息一般没人理）`;
}

export function koishiMockSandboxSend(
    instanceId: string,
    msg: { platform: string; user: string; channel: string; content: string },
): Promise<void> {
    const log = sandboxLogs.get(instanceId) ?? [];
    sandboxLogs.set(instanceId, log);
    const id = () => Math.random().toString(36).slice(2, 10);
    log.push({
        id: id(),
        user: msg.user,
        channel: msg.channel,
        content: msg.content,
        platform: msg.platform,
        quote: null,
    });
    // 群聊里 Bot 回在 `#`，私聊回在 `@用户`
    log.push({
        id: id(),
        user: 'koishi',
        channel: msg.channel,
        content: sandboxReply(msg.content),
        platform: msg.platform,
        quote: null,
    });
    return withMockDelay(undefined);
}

export function koishiMockSandboxMessages(instanceId: string): Promise<KoishiSandboxMessage[]> {
    return withMockDelay(structuredClone(sandboxLogs.get(instanceId) ?? []));
}

interface MockCommand {
    name: string;
    children: string[];
    created: boolean;
    paths: string[];
    aliases: string[];
    config: Record<string, unknown>;
}

const mockCommands = new Map<string, MockCommand[]>();

function defaultCommands(): MockCommand[] {
    return [
        {
            name: 'help',
            children: [],
            created: false,
            paths: ['ejb1rf'],
            aliases: ['帮助'],
            config: { authority: 1, showTip: true },
        },
        {
            name: 'echo',
            children: [],
            created: false,
            paths: [],
            aliases: [],
            config: { authority: 1 },
        },
        {
            name: 'bind',
            children: [],
            created: false,
            paths: ['5cq4q4'],
            aliases: ['绑定'],
            config: { authority: 1 },
        },
        {
            name: 'inspect',
            children: [],
            created: false,
            paths: ['wppufr'],
            aliases: [],
            config: { authority: 1 },
        },
        {
            name: 'admin',
            children: ['admin.channel'],
            created: false,
            paths: ['9ypkqi'],
            aliases: [],
            config: { authority: 3 },
        },
        {
            name: 'admin.channel',
            children: [],
            created: false,
            paths: ['9ypkqi'],
            aliases: [],
            config: { authority: 3, minInterval: 1000 },
        },
        {
            name: 'status',
            children: [],
            created: false,
            paths: ['9dlk7e'],
            aliases: ['状态'],
            config: { authority: 1, maxUsage: 5 },
        },
    ];
}

export function koishiMockCommands(instanceId: string): Promise<KoishiCommandRow[]> {
    const list = mockCommands.get(instanceId) ?? defaultCommands();
    mockCommands.set(instanceId, list);
    return withMockDelay(structuredClone(list));
}

export function koishiMockCommandUpdate(
    instanceId: string,
    name: string,
    config: Record<string, unknown>,
): Promise<void> {
    const list = mockCommands.get(instanceId) ?? defaultCommands();
    const row = list.find((c) => c.name === name);
    if (row) row.config = { ...row.config, ...config };
    return withMockDelay(undefined);
}

export function koishiMockCommandAliases(
    instanceId: string,
    name: string,
    aliases: string[],
): Promise<void> {
    const list = mockCommands.get(instanceId) ?? defaultCommands();
    const row = list.find((c) => c.name === name);
    if (row) row.aliases = aliases;
    return withMockDelay(undefined);
}

const MOCK_TABLES: Record<
    string,
    { primary: string[]; fields: Record<string, unknown>; rows: Record<string, unknown>[] }
> = {
    user: {
        primary: ['id'],
        fields: {
            id: { type: 'integer' },
            name: { type: 'string' },
            authority: { type: 'integer' },
        },
        rows: [
            { id: 1, name: 'Alice', authority: 1 },
            { id: 2, name: 'koishi', authority: 4 },
        ],
    },
    channel: {
        primary: ['platform', 'id'],
        fields: {
            platform: { type: 'string' },
            id: { type: 'string' },
            assignee: { type: 'string' },
        },
        rows: [{ platform: 'sandbox:ncd-desktop', id: '#', assignee: 'koishi' }],
    },
    binding: {
        primary: ['aid', 'platform'],
        fields: { aid: { type: 'integer' }, platform: { type: 'string' }, pid: { type: 'string' } },
        rows: [{ aid: 1, platform: 'sandbox:ncd-desktop', pid: 'Alice' }],
    },
};

export function koishiMockDatabaseTables(): Promise<KoishiDatabaseTable[]> {
    return withMockDelay(
        Object.entries(MOCK_TABLES).map(([name, t]) => ({
            name,
            primary: t.primary,
            fields: t.fields,
            count: t.rows.length,
        })),
    );
}

export function koishiMockDatabaseRows(
    table: string,
    offset: number,
    limit: number,
): Promise<Record<string, unknown>[]> {
    const t = MOCK_TABLES[table];
    return withMockDelay(t ? structuredClone(t.rows.slice(offset, offset + limit)) : []);
}

interface MockFsNode {
    type: 'file' | 'directory';
    name: string;
    content?: string;
    children?: MockFsNode[];
}

function defaultFs(): MockFsNode[] {
    return [
        { type: 'file', name: 'koishi.yml', content: '# Koishi 配置\nplugins:\n  group:entry:\n' },
        {
            type: 'file',
            name: 'package.json',
            content: '{\n  "name": "@koishijs/boilerplate"\n}\n',
        },
        { type: 'file', name: '.env', content: 'KOISHI_ENV=production\n' },
        {
            type: 'directory',
            name: 'data',
            children: [
                { type: 'file', name: 'koishi.db', content: '' },
                {
                    type: 'directory',
                    name: 'logs',
                    children: [{ type: 'file', name: '2026-10-01.log', content: '...' }],
                },
            ],
        },
        {
            type: 'directory',
            name: 'locales',
            children: [{ type: 'file', name: 'zh-CN.yml', content: 'help: 帮助\n' }],
        },
    ];
}

const mockFs = new Map<string, MockFsNode[]>();

function fsFind(tree: MockFsNode[], path: string): MockFsNode | undefined {
    const parts = path.split('/').filter(Boolean);
    let list = tree;
    let cur: MockFsNode | undefined;
    for (const part of parts) {
        cur = list.find((n) => n.name === part);
        if (!cur) return undefined;
        list = cur.children ?? [];
    }
    return cur;
}

function fsParent(tree: MockFsNode[], path: string): MockFsNode[] {
    const parts = path.split('/').filter(Boolean);
    parts.pop();
    let list = tree;
    for (const part of parts) {
        const dir = list.find((n) => n.name === part && n.type === 'directory');
        if (!dir) return list;
        list = dir.children ??= [];
    }
    return list;
}

export function koishiMockExplorerTree(instanceId: string): Promise<KoishiFileEntry[]> {
    const tree = mockFs.get(instanceId) ?? defaultFs();
    mockFs.set(instanceId, tree);
    return withMockDelay(structuredClone(tree) as KoishiFileEntry[]);
}

export function koishiMockExplorerRead(
    instanceId: string,
    path: string,
): Promise<KoishiFileContent> {
    const node = fsFind(mockFs.get(instanceId) ?? defaultFs(), path);
    if (!node || node.type !== 'file') throw new Error(`文件不存在：${path}`);
    const text = node.content ?? '';
    return withMockDelay({
        base64: btoa(unescape(encodeURIComponent(text))),
        mime: null,
        encoding: 'UTF-8',
    });
}

export function koishiMockExplorerWrite(
    instanceId: string,
    path: string,
    content: string,
    _binary?: boolean,
): Promise<void> {
    const tree = mockFs.get(instanceId) ?? defaultFs();
    mockFs.set(instanceId, tree);
    const existing = fsFind(tree, path);
    if (existing?.type === 'file') existing.content = content;
    else {
        const name = path.split('/').filter(Boolean).pop() ?? path;
        fsParent(tree, path).push({ type: 'file', name, content });
    }
    return withMockDelay(undefined);
}

export function koishiMockExplorerMkdir(instanceId: string, path: string): Promise<void> {
    const tree = mockFs.get(instanceId) ?? defaultFs();
    mockFs.set(instanceId, tree);
    const name = path.split('/').filter(Boolean).pop() ?? path;
    fsParent(tree, path).push({ type: 'directory', name, children: [] });
    return withMockDelay(undefined);
}

export function koishiMockExplorerRemove(instanceId: string, path: string): Promise<void> {
    const tree = mockFs.get(instanceId) ?? defaultFs();
    mockFs.set(instanceId, tree);
    const list = fsParent(tree, path);
    const name = path.split('/').filter(Boolean).pop() ?? path;
    const i = list.findIndex((n) => n.name === name);
    if (i >= 0) list.splice(i, 1);
    return withMockDelay(undefined);
}

export function koishiMockExplorerRename(
    instanceId: string,
    from: string,
    to: string,
): Promise<void> {
    const tree = mockFs.get(instanceId) ?? defaultFs();
    mockFs.set(instanceId, tree);
    const node = fsFind(tree, from);
    if (node) node.name = to.split('/').filter(Boolean).pop() ?? to;
    return withMockDelay(undefined);
}
