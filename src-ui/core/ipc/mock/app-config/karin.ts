// Karin 假配置：seed（.env 注释、master）、逐文档版本状态、类型化保存与原始 Tab 读写。

import type {
    AppConfigDocument,
    AppConfigWriteResult,
    AppInstance,
    AppInstanceConfigEnvelope,
    KarinInstanceConfig,
} from '../../types';
import {
    karinDefaultConfig,
    karinEnvToDotenv,
    karinLinkInputsChanged,
    validateKarinConfig,
} from '../../../domain/apps/karinConfig';
import { makeAppConfigError } from '../../../domain/apps/appConfigError';
import { withMockDelay } from '../bootstrap.mock';
import { KARIN_DOCS } from './data';
import { combined, consumeConflictOnce, rev, type MockAppConfigDeps } from './shared';

interface KarinState {
    config: KarinInstanceConfig;
    /** 逐文档版本号（整数递增，渲染成 `mock-r<n>`） */
    docRev: Record<string, number>;
    /** 原始文件 Tab 手改过的文本（优先于从类型化配置渲染） */
    rawOverride: Record<string, string>;
}

const karinStates = new Map<string, KarinState>();

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
        {
            key: 'LOG_API_MAX_CONNECTIONS',
            value: '5',
            comment: '日志实时Api最多支持同时连接数（旧键）',
        },
    ];
    config.config.master = ['console', '10001'];
    const docRev: Record<string, number> = {};
    for (const d of KARIN_DOCS) docRev[d.id] = 1;
    return { config, docRev, rawOverride: {} };
}

export function karinState(instance: AppInstance): KarinState {
    let s = karinStates.get(instance.id);
    if (!s) {
        s = seedKarin(instance);
        karinStates.set(instance.id, s);
    }
    return s;
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

export function karinEnvelope(s: KarinState): AppInstanceConfigEnvelope {
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

export async function karinWriteConfig(
    inst: AppInstance,
    data: KarinInstanceConfig,
    baseRevision: string | null,
    deps: MockAppConfigDeps,
): Promise<AppConfigWriteResult> {
    const s = karinState(inst);
    if (baseRevision != null) {
        if (consumeConflictOnce()) {
            s.docRev.config += 1;
        }
        if (baseRevision !== combined(s.docRev, KARIN_DOCS)) {
            throw makeAppConfigError('conflict', '配置已被修改（config），请重新加载后再保存');
        }
    }
    const next = structuredClone(data);
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

    const env = karinEnvelope(s);
    return withMockDelay({
        config: env.config,
        revision: env.revision,
        documents: env.documents,
        restart_required: restartRequired,
        relinked,
        port_changed: portChanged,
    });
}

/** 原始 Tab：插件配置兜底（没手改过就回一段空 JSON 壳） */
export async function karinReadPluginText(inst: AppInstance, docId: string) {
    const s = karinState(inst);
    return withMockDelay({
        doc_id: docId,
        text: s.rawOverride[docId] ?? '{\n  \n}\n',
        revision: rev(s.docRev[docId] ?? 0),
    });
}

export async function karinReadDocText(inst: AppInstance, docId: string) {
    const s = karinState(inst);
    return withMockDelay({
        doc_id: docId,
        text: karinDocText(s, docId),
        revision: rev(s.docRev[docId] ?? 0),
    });
}

/** 原始 Tab 保存插件配置：只存文本，不回写类型化配置 */
export async function karinWritePluginText(
    inst: AppInstance,
    docId: string,
    text: string,
    baseRevision: string | null,
) {
    const s = karinState(inst);
    const current = rev(s.docRev[docId] ?? 0);
    if (baseRevision != null && baseRevision !== current) {
        throw makeAppConfigError('conflict', `配置已被修改（${docId}），请重新加载后再保存`);
    }
    s.docRev[docId] = (s.docRev[docId] ?? 0) + 1;
    s.rawOverride[docId] = text;
    return withMockDelay({ doc_id: docId, text, revision: rev(s.docRev[docId]) });
}

/** 原始 Tab 保存内置文档：JSON 文档手改后同步回类型化配置并清掉覆盖文本 */
export async function karinWriteDocText(
    inst: AppInstance,
    doc: AppConfigDocument,
    text: string,
    baseRevision: string | null,
) {
    const docId = doc.id;
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

export function reset() {
    karinStates.clear();
}
