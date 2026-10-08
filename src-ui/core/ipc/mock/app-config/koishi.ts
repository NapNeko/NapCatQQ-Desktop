// Koishi 假配置：koishi.yml 类型化状态（版本号只有 koishi 一份参与合并修订）。

import type {
    AppConfigWriteResult,
    AppInstance,
    AppInstanceConfigEnvelope,
    KoishiInstanceConfig,
} from '../../types';
import { koishiServer, validateKoishiConfig } from '../../../domain/apps/koishiConfig';
import { makeAppConfigError } from '../../../domain/apps/appConfigError';
import { withMockDelay } from '../bootstrap.mock';
import { koishiMockConfig } from '../koishi.mock';
import { KOISHI_DOCS } from './data';
import { combined, rev, type MockAppConfigDeps, type TypedState } from './shared';

const koStates = new Map<string, TypedState<KoishiInstanceConfig>>();

function koState(instance: AppInstance): TypedState<KoishiInstanceConfig> {
    let s = koStates.get(instance.id);
    if (!s) {
        s = { config: koishiMockConfig(instance.port), docRev: { koishi: 1, env: 1, package: 1 } };
        koStates.set(instance.id, s);
    }
    return s;
}

function koEnvelope(s: TypedState<KoishiInstanceConfig>): AppInstanceConfigEnvelope {
    return {
        config: { framework: 'koishi', data: structuredClone(s.config) },
        revision: combined(s.docRev, KOISHI_DOCS.slice(0, 1)),
        documents: [{ doc_id: 'koishi', revision: rev(s.docRev.koishi ?? 0), hot_reload: false }],
    };
}

export function koReadConfig(inst: AppInstance): AppInstanceConfigEnvelope {
    return koEnvelope(koState(inst));
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

export async function koWriteConfig(
    inst: AppInstance,
    data: KoishiInstanceConfig,
    baseRevision: string | null,
    deps: MockAppConfigDeps,
): Promise<AppConfigWriteResult> {
    const s = koState(inst);
    if (baseRevision != null && baseRevision !== combined(s.docRev, KOISHI_DOCS.slice(0, 1))) {
        throw makeAppConfigError('conflict', '配置已被修改（koishi.yml），请重新加载后再保存');
    }
    const next = structuredClone(data);
    const issues = validateKoishiConfig(next);
    if (issues.length) {
        throw makeAppConfigError(
            'invalid',
            `配置校验未通过：${issues.map((i) => `${i.path}: ${i.message}`).join('; ')}`,
            issues,
        );
    }
    if (JSON.stringify(s.config) !== JSON.stringify(next))
        s.docRev.koishi = (s.docRev.koishi ?? 0) + 1;
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

export function reset() {
    koStates.clear();
}
