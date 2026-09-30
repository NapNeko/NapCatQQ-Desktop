import { describe, expect, it } from 'vitest';
import {
    CENTER_MIN_WIDTH,
    EXPLICIT_WIDTH_VERSION,
    LEFT_RAIL_WIDTH,
    NARROW_DEFAULTS,
    NARROW_WORKBENCH_WIDTH,
    SPLITTER_WIDTH,
    UNSET_COLUMN_WIDTH,
    clampColumnWidth,
    dragBounds,
    resolveColumns,
    upgradeLayoutWidths,
} from './workbenchLayout';
import type { DebugWorkspace } from '../../ipc/generated/debug/DebugWorkspace';

// 两栏都没拖过
const base = { left_collapsed: false, right_collapsed: false, left_width: UNSET_COLUMN_WIDTH, right_width: UNSET_COLUMN_WIDTH };

describe('clampColumnWidth', () => {
    it('夹在上下限之内，坏值回到默认', () => {
        expect(clampColumnWidth('left', 100)).toBe(200);
        expect(clampColumnWidth('left', 999)).toBe(360);
        expect(clampColumnWidth('right', 250)).toBe(300);
        expect(clampColumnWidth('right', 700)).toBe(560);
        expect(clampColumnWidth('left', Number.NaN)).toBe(240);
        expect(clampColumnWidth('right', 412.6)).toBe(413);
    });
});

describe('resolveColumns', () => {
    it('没量到宽度时直接用想要的宽度', () => {
        expect(resolveColumns(base, null)).toEqual({ left: 240, right: 380, splitters: SPLITTER_WIDTH * 2 });
    });

    it('放得下时不动', () => {
        expect(resolveColumns(base, 1400)).toMatchObject({ left: 240, right: 380 });
    });

    it('窄了先挤右栏，再挤左栏', () => {
        const dragged = { ...base, left_width: 260, right_width: 400 };
        // 260 + 400 + 8 + 420 = 1088；少 50 全由右栏让
        expect(resolveColumns(dragged, 1038)).toMatchObject({ left: 260, right: 350 });
        // 右栏挤到 300 还差 20，左栏让 20
        expect(resolveColumns(dragged, 968)).toMatchObject({ left: 240, right: 300 });
        // 都到下限就不再挤（中栏自己变窄）
        expect(resolveColumns(dragged, 700)).toMatchObject({ left: 200, right: 300 });
    });

    it('工作台不到 1100 宽时，没拖过的栏换成窄的默认值（左 220 / 右 320）；拖过的照旧', () => {
        expect(resolveColumns(base, 1099)).toMatchObject({ left: NARROW_DEFAULTS.left, right: NARROW_DEFAULTS.right });
        expect(resolveColumns(base, NARROW_WORKBENCH_WIDTH)).toMatchObject({ left: 240, right: 380 });
        expect(resolveColumns({ ...base, left_width: 300 }, 1099)).toMatchObject({ left: 300, right: 320 });
        // 恰好拖到宽工作台的默认值也是拖过的：不弹回窄的默认值
        expect(resolveColumns({ ...base, left_width: 240, right_width: 380 }, 1099)).toMatchObject({ left: 240, right: 380 });
        // 窄默认值也放不下时照样按规则挤：220 + 320 + 8 + 420 = 968
        expect(resolveColumns(base, 948)).toMatchObject({ left: 220, right: 300 });
        // 还没量到宽度时不知道窄不窄，用想要的宽度
        expect(resolveColumns(base, null)).toMatchObject({ left: 240, right: 380 });
    });

    it('收起的一侧：左栏是窄边、右栏是 0，也不占分隔条', () => {
        const r = resolveColumns({ ...base, left_collapsed: true, right_collapsed: true }, 1200);
        expect(r).toEqual({ left: LEFT_RAIL_WIDTH, right: 0, splitters: 0 });
    });

    it('收起的一侧不参与挤', () => {
        const r = resolveColumns({ ...base, right_collapsed: true }, 600);
        expect(r).toMatchObject({ left: 200, right: 0, splitters: SPLITTER_WIDTH });
    });
});

describe('dragBounds', () => {
    it('宽窗口时就是固定上下限', () => {
        const resolved = resolveColumns(base, 1600);
        expect(dragBounds('left', 1600, resolved)).toEqual({ min: 200, max: 360 });
        expect(dragBounds('right', 1600, resolved)).toEqual({ min: 300, max: 560 });
    });

    it('上限保证中栏留够宽度', () => {
        const available = 1100;
        const resolved = resolveColumns(base, available);
        // 左栏最多 = 1100 - 右 380 - 8 - 420 = 292
        expect(dragBounds('left', available, resolved).max).toBe(available - 380 - 8 - CENTER_MIN_WIDTH);
    });

    it('连下限都保不住时上限退到下限', () => {
        const resolved = resolveColumns(base, 700);
        expect(dragBounds('right', 700, resolved)).toEqual({ min: 300, max: 300 });
    });
});

describe('upgradeLayoutWidths', () => {
    const ws = (version: number, left_width: number, right_width: number) =>
        ({
            version,
            layout: { left_collapsed: false, right_collapsed: true, left_width, right_width, right_view: 'list' },
        }) as DebugWorkspace;

    it('旧版本里等于默认值的宽度当作没拖过，拖过的原样留着，版本号升上去', () => {
        expect(upgradeLayoutWidths(ws(1, 240, 380))).toEqual(ws(EXPLICIT_WIDTH_VERSION, UNSET_COLUMN_WIDTH, UNSET_COLUMN_WIDTH));
        expect(upgradeLayoutWidths(ws(1, 260, 380))).toEqual(ws(EXPLICIT_WIDTH_VERSION, 260, UNSET_COLUMN_WIDTH));
    });

    it('已经是新版本的不动：240 / 380 就是用户拖到的宽度', () => {
        const current = ws(EXPLICIT_WIDTH_VERSION, 240, 380);
        expect(upgradeLayoutWidths(current)).toBe(current);
    });
});
