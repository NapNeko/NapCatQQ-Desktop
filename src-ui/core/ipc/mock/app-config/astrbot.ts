// AstrBot 假配置：cmd_config.json 类型化状态 + 插件配置原始文本（缺省按 schema 默认值兜底）。

import type {
    AppConfigText,
    AppConfigWriteResult,
    AppInstance,
    AppInstanceConfigEnvelope,
    AstrBotInstanceConfig,
} from '../../types';
import {
    astrbotDefaultConfig,
    astrbotLinkInputsChanged,
    validateAstrBotConfig,
} from '../../../domain/apps/astrbotConfig';
import { makeAppConfigError } from '../../../domain/apps/appConfigError';
import { withMockDelay } from '../bootstrap.mock';
import { ASTRBOT_DOCS } from './data';
import { ASTRBOT_PLUGIN_DEFAULTS, ASTRBOT_TEXT } from './texts';
import {
    combined,
    rawStates,
    rev,
    type MockAppConfigDeps,
    type MockRawState,
    type TypedState,
} from './shared';

const abStates = new Map<string, TypedState<AstrBotInstanceConfig>>();

function abState(instance: AppInstance): TypedState<AstrBotInstanceConfig> {
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

function abEnvelope(s: TypedState<AstrBotInstanceConfig>): AppInstanceConfigEnvelope {
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

export function abReadConfig(inst: AppInstance): AppInstanceConfigEnvelope {
    return abEnvelope(abState(inst));
}

export async function abWriteConfig(
    inst: AppInstance,
    data: AstrBotInstanceConfig,
    baseRevision: string | null,
    deps: MockAppConfigDeps,
): Promise<AppConfigWriteResult> {
    const s = abState(inst);
    if (baseRevision != null && baseRevision !== combined(s.docRev, ASTRBOT_DOCS)) {
        throw makeAppConfigError('conflict', '配置已被修改（cmd_config），请重新加载后再保存');
    }
    const next = structuredClone(data);
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

function astrbotRaw(inst: AppInstance): MockRawState {
    let raw = rawStates.get(inst.id);
    if (!raw) {
        raw = { text: { ...ASTRBOT_TEXT }, rev: { cmd_config: 1 } };
        rawStates.set(inst.id, raw);
    }
    return raw;
}

export function abReadText(inst: AppInstance, docId: string): AppConfigText {
    const raw = astrbotRaw(inst);
    return {
        doc_id: docId,
        text: raw.text[docId] ?? '',
        revision: rev(raw.rev[docId] ?? 0),
    };
}

/** 插件配置 Tab：缺文件时返回 schema 默认值形状（`plugin:` 前缀在 api.ts 分发） */
export function abReadPluginText(inst: AppInstance, docId: string): AppConfigText {
    const raw = astrbotRaw(inst);
    return {
        doc_id: docId,
        text: raw.text[docId] ?? ASTRBOT_PLUGIN_DEFAULTS,
        revision: rev(raw.rev[docId] ?? 0),
    };
}

/** 写插件配置：JSON 语法检查在 api.ts 完成，这里只做冲突检测与落库 */
export function abWritePluginText(
    inst: AppInstance,
    docId: string,
    text: string,
    baseRevision: string | null,
): AppConfigText {
    const raw = astrbotRaw(inst);
    const current = rev(raw.rev[docId] ?? 0);
    if (baseRevision != null && baseRevision !== current) {
        throw makeAppConfigError('conflict', `配置已被修改（${docId}），请重新加载后再保存`);
    }
    raw.rev[docId] = (raw.rev[docId] ?? 0) + 1;
    raw.text[docId] = text;
    return { doc_id: docId, text, revision: rev(raw.rev[docId]) };
}

export function reset() {
    abStates.clear();
}
