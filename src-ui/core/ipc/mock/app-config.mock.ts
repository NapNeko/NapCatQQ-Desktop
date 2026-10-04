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
    KoishiInstanceConfig,
    MaiBotInstanceConfig,
    NoneBot2InstanceConfig,
    YunzaiInstanceConfig,
} from '../types';
import {
    validateYunzaiConfig,
    yunzaiDefaultConfig,
    yunzaiLinkInputsChanged,
    yunzaiRestartInputsChanged,
} from '../../domain/apps/yunzaiConfig';
import { maibotDefaultConfig, validateMaiBotConfig } from '../../domain/apps/maibotConfig';
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
import { koishiServer, validateKoishiConfig } from '../../domain/apps/koishiConfig';
import { koishiMockConfig } from './koishi.mock';
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

/** NeoBot 两份 TOML（对齐后端 neobot_config_documents 的 id / rel_path / hot_reload） */
const NEOBOT_DOCS: AppConfigDocument[] = [
    {
        id: 'adapter',
        label: 'OneBot 对接（data/config.toml）',
        rel_path: 'data/config.toml',
        format: 'toml',
        hot_reload: true,
    },
    {
        id: 'dashboard',
        label: '网页面板（plugins_data/dashboard/config.toml）',
        rel_path: 'plugins_data/dashboard/config.toml',
        format: 'toml',
        hot_reload: true,
    },
];

const MAIBOT_DOCS: AppConfigDocument[] = [
    { id: 'bot_config', label: '主配置 bot_config.toml', rel_path: 'config/bot_config.toml', format: 'toml', hot_reload: true },
    { id: 'model_config', label: '模型配置 model_config.toml', rel_path: 'config/model_config.toml', format: 'toml', hot_reload: true },
    {
        id: 'adapter_config',
        label: 'NapCat 适配器 config.toml',
        rel_path: 'plugins/MaiBot-Napcat-Adapter/config.toml',
        format: 'toml',
        hot_reload: true,
    },
];

const KOISHI_DOCS: AppConfigDocument[] = [
    { id: 'koishi', label: 'koishi.yml', rel_path: 'koishi.yml', format: 'yaml', hot_reload: false },
    { id: 'env', label: '.env', rel_path: '.env', format: 'dot_env', hot_reload: false },
    { id: 'package', label: 'package.json', rel_path: 'package.json', format: 'json', hot_reload: false },
];

const KOISHI_TEXT: Record<string, string> = {
    koishi: 'plugins:\n  group:server:\n    server:cj4vi7:\n      port: 23140\n      host: 127.0.0.1\n',
    env: 'GITHUB_MIRROR = https://ghproxy.com/https://github.com\n',
    package: '{\n  "name": "@koishijs/boilerplate",\n  "version": "1.16.0"\n}\n',
};

const MAIBOT_TEXT: Record<string, string> = {
    bot_config: '[inner]\nversion = "8.14.40"\n\n[webui]\nport = 23001\n\n[maim_message]\nws_server_port = 23002\n',
    model_config: '[inner]\nversion = "1.17.9"\n',
    adapter_config: '[plugin]\nconfig_version = "0.1.0"\nenabled = false\n',
};

/** 和后端 yunzai_config_documents 一致：config/config 下九份，server / redis / db 只在启动时读 */
const YUNZAI_DOCS: AppConfigDocument[] = ['bot', 'other', 'group', 'server', 'redis', 'renderer', 'db', 'milky', 'satori'].map(
    (name) => ({
        id: name,
        label: `${name}.yaml`,
        rel_path: `config/config/${name}.yaml`,
        format: 'yaml',
        hot_reload: !['server', 'redis', 'db'].includes(name),
    }),
);

