import { describe, expect, it } from 'vitest';
import {
    celestialPosition,
    getDayPhase,
    getGreeting,
    starCountFor,
    starField,
} from './dayPhase';

describe('getDayPhase', () => {
    it('时段边界与问候语一致', () => {
        expect(getDayPhase(5)).toBe('morning');
        expect(getDayPhase(10)).toBe('morning');
        expect(getDayPhase(11)).toBe('noon');
        expect(getDayPhase(14)).toBe('afternoon');
        expect(getDayPhase(18)).toBe('evening');
        expect(getDayPhase(22)).toBe('evening');
        expect(getDayPhase(23)).toBe('night');
        expect(getDayPhase(0)).toBe('night');
        expect(getDayPhase(4)).toBe('night');
        expect(getGreeting('night').title).toBe('夜深了');
    });
});

describe('celestialPosition', () => {
    it('白天是太阳，正午最高；夜里是月亮', () => {
        const dawn = celestialPosition(5, 0);
        const noon = celestialPosition(11, 30);
        const dusk = celestialPosition(17, 59);
        expect(dawn.kind).toBe('sun');
        expect(noon.kind).toBe('sun');
        expect(dusk.kind).toBe('sun');
        expect(noon.altitude).toBeCloseTo(1, 2);
        expect(noon.y).toBeLessThan(dawn.y);
        expect(noon.y).toBeLessThan(dusk.y);
        expect(dawn.x).toBeLessThan(noon.x);
        expect(noon.x).toBeLessThan(dusk.x);

        expect(celestialPosition(18, 0).kind).toBe('moon');
        expect(celestialPosition(0, 30).kind).toBe('moon');
        expect(celestialPosition(4, 59).kind).toBe('moon');
    });

    it('月亮跨午夜连续走，不会在 0 点跳回起点', () => {
        const before = celestialPosition(23, 59);
        const after = celestialPosition(0, 1);
        expect(Math.abs(after.x - before.x)).toBeLessThan(1);
    });

    it('轨迹始终在卡片范围内且偏右', () => {
        for (let h = 0; h < 24; h += 1) {
            const p = celestialPosition(h, 30);
            expect(p.x).toBeGreaterThanOrEqual(40);
            expect(p.x).toBeLessThanOrEqual(100);
            expect(p.y).toBeGreaterThanOrEqual(0);
            expect(p.y).toBeLessThanOrEqual(100);
        }
    });
});

describe('starField', () => {
    it('固定种子结果稳定，左半边星星只在顶部一条带', () => {
        const a = starField(18);
        const b = starField(18);
        expect(a).toEqual(b);
        for (const s of a) {
            expect(s.x).toBeGreaterThanOrEqual(0);
            expect(s.x).toBeLessThanOrEqual(100);
            if (s.x < 55) expect(s.y).toBeLessThan(20);
        }
    });

    it('只有傍晚和夜里有星星', () => {
        expect(starCountFor('morning')).toBe(0);
        expect(starCountFor('noon')).toBe(0);
        expect(starCountFor('afternoon')).toBe(0);
        expect(starCountFor('evening')).toBeGreaterThan(0);
        expect(starCountFor('night')).toBeGreaterThan(starCountFor('evening'));
    });
});
