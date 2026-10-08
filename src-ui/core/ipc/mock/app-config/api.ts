// 类型化 / 原始文本两条读写路径的框架分发：状态与保存细节在各框架文件里。

import type {
    AppConfigDocument,
    AppConfigText,
    AppConfigWriteResult,
    AppInstanceConfig,
    AppInstanceConfigEnvelope,
} from '../../types';
import { makeAppConfigError } from '../../../domain/apps/appConfigError';
import { withMockDelay } from '../bootstrap.mock';
import { docsOf } from './data';
import {
    armConflictOnce,
    clearConflictOnce,
    neobotState,
    rawReadFallback,
    rawStates,
    rawWriteFallback,
    rev,
    type MockAppConfigDeps,
} from './shared';
import {
    karinEnvelope,
    karinReadDocText,
    karinReadPluginText,
    karinState,
    karinWriteConfig,
    karinWriteDocText,
    karinWritePluginText,
    reset as resetKarin,
} from './karin';
import { nbReadConfig, nbWriteConfig, reset as resetNoneBot2 } from './nonebot2';
import {
    abReadConfig,
    abReadPluginText,
    abReadText,
    abWriteConfig,
    abWritePluginText,
    reset as resetAstrBot,
} from './astrbot';
import { mbReadConfig, mbWriteConfig, reset as resetMaiBot } from './maibot';
import { koReadConfig, koWriteConfig, reset as resetKoishi } from './koishi';
import {
    yzReadConfig,
    yzReadText,
    yzWriteConfig,
    yzWriteText,
    reset as resetYunzai,
} from './yunzai';

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
                return withMockDelay(nbReadConfig(inst));
            }
            if (inst.framework_id === 'astrbot') {
                return withMockDelay(abReadConfig(inst));
            }
            if (inst.framework_id === 'maibot') {
                return withMockDelay(mbReadConfig(inst));
            }
            if (inst.framework_id === 'koishi') {
                return withMockDelay(koReadConfig(inst));
            }
            if (inst.framework_id === 'yunzai') {
                return withMockDelay(yzReadConfig(inst));
            }
            if (inst.framework_id !== 'karin') {
                throw makeAppConfigError(
                    'unsupported',
                    `该应用端暂不支持类型化配置: ${inst.framework_id}`,
                );
            }
            return withMockDelay(karinEnvelope(karinState(inst)));
        },

        writeConfig: async (
            instanceId: string,
            config: AppInstanceConfig,
            baseRevision: string | null,
            _confId?: string | null,
        ): Promise<AppConfigWriteResult> => {
            const inst = requireInstalled(instanceId);
            if (inst.framework_id === 'nonebot2' && config.framework === 'nonebot2') {
                return nbWriteConfig(inst, config.data, baseRevision, deps);
            }
            if (inst.framework_id === 'astrbot' && config.framework === 'astrbot') {
                return abWriteConfig(inst, config.data, baseRevision, deps);
            }
            if (inst.framework_id === 'maibot' && config.framework === 'maibot') {
                return mbWriteConfig(inst, config.data, baseRevision, deps);
            }
            if (inst.framework_id === 'koishi' && config.framework === 'koishi') {
                return koWriteConfig(inst, config.data, baseRevision, deps);
            }
            if (inst.framework_id === 'yunzai' && config.framework === 'yunzai') {
                return yzWriteConfig(inst, config.data, baseRevision, deps);
            }
            if (inst.framework_id !== 'karin' || config.framework !== 'karin') {
                throw makeAppConfigError(
                    'unsupported',
                    `该应用端暂不支持类型化配置: ${inst.framework_id}`,
                );
            }
            return karinWriteConfig(inst, config.data, baseRevision, deps);
        },

        listConfigDocuments: async (instanceId: string): Promise<AppConfigDocument[]> => {
            const inst = deps.require(instanceId);
            return withMockDelay(docsOf(inst.framework_id));
        },

        readConfigText: async (instanceId: string, docId: string): Promise<AppConfigText> => {
            const inst = requireInstalled(instanceId);
            if (docId.startsWith('plugin:') && inst.framework_id === 'astrbot') {
                return withMockDelay(abReadPluginText(inst, docId));
            }
            if (inst.framework_id === 'yunzai') {
                return withMockDelay(yzReadText(inst, docId));
            }
            if (docId.startsWith('plugin:')) {
                return karinReadPluginText(inst, docId);
            }
            if (inst.framework_id === 'karin') {
                return karinReadDocText(inst, docId);
            }
            if (inst.framework_id === 'astrbot') {
                return withMockDelay(abReadText(inst, docId));
            }
            if (inst.framework_id === 'neobot') {
                const s = neobotState(inst);
                return withMockDelay({
                    doc_id: docId,
                    text: s.text[docId] ?? '',
                    revision: rev(s.rev[docId] ?? 0),
                });
            }
            const raw = rawReadFallback(inst);
            return withMockDelay({
                doc_id: docId,
                text: raw.text[docId] ?? '',
                revision: rev(raw.rev[docId] ?? 0),
            });
        },

        writeConfigText: async (
            instanceId: string,
            docId: string,
            text: string,
            baseRevision: string | null,
        ): Promise<AppConfigText> => {
            const inst = requireInstalled(instanceId);
            if (inst.framework_id === 'yunzai') {
                return withMockDelay(yzWriteText(inst, docId, text, baseRevision));
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
                    return withMockDelay(abWritePluginText(inst, docId, text, baseRevision));
                }
                return karinWritePluginText(inst, docId, text, baseRevision);
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
                return karinWriteDocText(inst, doc, text, baseRevision);
            }
            const raw = rawWriteFallback(inst);
            const current = rev(raw.rev[docId] ?? 0);
            if (baseRevision != null && baseRevision !== current) {
                throw makeAppConfigError(
                    'conflict',
                    `配置已被修改（${docId}），请重新加载后再保存`,
                );
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
        armConflictOnce();
    },
    reset: () => {
        resetKarin();
        resetNoneBot2();
        resetAstrBot();
        resetMaiBot();
        resetKoishi();
        resetYunzai();
        rawStates.clear();
        clearConflictOnce();
    },
};