const YUNZAI_TEXT: Record<string, string> = {
    bot: '# 日志等级\nlog_level: info\n# 渲染用的浏览器，留空用装时下载的\nchromium_path:\n',
    other: '# 主人QQ号\nmasterQQ:\n# Bot号:主人号\nmaster:\n',
    group: 'default:\n  groupCD: 500\n  singleCD: 2000\n  onlyReplyAt: 0\n  botAlias:\n    - 云崽\n    - 云宝\n',
    server: 'url: http://localhost:2536\nport: 2536\nredirect: https://git.trss.me/Yunzai\nauth:\n',
    redis: 'path: redis-server\nhost: 127.0.0.1\nport: 2537\nusername:\npassword:\ndb: 0\n',
    renderer: '# 渲染后端，留空自动\nname:\n',
    db: 'dialect: sqlite\nstorage: data/db/data.db\nlogging: false\n',
    milky: '# Milky 协议端，Desktop 不用\n',
    satori: '# Satori 协议端，Desktop 不用\n',
};

function docsOf(frameworkId: string): AppConfigDocument[] {
    switch (frameworkId) {
        case 'karin':
            return KARIN_DOCS;
        case 'astrbot':
            return ASTRBOT_DOCS;
        case 'maibot':
            return MAIBOT_DOCS;
        case 'koishi':
            return KOISHI_DOCS;
        case 'yunzai':
            return YUNZAI_DOCS;
        case 'neobot':
            return NEOBOT_DOCS;
        default:
            return NONEBOT2_DOCS;
    }
}

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

