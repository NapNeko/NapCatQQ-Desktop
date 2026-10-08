// NoneBot2 假配置：.env / .env.prod / pyproject 三份文档的类型化状态与保存。

import type {
    AppConfigWriteResult,
    AppInstance,
    AppInstanceConfigEnvelope,
    NoneBot2InstanceConfig,
} from '../../types';
import {
    nonebot2DefaultConfig,
    nonebot2LinkInputsChanged,
    validateNoneBot2Config,
} from '../../../domain/apps/nonebot2Config';
import { makeAppConfigError } from '../../../domain/apps/appConfigError';
import { withMockDelay } from '../bootstrap.mock';
import { NONEBOT2_DOCS } from './data';
import { combined, rev, type MockAppConfigDeps, type TypedState } from './shared';

const nbStates = new Map<string, TypedState<NoneBot2InstanceConfig>>();

function nbState(instance: AppInstance): TypedState<NoneBot2InstanceConfig> {
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

function nbEnvelope(s: TypedState<NoneBot2InstanceConfig>): AppInstanceConfigEnvelope {
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

export function nbReadConfig(inst: AppInstance): AppInstanceConfigEnvelope {
    return nbEnvelope(nbState(inst));
}

export async function nbWriteConfig(
    inst: AppInstance,
    data: NoneBot2InstanceConfig,
    baseRevision: string | null,
    deps: MockAppConfigDeps,
): Promise<AppConfigWriteResult> {
    const s = nbState(inst);
    if (baseRevision != null && baseRevision !== combined(s.docRev, NONEBOT2_DOCS)) {
        throw makeAppConfigError('conflict', '配置已被修改（config），请重新加载后再保存');
    }
    const next = structuredClone(data);
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

export function reset() {
    nbStates.clear();
}
