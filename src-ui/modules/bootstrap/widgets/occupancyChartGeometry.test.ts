import { describe, expect, it } from 'vitest';
import { buildSmoothPath, clipDisplayPoints, EDGE_EASE, pickHover, scrollPoints, steadyPoints } from './occupancyChartGeometry';

describe('occupancyChartGeometry', () => {
    // 端点圆点用 EDGE_EASE 做纵向动画，前提是它和曲线每一段的形状一致
    it('EDGE_EASE matches one normalized segment of the smooth path', () => {
        const a = { x: 10, y: 80 };
        const b = { x: 30, y: 20 };
        const nums = buildSmoothPath([a, b]).match(/-?\d+(\.\d+)?/g)!.map(Number);
        // M ax ay C c1x c1y, c2x c2y, bx by
        const [, , c1x, c1y, c2x, c2y] = nums;
        const nx = (x: number) => (x - a.x) / (b.x - a.x);
        const ny = (y: number) => (y - a.y) / (b.y - a.y);
        const ease = EDGE_EASE.match(/-?\d+(\.\d+)?/g)!.map(Number);
        // toBeCloseTo 而不是 toEqual：0 / 负数 得到的是 -0
        [nx(c1x), ny(c1y), nx(c2x), ny(c2y)].forEach((v, i) => expect(v).toBeCloseTo(ease[i]));
    });

    // 滚完那一刻和下一段的稳态必须完全重合，WAAPI 平移换段时才不会跳
    it('a finished scroll lands exactly on the next steady layout', () => {
        const prev = [10, 40, 70, 30];
        const incoming = 55;
        const next = [...prev.slice(1), incoming];
        const done = scrollPoints(prev, incoming, 1, next.length, 300, 100).points.slice(1);
        expect(done).toEqual(steadyPoints(next, 300, 100));
    });

    it('scroll progress is a pure horizontal shift of the unshifted track', () => {
        const prev = [10, 40, 70, 30];
        const start = scrollPoints(prev, 55, 0, 4, 300, 100).points;
        const mid = scrollPoints(prev, 55, 0.4, 4, 300, 100).points;
        const stepX = 300 / 3;
        mid.forEach((p, i) => {
            expect(p.y).toBe(start[i].y);
            expect(p.x).toBeCloseTo(start[i].x - stepX * 0.4);
        });
    });

    it('hover at rest reads the same value from the finished scroll and from steady', () => {
        const prev = [10, 40, 70, 30];
        const incoming = 55;
        const next = [...prev.slice(1), incoming];
        const scrolled = scrollPoints(prev, incoming, 1, 4, 300, 100);
        const steady = steadyPoints(next, 300, 100);
        for (const x of [20, 150, 290]) {
            const a = pickHover(clipDisplayPoints(scrolled.points, 0, 300), scrolled.values, x);
            const b = pickHover(clipDisplayPoints(steady, 0, 300), next, x);
            expect(a?.value).toBeCloseTo(b!.value);
            expect(a?.p.y).toBeCloseTo(b!.p.y);
        }
    });
});