const NEOBOT_TEXT: Record<string, string> = {
    adapter: `[bot]
qq = 10001

[adapter]
mode = "onebot"
reverse_ws_host = "127.0.0.1"
reverse_ws_port = 8080
reverse_ws_access_token = "ncd-mock-token"
`,
    dashboard: `host = "127.0.0.1"
port = 9981
`,
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
const mbStates = new Map<string, { config: MaiBotInstanceConfig; docRev: Record<string, number> }>();
const koStates = new Map<string, { config: KoishiInstanceConfig; docRev: Record<string, number> }>();
const yzStates = new Map<string, { config: YunzaiInstanceConfig; docRev: Record<string, number> }>();
const rawStates = new Map<string, { text: Record<string, string>; rev: Record<string, number> }>();
function neobotState(inst: AppInstance): { text: Record<string, string>; rev: Record<string, number> } {
    let s = rawStates.get(inst.id);
    if (!s) {
        s = { text: { ...NEOBOT_TEXT }, rev: { adapter: 1, dashboard: 1 } };
        rawStates.set(inst.id, s);
    }
    return s;
}
let conflictOnce = false;

function mbState(instance: AppInstance) {
    let s = mbStates.get(instance.id);
    if (!s) {
        const docRev: Record<string, number> = {};
        for (const d of MAIBOT_DOCS) docRev[d.id] = 1;
        s = { config: maibotDefaultConfig(instance.port), docRev };
        s.config.webui_token = 'Ncd_mockMockMockMockMock';
        if (instance.link && s.config.adapter) {
            s.config.adapter = { ...s.config.adapter, enabled: true, napcat_port: 23456, has_token: true };
        }
        mbStates.set(instance.id, s);
    }
    return s;
}

function mbEnvelope(s: { config: MaiBotInstanceConfig; docRev: Record<string, number> }): AppInstanceConfigEnvelope {
    return {
        config: { framework: 'maibot', data: structuredClone(s.config) },
        revision: combined(s.docRev, MAIBOT_DOCS),
        documents: MAIBOT_DOCS.map((d) => ({
            doc_id: d.id,
            revision: rev(s.docRev[d.id] ?? 0),
            hot_reload: d.hot_reload,
        })),
    };
}

function koState(instance: AppInstance) {
    let s = koStates.get(instance.id);
    if (!s) {
        s = { config: koishiMockConfig(instance.port), docRev: { koishi: 1, env: 1, package: 1 } };
        koStates.set(instance.id, s);
    }
    return s;
}

function koEnvelope(s: { config: KoishiInstanceConfig; docRev: Record<string, number> }): AppInstanceConfigEnvelope {
    return {
        config: { framework: 'koishi', data: structuredClone(s.config) },
        revision: combined(s.docRev, KOISHI_DOCS.slice(0, 1)),
        documents: [{ doc_id: 'koishi', revision: rev(s.docRev.koishi ?? 0), hot_reload: false }],
    };
}

export function peekKoishiConfig(instance: AppInstance): KoishiInstanceConfig {
    return structuredClone(koState(instance).config);
}

/** 对接、商店装卸这些后端直接改 koishi.yml 的操作在 mock 里走这里 */
export function editKoishiConfig(
    instance: AppInstance,
    edit: (cfg: KoishiInstanceConfig) => KoishiInstanceConfig,
): void {
    const s = koState(instance);
    s.config = edit(structuredClone(s.config));
    s.docRev.koishi = (s.docRev.koishi ?? 0) + 1;
}

/** 运行期 mock 读当前落盘的配置（MCP 服务列表、提供商） */
export function peekMaiBotConfig(instance: AppInstance): MaiBotInstanceConfig {
    return structuredClone(mbState(instance).config);
}

/** 对接后假装适配器配置被写了（真机由后端 apply_link 写 config.toml） */
export function syncMaiBotLink(instance: AppInstance, linked: boolean): void {
    const s = mbState(instance);
    if (!s.config.adapter) return;
    s.config.adapter = {
        ...s.config.adapter,
        enabled: linked,
        napcat_port: linked ? 23456 : s.config.adapter.napcat_port,
        has_token: linked || s.config.adapter.has_token,
    };
    s.docRev.adapter_config = (s.docRev.adapter_config ?? 0) + 1;
}

function yzState(instance: AppInstance) {
    let s = yzStates.get(instance.id);
    if (!s) {
        const docRev: Record<string, number> = {};
        for (const d of YUNZAI_DOCS) docRev[d.id] = 1;
        s = { config: yunzaiDefaultConfig(instance.port), docRev };
        s.config.redis.path = `${instance.install_dir}/../../../tools/redis/redis-server.exe`;
        if (instance.link) s.config.server.access_token = 'mockmockmockmockmockmock';
        yzStates.set(instance.id, s);
    }
    return s;
}

function yzEnvelope(s: { config: YunzaiInstanceConfig; docRev: Record<string, number> }): AppInstanceConfigEnvelope {
    return {
        config: { framework: 'yunzai', data: structuredClone(s.config) },
        revision: combined(s.docRev, YUNZAI_DOCS),
        documents: YUNZAI_DOCS.map((d) => ({
            doc_id: d.id,
            revision: rev(s.docRev[d.id] ?? 0),
            hot_reload: d.hot_reload,
        })),
    };
}

/** 对接后假装 server.yaml 被写了：auth 只剩 Authorization: Bearer，端口是实例口 */
export function syncYunzaiLink(instance: AppInstance, linked: boolean): void {
    if (!linked) return;
    const s = yzState(instance);
    s.config.server = {
        ...s.config.server,
        port: instance.port,
        access_token: s.config.server.access_token || 'mockmockmockmockmockmock',
        extra_auth_headers: [],
    };
    s.docRev.server = (s.docRev.server ?? 0) + 1;
}

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

const YUNZAI_PLUGIN_TEXT = '# 插件自己的配置，改完插件一般会自己重新读\nenable: true\n';

function yunzaiRaw(inst: AppInstance) {
    let raw = rawStates.get(inst.id);
    if (!raw) {
        const r: Record<string, number> = {};
        for (const d of YUNZAI_DOCS) r[d.id] = 1;
        raw = { text: { ...YUNZAI_TEXT }, rev: r };
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
            if (inst.framework_id === 'maibot') {
                return withMockDelay(mbEnvelope(mbState(inst)));
            }
            if (inst.framework_id === 'koishi') {
                return withMockDelay(koEnvelope(koState(inst)));
            }
            if (inst.framework_id === 'yunzai') {
                return withMockDelay(yzEnvelope(yzState(inst)));
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
            _confId?: string | null,
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
                    restart_required: false,
                    relinked,
                    port_changed: portChanged,
                });
            }
            if (inst.framework_id === 'maibot' && config.framework === 'maibot') {
                const s = mbState(inst);
                if (baseRevision != null && baseRevision !== combined(s.docRev, MAIBOT_DOCS)) {
                    throw makeAppConfigError('conflict', '配置已被修改（bot_config），请重新加载后再保存');
                }
                const next = structuredClone(config.data);
                const issues = validateMaiBotConfig(next);
                if (issues.length) {
                    throw makeAppConfigError(
                        'invalid',
                        `配置校验未通过：${issues.map((i) => `${i.path}: ${i.message}`).join('; ')}`,
                        issues,
                    );
                }
                const before = s.config;
                const changed = (a: unknown, b: unknown) => JSON.stringify(a) !== JSON.stringify(b);
                if (changed(before.bot, next.bot)) s.docRev.bot_config = (s.docRev.bot_config ?? 0) + 1;
                if (changed(before.models, next.models)) {
                    s.docRev.model_config = (s.docRev.model_config ?? 0) + 1;
                }
                if (changed(before.adapter?.chat, next.adapter?.chat)) {
                    s.docRev.adapter_config = (s.docRev.adapter_config ?? 0) + 1;
                }
                // 和后端一样：只在启动时读的字段（端口、日志、插件运行时…）改了才提示重启
                const restartFields =
                    before.bot.webui.port !== next.bot.webui.port
                    || changed(before.bot.maim_message, next.bot.maim_message)
                    || changed(before.bot.log, next.bot.log)
                    || before.bot.plugin_runtime.enabled !== next.bot.plugin_runtime.enabled;
                // 只读字段以落盘为准
                s.config = { ...next, webui_token: before.webui_token, adapter: next.adapter && before.adapter
                    ? { ...before.adapter, chat: next.adapter.chat }
                    : before.adapter };
                let portChanged = false;
                if (next.bot.webui.port !== inst.port) {
                    deps.publish({ ...inst, port: next.bot.webui.port }, 'port_changed');
                    portChanged = true;
                }
                const env = mbEnvelope(s);
                return withMockDelay({
                    config: env.config,
                    revision: env.revision,
                    documents: env.documents,
                    restart_required: restartFields && inst.state === 'running',
                    relinked: false,
                    port_changed: portChanged,
                });
            }
            if (inst.framework_id === 'koishi' && config.framework === 'koishi') {
                const s = koState(inst);
                if (baseRevision != null && baseRevision !== combined(s.docRev, KOISHI_DOCS.slice(0, 1))) {
                    throw makeAppConfigError('conflict', '配置已被修改（koishi.yml），请重新加载后再保存');
                }
                const next = structuredClone(config.data);
                const issues = validateKoishiConfig(next);
                if (issues.length) {
                    throw makeAppConfigError(
                        'invalid',
                        `配置校验未通过：${issues.map((i) => `${i.path}: ${i.message}`).join('; ')}`,
                        issues,
                    );
                }
                if (JSON.stringify(s.config) !== JSON.stringify(next)) s.docRev.koishi = (s.docRev.koishi ?? 0) + 1;
                s.config = next;
                const port = koishiServer(next).port;
                let portChanged = false;
                if (port !== inst.port) {
                    deps.publish({ ...inst, port }, 'port_changed');
                    portChanged = true;
                }
                const env = koEnvelope(s);
                return withMockDelay({
                    config: env.config,
                    revision: env.revision,
                    documents: env.documents,
                    // 跑着的时候后端走控制台，当场生效
                    restart_required: false,
                    relinked: portChanged && !!inst.link,
                    port_changed: portChanged,
                });
            }
            if (inst.framework_id === 'yunzai' && config.framework === 'yunzai') {
                const s = yzState(inst);
                if (baseRevision != null && baseRevision !== combined(s.docRev, YUNZAI_DOCS)) {
                    throw makeAppConfigError('conflict', '配置已被修改（other），请重新加载后再保存');
                }
                const next = structuredClone(config.data);
                const issues = validateYunzaiConfig(next);
                if (issues.length) {
                    throw makeAppConfigError(
                        'invalid',
                        `配置校验未通过：${issues.map((i) => `${i.path}: ${i.message}`).join('; ')}`,
                        issues,
                    );
                }
                const before = s.config;
                const changed = (a: unknown, b: unknown) => JSON.stringify(a) !== JSON.stringify(b);
                for (const key of ['bot', 'other', 'group', 'server', 'redis', 'renderer'] as const) {
                    if (changed(before[key], next[key])) s.docRev[key] = (s.docRev[key] ?? 0) + 1;
                }
                // auth 里别的头是读出来给提示的，不从表单写回
                s.config = { ...next, server: { ...next.server, extra_auth_headers: before.server.extra_auth_headers } };
                let portChanged = false;
                if (next.server.port !== inst.port) {
                    deps.publish({ ...inst, port: next.server.port }, 'port_changed');
                    portChanged = true;
                }
                const env = yzEnvelope(s);
                return withMockDelay({
                    config: env.config,
                    revision: env.revision,
                    documents: env.documents,
                    restart_required: yunzaiRestartInputsChanged(before, next) && inst.state === 'running',
                    relinked: !!inst.link && yunzaiLinkInputsChanged(before, next),
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
            return withMockDelay(docsOf(inst.framework_id));
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
            if (inst.framework_id === 'yunzai') {
                const raw = yunzaiRaw(inst);
                return withMockDelay({
                    doc_id: docId,
                    text: raw.text[docId] ?? (docId.startsWith('plugin:') ? YUNZAI_PLUGIN_TEXT : ''),
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
            if (inst.framework_id === 'neobot') {
                const s = neobotState(inst);
                return withMockDelay({
                    doc_id: docId,
                    text: s.text[docId] ?? '',
                    revision: rev(s.rev[docId] ?? 0),
                });
            }
            let raw = rawStates.get(inst.id);
            if (!raw) {
                raw =
                    inst.framework_id === 'maibot'
                        ? { text: { ...MAIBOT_TEXT }, rev: { bot_config: 1, model_config: 1, adapter_config: 1 } }
                        : inst.framework_id === 'koishi'
                          ? { text: { ...KOISHI_TEXT }, rev: { koishi: 1, env: 1, package: 1 } }
                          : { text: { ...NONEBOT2_TEXT }, rev: { env: 1, env_prod: 1, pyproject: 1 } };
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
            if (inst.framework_id === 'yunzai') {
                const raw = yunzaiRaw(inst);
                const current = rev(raw.rev[docId] ?? 0);
                if (baseRevision != null && baseRevision !== current) {
                    throw makeAppConfigError('conflict', `配置已被修改（${docId}），请重新加载后再保存`);
                }
                raw.rev[docId] = (raw.rev[docId] ?? 0) + 1;
                raw.text[docId] = text;
                if (!docId.startsWith('plugin:')) {
                    const s = yzState(inst);
                    s.docRev[docId] = (s.docRev[docId] ?? 0) + 1;
                }
                return withMockDelay({ doc_id: docId, text, revision: rev(raw.rev[docId]) });
            }
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
            const doc = docsOf(inst.framework_id).find((d) => d.id === docId);
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
                        : inst.framework_id === 'maibot'
                          ? { text: { ...MAIBOT_TEXT }, rev: { bot_config: 1, model_config: 1, adapter_config: 1 } }
                          : inst.framework_id === 'koishi'
                            ? { text: { ...KOISHI_TEXT }, rev: { koishi: 1, env: 1, package: 1 } }
                            : inst.framework_id === 'neobot'
                              ? { text: { ...NEOBOT_TEXT }, rev: { adapter: 1, dashboard: 1 } }
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
        mbStates.clear();
        koStates.clear();
        yzStates.clear();
        rawStates.clear();
        conflictOnce = false;
    },
};

if (typeof window !== 'undefined') {
    const w = window as unknown as { __ncdMock?: Record<string, unknown> };
    w.__ncdMock = { ...(w.__ncdMock ?? {}), ...mockAppConfigControls };
}
