// 浏览器预览模式下的应用端实例配置假数据：内存版 Karin 配置 + 版本号递增 + 冲突模拟。
//
// 冲突模拟：控制台执行 `__ncdMock.appConfigConflictOnce()` 后，下一次带 base_revision 的保存
// 会返回 conflict（等价于「Karin WebUI 在你编辑期间改了文件」）。

import type {
    AppConfigDocument,
    AppConfigText,
    AppConfigWriteResult,
    AppInstance,
    AppInstanceConfig,
    AppInstanceConfigEnvelope,
    AstrBotInstanceConfig,
    KarinInstanceConfig,
    NoneBot2InstanceConfig,
} from '../types';
import {
    karinDefaultConfig,
    karinEnvToDotenv,
    karinLinkInputsChanged,
    validateKarinConfig,
} from '../../domain/apps/karinConfig';
import {
    nonebot2DefaultConfig,
    nonebot2LinkInputsChanged,
    validateNoneBot2Config,
} from '../../domain/apps/nonebot2Config';
import {
    astrbotDefaultConfig,
    astrbotLinkInputsChanged,
    validateAstrBotConfig,
} from '../../domain/apps/astrbotConfig';
import { makeAppConfigError } from '../../domain/apps/appConfigError';
import { withMockDelay } from './bootstrap.mock';

const KARIN_DOCS: AppConfigDocument[] = [
    { id: 'env', label: '.env', rel_path: '.env', format: 'dot_env', hot_reload: true },
    { id: 'config', label: 'config.json', rel_path: '@karinjs/config/config.json', format: 'json', hot_reload: true },
    { id: 'adapter', label: 'adapter.json', rel_path: '@karinjs/config/adapter.json', format: 'json', hot_reload: true },
    { id: 'groups', label: 'groups.json', rel_path: '@karinjs/config/groups.json', format: 'json', hot_reload: true },
    { id: 'privates', label: 'privates.json', rel_path: '@karinjs/config/privates.json', format: 'json', hot_reload: true },
    { id: 'render', label: 'render.json', rel_path: '@karinjs/config/render.json', format: 'json', hot_reload: true },
    { id: 'redis', label: 'redis.json', rel_path: '@karinjs/config/redis.json', format: 'json', hot_reload: false },
];

const NONEBOT2_DOCS: AppConfigDocument[] = [
    { id: 'env', label: '.env', rel_path: '.env', format: 'dot_env', hot_reload: false },
    { id: 'env_prod', label: '.env.prod', rel_path: '.env.prod', format: 'dot_env', hot_reload: false },
    { id: 'pyproject', label: 'pyproject.toml', rel_path: 'pyproject.toml', format: 'toml', hot_reload: false },
];

const ASTRBOT_DOCS: AppConfigDocument[] = [
    { id: 'cmd_config', label: 'cmd_config.json', rel_path: 'data/cmd_config.json', format: 'json', hot_reload: false },
];

const ASTRBOT_TEXT: Record<string, string> = {
    cmd_config: `${JSON.stringify(
        {
            dashboard: { port: 6185 },
            platform: [
                {
                    id: 'ncd-app:ab12cd34',
                    type: 'aiocqhttp',
                    enable: true,
                    ws_reverse_host: '0.0.0.0',
                    ws_reverse_port: 6199,
                    ws_reverse_token: '',
                },
            ],
        },
        null,
        2,
    )}\n`,
};

const NONEBOT2_TEXT: Record<string, string> = {
    env: 'ENVIRONMENT=prod\n',
    env_prod: 'DRIVER=~fastapi+~websockets\nHOST=127.0.0.1\nPORT=8080\nONEBOT_ACCESS_TOKEN=mock\n',
    pyproject:
        '[project]\nname = "nonebot2-instance"\nversion = "0.1.0"\nrequires-python = ">=3.10,<3.14"\ndependencies = [\n  "nonebot2[fastapi,websockets]>=2.3",\n  "nonebot-adapter-onebot>=2.4",\n]\n\n[tool.nonebot]\nadapters = [{ name = "OneBot V11", module_name = "nonebot.adapters.onebot.v11" }]\nplugins = []\n',
};

