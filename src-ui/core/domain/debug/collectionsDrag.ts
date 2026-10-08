// 收藏拖放落点的判定：落点类型、以及「落点变没变 / 落下去是什么结果」两个纯函数。
//
// 拖动状态机本身（指针事件、命中测试、自动滚动）离不开 ref 和 DOM，留在 UI 侧；
// 但落点比较和「这个落点会不会真的改动收藏」可以在这里算清楚并单测。
// 命中测试拿 applyDrop 的返回值是否还是原对象来过滤原位落点（原位不画指示线），
// 所以这里「同位返回同一个对象」的约定不能破，moveRequest / moveFolder 的同位短路是它的前提。

import type { DebugCollections } from '../../ipc/generated/debug/DebugCollections';
import { moveFolder, moveRequest, type RequestSlot } from './collectionsOps';

/** 按下后移动超过这么多像素才算开始拖，免得手抖把单击变成拖动 */
export const DRAG_THRESHOLD_PX = 5;
/** 离列表上下边缘这么近时自动滚动 */
export const AUTO_SCROLL_EDGE_PX = 32;
export const AUTO_SCROLL_MAX_SPEED = 14;

export type DragSource = { kind: 'request' | 'folder'; id: string; label: string };

export type DropTarget =
    | { kind: 'slot'; slot: RequestSlot; y: number; nested: boolean }
    | { kind: 'into'; folderId: string }
    | { kind: 'folder-slot'; index: number; y: number };

export interface DragState {
    pointerId: number;
    startX: number;
    startY: number;
    x: number;
    y: number;
    source: DragSource;
    started: boolean;
    target: DropTarget | null;
    scrollFrame: number;
}

export function sameTarget(a: DropTarget | null, b: DropTarget | null): boolean {
    if (a === null || b === null) return a === b;
    if (a.kind !== b.kind) return false;
    if (a.kind === 'into' && b.kind === 'into') return a.folderId === b.folderId;
    if (a.kind === 'slot' && b.kind === 'slot')
        return a.slot.folderId === b.slot.folderId && a.slot.index === b.slot.index && a.y === b.y;
    if (a.kind === 'folder-slot' && b.kind === 'folder-slot')
        return a.index === b.index && a.y === b.y;
    return false;
}

/** 把落点换成新的整份收藏；落点等于原位时返回原对象 */
export function applyDrop(
    c: DebugCollections,
    source: DragSource,
    target: DropTarget,
): DebugCollections {
    if (source.kind === 'folder')
        return target.kind === 'folder-slot' ? moveFolder(c, source.id, target.index) : c;
    if (target.kind === 'slot') return moveRequest(c, source.id, target.slot);
    if (target.kind === 'into')
        return moveRequest(c, source.id, {
            folderId: target.folderId,
            index: Number.MAX_SAFE_INTEGER,
        });
    return c;
}

/** 收藏只收对象参数；存盘时万一不是对象（旧数据、手改过），确认框按「参数有错」拦下 */
export function plainParams(params: unknown): Record<string, unknown> {
    return params && typeof params === 'object' && !Array.isArray(params)
        ? (params as Record<string, unknown>)
        : {};
}
