// Yunzai 假配置：config/config 九份 YAML 的类型化状态 + 原始文本双轨（类型化保存与原始 Tab 各推各的版本）。

import type {
    AppConfigText,
    AppConfigWriteResult,
    AppInstance,
    AppInstanceConfigEnvelope,
    YunzaiInstanceConfig,
} from '../../types';
import {
    validateYunzaiConfig,
    yunzaiDefaultConfig,
    yunzaiLinkInputsChanged,
    yunzaiRestartInputsChanged,
} from '../../../domain/apps/yunzaiConfig';
import { makeAppConfigError } from '../../../domain/apps/appConfigError';
import { withMockDelay } from '../bootstrap.mock';
import { YUNZAI_DOCS } from './data';
import { YUNZAI_PLUGIN_TEXT, YUNZAI_TEXT } from './texts';
import {
    combined,
    rawStates,
    rev,
    type MockAppConfigDeps,
    type MockRawState,
    type TypedState,
} from './shared';

const yzStates = new Map<string, TypedState<YunzaiInstanceConfig>>();

function yzState(instance: AppInstance): TypedState<YunzaiInstanceConfig> {
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

function yzEnvelope(s: TypedState<YunzaiInstanceConfig>): AppInstanceConfigEnvelope {
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

export function yzReadConfig(inst: AppInstance): AppInstanceConfigEnvelope {
    return yzEnvelope(yzState(inst));
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

export async function yzWriteConfig(
    inst: AppInstance,
    data: YunzaiInstanceConfig,
    baseRevision: string | null,
    deps: MockAppConfigDeps,
): Promise<AppConfigWriteResult> {
    const s = yzState(inst);
    if (baseRevision != null && baseRevision !== combined(s.docRev, YUNZAI_DOCS)) {
        throw makeAppConfigError('conflict', '配置已被修改（other），请重新加载后再保存');
    }
    const next = structuredClone(data);
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
    s.config = {
        ...next,
        server: {
            ...next.server,
            extra_auth_headers: before.server.extra_auth_headers,
        },
    };
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

/** 原始文件 Tab 的状态与类型化 docRev 分开递增；插件文档没写过就回通用样本 */
function yunzaiRaw(inst: AppInstance): MockRawState {
    let raw = rawStates.get(inst.id);
    if (!raw) {
        const r: Record<string, number> = {};
        for (const d of YUNZAI_DOCS) r[d.id] = 1;
        raw = { text: { ...YUNZAI_TEXT }, rev: r };
        rawStates.set(inst.id, raw);
    }
    return raw;
}

export function yzReadText(inst: AppInstance, docId: string): AppConfigText {
    const raw = yunzaiRaw(inst);
    return {
        doc_id: docId,
        text: raw.text[docId] ?? (docId.startsWith('plugin:') ? YUNZAI_PLUGIN_TEXT : ''),
        revision: rev(raw.rev[docId] ?? 0),
    };
}

/** 原始 Tab 保存会同时推类型化 docRev（非 plugin 文档），让表单侧感知外部改动 */
export function yzWriteText(
    inst: AppInstance,
    docId: string,
    text: string,
    baseRevision: string | null,
): AppConfigText {
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
    return { doc_id: docId, text, revision: rev(raw.rev[docId]) };
}

export function reset() {
    yzStates.clear();
}
