import { describe, expect, it } from 'vitest';
import { boundsOf, fitView, forceLayout } from './graphLayout';

const dist = (a: { x: number; y: number }, b: { x: number; y: number }) => Math.hypot(a.x - b.x, a.y - b.y);

describe('forceLayout', () => {
    const nodes = ['a', 'b', 'c', 'd', 'e', 'f'].map((id) => ({ id }));
    // 两个三角形，中间不连
    const edges = [
        { source: 'a', target: 'b' },
        { source: 'b', target: 'c' },
        { source: 'c', target: 'a' },
        { source: 'd', target: 'e' },
        { source: 'e', target: 'f' },
        { source: 'f', target: 'd' },
    ];

    it('is deterministic and finite', () => {
        const one = forceLayout(nodes, edges);
        const two = forceLayout(nodes, edges);
        expect([...one.entries()]).toEqual([...two.entries()]);
        for (const p of one.values()) {
            expect(Number.isFinite(p.x) && Number.isFinite(p.y)).toBe(true);
        }
    });

    it('pulls linked nodes closer than unlinked ones', () => {
        const pos = forceLayout(nodes, edges);
        const inside = dist(pos.get('a')!, pos.get('b')!);
        const across = dist(pos.get('a')!, pos.get('d')!);
        expect(inside).toBeLessThan(across);
    });

    it('handles empty, single and unknown-edge input', () => {
        expect(forceLayout([], []).size).toBe(0);
        expect(forceLayout([{ id: 'x' }], [{ source: 'x', target: 'nope' }]).get('x')).toEqual({ x: 0, y: 0 });
    });
});

describe('fitView', () => {
    it('centres the bounds inside the viewport', () => {
        const b = boundsOf([
            { x: -100, y: -50 },
            { x: 100, y: 50 },
        ]);
        const v = fitView(b, 500, 300, 50);
        // 宽 200 塞进 400、高 100 塞进 200：取 2（上限也是 2）
        expect(v.k).toBe(2);
        expect(v.x).toBe(250);
        expect(v.y).toBe(150);
    });
});
