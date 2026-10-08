// 跨框架共用的 mock 状态件：版本号渲染、合并修订号、原始文本状态表、冲突模拟开关。

import type { AppConfigDocument, AppInstance } from '../../types';
import { ASTRBOT_TEXT, KOISHI_TEXT, MAIBOT_TEXT, NEOBOT_TEXT, NONEBOT2_TEXT } from './texts';

export const rev = (n: number) => `mock-r${n}`;

export function combined(docRev: Record<string, number>, docs: AppConfigDocument[]): string {
    return `mock-${docs.map((d) => `${d.id}${docRev[d.id] ?? 0}`).join('.')}`;
}

/** 类型化配置的每实例内存状态（版本号逐文档整数递增，渲染成 `mock-r<n>`） */
export interface TypedState<C> {
    config: C;
    docRev: Record<string, number>;
}

export interface MockRawState {
    text: Record<string, string>;
    rev: Record<string, number>;
}

/** 原始文件 Tab 的状态表：astrbot / neobot / maibot / koishi / nonebot2 共用（键是 instance.id） */
export const rawStates = new Map<string, MockRawState>();

export function neobotState(inst: AppInstance): MockRawState {
    let s = rawStates.get(inst.id);
    if (!s) {
        s = { text: { ...NEOBOT_TEXT }, rev: { adapter: 1, dashboard: 1 } };
        rawStates.set(inst.id, s);
    }
    return s;
}

/** readConfigText 兜底种子：走到这里的只有 maibot / koishi / nonebot2（其余框架已提前返回） */
export function rawReadFallback(inst: AppInstance): MockRawState {
    let raw = rawStates.get(inst.id);
    if (!raw) {
        raw =
            inst.framework_id === 'maibot'
                ? {
                      text: { ...MAIBOT_TEXT },
                      rev: { bot_config: 1, model_config: 1, adapter_config: 1 },
                  }
                : inst.framework_id === 'koishi'
                  ? { text: { ...KOISHI_TEXT }, rev: { koishi: 1, env: 1, package: 1 } }
                  : {
                        text: { ...NONEBOT2_TEXT },
                        rev: { env: 1, env_prod: 1, pyproject: 1 },
                    };
        rawStates.set(inst.id, raw);
    }
    return raw;
}

/** writeConfigText 兜底种子：比读侧多 astrbot / neobot 两支（写侧没有对它们的提前返回） */
export function rawWriteFallback(inst: AppInstance): MockRawState {
    let raw = rawStates.get(inst.id);
    if (!raw) {
        raw =
            inst.framework_id === 'astrbot'
                ? { text: { ...ASTRBOT_TEXT }, rev: { cmd_config: 1 } }
                : inst.framework_id === 'maibot'
                  ? {
                        text: { ...MAIBOT_TEXT },
                        rev: { bot_config: 1, model_config: 1, adapter_config: 1 },
                    }
                  : inst.framework_id === 'koishi'
                    ? { text: { ...KOISHI_TEXT }, rev: { koishi: 1, env: 1, package: 1 } }
                    : inst.framework_id === 'neobot'
                      ? { text: { ...NEOBOT_TEXT }, rev: { adapter: 1, dashboard: 1 } }
                      : {
                            text: { ...NONEBOT2_TEXT },
                            rev: { env: 1, env_prod: 1, pyproject: 1 },
                        };
        rawStates.set(inst.id, raw);
    }
    return raw;
}

let conflictOnce = false;
export function armConflictOnce(): void {
    conflictOnce = true;
}
/** 消费式读取：与拆分前一致，只在带 base_revision 的 karin 保存时被调用并置回 false */
export function consumeConflictOnce(): boolean {
    if (!conflictOnce) return false;
    conflictOnce = false;
    return true;
}
export function clearConflictOnce(): void {
    conflictOnce = false;
}

export interface MockAppConfigDeps {
    require: (id: string) => AppInstance;
    publish: (instance: AppInstance, reason: string) => void;
}