interface KarinState {
    config: KarinInstanceConfig;
    /** 逐文档版本号（整数递增，渲染成 `mock-r<n>`） */
    docRev: Record<string, number>;
    /** 原始文件 Tab 手改过的文本（优先于从类型化配置渲染） */
    rawOverride: Record<string, string>;
}

const karinStates = new Map<string, KarinState>();
const nbStates = new Map<string, { config: NoneBot2InstanceConfig; docRev: Record<string, number> }>();
const abStates = new Map<string, { config: AstrBotInstanceConfig; docRev: Record<string, number> }>();
const rawStates = new Map<string, { text: Record<string, string>; rev: Record<string, number> }>();
let conflictOnce = false;

const rev = (n: number) => `mock-r${n}`;

/// AstrBot 插件配置缺文件时后端按 schema 物化默认值；mock 里同一份形状。
const ASTRBOT_PLUGIN_DEFAULTS = `${JSON.stringify(
    {
        token: '',
        mode: 'chat',
        prompt: '',
        enabled_groups: [],
        limits: { per_user: 20, strict: false },
        extra: {},
    },
    null,
    2,
)}\n`;

function astrbotRaw(inst: AppInstance) {
    let raw = rawStates.get(inst.id);
    if (!raw) {
        raw = { text: { ...ASTRBOT_TEXT }, rev: { cmd_config: 1 } };
        rawStates.set(inst.id, raw);
    }
    return raw;
}

function seedKarin(instance: AppInstance): KarinState {
    const config = karinDefaultConfig(instance.port);
    config.env.http_auth_key = 'http-mock-key';
    config.env.ws_server_auth_key = instance.link ? 'mockmockmockmockmockmock' : '';
    config.env.comments = {
        HTTP_ENABLE: '是否启用HTTP',
        HTTP_PORT: 'HTTP监听端口',
        HTTP_HOST: 'HTTP监听地址',
        HTTP_AUTH_KEY: 'HTTP鉴权秘钥 仅用于karin自身Api',
        WS_SERVER_AUTH_KEY: 'ws_server鉴权秘钥',
        REDIS_ENABLE: '是否启用Redis 关闭后将使用内部虚拟Redis',
        PM2_RESTART: '重启是否调用pm2 如果不调用则会直接关机 此配置适合有进程守护的程序',
        LOG_LEVEL: '日志等级',
        LOG_DAYS_TO_KEEP: '日志保留天数',
        LOG_MAX_LOG_SIZE: '日志文件最大大小 如果此项大于0则启用日志分割',
        LOG_FNC_COLOR: 'logger.fnc颜色',
        LOG_MAX_CONNECTIONS: '日志实时Api最多支持同时连接数',
    };
    config.env.custom = [
        { key: 'LOG_API_MAX_CONNECTIONS', value: '5', comment: '日志实时Api最多支持同时连接数（旧键）' },
    ];
    config.config.master = ['console', '10001'];
    const docRev: Record<string, number> = {};
    for (const d of KARIN_DOCS) docRev[d.id] = 1;
    return { config, docRev, rawOverride: {} };
}

export function peekKarinHttpAuthKey(instanceId: string): string {
    return karinStates.get(instanceId)?.config.env.http_auth_key ?? 'http-mock-key';
}

/** 对接写 `.env` 的 WS_SERVER_AUTH_KEY；已有内存状态才改，未读过的实例走 seedKarin。 */
export function syncKarinLinkToken(instanceId: string, token?: string): void {
    const s = karinStates.get(instanceId);
    if (!s) return;
    const next = (token ?? s.config.env.ws_server_auth_key).trim() || 'mockmockmockmockmockmock';
    if (s.config.env.ws_server_auth_key === next) return;
    s.config = {
        ...s.config,
        env: { ...s.config.env, ws_server_auth_key: next },
    };
    s.docRev.env = (s.docRev.env ?? 0) + 1;
    delete s.rawOverride.env;
}

function karinState(instance: AppInstance): KarinState {
    let s = karinStates.get(instance.id);
    if (!s) {
        s = seedKarin(instance);
        karinStates.set(instance.id, s);
    }
    return s;
}

