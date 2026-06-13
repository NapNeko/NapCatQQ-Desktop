// Bot 配置 · 远程 + 直接运行：按「组件」页探测结果提示缺项。
//
// 依赖链路（与组件页 ComponentId 一致，勿按 Node 猜 NapCat）：
//
// NapCat 底座：
//   QQ → NapCat（NapCat 注入 Linux QQ，不单独要求 Node.js / noVNC）
//
// SnowLuma 底座：
//   Node.js → QQ → noVNC → SnowLuma

import type { BackendType } from '../../ipc/generated/domain/BackendType';
import type { ComponentId } from '../../ipc/generated/domain/ComponentId';

export type RemoteDirectRunComponentId = Extract<
    ComponentId,
    'qq' | 'napcat' | 'nodejs' | 'snowluma' | 'novnc'
>;

const DISPLAY: Record<RemoteDirectRunComponentId, string> = {
    qq: 'QQ',
    napcat: 'NapCat',
    nodejs: 'Node.js',
    snowluma: 'SnowLuma',
    novnc: 'noVNC',
};

const CHAIN_BY_BACKEND: Record<BackendType, readonly RemoteDirectRunComponentId[]> = {
    napcat: ['qq', 'napcat'],
    snowluma: ['nodejs', 'qq', 'novnc', 'snowluma'],
};

export function remoteDirectRunChain(
    backendType: BackendType,
): readonly RemoteDirectRunComponentId[] {
    return CHAIN_BY_BACKEND[backendType];
}

/** 缺项时返回一行文案；齐了或仍在探测时返回 null。 */
export function formatMissingDirectRunNotice(
    backendType: BackendType,
    installed: Partial<Record<RemoteDirectRunComponentId, boolean | undefined>>,
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