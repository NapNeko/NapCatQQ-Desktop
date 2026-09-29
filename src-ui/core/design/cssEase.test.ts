import { describe, expect, it } from 'vitest';
import { cssEase } from './cssEase';
import { motionPresets } from './motion';

describe('cssEase', () => {
    it('maps GSAP power eases by their polynomial degree', () => {
        expect(cssEase('power2.out')).toBe('cubic-bezier(0.33, 1, 0.68, 1)');
        expect(cssEase('power3.in')).toBe('cubic-bezier(0.5, 0, 0.75, 0)');
    });

    it('keeps single-segment custom curves exact', () => {
        expect(cssEase('ndf-critical')).toBe('cubic-bezier(0.18, 1, 0.45, 1)');
        expect(cssEase('ndf-spring')).toBe('cubic-bezier(0.34, 1.32, 0.46, 1.06)');
    });

    it('falls back for curves CSS cannot express', () => {
        expect(cssEase('ndf-elastic')).toBe('ease-out');
        expect(cssEase('ndf-bounce', 'ease')).toBe('ease');
    });

    // 页面切换走 WAAPI，每个档位的进场 / 退场缓动都必须有对应，不然就悄悄退化成 ease-out
    it('covers page enter / exit eases of every motion level', () => {
        for (const preset of Object.values(motionPresets)) {
            expect(cssEase(preset.timing.ease.enter, 'MISSING')).not.toBe('MISSING');
            expect(cssEase(preset.timing.ease.exit, 'MISSING')).not.toBe('MISSING');
        }
    });
});