function combined(docRev: Record<string, number>, docs: AppConfigDocument[]): string {
    return `mock-${docs.map((d) => `${d.id}${docRev[d.id] ?? 0}`).join('.')}`;
}

function nbState(instance: AppInstance) {
    let s = nbStates.get(instance.id);
    if (!s) {
        const docRev: Record<string, number> = {};
        for (const d of NONEBOT2_DOCS) docRev[d.id] = 1;
        s = { config: nonebot2DefaultConfig(instance.port), docRev };
        if (instance.link) s.config.env_prod.onebot_access_token = 'mock';
        nbStates.set(instance.id, s);
    }
    return s;
}

function abState(instance: AppInstance) {
    let s = abStates.get(instance.id);
    if (!s) {
        const docRev: Record<string, number> = {};
        for (const d of ASTRBOT_DOCS) docRev[d.id] = 1;
        s = { config: astrbotDefaultConfig(instance.port), docRev };
        s.config.onebot.id = `ncd-app:${instance.id}`;
        s.config.claimed = true;
        if (instance.link) s.config.onebot.ws_reverse_token = 'mock';
        abStates.set(instance.id, s);
    }
    return s;
}

function abEnvelope(s: { config: AstrBotInstanceConfig; docRev: Record<string, number> }): AppInstanceConfigEnvelope {
    return {
        config: { framework: 'astrbot', data: structuredClone(s.config) },
        revision: combined(s.docRev, ASTRBOT_DOCS),
        documents: ASTRBOT_DOCS.map((d) => ({
            doc_id: d.id,
            revision: rev(s.docRev[d.id] ?? 0),
            hot_reload: d.hot_reload,
        })),
    };
}

function nbEnvelope(s: { config: NoneBot2InstanceConfig; docRev: Record<string, number> }): AppInstanceConfigEnvelope {
    return {
        config: { framework: 'nonebot2', data: structuredClone(s.config) },
        revision: combined(s.docRev, NONEBOT2_DOCS),
        documents: NONEBOT2_DOCS.map((d) => ({
            doc_id: d.id,
            revision: rev(s.docRev[d.id] ?? 0),
            hot_reload: d.hot_reload,
        })),
    };
}

function envelope(s: KarinState): AppInstanceConfigEnvelope {
    return {
        config: { framework: 'karin', data: structuredClone(s.config) },
        revision: combined(s.docRev, KARIN_DOCS),
        documents: KARIN_DOCS.map((d) => ({
            doc_id: d.id,
            revision: rev(s.docRev[d.id] ?? 0),
            hot_reload: d.hot_reload,
        })),
    };
}

function karinDocText(s: KarinState, docId: string): string {
    if (s.rawOverride[docId] != null) return s.rawOverride[docId];
    const c = s.config;
    const json = (v: unknown) => `${JSON.stringify(v, null, 2)}\n`;
    switch (docId) {
        case 'env':
            return karinEnvToDotenv(c.env);
        case 'config':
            return json(c.config);
        case 'adapter':
            return json(c.adapter);
        case 'groups':
            return json(c.groups);
        case 'privates':
            return json(c.privates);
        case 'render':
            return json(c.render);
        case 'redis':
            return json(c.redis);
        default:
            return '';
    }
}

/** 从类型化配置里找出哪些文档变了（mock 版的「只写变了的文件」） */
function changedDocs(before: KarinInstanceConfig, after: KarinInstanceConfig): string[] {
    const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);
    const out: string[] = [];
    if (!same(before.env, after.env)) out.push('env');
    if (!same(before.config, after.config)) out.push('config');
    if (!same(before.adapter, after.adapter)) out.push('adapter');
    if (!same(before.groups, after.groups)) out.push('groups');
    if (!same(before.privates, after.privates)) out.push('privates');
    if (!same(before.render, after.render)) out.push('render');
    if (!same(before.redis, after.redis)) out.push('redis');
    return out;
}

export interface MockAppConfigDeps {
    require: (id: string) => AppInstance;
    publish: (instance: AppInstance, reason: string) => void;
}

