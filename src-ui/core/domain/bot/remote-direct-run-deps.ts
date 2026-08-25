// Bot 配置 · 运行时依赖链定义
//
// 区分「远程直接运行」和「本地直接运行」的依赖，因为形态不同：
//
// - 远程直接运行（Remote + Native）：
//   NapCat：QQ + NapCat。
//   SnowLuma 完整包自带 node，不再要求单独的 Node.js 组件；lite 才要 Node.js + noVNC。
//
// - 本地直接运行（Local + Native）：
//   SnowLuma 本机包自带便携 Node，不探测 nodejs。
//
// 组件 ID 与组件页 ComponentId 保持一致。

import type { BackendType } from '../../ipc/generated/domain/BackendType';
import type { ComponentId } from '../../ipc/generated/domain/ComponentId';
import type { RemoteInventory } from '../../ipc/generated/domain/RemoteInventory';
import type { RemoteSelectedPaths } from '../../ipc/generated/domain/RemoteSelectedPaths';
import type { SnowLumaLinuxPackage } from '../../ipc/generated/domain/SnowLumaLinuxPackage';

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
 * SnowLuma 完整包自带 node，链里不含 nodejs；lite 才要。
 */
const REMOTE_CHAIN_NAPCAT: readonly DirectRunComponentId[] = ['qq', 'napcat'];
const REMOTE_CHAIN_SNOWLUMA_FULL: readonly DirectRunComponentId[] = [
    'qq',
    'novnc',
    'snowluma',
];
const REMOTE_CHAIN_SNOWLUMA_LITE: readonly DirectRunComponentId[] = [
    'nodejs',
    'qq',
    'novnc',
    'snowluma',
];

/**
 * 本地直接运行需要的组件链。
 * 注意：本地 SnowLuma 包通常自带 Node 运行时，因此不包含 'nodejs'。
 */
const LOCAL_CHAIN: Record<BackendType, readonly DirectRunComponentId[]> = {
    napcat: ['qq', 'napcat'],
    snowluma: ['qq', 'snowluma'],   // 本地 SL 包自带 node，不需要单独装 nodejs
};

export function isBundledSnowlumaNode(
    nodeBin?: string | null,
    snowlumaDir?: string | null,
): boolean {
    const bin = (nodeBin ?? '').replace(/\\/g, '/').replace(/\/+$/, '');
    if (!bin) return false;
    const dir = (snowlumaDir ?? '').replace(/\\/g, '/').replace(/\/+$/, '');
    if (dir && bin === `${dir}/node`) return true;
    return bin.endsWith('/snowluma/node');
}

/** 对照库存：完整包自带 `{snowluma}/node`，lite 没有。 */
export function inferSnowLumaLinuxPackageFromInventory(
    inventory?: RemoteInventory | null,
): SnowLumaLinuxPackage | null {
    if (!inventory) return null;
    const selected: RemoteSelectedPaths | undefined = inventory.selected;
    const dir = selected?.snowlumaDir?.replace(/\\/g, '/').replace(/\/+$/, '');
    if (!dir) return null;
    if (isBundledSnowlumaNode(selected?.nodeBin, dir)) {
        return 'full';
    }
    const bundled = `${dir}/node`;
    const hasBundled = (inventory.items ?? []).some((it) => {
        if (it.kind !== 'nodejs') return false;
        const p = (it.nodeBin || it.root || '').replace(/\\/g, '/').replace(/\/+$/, '');
        return p === bundled || p.endsWith('/snowluma/node');
    });
    return hasBundled ? 'full' : 'lite';
}

/** 获取远程直接运行的依赖链 */
export function remoteDirectRunChain(
    backendType: BackendType,
    snowlumaLinuxPackage?: SnowLumaLinuxPackage | null,
): readonly DirectRunComponentId[] {
    if (backendType === 'napcat') {
        return REMOTE_CHAIN_NAPCAT;
    }
    if (snowlumaLinuxPackage === 'full') {
        return REMOTE_CHAIN_SNOWLUMA_FULL;
    }
    return REMOTE_CHAIN_SNOWLUMA_LITE;
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
    snowlumaLinuxPackage?: SnowLumaLinuxPackage | null,
): string | null {
    const chain = remoteDirectRunChain(backendType, snowlumaLinuxPackage);
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