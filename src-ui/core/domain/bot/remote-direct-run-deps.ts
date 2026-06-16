// Bot 配置 · 运行时依赖链定义
//
// 区分「远程直接运行」和「本地直接运行」的依赖，因为形态不同：
//
// - 远程直接运行（Remote + Native）：
//   远程主机上是“干净”的 Linux 环境，需要单独安装各个组件。
//   SnowLuma 需要独立的 Node.js 运行时 + noVNC。
//
// - 本地直接运行（Local + Native）：
//   用户安装的是官方/打包好的 SnowLuma / NapCat 本机包。
//   SnowLuma 本机包通常自带便携 Node 运行时，因此本地不需要再单独探测 "nodejs" 组件。
//
// 组件 ID 与组件页 ComponentId 保持一致。

import type { BackendType } from '../../ipc/generated/domain/BackendType';
import type { ComponentId } from '../../ipc/generated/domain/ComponentId';

export type DirectRunComponentId = Extract<
    ComponentId,
    'qq' | 'napcat' | 'nodejs' | 'snowluma' | 'novnc'
>;

const DISPLAY: Record<DirectRunComponentId, string> = {
    qq: 'QQ',
    napcat: 'NapCat',
    nodejs: 'Node.js',
    snowluma: 'SnowLuma',
    novnc: 'noVNC',
};

/**
 * 远程直接运行需要的组件链（按安装顺序）。
 * 用于远程主机上的 "直接运行" 模式。
 */
const REMOTE_CHAIN: Record<BackendType, readonly DirectRunComponentId[]> = {
    napcat: ['qq', 'napcat'],
    snowluma: ['nodejs', 'qq', 'novnc', 'snowluma'],
};

/**
 * 本地直接运行需要的组件链。
 * 注意：本地 SnowLuma 包通常自带 Node 运行时，因此不包含 'nodejs'。
 */
const LOCAL_CHAIN: Record<BackendType, readonly DirectRunComponentId[]> = {
    napcat: ['qq', 'napcat'],
    snowluma: ['qq', 'snowluma'],   // 本地 SL 包自带 node，不需要单独装 nodejs
};

/** 获取远程直接运行的依赖链 */
export function remoteDirectRunChain(
    backendType: BackendType,
): readonly DirectRunComponentId[] {
    return REMOTE_CHAIN[backendType];
}

/** 获取本地直接运行的依赖链（SnowLuma 不要求 nodejs） */
export function localDirectRunChain(
    backendType: BackendType,
): readonly DirectRunComponentId[] {
    return LOCAL_CHAIN[backendType];
}

/**
 * 向后兼容的旧函数名。
 * 历史代码主要用于远程场景，保留原有行为。
 */
export const directRunRequiredComponents = remoteDirectRunChain;

/** 旧导出别名（兼容旧 import） */
export { remoteDirectRunChain as CHAIN_BY_BACKEND };
export { remoteDirectRunChain as remoteDirectRunChainLegacy };

/** 把组件 ID 转成用户可读名称 */
export function componentIdToDisplayName(id: DirectRunComponentId): string {
    return DISPLAY[id] ?? id;
}

/** 旧函数：仅用于远程场景的文案生成，保留兼容 */
export function formatMissingDirectRunNotice(
    backendType: BackendType,
    installed: Partial<Record<DirectRunComponentId, boolean | undefined>>,
): string | null {
    const chain = remoteDirectRunChain(backendType);
    const missing: string[] = [];

    for (const id of chain) {
        if (installed[id] === false) {
            missing.push(DISPLAY[id]);
        }
    }

    if (missing.length === 0) {
        return null;
    }

    return `未安装 ${missing.join('、')}，请安装`;
}