export function createMockAppConfigApi(deps: MockAppConfigDeps) {
    const requireInstalled = (id: string) => {
        const inst = deps.require(id);
        if (inst.state === 'not_installed' || inst.state === 'installing') {
            throw makeAppConfigError('other', '应用实例尚未安装，还没有可编辑的配置');
        }
        return inst;
    };

    return {
        readConfig: async (instanceId: string): Promise<AppInstanceConfigEnvelope> => {
            const inst = requireInstalled(instanceId);
            if (inst.framework_id === 'nonebot2') {
                return withMockDelay(nbEnvelope(nbState(inst)));
            }
            if (inst.framework_id === 'astrbot') {
                return withMockDelay(abEnvelope(abState(inst)));
            }
            if (inst.framework_id !== 'karin') {
                throw makeAppConfigError('unsupported', `该应用端暂不支持类型化配置: ${inst.framework_id}`);
            }
            return withMockDelay(envelope(karinState(inst)));
        },

        writeConfig: async (
            instanceId: string,
            config: AppInstanceConfig,
            baseRevision: string | null,
        ): Promise<AppConfigWriteResult> => {
            const inst = requireInstalled(instanceId);
            if (inst.framework_id === 'nonebot2' && config.framework === 'nonebot2') {
                const s = nbState(inst);
                if (baseRevision != null && baseRevision !== combined(s.docRev, NONEBOT2_DOCS)) {
                    throw makeAppConfigError('conflict', '配置已被修改（config），请重新加载后再保存');
                }
                const next = structuredClone(config.data);
                const issues = validateNoneBot2Config(next);
                if (issues.length) {
                    throw makeAppConfigError(
                        'invalid',
                        `配置校验未通过：${issues.map((i) => `${i.path}: ${i.message}`).join('; ')}`,
                        issues,
                    );
                }
                const before = s.config;
                if (JSON.stringify(before.env_prod) !== JSON.stringify(next.env_prod)) {
                    s.docRev.env_prod = (s.docRev.env_prod ?? 0) + 1;
                }
                s.config = next;
                let portChanged = false;
                let relinked = false;
                if (next.env_prod.port !== inst.port) {
                    deps.publish({ ...inst, port: next.env_prod.port }, 'port_changed');
                    portChanged = true;
                }
                if (inst.link && nonebot2LinkInputsChanged(before.env_prod, next.env_prod)) {
                    relinked = true;
                }
                const env = nbEnvelope(s);
                return withMockDelay({
                    config: env.config,
                    revision: env.revision,
                    documents: env.documents,
                    restart_required: inst.state === 'running',
                    relinked,
                    port_changed: portChanged,
                });
            }
            if (inst.framework_id === 'astrbot' && config.framework === 'astrbot') {
                const s = abState(inst);
                if (baseRevision != null && baseRevision !== combined(s.docRev, ASTRBOT_DOCS)) {
                    throw makeAppConfigError('conflict', '配置已被修改（cmd_config），请重新加载后再保存');
                }
                const next = structuredClone(config.data);
                const issues = validateAstrBotConfig(next);
                if (issues.length) {
                    throw makeAppConfigError(
                        'invalid',
                        `配置校验未通过：${issues.map((i) => `${i.path}: ${i.message}`).join('; ')}`,
                        issues,
                    );
                }
                const before = s.config;
                if (JSON.stringify(before) !== JSON.stringify(next)) {
                    s.docRev.cmd_config = (s.docRev.cmd_config ?? 0) + 1;
                }
                s.config = next;
                let portChanged = false;
                let relinked = false;
                if (next.onebot.ws_reverse_port !== inst.port) {
                    deps.publish({ ...inst, port: next.onebot.ws_reverse_port }, 'port_changed');
                    portChanged = true;
                }
                if (inst.link && astrbotLinkInputsChanged(before, next)) {
                    relinked = true;
                }
                const env = abEnvelope(s);
                return withMockDelay({
                    config: env.config,
                    revision: env.revision,
                    documents: env.documents,
                    restart_required: inst.state === 'running',
                    relinked,
                    port_changed: portChanged,
                });
            }
            if (inst.framework_id !== 'karin' || config.framework !== 'karin') {
                throw makeAppConfigError('unsupported', `该应用端暂不支持类型化配置: ${inst.framework_id}`);
            }
            const s = karinState(inst);
            if (baseRevision != null) {
                if (conflictOnce) {
                    conflictOnce = false;
                    s.docRev.config += 1;
                }
                if (baseRevision !== combined(s.docRev, KARIN_DOCS)) {
                    throw makeAppConfigError('conflict', '配置已被修改（config），请重新加载后再保存');
                }
            }
            const next = structuredClone(config.data);
            const issues = validateKarinConfig(next);
            if (issues.length) {
                throw makeAppConfigError(
                    'invalid',
                    `配置校验未通过：${issues.map((i) => `${i.path}: ${i.message}`).join('; ')}`,
                    issues,
                );
            }
            const before = s.config;
            const changed = changedDocs(before, next);
            for (const d of changed) {
                s.docRev[d] = (s.docRev[d] ?? 0) + 1;
                delete s.rawOverride[d];
            }
            s.config = next;

            let portChanged = false;
            let relinked = false;
            let restartRequired = false;
            if (next.env.http_port !== inst.port) {
                deps.publish({ ...inst, port: next.env.http_port }, 'port_changed');
                portChanged = true;
                restartRequired ||= inst.state === 'running';
            }
            if (inst.link && karinLinkInputsChanged(before.env, next.env)) {
                relinked = true;
            }
            if (inst.state === 'running' && changed.includes('redis')) restartRequired = true;

            const env = envelope(s);
            return withMockDelay({
                config: env.config,
                revision: env.revision,
                documents: env.documents,
                restart_required: restartRequired,
                relinked,
                port_changed: portChanged,
            });
        },

        listConfigDocuments: async (instanceId: string): Promise<AppConfigDocument[]> => {
            const inst = deps.require(instanceId);
            return withMockDelay(
                inst.framework_id === 'karin'
                    ? KARIN_DOCS
                    : inst.framework_id === 'astrbot'
                      ? ASTRBOT_DOCS
                      : NONEBOT2_DOCS,
            );
        },

        readConfigText: async (instanceId: string, docId: string): Promise<AppConfigText> => {
            const inst = requireInstalled(instanceId);
            if (docId.startsWith('plugin:') && inst.framework_id === 'astrbot') {
                const raw = astrbotRaw(inst);
                return withMockDelay({
                    doc_id: docId,
                    text: raw.text[docId] ?? ASTRBOT_PLUGIN_DEFAULTS,
                    revision: rev(raw.rev[docId] ?? 0),
                });
            }
            if (docId.startsWith('plugin:')) {
                const s = karinState(inst);
                return withMockDelay({
                    doc_id: docId,
                    text: s.rawOverride[docId] ?? '{\n  \n}\n',
                    revision: rev(s.docRev[docId] ?? 0),
                });
            }
            if (inst.framework_id === 'karin') {
                const s = karinState(inst);
                return withMockDelay({ doc_id: docId, text: karinDocText(s, docId), revision: rev(s.docRev[docId] ?? 0) });
            }
            if (inst.framework_id === 'astrbot') {
                const raw = astrbotRaw(inst);
                return withMockDelay({
                    doc_id: docId,
                    text: raw.text[docId] ?? '',
                    revision: rev(raw.rev[docId] ?? 0),
                });
            }
            let raw = rawStates.get(inst.id);
            if (!raw) {
                raw = { text: { ...NONEBOT2_TEXT }, rev: { env: 1, env_prod: 1, pyproject: 1 } };
                rawStates.set(inst.id, raw);
            }
            return withMockDelay({ doc_id: docId, text: raw.text[docId] ?? '', revision: rev(raw.rev[docId] ?? 0) });
        },

        writeConfigText: async (
            instanceId: string,
            docId: string,
            text: string,
            baseRevision: string | null,
        ): Promise<AppConfigText> => {
            const inst = requireInstalled(instanceId);
            if (docId.startsWith('plugin:')) {
                try {
                    JSON.parse(text);
                } catch (e) {
                    throw makeAppConfigError('invalid', `JSON 语法错误: ${(e as Error).message}`, [
                        { path: 'text', message: `JSON 语法错误: ${(e as Error).message}` },
                    ]);
                }
                if (inst.framework_id === 'astrbot') {
                    const raw = astrbotRaw(inst);
                    const current = rev(raw.rev[docId] ?? 0);
                    if (baseRevision != null && baseRevision !== current) {
                        throw makeAppConfigError('conflict', `配置已被修改（${docId}），请重新加载后再保存`);
                    }
                    raw.rev[docId] = (raw.rev[docId] ?? 0) + 1;
                    raw.text[docId] = text;
                    return withMockDelay({ doc_id: docId, text, revision: rev(raw.rev[docId]) });
                }
                const s = karinState(inst);
                const current = rev(s.docRev[docId] ?? 0);
                if (baseRevision != null && baseRevision !== current) {
                    throw makeAppConfigError('conflict', `配置已被修改（${docId}），请重新加载后再保存`);
                }
                s.docRev[docId] = (s.docRev[docId] ?? 0) + 1;
                s.rawOverride[docId] = text;
                return withMockDelay({ doc_id: docId, text, revision: rev(s.docRev[docId]) });
            }
            const docs =
                inst.framework_id === 'karin'
                    ? KARIN_DOCS
                    : inst.framework_id === 'astrbot'
                      ? ASTRBOT_DOCS
                      : NONEBOT2_DOCS;
            const doc = docs.find((d) => d.id === docId);
            if (!doc) throw makeAppConfigError('other', `未知的配置文档: ${docId}`);
            if (doc.format === 'json') {
                try {
                    JSON.parse(text);
                } catch (e) {
                    throw makeAppConfigError('invalid', `JSON 语法错误: ${(e as Error).message}`, [
                        { path: 'text', message: `JSON 语法错误: ${(e as Error).message}` },
                    ]);
                }
            }
            if (inst.framework_id === 'karin') {
                const s = karinState(inst);
                const current = rev(s.docRev[docId] ?? 0);
                if (baseRevision != null && baseRevision !== current) {
                    throw makeAppConfigError('conflict', `配置已被修改（${docId}），请重新加载后再保存`);
                }
                s.docRev[docId] = (s.docRev[docId] ?? 0) + 1;
                s.rawOverride[docId] = text;
                if (doc.format === 'json') {
                    const parsed = JSON.parse(text);
                    const c = s.config as unknown as Record<string, unknown>;
                    c[docId] = parsed;
                    delete s.rawOverride[docId];
                }
                return withMockDelay({ doc_id: docId, text, revision: rev(s.docRev[docId]) });
            }
            let raw = rawStates.get(inst.id);
            if (!raw) {
                raw =
                    inst.framework_id === 'astrbot'
                        ? { text: { ...ASTRBOT_TEXT }, rev: { cmd_config: 1 } }
                        : { text: { ...NONEBOT2_TEXT }, rev: { env: 1, env_prod: 1, pyproject: 1 } };
                rawStates.set(inst.id, raw);
            }
            const current = rev(raw.rev[docId] ?? 0);
            if (baseRevision != null && baseRevision !== current) {
                throw makeAppConfigError('conflict', `配置已被修改（${docId}），请重新加载后再保存`);
            }
            raw.rev[docId] = (raw.rev[docId] ?? 0) + 1;
            raw.text[docId] = text;
            return withMockDelay({ doc_id: docId, text, revision: rev(raw.rev[docId]) });
        },
    };
}

/** 浏览器控制台可调的开关 */
export const mockAppConfigControls = {
    appConfigConflictOnce: () => {
        conflictOnce = true;
    },
    reset: () => {
        karinStates.clear();
        nbStates.clear();
        abStates.clear();
        rawStates.clear();
        conflictOnce = false;
    },
};

if (typeof window !== 'undefined') {
    const w = window as unknown as { __ncdMock?: Record<string, unknown> };
    w.__ncdMock = { ...(w.__ncdMock ?? {}), ...mockAppConfigControls };
}
