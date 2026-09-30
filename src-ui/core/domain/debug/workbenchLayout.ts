// 调试台三栏的宽度规则：用户拖出来的宽度（落盘）和窗口放得下的宽度（显示）分开算。
//
// 落盘的是用户想要的宽度；窗口窄时先挤右栏、再挤左栏，都挤到下限还不够就让中栏让一点，
// 不自动收起任何一栏，也不改落盘值——窗口放大后自动回到原来的宽度。
// 没拖过的栏在窄工作台上换一套更窄的默认值：中栏是写请求的地方，窄窗口里先保它。

import type { DebugLayout } from '../../ipc/generated/debug/DebugLayout';
import type { DebugWorkspace } from '../../ipc/generated/debug/DebugWorkspace';

export const LEFT_RAIL_WIDTH = 44;
export const LEFT_WIDTH = { min: 200, max: 360, default: 240 } as const;
export const RIGHT_WIDTH = { min: 300, max: 560, default: 380 } as const;
/** 中栏想要的最小宽度：请求编辑器再窄就没法看了 */
export const CENTER_MIN_WIDTH = 420;
/** 工作台比这窄时，没拖过的栏用 NARROW_DEFAULTS */
export const NARROW_WORKBENCH_WIDTH = 1100;
export const NARROW_DEFAULTS = { left: 220, right: 320 } as const;
/** 可拖的分隔条宽度（也是栏与栏之间的缝） */
export const SPLITTER_WIDTH = 4;
/**
 * 落盘的栏宽是 0：这一栏没拖过（或双击分隔条恢复了默认），画多宽按工作台宽窄取默认值。
 * 显式记成 0，而不是拿「等于默认值」去猜：窄工作台上恰好拖到 240 / 380 也是拖过的，不该弹回窄的默认值
 */
export const UNSET_COLUMN_WIDTH = 0;
/** 从这一版工作区起，栏宽的 0 才表示「没拖过」；更早的文件要先过一遍 upgradeLayoutWidths */
export const EXPLICIT_WIDTH_VERSION = 2;

export type ColumnSide = 'left' | 'right';

function clamp(v: number, min: number, max: number): number {
    if (!Number.isFinite(v)) return min;
    return Math.min(max, Math.max(min, Math.round(v)));
}

/** 落盘值可能是旧版本写的、或者被手改过，读出来先夹一遍 */
export function clampColumnWidth(side: ColumnSide, width: number): number {
    const range = side === 'left' ? LEFT_WIDTH : RIGHT_WIDTH;
    const safe = Number.isFinite(width) ? width : range.default;
    return clamp(safe, range.min, range.max);
}

export interface ResolvedColumns {
    /** 左栏实际宽度；收起时是窄边的宽度 */
    left: number;
    /** 右栏实际宽度；收起时是 0 */
    right: number;
    /** 两条分隔条一共占多宽（收起的那边没有分隔条） */
    splitters: number;
}

function splittersOf(layout: Pick<DebugLayout, 'left_collapsed' | 'right_collapsed'>): number {
    return (layout.left_collapsed ? 0 : SPLITTER_WIDTH) + (layout.right_collapsed ? 0 : SPLITTER_WIDTH);
}

/**
 * 按容器宽度算出每栏该画多宽。`available` 为 null 表示还没量到（首帧），直接用想要的宽度。
 */
export function resolveColumns(
    layout: Pick<DebugLayout, 'left_collapsed' | 'right_collapsed' | 'left_width' | 'right_width'>,
    available: number | null,
): ResolvedColumns {
    const splitters = splittersOf(layout);
    const measured = available !== null && Number.isFinite(available);
    const narrow = measured && available < NARROW_WORKBENCH_WIDTH;
    const wantLeft =
        layout.left_width === UNSET_COLUMN_WIDTH ? (narrow ? NARROW_DEFAULTS.left : LEFT_WIDTH.default) : layout.left_width;
    const wantRight =
        layout.right_width === UNSET_COLUMN_WIDTH ? (narrow ? NARROW_DEFAULTS.right : RIGHT_WIDTH.default) : layout.right_width;
    let left = layout.left_collapsed ? LEFT_RAIL_WIDTH : clampColumnWidth('left', wantLeft);
    let right = layout.right_collapsed ? 0 : clampColumnWidth('right', wantRight);
    if (!measured) return { left, right, splitters };

    let over = left + right + splitters + CENTER_MIN_WIDTH - available;
    if (over > 0 && !layout.right_collapsed) {
        const d = Math.min(over, right - RIGHT_WIDTH.min);
        right -= d;
        over -= d;
    }
    if (over > 0 && !layout.left_collapsed) {
        const d = Math.min(over, left - LEFT_WIDTH.min);
        left -= d;
    }
    return { left, right, splitters };
}

/**
 * 拖某一侧时允许的范围：固定上下限之内，再保证中栏至少留 CENTER_MIN_WIDTH。
 * 窗口太窄、连下限都保不住时上限退到下限（拖不动，但不会反着跳）。
 */
export function dragBounds(
    side: ColumnSide,
    available: number | null,
    resolved: ResolvedColumns,
): { min: number; max: number } {
    const range = side === 'left' ? LEFT_WIDTH : RIGHT_WIDTH;
    if (available === null || !Number.isFinite(available)) return { min: range.min, max: range.max };
    const other = side === 'left' ? resolved.right : resolved.left;
    const room = available - other - resolved.splitters - CENTER_MIN_WIDTH;
    return { min: range.min, max: Math.max(range.min, Math.min(range.max, Math.floor(room))) };
}

/** 键盘调宽（分隔条获得焦点时按 ← →）一步的像素 */
export const KEYBOARD_RESIZE_STEP = 16;

/**
 * 旧版本写的工作区还没有「0 = 没拖过」这回事，没拖过的栏存的就是默认宽度。读进来时换成 0，
 * 版本号跟着升上去，之后存下的 240 / 380 就是用户真拖到的宽度。旧文件里分不清「没拖过」和
 * 「恰好拖到默认值」，一律当没拖过：两者在宽工作台上画出来一样，只在窄工作台上差二三十像素
 */
export function upgradeLayoutWidths(ws: DebugWorkspace): DebugWorkspace {
    if (ws.version >= EXPLICIT_WIDTH_VERSION) return ws;
    const { layout } = ws;
    return {
        ...ws,
        version: EXPLICIT_WIDTH_VERSION,
        layout: {
            ...layout,
            left_width: layout.left_width === LEFT_WIDTH.default ? UNSET_COLUMN_WIDTH : layout.left_width,
            right_width: layout.right_width === RIGHT_WIDTH.default ? UNSET_COLUMN_WIDTH : layout.right_width,
        },
    };
}
